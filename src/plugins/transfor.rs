use crate::plugin::{Builder, BuiltPlugin};
use log::{error, info};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TransferConfig {
    pub default_server: String,
    pub servers: HashMap<String, String>,
    pub weights: HashMap<String, u32>,
    pub rules: Vec<TransferRule>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TransferRule {
    pub target: String,
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub asn: Option<String>,
}

impl Default for TransferConfig {
    fn default() -> Self {
        let mut servers = HashMap::new();
        servers.insert("hub".to_string(), "127.0.0.1:35565".to_string());
        servers.insert("survival".to_string(), "127.0.0.1:45565".to_string());

        let mut weights = HashMap::new();
        weights.insert("hub".to_string(), 100);

        let rules = Vec::new();

        Self {
            default_server: "hub".to_string(),
            servers,
            weights,
            rules,
        }
    }
}

pub fn create_plugin() -> BuiltPlugin {
    let config_path = Path::new("plugins/transfor");
    fs::create_dir_all(config_path).expect("Failed to create plugins/transfor directory");
    let file_path = config_path.join("config.yml");

    let config = if file_path.exists() {
        let content = fs::read_to_string(&file_path).expect("Failed to read transfor config");
        serde_saphyr::from_str(&content).expect("Failed to parse transfor config")
    } else {
        let conf = TransferConfig::default();
        let content = serde_saphyr::to_string(&conf).expect("Failed to serialize transfor config");
        fs::write(&file_path, content).expect("Failed to write transfor config");
        conf
    };

    let plugin_config = config.clone();

    Builder::<(), ()>::new("transfor")
        .setup(move |_app| {
            info!("Transfor plugin initialized!");
            Ok(())
        })
        .on_ready(move |app, client_id, ip, _locale, _view_distance, _chat_visibility, _chat_colors, _main_hand| {
            let app = app.clone();
            let config = plugin_config.clone();
            let ip_str = ip.to_string();

            Box::pin(async move {
                info!("Transfor plugin: Client {} is ready. Preparing to transfer...", client_id);

                let mut target_server = config.default_server.clone();

                let geo_info = crate::ip_info::get_geo(&ip_str);
                let asn_info = crate::ip_info::get_asn(&ip_str);

                for rule in &config.rules {
                    let mut is_match = true;

                    if let Some(ref c) = rule.country {
                        if Some(c) != geo_info.country.as_ref() { is_match = false; }
                    }
                    if let Some(ref r) = rule.region {
                        if Some(r) != geo_info.region.as_ref() { is_match = false; }
                    }
                    if let Some(ref c) = rule.city {
                        if Some(c) != geo_info.city.as_ref() { is_match = false; }
                    }
                    if let Some(ref a) = rule.asn {
                        if Some(a) != asn_info.as_ref() { is_match = false; }
                    }

                    if is_match {
                        target_server = rule.target.clone();
                        break;
                    }
                }

                if target_server == config.default_server && !config.weights.is_empty() {
                    let total_weight: u32 = config.weights.values().sum();
                    if total_weight > 0 {
                        let mut rng = rand::rng();
                        let mut r = rng.random_range(0..total_weight);

                        for (server, weight) in &config.weights {
                            if r < *weight {
                                target_server = server.clone();
                                break;
                            }
                            r -= weight;
                        }
                    }
                }

                if let Some(address) = config.servers.get(&target_server) {
                    let parts: Vec<&str> = address.split(':').collect();
                    let host = parts[0];
                    let port = if parts.len() > 1 { parts[1].parse::<i32>().unwrap_or(25565) } else { 25565 };

                    if let Err(e) = app.transfer(client_id, host, port).await {
                        error!("Failed to send Transfer packet to target server {}: {}", target_server, e);
                    } else {
                        info!("Transfor plugin: Successfully sent transfer packet to {} for client {}!", target_server, client_id);
                        let _ = app.disconnect(client_id).await;
                    }
                } else {
                    error!("Transfor plugin: Target server '{}' not found in config!", target_server);
                    let _ = app.disconnect(client_id).await;
                }
            })
        })
        .build()
}
