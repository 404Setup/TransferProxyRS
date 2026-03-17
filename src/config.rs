use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProxyConfig {
    pub bind_address: String,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:25565".to_string(),
        }
    }
}

pub fn load_config() -> ProxyConfig {
    let path = Path::new("config.yml");
    if path.exists() {
        let content = fs::read_to_string(path).expect("Failed to read config.yml");
        serde_saphyr::from_str(&content).expect("Failed to parse config.yml")
    } else {
        let config = ProxyConfig::default();
        let content = serde_saphyr::to_string(&config).expect("Failed to serialize default config");
        fs::write(path, content).expect("Failed to write config.yml");
        config
    }
}
