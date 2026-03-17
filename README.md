# TransferProxyRS

A Minecraft proxy server built in Rust, focusing exclusively on the 1.20.5+ Transfer Packet. It can be used to easily balance load or route players to different backends immediately upon connection.

## Features

- **Blazing Fast**: Asynchronous I/O via `tokio`.
- **Lightweight**: Uses low memory footprint. (1.2M)
- **Protocol 1.20.5+ (766+) Support**: Fully supports 1.20.5 `Configuration` state, keeps connections alive correctly while configuring, and redirects clients to the desired endpoint.
- **Offline IP Intelligence**: Supports reading `maxminddb` databases (`GeoLite2-ASN.mmdb` and `GeoLite2-City.mmdb`) placed in the `plugins/` directory to power routing and banning logic locally.
- **Built-in Plugins**:
  - `motd`: Display dynamic MOTDs and server icons based on the client's protocol version and geographic location (Country, Region, City). Includes support for fake online players and strict allowed/denied protocol version restrictions.
  - `transfor`: Manage virtual hosts, weighted random fallback routing, and mixable geo-routing rules (based on Country, Region, City, and ASN) to seamlessly transfer players.
  - `bbob`: A console-managed moderation plugin that blocks abusive connections by banning IPs, IP subnets (CIDR), Usernames, UUIDs, and ASNs directly via standard input.

## Installation

```bash
cargo build --release
```

Run the compiled executable. A `config.yml` and the `plugins/` configuration directories will be automatically generated upon the first start.

**Note:** For Geo-routing and ASN features to work, you must download `GeoLite2-ASN.mmdb` and `GeoLite2-City.mmdb` and place them in the `plugins/` directory.

## Core Commands

You can interact with the running proxy via the standard console input. The `bbob` plugin provides the following commands:
- `banip <ip>` / `unbanip <ip>` (also supports CIDR notation like `192.168.1.0/24`)
- `banuser <username>` / `unbanuser <username>`
- `banuuid <uuid>` / `unbanuuid <uuid>`
- `banasn <asn>` / `unbanasn <asn>` (e.g., `banasn AS15169`)

## Plugin Configuration Examples

**plugins/motd/config.yml**
```yaml
motds:
  - protocol: 766
    texts: ["Welcome to 1.20.5 Transfer Proxy"]
  - protocol: 767
    texts: ["Welcome to 1.21 Transfer Proxy"]
    country: "US"
default_motds: ["Welcome to the Server", "Random MOTD String"]
max_players: 1000
fake_online: true
fake_online_count: 50
per_server_limits:
  "hub.example.com": 100
icons:
  - "server-icon.png"
  - "iVBORw0KGgoAAAANSUhEUgAA..." # Raw Base64 is also supported
allowed_protocols: []
denied_protocols: [765]
```

**plugins/transfor/config.yml**
```yaml
default_server: "hub"
servers:
  hub: "127.0.0.1:35565"
  survival: "127.0.0.1:45565"
  us_hub: "127.0.0.1:55565"
weights:
  hub: 80
  survival: 20
rules:
  - target: "us_hub"
    country: "US"
    asn: "AS15169"
```

## API Usage

TransferProxyRS has a Rust plugin system akin to the Tauri Builder pattern. You can register your own custom logic by instantiating the builder and implementing the relevant lifecycle hooks.

Example:

```rust
use transfer_proxy_rs::plugin::{AppHandle, Builder, BuiltPlugin};
use log::info;

pub fn my_plugin() -> BuiltPlugin {
    Builder::<(), ()>::new("my_plugin")
        .setup(|app| {
            info!("Plugin initialized!");
            Ok(())
        })
        .on_status_request(|app, client_id, ip, protocol, hostname| {
            let app = app.clone();
            let hostname = hostname.to_string();
            Box::pin(async move {
                info!("Status requested for {} from {}", hostname, ip);
                app.send_status_response(client_id, "Proxy", protocol, 100, 10, "Custom MOTD").await.unwrap();
            })
        })
        .on_ready(|app, client_id, ip, locale, view_distance, chat_visibility, chat_colors, main_hand| {
            let app = app.clone();
            Box::pin(async move {
                info!("Client ready to transfer!");
                app.transfer(client_id, "127.0.0.1", 35565).await.unwrap();
                app.disconnect(client_id).await.unwrap();
            })
        })
        .build()
}
```

Then register it in `src/main.rs`:

```rust
plugin_manager.register(my_plugin()).await?;
```

## License
2026 404Setup. All rights reserved. Source code is licensed under a Apache-2.0 License.