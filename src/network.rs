use crate::plugin::{Packet, PluginManager, ServerCommand};
use bytes::{Buf, BufMut, BytesMut};
use log::{error, info, trace};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};

static CLIENT_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Handshake,
    Status,
    Login,
    #[allow(dead_code)]
    Transfer,
    Configuration,
    #[allow(dead_code)]
    Play,
}

pub fn read_varint(buf: &[u8]) -> Option<(i32, usize)> {
    let mut num_read = 0;
    let mut result: i32 = 0;

    for &read in buf {
        let value = (read & 0b0111_1111) as i32;
        result |= value << (7 * num_read);
        num_read += 1;

        if num_read > 5 {
            return None;
        }

        if (read & 0b1000_0000) == 0 {
            return Some((result, num_read));
        }
    }
    None
}

pub fn write_varint(mut value: i32, buf: &mut BytesMut) {
    loop {
        let mut temp = (value & 0b0111_1111) as u8;
        value = (value as u32 >> 7) as i32;
        if value != 0 {
            temp |= 0b1000_0000;
        }
        buf.put_u8(temp);
        if value == 0 {
            break;
        }
    }
}

pub fn varint_size(value: i32) -> usize {
    match value {
        0 => 1,
        v => ((38 - (v as u32).leading_zeros()) / 7) as usize,
    }
}

pub fn read_string(buf: &[u8]) -> Option<(&str, usize)> {
    if let Some((len, len_bytes)) = read_varint(buf) {
        if !(0..=32767).contains(&len) {
            return None;
        }
        let len = len as usize;
        if buf.len() >= len_bytes + len
            && let Ok(s) = std::str::from_utf8(&buf[len_bytes..len_bytes + len])
        {
            return Some((s, len_bytes + len));
        }
    }
    None
}

pub fn write_string(s: &str, buf: &mut BytesMut) {
    write_varint(s.len() as i32, buf);
    buf.put_slice(s.as_bytes());
}

#[allow(dead_code)]
pub fn write_bytes(data: &[u8], buf: &mut BytesMut) {
    write_varint(data.len() as i32, buf);
    buf.put_slice(data);
}

pub fn read_ushort(buf: &[u8]) -> Option<(u16, usize)> {
    if buf.len() >= 2 {
        let val = ((buf[0] as u16) << 8) | (buf[1] as u16);
        Some((val, 2))
    } else {
        None
    }
}

pub struct Server {
    listener: TcpListener,
    plugin_manager: Arc<PluginManager>,
    client_senders: Arc<RwLock<HashMap<u64, mpsc::Sender<Packet>>>>,
}

impl Server {
    pub async fn bind(addr: &str, plugin_manager: Arc<PluginManager>) -> std::io::Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Self {
            listener,
            plugin_manager,
            client_senders: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub async fn run(self, mut rx: mpsc::Receiver<ServerCommand>) {
        info!(
            "Server listening on {:?}",
            self.listener.local_addr().unwrap()
        );

        let client_senders = Arc::clone(&self.client_senders);
        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    ServerCommand::SendPacket { client_id, packet } => {
                        let senders = client_senders.read().await;
                        if let Some(tx) = senders.get(&client_id) {
                            let _ = tx.send(packet).await;
                        }
                    }
                    ServerCommand::Disconnect { client_id } => {
                        let mut senders = client_senders.write().await;
                        senders.remove(&client_id);
                    }
                }
            }
        });

        loop {
            match self.listener.accept().await {
                Ok((stream, addr)) => {
                    let client_id = CLIENT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
                    info!("Client connected: {} (ID: {})", addr, client_id);

                    let (client_tx, client_rx) = mpsc::channel(1024);
                    self.client_senders
                        .write()
                        .await
                        .insert(client_id, client_tx);

                    let plugin_manager = Arc::clone(&self.plugin_manager);
                    let client_senders = Arc::clone(&self.client_senders);
                    tokio::spawn(async move {
                        if let Err(e) =
                            handle_client(stream, client_id, plugin_manager, client_rx).await
                        {
                            error!("Error handling client {}: {}", client_id, e);
                        }
                        info!("Client disconnected (ID: {})", client_id);
                        client_senders.write().await.remove(&client_id);
                    });
                }
                Err(e) => {
                    error!("Error accepting connection: {}", e);
                }
            }
        }
    }
}

