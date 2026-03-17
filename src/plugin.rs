use crate::network::{write_bytes, write_string, write_varint, ConnectionState};
use async_trait::async_trait;
use bytes::BytesMut;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct Packet {
    pub id: i32,
    pub data: bytes::Bytes,
}

pub enum ServerCommand {
    SendPacket { client_id: u64, packet: Packet },
    Disconnect { client_id: u64 },
}

#[derive(Clone)]
pub struct AppHandle {
    pub tx: mpsc::Sender<ServerCommand>,
}

impl AppHandle {
    pub async fn send_packet(
        &self,
        client_id: u64,
        packet: Packet,
    ) -> Result<(), mpsc::error::SendError<ServerCommand>> {
        self.tx
            .send(ServerCommand::SendPacket { client_id, packet })
            .await
    }

    pub async fn disconnect(
        &self,
        client_id: u64,
    ) -> Result<(), mpsc::error::SendError<ServerCommand>> {
        self.tx.send(ServerCommand::Disconnect { client_id }).await
    }

    #[allow(dead_code)]
    pub async fn kick(
        &self,
        client_id: u64,
        reason: &str,
    ) -> Result<(), mpsc::error::SendError<ServerCommand>> {
        let mut payload = BytesMut::new();
        let json_reason = format!(r#"{{"text":"{}"}}"#, reason);
        write_string(&json_reason, &mut payload);

        let packet = Packet {
            id: 0x00,
            data: payload.freeze(),
        };

        self.send_packet(client_id, packet).await?;
        self.disconnect(client_id).await
    }

    #[allow(dead_code)]
    pub async fn transfer(
        &self,
        client_id: u64,
        host: &str,
        port: i32,
    ) -> Result<(), mpsc::error::SendError<ServerCommand>> {
        let mut transfer_payload = BytesMut::new();
        write_string(host, &mut transfer_payload);
        write_varint(port, &mut transfer_payload);

        let transfer_packet = Packet {
            id: 0x0B,
            data: transfer_payload.freeze(),
        };

        self.send_packet(client_id, transfer_packet).await
    }

    #[allow(dead_code)]
    pub async fn store_cookie(
        &self,
        client_id: u64,
        key: &str,
        payload: &[u8],
    ) -> Result<(), mpsc::error::SendError<ServerCommand>> {
        let mut cookie_payload = BytesMut::new();
        write_string(key, &mut cookie_payload);
        write_bytes(payload, &mut cookie_payload);

        let packet = Packet {
            id: 0x0A,
            data: cookie_payload.freeze(),
        };

        self.send_packet(client_id, packet).await
    }

    #[allow(dead_code)]
    pub async fn send_keep_alive(
        &self,
        client_id: u64,
        payload_val: i64,
    ) -> Result<(), mpsc::error::SendError<ServerCommand>> {
        use bytes::BufMut;
        let mut payload = BytesMut::new();
        payload.put_i64(payload_val);

        let packet = Packet {
            id: 0x04,
            data: payload.freeze(),
        };

        self.send_packet(client_id, packet).await
    }

    #[allow(dead_code)]
    pub async fn config_disconnect(
        &self,
        client_id: u64,
        component: &str,
    ) -> Result<(), mpsc::error::SendError<ServerCommand>> {
        use bytes::BufMut;
        let mut payload = BytesMut::new();

        payload.put_u8(10);

        payload.put_u8(8);
        let name = "text";
        payload.put_u16(name.len() as u16);
        payload.put_slice(name.as_bytes());

        let text_bytes = component.as_bytes();
        payload.put_u16(text_bytes.len() as u16);
        payload.put_slice(text_bytes);

        payload.put_u8(0);

        let packet = Packet {
            id: 0x02,
            data: payload.freeze(),
        };

        self.send_packet(client_id, packet).await?;
        self.disconnect(client_id).await
    }

    #[allow(dead_code)]
    pub async fn reset_chat(
        &self,
        client_id: u64,
    ) -> Result<(), mpsc::error::SendError<ServerCommand>> {
        let packet = Packet {
            id: 0x06,
            data: bytes::Bytes::new(),
        };

        self.send_packet(client_id, packet).await
    }

    #[allow(dead_code)]
    pub async fn fetch_cookie(
        &self,
        client_id: u64,
        key: &str,
    ) -> Result<(), mpsc::error::SendError<ServerCommand>> {
        let mut fetch_payload = BytesMut::new();
        write_string(key, &mut fetch_payload);

        let packet = Packet {
            id: 0x00,
            data: fetch_payload.freeze(),
        };

        self.send_packet(client_id, packet).await
    }

    #[allow(dead_code)]
    pub async fn fetch_login_cookie(
        &self,
        client_id: u64,
        key: &str,
    ) -> Result<(), mpsc::error::SendError<ServerCommand>> {
        let mut fetch_payload = BytesMut::new();
        write_string(key, &mut fetch_payload);

        let packet = Packet {
            id: 0x05,
            data: fetch_payload.freeze(),
        };

        self.send_packet(client_id, packet).await
    }

    #[allow(dead_code)]
    pub async fn send_plugin_message(
        &self,
        client_id: u64,
        channel: &str,
        data: &[u8],
    ) -> Result<(), mpsc::error::SendError<ServerCommand>> {
        let mut payload = BytesMut::new();
        write_string(channel, &mut payload);
        use bytes::BufMut;
        payload.put_slice(data);

        let packet = Packet {
            id: 0x01,
            data: payload.freeze(),
        };

        self.send_packet(client_id, packet).await
    }

    #[allow(dead_code)]
    pub async fn add_resource_pack(
        &self,
        client_id: u64,
        uuid: uuid::Uuid,
        url: &str,
        hash: &str,
        forced: bool,
        prompt_message: Option<&str>,
    ) -> Result<(), mpsc::error::SendError<ServerCommand>> {
        let mut payload = BytesMut::new();
        use bytes::BufMut;
        payload.put_slice(uuid.as_bytes());
        write_string(url, &mut payload);
        write_string(hash, &mut payload);
        payload.put_u8(if forced { 1 } else { 0 });
        if let Some(msg) = prompt_message {
            payload.put_u8(1);
            payload.put_u8(10);
            payload.put_u8(8);
            let name = "text";
            payload.put_u16(name.len() as u16);
            payload.put_slice(name.as_bytes());
            let text_bytes = msg.as_bytes();
            payload.put_u16(text_bytes.len() as u16);
            payload.put_slice(text_bytes);
            payload.put_u8(0);
        } else {
            payload.put_u8(0);
        }

        let packet = Packet {
            id: 0x09,
            data: payload.freeze(),
        };
        self.send_packet(client_id, packet).await
    }

    #[allow(dead_code)]
    pub async fn remove_resource_pack(
        &self,
        client_id: u64,
        uuid: Option<uuid::Uuid>,
    ) -> Result<(), mpsc::error::SendError<ServerCommand>> {
        let mut payload = BytesMut::new();
        use bytes::BufMut;
        if let Some(u) = uuid {
            payload.put_u8(1);
            payload.put_slice(u.as_bytes());
        } else {
            payload.put_u8(0);
        }
        let packet = Packet {
            id: 0x08,
            data: payload.freeze(),
        };
        self.send_packet(client_id, packet).await
    }

    #[allow(dead_code)]
    pub async fn send_status_response(
        &self,
        client_id: u64,
        version_name: &str,
        protocol: i32,
        max_players: i32,
        online_players: i32,
        motd: &str,
        favicon: Option<&str>,
    ) -> Result<(), mpsc::error::SendError<ServerCommand>> {
        let mut favicon_str = String::new();
        if let Some(f) = favicon {
            favicon_str = format!(",\"favicon\":\"{}\"", f);
        }
        let response_json = format!(
            r#"{{"version":{{"name":"{}","protocol":{}}},"players":{{"max":{},"online":{}}},"description":{{"text":"{}"}}{}}}"#,
            version_name, protocol, max_players, online_players, motd, favicon_str
        );

        let mut payload = BytesMut::new();
        write_string(&response_json, &mut payload);

        let packet = Packet {
            id: 0x00,
            data: payload.freeze(),
        };

        self.send_packet(client_id, packet).await
    }

    #[allow(dead_code)]
    pub async fn finish_configuration(
        &self,
        client_id: u64,
    ) -> Result<(), mpsc::error::SendError<ServerCommand>> {
        let packet = Packet {
            id: 0x03,
            data: bytes::Bytes::new(),
        };
        self.send_packet(client_id, packet).await
    }

    #[allow(dead_code)]
    pub async fn server_select_known_packs(
        &self,
        client_id: u64,
        packs: &[(&str, &str, &str)],
    ) -> Result<(), mpsc::error::SendError<ServerCommand>> {
        let mut payload = BytesMut::new();
        write_varint(packs.len() as i32, &mut payload);
        for &(namespace, id, version) in packs {
            write_string(namespace, &mut payload);
            write_string(id, &mut payload);
            write_string(version, &mut payload);
        }

        let packet = Packet {
            id: 0x0E,
            data: payload.freeze(),
        };

        self.send_packet(client_id, packet).await
    }
}

pub type BoxedFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
pub type BoxedResultFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

type SetupFn =
    Box<dyn Fn(&AppHandle) -> Result<(), Box<dyn std::error::Error + Send + Sync>> + Send + Sync>;
type OnConsoleCommandFn =
    Box<dyn for<'a> Fn(&'a AppHandle, &'a str, &'a [String]) -> BoxedFuture<'a> + Send + Sync>;
