# Leni-Deni: P2P File Sharing Application

A peer-to-peer file sharing application written in Rust that allows users to share files directly without a central server.

## Features

- Peer discovery on local networks
- Direct file transfers between peers
- Secure and encrypted file transfers
- Simple CLI interface

## Building

```bash
cargo build --release
```

## Running

```bash
cargo run -- --help
```

### Commands

- `list-peers` - List discovered peers
- `share <file_path> <peer_id>` - Share a file with a peer
- `receive <save_path> <peer_id>` - Receive a file from a peer
- `progress <transfer_id>` - Show transfer progress

## Networking Permissions

This application uses UDP broadcasts for peer discovery, which may require appropriate permissions:

### Linux

If you encounter "Permission denied" errors when running the application, you might need to grant the necessary network permissions:

```bash
sudo setcap 'cap_net_raw,cap_net_admin=eip' target/debug/leni-deni
```

Or run with sudo (not recommended for production):

```bash
sudo cargo run -- list-peers
```

### Windows

Run as administrator or ensure your firewall allows the application to send UDP broadcasts.

### macOS

You may need to allow the application in System Preferences > Security & Privacy > Firewall.

## Troubleshooting

If peer discovery is not working:

1. Ensure your firewall allows UDP traffic on ports 45678 and 45679
2. Make sure you are on the same network as your peers
3. Check if your network allows broadcast traffic 
