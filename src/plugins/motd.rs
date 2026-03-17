use crate::plugin::{Builder, BuiltPlugin};
use log::{error, info};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MotdEntry {
    pub protocol: i32,
    pub texts: Vec<String>,
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MotdConfig {
    pub motds: Vec<MotdEntry>,
    pub default_motds: Vec<String>,
    pub max_players: i32,
    pub fake_online: bool,
    pub fake_online_count: i32,
    pub per_server_limits: HashMap<String, i32>,
    pub icons: Vec<String>,
    pub allowed_protocols: Vec<i32>,
    pub denied_protocols: Vec<i32>,
}

impl Default for MotdConfig {
    fn default() -> Self {
        Self {
            motds: vec![
                MotdEntry {
                    protocol: 766,
                    texts: vec![
                        "Welcome to TransferProxyRS (1.20.5)".to_string(),
                        "Random MOTD 1".to_string(),
                    ],
                    country: None,
                    region: None,
                    city: None,
                },
                MotdEntry {
                    protocol: 767,
                    texts: vec!["Welcome to TransferProxyRS (1.21)".to_string()],
                    country: None,
                    region: None,
                    city: None,
                },
            ],
            default_motds: vec![
                "A TransferProxyRS server".to_string(),
                "Another Default MOTD".to_string(),
            ],
            max_players: 1000,
            fake_online: true,
            fake_online_count: 50,
            per_server_limits: {
                let mut map = HashMap::new();
                map.insert("hub.example.com".to_string(), 100);
                map
            },
            icons: vec![],
            allowed_protocols: vec![],
            denied_protocols: vec![],
        }
    }
}

pub fn create_plugin() -> BuiltPlugin {
    let config_path = Path::new("plugins/motd");
    fs::create_dir_all(config_path).expect("Failed to create plugins/motd directory");
    let file_path = config_path.join("config.yml");

    let mut config: MotdConfig = if file_path.exists() {
        let content = fs::read_to_string(&file_path).expect("Failed to read motd config");
        serde_saphyr::from_str(&content).expect("Failed to parse motd config")
    } else {
        let conf = MotdConfig::default();
        let content = serde_saphyr::to_string(&conf).expect("Failed to serialize motd config");
        fs::write(&file_path, content).expect("Failed to write motd config");
        conf
    };

    for icon_path_or_base64 in &mut config.icons {
        if Path::new(icon_path_or_base64).exists() {
            if let Ok(bytes) = fs::read(&icon_path_or_base64) {
                use base64::{engine::general_purpose, Engine as _};
                *icon_path_or_base64 = format!(
                    "data:image/png;base64,{}",
                    general_purpose::STANDARD.encode(&bytes)
                );
            }
        }
    }

    let plugin_config1 = config.clone();
    let plugin_config2 = config.clone();

    Builder::<(), ()>::new("motd")
        .setup(move |_app| {
            info!("MOTD plugin initialized!");
            Ok(())
        })

        .on_handshake(move |app, client_id, _ip, protocol, _hostname, _port, next_state| {
            let app = app.clone();
            let config = plugin_config1.clone();

            Box::pin(async move {
                if next_state == 2 || next_state == 3 {
                    let mut denied = false;

                    if !config.denied_protocols.is_empty() && config.denied_protocols.contains(&protocol) {
                        denied = true;
                    }

                    if !config.allowed_protocols.is_empty() && !config.allowed_protocols.contains(&protocol) {
                        denied = true;
                    }

                    if denied {
                        info!("Motd plugin: Denying connection for client {} due to restricted protocol {}", client_id, protocol);

                        let _ = app.kick(client_id, "Your Minecraft version is not permitted on this server.").await;
                    }
                }
            })
        })
        .on_status_request(move |app, client_id, ip, protocol, hostname| {
            let app = app.clone();
            let config = plugin_config2.clone();
            let geo_info = crate::ip_info::get_geo(ip);

            Box::pin(async move {
                let clean_hostname = hostname.split(':').next().unwrap_or(hostname);

                let max_players = *config.per_server_limits.get(clean_hostname).unwrap_or(&config.max_players);

                let mut online_players = 0;
                if config.fake_online {
                    online_players = config.fake_online_count;
                }

                let (motd_text, icon_base64) = {
                    let mut rng = rand::rng();

                    if config.fake_online {
                        let jitter: i32 = rng.random_range(-5..=5);
                        online_players = (online_players + jitter).max(0);
                    }

                    let matching_motds: Vec<&MotdEntry> = config.motds.iter()
                        .filter(|m| m.protocol == protocol)
                        .filter(|m| {
                            if let Some(c) = &m.country {
                                if Some(c) != geo_info.country.as_ref() { return false; }
                            }
                            if let Some(r) = &m.region {
                                if Some(r) != geo_info.region.as_ref() { return false; }
                            }
                            if let Some(c) = &m.city {
                                if Some(c) != geo_info.city.as_ref() { return false; }
                            }
                            true
                        })
                        .collect();

                    let motd_text = if matching_motds.is_empty() {
                        if config.default_motds.is_empty() {
                            "TransferProxyRS".to_string()
                        } else {
                            let idx = rng.random_range(0..config.default_motds.len());
                            config.default_motds[idx].clone()
                        }
                    } else {
                        let entry_idx = rng.random_range(0..matching_motds.len());
                        let entry = matching_motds[entry_idx];
                        if entry.texts.is_empty() {
                            "".to_string()
                        } else {
                            let text_idx = rng.random_range(0..entry.texts.len());
                            entry.texts[text_idx].clone()
                        }
                    };

                    let mut icon_base64 = String::new();
                    if !config.icons.is_empty() {
                        let idx = rng.random_range(0..config.icons.len());
                        icon_base64 = config.icons[idx].clone();
                    }

                    (motd_text, icon_base64)
                };

                let version_name = "TransferProxyRS";

                let favicon_opt = if !icon_base64.is_empty() {
                    Some(icon_base64.as_str())
                } else {
                    None
                };

                if let Err(e) = app.send_status_response(client_id, version_name, protocol, max_players, online_players, &motd_text, favicon_opt).await {
                    error!("Failed to send status response: {}", e);
                }
            })
        })
        .build()
}