type OnPacketFn = Box<
    dyn for<'a> Fn(&'a AppHandle, u64, ConnectionState, &'a Packet) -> BoxedFuture<'a>
        + Send
        + Sync,
>;
type OnConnectFn =
    Box<dyn for<'a> Fn(&'a AppHandle, u64, &'a str) -> BoxedFuture<'a> + Send + Sync>;
type OnCookieResponseFn = Box<
    dyn for<'a> Fn(&'a AppHandle, u64, &'a str, Option<&'a [u8]>) -> BoxedFuture<'a> + Send + Sync,
>;
type OnStatusRequestFn =
    Box<dyn for<'a> Fn(&'a AppHandle, u64, &'a str, i32, &'a str) -> BoxedFuture<'a> + Send + Sync>;
type OnHandshakeFn = Box<
    dyn for<'a> Fn(&'a AppHandle, u64, &'a str, i32, &'a str, u16, i32) -> BoxedFuture<'a>
        + Send
        + Sync,
>;
type OnPreLoginFn = Box<
    dyn for<'a> Fn(&'a AppHandle, u64, &'a str, &'a str, bool) -> BoxedResultFuture<'a, bool>
        + Send
        + Sync,
>;
type OnReadyFn = Box<
    dyn for<'a> Fn(&'a AppHandle, u64, &'a str, &'a str, i8, i32, bool, i32) -> BoxedFuture<'a>
        + Send
        + Sync,
