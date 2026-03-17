mod config;
mod ip_info;
mod network;
mod plugin;
pub mod plugins;

use crate::network::Server;
use crate::plugin::{AppHandle, PluginManager};
use log::info;
use std::sync::Arc;
use tokio::io::{self, AsyncBufReadExt, BufReader};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    info!("Starting TransferProxyRS...");

    ip_info::init_dbs();

    let (tx, rx) = tokio::sync::mpsc::channel(1024);

    let config = config::load_config();
    info!("Bind address: {}", config.bind_address);

    let app_handle = AppHandle { tx: tx.clone() };
    let mut plugin_manager = PluginManager::new(app_handle);

    plugin_manager
        .register(plugins::motd::create_plugin())
        .await?;
    plugin_manager
        .register(plugins::transfor::create_plugin())
        .await?;
    plugin_manager
        .register(plugins::bbob::create_plugin())
        .await?;

    let plugin_manager = Arc::new(plugin_manager);

    let pm_clone = Arc::clone(&plugin_manager);
    tokio::spawn(async move {
        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin);
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => break,
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    let command = parts[0];
                    let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

                    pm_clone.dispatch_console_command(command, &args).await;
                }
                Err(e) => {
                    log::error!("Error reading from stdin: {}", e);
                    break;
                }
            }
        }
    });

    let server = Server::bind(&config.bind_address, plugin_manager).await?;

    tokio::spawn(async move {
        server.run(rx).await;
    });

    tokio::signal::ctrl_c().await?;
    info!("Shutting down TransferProxyRS...");

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::network::{read_varint, write_varint};
    use bytes::BytesMut;

    #[test]
    fn test_varint_encode_decode() {
        let mut buf = BytesMut::new();
        let value = 12345;
        write_varint(value, &mut buf);

        let decode_buf = buf.clone();
        let (decoded, bytes_read) = read_varint(&decode_buf).unwrap();

        assert_eq!(decoded, value);
        assert_eq!(bytes_read, buf.len());
    }
}
