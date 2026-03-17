use crate::plugin::{Builder, BuiltPlugin};
use ipnetwork::IpNetwork;
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::net::IpAddr;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct BbobConfig {
    pub banned_ips: HashSet<String>,
    #[serde(skip)]
    pub parsed_banned_ips: Vec<IpNetwork>,
    pub banned_names: HashSet<String>,
    pub banned_uuids: HashSet<String>,
    pub banned_asns: HashSet<String>,
}

impl BbobConfig {
    pub fn save(&self, path: &Path) {
        if let Ok(content) = serde_saphyr::to_string(self) {
            let _ = fs::write(path, content);
        }
    }

    pub fn update_parsed_ips(&mut self) {
        self.parsed_banned_ips.clear();
        for ip_str in &self.banned_ips {
            if let Ok(net) = IpNetwork::from_str(ip_str) {
                self.parsed_banned_ips.push(net);
            }
        }
    }
}

pub fn create_plugin() -> BuiltPlugin {
    let config_path = Path::new("plugins/bbob");
    fs::create_dir_all(config_path).expect("Failed to create plugins/bbob directory");
    let file_path = config_path.join("config.yml");

    let mut config = if file_path.exists() {
        let content = fs::read_to_string(&file_path).unwrap_or_default();
        serde_saphyr::from_str(&content).unwrap_or_else(|_| BbobConfig::default())
    } else {
        let conf = BbobConfig::default();
        conf.save(&file_path);
        conf
    };

    config.update_parsed_ips();

    let plugin_config = Arc::new(RwLock::new(config));
    let cfg_clone1 = Arc::clone(&plugin_config);
    let cfg_clone2 = Arc::clone(&plugin_config);

    Builder::<(), ()>::new("bbob")
        .setup(move |_app| {
            info!("Bbob plugin initialized!");
            Ok(())
        })
        .on_console_command(move |_app, command, args| {
            let config = Arc::clone(&cfg_clone1);
            let command = command.to_lowercase();
            let args: Vec<String> = args.to_vec();

            Box::pin(async move {
                let mut cfg = config.write().await;
                let mut changed = false;
                let arg_val = args.first().cloned();

                match command.as_str() {
                    "banip" => {
                        if let Some(ip) = arg_val {
                            cfg.banned_ips.insert(ip.clone());
                            info!("Banned IP: {}", ip);
                            changed = true;
                        } else {
                            error!("Usage: banip <ip>");
                        }
                    }
                    "unbanip" => {
                        if let Some(ip) = arg_val {
                            if cfg.banned_ips.remove(&ip) {
                                info!("Unbanned IP: {}", ip);
                                changed = true;
                            }
                        }
                    }
                    "banuser" => {
                        if let Some(name) = arg_val {
                            cfg.banned_names.insert(name.clone().to_lowercase());
                            info!("Banned user: {}", name);
                            changed = true;
                        }
                    }
                    "unbanuser" => {
                        if let Some(name) = arg_val {
                            if cfg.banned_names.remove(&name.to_lowercase()) {
                                info!("Unbanned user: {}", name);
                                changed = true;
                            }
                        }
                    }
                    "banuuid" => {
                        if let Some(uuid) = arg_val {
                            cfg.banned_uuids.insert(uuid.clone().to_lowercase());
                            info!("Banned UUID: {}", uuid);
                            changed = true;
                        }
                    }
                    "unbanuuid" => {
                        if let Some(uuid) = arg_val {
                            if cfg.banned_uuids.remove(&uuid.to_lowercase()) {
                                info!("Unbanned UUID: {}", uuid);
                                changed = true;
                            }
                        }
                    }
                    "banasn" => {
                        if let Some(asn) = arg_val {
                            cfg.banned_asns.insert(asn.clone());
                            info!("Banned ASN: {}", asn);
                            changed = true;
                        }
                    }
                    "unbanasn" => {
                        if let Some(asn) = arg_val {
                            if cfg.banned_asns.remove(&asn) {
                                info!("Unbanned ASN: {}", asn);
                                changed = true;
                            }
                        }
                    }
                    _ => {}
                }

                if changed {
                    cfg.update_parsed_ips();
                    let cp = Path::new("plugins/bbob/config.yml");
                    cfg.save(cp);
                }
            })
        })
        .on_pre_login(move |app, client_id, ip, name, _is_from_transfer| {
            let config = Arc::clone(&cfg_clone2);
            let app = app.clone();
            let ip_str = ip.to_string();
            let name_str = name.to_lowercase();

            Box::pin(async move {
                let cfg = config.read().await;

                if cfg.banned_ips.contains(&ip_str) {
                    let _ = app.kick(client_id, "Your IP is banned.").await;
                    return false;
                }

                if let Ok(ip_addr) = IpAddr::from_str(&ip_str) {
                    for net in &cfg.parsed_banned_ips {
                        if net.contains(ip_addr) {
                            let _ = app.kick(client_id, "Your IP range is banned.").await;
                            return false;
                        }
                    }
                }

                if cfg.banned_names.contains(&name_str) {
                    let _ = app.kick(client_id, "Your account is banned.").await;
                    return false;
                }

                if let Some(asn) = crate::ip_info::get_asn(&ip_str) {
                    if cfg.banned_asns.contains(&asn) {
                        let _ = app.kick(client_id, "Your network (ASN) is banned.").await;
                        return false;
                    }
                }

                true
            })
        })
        .build()
}