>;
type OnPluginMessageFn =
    Box<dyn for<'a> Fn(&'a AppHandle, u64, &'a str, &'a [u8]) -> BoxedFuture<'a> + Send + Sync>;
type OnResourcePackResponseFn =
    Box<dyn for<'a> Fn(&'a AppHandle, u64, uuid::Uuid, i32) -> BoxedFuture<'a> + Send + Sync>;

#[async_trait]
pub trait Plugin: Send + Sync {
    fn name(&self) -> &'static str;

    /// Called when the plugin is initializing
    async fn initialize(
        &self,
        app: &AppHandle,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let _ = app;
        Ok(())
    }

    /// Called when a client connects
    async fn on_connect(&self, app: &AppHandle, client_id: u64, ip: &str) {
        let _ = (app, client_id, ip);
    }

    /// Called when a packet is received
    async fn on_packet(
        &self,
        app: &AppHandle,
        client_id: u64,
        state: ConnectionState,
        packet: &Packet,
    ) {
        let _ = (app, client_id, state, packet);
    }

    /// Called when a cookie response is received
    async fn on_cookie_response(
        &self,
        app: &AppHandle,
        client_id: u64,
        key: &str,
        payload: Option<&[u8]>,
    ) {
        let _ = (app, client_id, key, payload);
    }