async fn handle_client(
    mut stream: TcpStream,
    client_id: u64,
    plugin_manager: Arc<PluginManager>,
    mut client_rx: mpsc::Receiver<Packet>,
) -> std::io::Result<()> {
    let ip = stream.peer_addr()?.ip().to_string();
    plugin_manager.dispatch_connect(client_id, &ip).await;

    let mut buf = BytesMut::with_capacity(4096);
    let mut state = ConnectionState::Handshake;
    let mut is_from_transfer = false;
    let mut client_name: Option<String> = None;
    let mut client_protocol: i32 = 0;
    let mut client_hostname: String = String::new();

    let mut keep_alive_interval = tokio::time::interval(std::time::Duration::from_secs(10));
    keep_alive_interval.tick().await;

    loop {
        tokio::select! {
            _ = keep_alive_interval.tick() => {
                if state == ConnectionState::Configuration {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as i64;

                    let payload_len = 1 + 8;
                    let mut out_buf = BytesMut::with_capacity(varint_size(payload_len as i32) + payload_len);

                    write_varint(payload_len as i32, &mut out_buf);
                    write_varint(0x04, &mut out_buf);
                    out_buf.put_i64(now);

                    if stream.write_all(&out_buf).await.is_err() {
                        return Ok(());
                    }
                }
            }

            n = stream.read_buf(&mut buf) => {
                let n = n?;
                if n == 0 {
                    return Ok(());
                }

                loop {
                    let (length, length_bytes) = match read_varint(&buf) {
                        Some(res) => res,
                        None => break,
                    };

                    if buf.len() < length as usize + length_bytes {
                        break;
                    }

                    buf.advance(length_bytes);
                    let mut packet_buf = buf.split_to(length as usize);

                    if let Some((packet_id, id_bytes)) = read_varint(&packet_buf) {
                        packet_buf.advance(id_bytes);

                        let packet_data = packet_buf.freeze();
                        let packet = Packet {
                            id: packet_id,
                            data: packet_data,
                        };

                        trace!("Received packet ID 0x{:02X} in state {:?} from client {}", packet_id, state, client_id);

                        match state {
                            ConnectionState::Handshake => {
                                if packet_id == 0x00 {
                                    let mut payload = &packet.data[..];
                                    if let Some((protocol_version, pv_bytes)) = read_varint(payload) {
                                        payload.advance(pv_bytes);
                                        if let Some((server_address, sa_bytes)) = read_string(payload) {
                                            payload.advance(sa_bytes);
                                            if let Some((server_port, sp_bytes)) = read_ushort(payload) {
                                                payload.advance(sp_bytes);
                                                if let Some((next_state, _ns_bytes)) = read_varint(payload) {
                                                    client_protocol = protocol_version;
                                                    client_hostname = server_address.to_string();
                                                    plugin_manager.dispatch_handshake(client_id, &ip, protocol_version, server_address, server_port, next_state).await;

                                                    if (next_state == 2 || next_state == 3) && protocol_version < 766 {
                                                        let mut out_payload = BytesMut::new();
                                                        let reason = r#"{"text":"Please use Minecraft 1.20.5 or above."}"#;
                                                        write_string(reason, &mut out_payload);

                                                        let mut response = BytesMut::new();
                                                        write_varint(1 + out_payload.len() as i32, &mut response);
                                                        write_varint(0x00, &mut response);
                                                        response.put_slice(&out_payload);

                                                        stream.write_all(&response).await?;
                                                        return Ok(());
                                                    }

                                                    match next_state {
                                                        1 => state = ConnectionState::Status,
                                                        2 => state = ConnectionState::Login,
                                                        3 => {
                                                            state = ConnectionState::Login;
                                                            is_from_transfer = true;
                                                        }
                                                        _ => return Ok(()),
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            ConnectionState::Status => {
                                if packet_id == 0x00 {
                                    plugin_manager.dispatch_status_request(client_id, &ip, client_protocol, &client_hostname).await;
                                } else if packet_id == 0x01 {
                                    if packet.data.len() >= 8 {
                                        let mut response = BytesMut::new();
                                        write_varint(1 + 8, &mut response);
                                        write_varint(0x01, &mut response);
                                        response.put_slice(&packet.data[0..8]);

                                        stream.write_all(&response).await?;
                                    }
                                }
                            }
                            ConnectionState::Login | ConnectionState::Transfer => {
                                if packet_id == 0x00 {
                                    let mut payload = &packet.data[..];
                                    if let Some((name, name_bytes)) = read_string(payload) {
                                        payload.advance(name_bytes);

                                        client_name = Some(name.to_string());

                                        let allow_login = plugin_manager.dispatch_pre_login(client_id, &ip, name, is_from_transfer).await;

                                        if allow_login {
                                            let uuid_bytes: [u8; 16] = if payload.len() >= 16 {
                                                let (u, _rest) = payload.split_at(16);
                                                u.try_into().unwrap_or_default()
                                            } else {
                                                use md5::{Md5, Digest};
                                                let mut hasher = Md5::new();
                                                hasher.update(b"OfflinePlayer:");
                                                hasher.update(name.as_bytes());
                                                let mut result: [u8; 16] = hasher.finalize().into();
                                                result[6] = (result[6] & 0x0f) | 0x30;
                                                result[8] = (result[8] & 0x3f) | 0x80;
                                                result
                                            };

                                            let name_bytes = name.as_bytes();
                                            let login_payload_len = 16 + varint_size(name_bytes.len() as i32) + name_bytes.len() + 1;

                                            let packet_len = 1 + login_payload_len;

                                            let mut final_response = BytesMut::with_capacity(varint_size(packet_len as i32) + packet_len);
                                            write_varint(packet_len as i32, &mut final_response);
                                            write_varint(0x02, &mut final_response);
                                            final_response.put_slice(&uuid_bytes);
                                            write_string(name, &mut final_response);
                                            write_varint(0, &mut final_response);

                                            stream.write_all(&final_response).await?;
                                        }
                                    }
                                } else if packet_id == 0x03 {
                                    plugin_manager.dispatch_packet(client_id, state, &packet).await;
                                    state = ConnectionState::Configuration;
                                    let display_name = client_name.as_deref().unwrap_or("Unknown");
                                    if is_from_transfer {
                                        info!("Player {} is now connected and comes from transfer", display_name);
                                    } else {
                                        info!("Player {} is now connected", display_name);
                                    }
                                    continue;
                                } else if packet_id == 0x04 {
                                    let mut payload = &packet.data[..];
                                    if let Some((key, key_bytes)) = read_string(payload) {
                                        payload.advance(key_bytes);
                                        if payload.has_remaining() {
                                            let has_payload = payload.get_u8() != 0;
                                            if has_payload {
                                                if let Some((payload_len, len_bytes)) = read_varint(payload) {
                                                    payload.advance(len_bytes);
                                                    if payload.len() >= payload_len as usize {
                                                        let (cookie_payload, _rest) = payload.split_at(payload_len as usize);
                                                        plugin_manager.dispatch_cookie_response(client_id, key, Some(cookie_payload)).await;
                                                    }
                                                }
                                            } else {
                                                plugin_manager.dispatch_cookie_response(client_id, key, None).await;
                                            }
                                        }
                                    }
                                }
                                plugin_manager.dispatch_packet(client_id, state, &packet).await;
                            }
                            ConnectionState::Configuration => {
                                if packet_id == 0x00 {
                                    let mut payload = &packet.data[..];

                                    if let Some((locale, locale_bytes)) = read_string(payload) {
                                        payload.advance(locale_bytes);
                                        if payload.has_remaining() {
                                            let view_distance = payload.get_i8();
                                            if let Some((chat_visibility, cv_bytes)) = read_varint(payload) {
                                                payload.advance(cv_bytes);
                                                if payload.has_remaining() {
                                                    let chat_colors = payload.get_u8() != 0;
                                                    if payload.has_remaining() {
                                                        let _displayed_skin_parts = payload.get_u8();
                                                        if let Some((main_hand, mh_bytes)) = read_varint(payload) {
                                                            payload.advance(mh_bytes);
                                                            if payload.len() >= 2 {
                                                                let _enable_text_filtering = payload.get_u8() != 0;
                                                                let _allow_server_listing = payload.get_u8() != 0;

                                                                if client_protocol >= 768 {
                                                                    if let Some((_particle_status, ps_bytes)) = read_varint(payload) {
                                                                        payload.advance(ps_bytes);
                                                                    }
                                                                }

                                                                plugin_manager.dispatch_ready(client_id, &ip, locale, view_distance, chat_visibility, chat_colors, main_hand).await;
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else if packet_id == 0x01 {
                                    let mut payload = &packet.data[..];
                                    if let Some((key, key_bytes)) = read_string(payload) {
                                        payload.advance(key_bytes);
                                        if payload.has_remaining() {
                                            let has_payload = payload.get_u8() != 0;
                                            if has_payload {
                                                if let Some((payload_len, len_bytes)) = read_varint(payload) {
                                                    payload.advance(len_bytes);
                                                    if payload.len() >= payload_len as usize {
                                                        let (cookie_payload, _rest) = payload.split_at(payload_len as usize);
                                                        plugin_manager.dispatch_cookie_response(client_id, key, Some(cookie_payload)).await;
                                                    }
                                                }
                                            } else {
                                                plugin_manager.dispatch_cookie_response(client_id, key, None).await;
                                            }
                                        }
                                    }
                                } else if packet_id == 0x02 {
                                    let mut payload = &packet.data[..];
                                    if let Some((channel, channel_bytes)) = read_string(payload) {
                                        payload.advance(channel_bytes);
                                        plugin_manager.dispatch_plugin_message(client_id, channel, payload).await;
                                    }
                                } else if packet_id == 0x03 {
                                    plugin_manager.dispatch_packet(client_id, state, &packet).await;
                                    state = ConnectionState::Play;
                                    continue;
                                } else if packet_id == 0x04 {
                                    // Keep Alive
                                    // Just read and ignore
                                    continue;
                                } else if packet_id == 0x06 {
                                    let mut payload = &packet.data[..];
                                    if payload.len() >= 16 {
                                        let (uuid_bytes, rest) = payload.split_at(16);
                                        payload = rest;
                                        if let Ok(uuid) = uuid::Uuid::from_slice(uuid_bytes)
                                            && let Some((result, _)) = read_varint(payload) {
                                                plugin_manager.dispatch_resource_pack_response(client_id, uuid, result).await;
                                            }
                                    }
                                } else if packet_id == 0x07 {
                                    // Client Select Known Packs
                                    // Ignore
                                    continue;
                                }

                                plugin_manager.dispatch_packet(client_id, state, &packet).await;
                            }
                            _ => {
                                plugin_manager.dispatch_packet(client_id, state, &packet).await;
                            }
                        }
                    } else {
                        break;
                    }
                }
            }

            Some(packet) = client_rx.recv() => {
                let packet_id_size = varint_size(packet.id);
                let payload_len = packet_id_size + packet.data.len();
                let length_varint_size = varint_size(payload_len as i32);

                let mut out_buf = BytesMut::with_capacity(length_varint_size + payload_len);
                write_varint(payload_len as i32, &mut out_buf);
                write_varint(packet.id, &mut out_buf);
                out_buf.put_slice(&packet.data);

                stream.write_all(&out_buf).await?;
            }

            else => {
                return Ok(());
            }
        }
    }
}