    /// Called when a status request is received
    async fn on_status_request(
        &self,
        app: &AppHandle,
        client_id: u64,
        ip: &str,
        protocol: i32,
        hostname: &str,
    ) {
        let _ = (app, client_id, ip, protocol, hostname);
    }

    /// Called when a handshake is received
    async fn on_handshake(
        &self,
        app: &AppHandle,
        client_id: u64,
        ip: &str,
        protocol: i32,
        hostname: &str,
        port: u16,
        next_state: i32,
    ) {
        let _ = (app, client_id, ip, protocol, hostname, port, next_state);
    }

    /// Called when a login start is received (PreLoginEvent)
    async fn on_pre_login(
        &self,
        app: &AppHandle,
        client_id: u64,
        ip: &str,
        name: &str,
        is_from_transfer: bool,
    ) -> bool {
        let _ = (app, client_id, ip, name, is_from_transfer);
        true
    }

    /// Called when the connection is ready (after configuration)
    async fn on_ready(
        &self,
        app: &AppHandle,
        client_id: u64,
        ip: &str,
        locale: &str,
        view_distance: i8,
        chat_visibility: i32,
        chat_colors: bool,
        main_hand: i32,
    ) {
        let _ = (
            app,
            client_id,
            ip,
            locale,
            view_distance,
            chat_visibility,
            chat_colors,
            main_hand,
        );
    }

    /// Called when a plugin message is received
    async fn on_plugin_message(&self, app: &AppHandle, client_id: u64, channel: &str, data: &[u8]) {
        let _ = (app, client_id, channel, data);
    }

    /// Called when a resource pack response is received
    async fn on_console_command(&self, app: &AppHandle, command: &str, args: &[String]) {
        let _ = (app, command, args);
    }

    async fn on_resource_pack_response(
        &self,
        app: &AppHandle,
        client_id: u64,
        uuid: uuid::Uuid,
        result: i32,
    ) {
        let _ = (app, client_id, uuid, result);
    }
}

pub struct Builder<C, P>
where
    C: Send + Sync + 'static,
    P: Send + Sync + 'static,
{
    name: &'static str,
    setup: Option<SetupFn>,
    on_console_command: Option<OnConsoleCommandFn>,
    on_packet: Option<OnPacketFn>,
    on_connect: Option<OnConnectFn>,
    on_cookie_response: Option<OnCookieResponseFn>,
    on_status_request: Option<OnStatusRequestFn>,
    on_handshake: Option<OnHandshakeFn>,
    on_pre_login: Option<OnPreLoginFn>,
    on_ready: Option<OnReadyFn>,
    on_plugin_message: Option<OnPluginMessageFn>,
    on_resource_pack_response: Option<OnResourcePackResponseFn>,
    _marker: std::marker::PhantomData<(C, P)>,
}

impl<C, P> Builder<C, P>
where
    C: Send + Sync + 'static,
    P: Send + Sync + 'static,
{
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            setup: None,
            on_console_command: None,
            on_packet: None,
            on_connect: None,
            on_cookie_response: None,
            on_status_request: None,
            on_handshake: None,
            on_pre_login: None,
            on_ready: None,
            on_plugin_message: None,
            on_resource_pack_response: None,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn setup<F>(mut self, setup: F) -> Self
    where
        F: Fn(&AppHandle) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
            + Send
            + Sync
            + 'static,
    {
        self.setup = Some(Box::new(setup));
        self
    }

    #[allow(dead_code)]
    pub fn on_packet<F>(mut self, on_packet: F) -> Self
    where
        F: for<'a> Fn(&'a AppHandle, u64, ConnectionState, &'a Packet) -> BoxedFuture<'a>
            + Send
            + Sync
            + 'static,
    {
        self.on_packet = Some(Box::new(on_packet));
        self
    }

    #[allow(dead_code)]
    pub fn on_connect<F>(mut self, on_connect: F) -> Self
    where
        F: for<'a> Fn(&'a AppHandle, u64, &'a str) -> BoxedFuture<'a> + Send + Sync + 'static,
    {
        self.on_connect = Some(Box::new(on_connect));
        self
    }

    #[allow(dead_code)]
    pub fn on_cookie_response<F>(mut self, on_cookie_response: F) -> Self
    where
        F: for<'a> Fn(&'a AppHandle, u64, &'a str, Option<&'a [u8]>) -> BoxedFuture<'a>
            + Send
            + Sync
            + 'static,
    {
        self.on_cookie_response = Some(Box::new(on_cookie_response));
        self
    }

    #[allow(dead_code)]
    pub fn on_status_request<F>(mut self, on_status_request: F) -> Self
    where
        F: for<'a> Fn(&'a AppHandle, u64, &'a str, i32, &'a str) -> BoxedFuture<'a>
            + Send
            + Sync
            + 'static,
    {
        self.on_status_request = Some(Box::new(on_status_request));
        self
    }

    #[allow(dead_code)]
    pub fn on_handshake<F>(mut self, on_handshake: F) -> Self
    where
        F: for<'a> Fn(&'a AppHandle, u64, &'a str, i32, &'a str, u16, i32) -> BoxedFuture<'a>
            + Send
            + Sync
            + 'static,
    {
        self.on_handshake = Some(Box::new(on_handshake));
        self
    }

    #[allow(dead_code)]
    pub fn on_pre_login<F>(mut self, on_pre_login: F) -> Self
    where
        F: for<'a> Fn(&'a AppHandle, u64, &'a str, &'a str, bool) -> BoxedResultFuture<'a, bool>
            + Send
            + Sync
            + 'static,
    {
        self.on_pre_login = Some(Box::new(on_pre_login));
        self
    }

    #[allow(dead_code)]
    #[allow(dead_code)]
    pub fn on_ready<F>(mut self, on_ready: F) -> Self
    where
        F: for<'a> Fn(&'a AppHandle, u64, &'a str, &'a str, i8, i32, bool, i32) -> BoxedFuture<'a>
            + Send
            + Sync
            + 'static,
    {
        self.on_ready = Some(Box::new(on_ready));
        self
    }

    #[allow(dead_code)]
    pub fn on_plugin_message<F>(mut self, on_plugin_message: F) -> Self
    where
        F: for<'a> Fn(&'a AppHandle, u64, &'a str, &'a [u8]) -> BoxedFuture<'a>
            + Send
            + Sync
            + 'static,
    {
        self.on_plugin_message = Some(Box::new(on_plugin_message));
        self
    }

    #[allow(dead_code)]
    pub fn on_console_command<F>(mut self, on_console_command: F) -> Self
    where
        F: for<'a> Fn(&'a AppHandle, &'a str, &'a [String]) -> BoxedFuture<'a>
            + Send
            + Sync
            + 'static,
    {
        self.on_console_command = Some(Box::new(on_console_command));
        self
    }

    #[allow(dead_code)]
    pub fn on_resource_pack_response<F>(mut self, on_resource_pack_response: F) -> Self
    where
        F: for<'a> Fn(&'a AppHandle, u64, uuid::Uuid, i32) -> BoxedFuture<'a>
            + Send
            + Sync
            + 'static,
    {
        self.on_resource_pack_response = Some(Box::new(on_resource_pack_response));
        self
    }

    pub fn build(self) -> BuiltPlugin {
        BuiltPlugin {
            name: self.name,
            setup: self.setup,
            on_console_command: self.on_console_command,
            on_packet: self.on_packet,
            on_connect: self.on_connect,
            on_cookie_response: self.on_cookie_response,
            on_status_request: self.on_status_request,
            on_handshake: self.on_handshake,
            on_pre_login: self.on_pre_login,
            on_ready: self.on_ready,
            on_plugin_message: self.on_plugin_message,
            on_resource_pack_response: self.on_resource_pack_response,
        }
    }
}

pub struct BuiltPlugin {
    name: &'static str,
    setup: Option<SetupFn>,
    on_console_command: Option<OnConsoleCommandFn>,
    on_packet: Option<OnPacketFn>,
    on_connect: Option<OnConnectFn>,
    on_cookie_response: Option<OnCookieResponseFn>,
    on_status_request: Option<OnStatusRequestFn>,
    on_handshake: Option<OnHandshakeFn>,
    on_pre_login: Option<OnPreLoginFn>,
    on_ready: Option<OnReadyFn>,
    on_plugin_message: Option<OnPluginMessageFn>,
    on_resource_pack_response: Option<OnResourcePackResponseFn>,
}

#[async_trait]
impl Plugin for BuiltPlugin {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn initialize(
        &self,
        app: &AppHandle,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(setup) = &self.setup {
            setup(app)?;
        }
        Ok(())
    }

    async fn on_connect(&self, app: &AppHandle, client_id: u64, ip: &str) {
        if let Some(on_connect) = &self.on_connect {
            on_connect(app, client_id, ip).await;
        }
    }

    async fn on_packet(
        &self,
        app: &AppHandle,
        client_id: u64,
        state: ConnectionState,
        packet: &Packet,
    ) {
        if let Some(on_packet) = &self.on_packet {
            on_packet(app, client_id, state, packet).await;
        }
    }

    async fn on_cookie_response(
        &self,
        app: &AppHandle,
        client_id: u64,
        key: &str,
        payload: Option<&[u8]>,
    ) {
        if let Some(on_cookie_response) = &self.on_cookie_response {
            on_cookie_response(app, client_id, key, payload).await;
        }
    }

    async fn on_status_request(
        &self,
        app: &AppHandle,
        client_id: u64,
        ip: &str,
        protocol: i32,
        hostname: &str,
    ) {
        if let Some(on_status_request) = &self.on_status_request {
            on_status_request(app, client_id, ip, protocol, hostname).await;
        }
    }

    async fn on_handshake(
        &self,
        app: &AppHandle,
        client_id: u64,
        ip: &str,
        protocol: i32,
        hostname: &str,
        port: u16,
        next_state: i32,
    ) {
        if let Some(on_handshake) = &self.on_handshake {
            on_handshake(app, client_id, ip, protocol, hostname, port, next_state).await;
        }
    }

    async fn on_pre_login(
        &self,
        app: &AppHandle,
        client_id: u64,
        ip: &str,
        name: &str,
        is_from_transfer: bool,
    ) -> bool {
        if let Some(on_pre_login) = &self.on_pre_login {
            on_pre_login(app, client_id, ip, name, is_from_transfer).await
        } else {
            true
        }
    }

    async fn on_ready(
        &self,
        app: &AppHandle,
        client_id: u64,
        ip: &str,
        locale: &str,
        view_distance: i8,
        chat_visibility: i32,
        chat_colors: bool,
        main_hand: i32,
    ) {
        if let Some(on_ready) = &self.on_ready {
            on_ready(
                app,
                client_id,
                ip,
                locale,
                view_distance,
                chat_visibility,
                chat_colors,
                main_hand,
            )
            .await;
        }
    }

    async fn on_plugin_message(&self, app: &AppHandle, client_id: u64, channel: &str, data: &[u8]) {
        if let Some(on_plugin_message) = &self.on_plugin_message {
            on_plugin_message(app, client_id, channel, data).await;
        }
    }

    async fn on_console_command(&self, app: &AppHandle, command: &str, args: &[String]) {
        if let Some(on_console_command) = &self.on_console_command {
            on_console_command(app, command, args).await;
        }
    }

    async fn on_resource_pack_response(
        &self,
        app: &AppHandle,
        client_id: u64,
        uuid: uuid::Uuid,
        result: i32,
    ) {
        if let Some(on_resource_pack_response) = &self.on_resource_pack_response {
            on_resource_pack_response(app, client_id, uuid, result).await;
        }
    }
}

pub struct PluginManager {
    plugins: HashMap<&'static str, Arc<dyn Plugin>>,
    app_handle: AppHandle,
}

impl PluginManager {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            plugins: HashMap::new(),
            app_handle,
        }
    }

    pub async fn register(
        &mut self,
        plugin: impl Plugin + 'static,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        plugin.initialize(&self.app_handle).await?;
        self.plugins.insert(plugin.name(), Arc::new(plugin));
        Ok(())
    }

    pub async fn dispatch_connect(&self, client_id: u64, ip: &str) {
        for plugin in self.plugins.values() {
            plugin.on_connect(&self.app_handle, client_id, ip).await;
        }
    }

    pub async fn dispatch_packet(&self, client_id: u64, state: ConnectionState, packet: &Packet) {
        for plugin in self.plugins.values() {
            plugin
                .on_packet(&self.app_handle, client_id, state, packet)
                .await;
        }
    }

    pub async fn dispatch_cookie_response(
        &self,
        client_id: u64,
        key: &str,
        payload: Option<&[u8]>,
    ) {
        for plugin in self.plugins.values() {
            plugin
                .on_cookie_response(&self.app_handle, client_id, key, payload)
                .await;
        }
    }

    pub async fn dispatch_status_request(
        &self,
        client_id: u64,
        ip: &str,
        protocol: i32,
        hostname: &str,
    ) {
        for plugin in self.plugins.values() {
            plugin
                .on_status_request(&self.app_handle, client_id, ip, protocol, hostname)
                .await;
        }
    }

    pub async fn dispatch_handshake(
        &self,
        client_id: u64,
        ip: &str,
        protocol: i32,
        hostname: &str,
        port: u16,
        next_state: i32,
    ) {
        for plugin in self.plugins.values() {
            plugin
                .on_handshake(
                    &self.app_handle,
                    client_id,
                    ip,
                    protocol,
                    hostname,
                    port,
                    next_state,
                )
                .await;
        }
    }

    pub async fn dispatch_pre_login(
        &self,
        client_id: u64,
        ip: &str,
        name: &str,
        is_from_transfer: bool,
    ) -> bool {
        let mut allow = true;
        for plugin in self.plugins.values() {
            if !plugin
                .on_pre_login(&self.app_handle, client_id, ip, name, is_from_transfer)
                .await
            {
                allow = false;
            }
        }
        allow
    }

    pub async fn dispatch_ready(
        &self,
        client_id: u64,
        ip: &str,
        locale: &str,
        view_distance: i8,
        chat_visibility: i32,
        chat_colors: bool,
        main_hand: i32,
    ) {
        for plugin in self.plugins.values() {
            plugin
                .on_ready(
                    &self.app_handle,
                    client_id,
                    ip,
                    locale,
                    view_distance,
                    chat_visibility,
                    chat_colors,
                    main_hand,
                )
                .await;
        }
    }

    pub async fn dispatch_plugin_message(&self, client_id: u64, channel: &str, data: &[u8]) {
        for plugin in self.plugins.values() {
            plugin
                .on_plugin_message(&self.app_handle, client_id, channel, data)
                .await;
        }
    }

    pub async fn dispatch_console_command(&self, command: &str, args: &[String]) {
        for plugin in self.plugins.values() {
            plugin
                .on_console_command(&self.app_handle, command, args)
                .await;
        }
    }

    pub async fn dispatch_resource_pack_response(
        &self,
        client_id: u64,
        uuid: uuid::Uuid,
        result: i32,
    ) {
        for plugin in self.plugins.values() {
            plugin
                .on_resource_pack_response(&self.app_handle, client_id, uuid, result)
                .await;
        }
    }
}
