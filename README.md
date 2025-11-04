# Decentralized VPN (DVPN)

A peer-to-peer VPN system built in Rust that allows users to discover peers and route their internet traffic through other peers in the network.

## Features

- **Peer Discovery**: Uses libp2p with mDNS and Kademlia DHT for automatic peer discovery
- **Request/Approval Flow**: Peers must approve routing requests before traffic is routed
- **SOCKS5 Proxy**: Standard SOCKS5 proxy interface for compatibility with most applications
- **Real-time Notifications**: Get notified when peers request to route through your network
- **Region-based Filtering**: Filter and select peers by geographical region
- **Decentralized Architecture**: No central server required

## Architecture

### Components

1. **P2P Network Layer** (`src/p2p/`)
   - `network.rs`: libp2p-based networking with mDNS, Kademlia DHT, GossipSub
   - `peer_manager.rs`: Tracks discovered peers and routing requests
   - `protocol.rs`: Message protocol for routing requests and approvals

2. **SOCKS5 Proxy** (`src/socks5.rs`)
   - Standard SOCKS5 proxy server
   - Routes traffic through approved peers
   - Supports IPv4, IPv6, and domain name resolution

3. **CLI Interface** (`src/main.rs`)
   - Interactive command-line interface
   - Commands for starting nodes, discovering peers, and managing requests

### How It Works

```
┌─────────────┐         P2P Network          ┌─────────────┐
│   Peer A    │◄──────(mDNS/Kademlia)───────►│   Peer B    │
│             │                               │             │
│ 1. Discover │         GossipSub            │ 2. Receive  │
│    Peers    │────Route Request Message────►│    Request  │
│             │                               │             │
│ 4. Route    │◄───Route Approval Message────│ 3. Approve  │
│   Traffic   │                               │             │
│             │                               │             │
│ SOCKS5 :1080│──────HTTP/HTTPS Traffic─────►│ Proxy :4202 │
└─────────────┘                               └─────────────┘
```

## Installation

```bash
# Clone the repository
git clone <repository-url>
cd dvpn

# Build the project
cargo build --release
```

## Usage

### 1. Start a VPN Node

Start your first node (this will be a peer in the network):

```bash
cargo run -- start -r "US-East" -p 4202 -s 1080
```

Options:
- `-r, --region`: Your region identifier (default: "unknown")
- `-p, --p2p-port`: Port for P2P networking (default: 4202)
- `-s, --socks-port`: Port for SOCKS5 proxy (default: 1080)

### 2. Start Another Node

On a different machine or terminal (in the same local network for mDNS):

```bash
cargo run -- start -r "Europe" -p 4203 -s 1081
```

The nodes will automatically discover each other via mDNS (on local network) or Kademlia DHT (for internet-wide discovery).

### 3. Connect to a Peer

To route your traffic through another peer:

```bash
# In a separate terminal, while the first node is running
cargo run -- connect

# Or filter by region
cargo run -- connect -r "Europe"
```

This will:
1. Show a list of discovered peers
2. Let you select a peer
3. Send a routing request to that peer
4. Wait for approval

### 4. Approve Routing Requests

When you receive a routing request, you'll see a notification. To approve or deny:

```bash
cargo run -- requests
```

This will show all pending requests and let you approve/deny each one.

### 5. List Available Peers

```bash
cargo run -- list
```

Shows all discovered peers with their regions and availability.

## Configuring Applications

Once you've connected to a peer and they've approved your request, configure your applications to use the SOCKS5 proxy:

- **Proxy Host**: `127.0.0.1`
- **Proxy Port**: `1080` (or your custom port)
- **Proxy Type**: SOCKS5

### Example: Configure Firefox

1. Open Firefox Settings
2. Search for "proxy"
3. Choose "Manual proxy configuration"
4. SOCKS Host: `127.0.0.1`, Port: `1080`
5. Select "SOCKS v5"

### Example: Configure curl

```bash
curl --socks5 127.0.0.1:1080 https://api.ipify.org
```

### Example: Configure Git

```bash
git config --global http.proxy socks5://127.0.0.1:1080
git config --global https.proxy socks5://127.0.0.1:1080
```

## Network Protocols

### Message Types

1. **RouteRequest**: Sent when a peer wants to route traffic
   - Contains: request_id, requester_peer_id, requester_name

2. **RouteResponse**: Response to a routing request
   - Contains: request_id, approved (bool)

3. **PeerInfo**: Broadcast peer information
   - Contains: peer_id, region, available_slots

4. **Heartbeat**: Keep-alive messages

### Discovery Mechanisms

1. **mDNS**: For local network discovery
2. **Kademlia DHT**: For internet-wide peer discovery
3. **GossipSub**: For message propagation across the network

## Security Considerations

**⚠️ Important Security Notes:**

This is a proof-of-concept implementation. For production use, consider:

1. **Encryption**: All traffic between peers should be encrypted (currently traffic is routed but not additionally encrypted)
2. **Authentication**: Implement peer authentication to prevent unauthorized access
3. **Rate Limiting**: Add rate limiting to prevent abuse
4. **Traffic Filtering**: Implement traffic inspection and filtering
5. **Privacy**: Consider implementing onion routing for better anonymity
6. **Legal Compliance**: Ensure compliance with local laws regarding VPN services

## Testing

### Local Testing

1. Start node 1:
```bash
cargo run -- start -r "TestRegion1" -p 4202 -s 1080
```

2. Start node 2 (different terminal):
```bash
cargo run -- start -r "TestRegion2" -p 4203 -s 1081
```

3. On node 1, connect to node 2:
```bash
cargo run -- connect
```

4. On node 2, approve the request:
```bash
cargo run -- requests
```

5. Test the connection:
```bash
curl --socks5 127.0.0.1:1080 https://api.ipify.org
```

### Network Testing

To test across different networks:

1. Ensure firewall allows the P2P port (default: 4202)
2. For internet-wide discovery, you may need to configure port forwarding
3. Peers will discover each other through the Kademlia DHT

## Troubleshooting

### Peers Not Discovering Each Other

- Check firewall settings
- Ensure both nodes are on the same local network for mDNS
- Wait a few moments for DHT to propagate

### Connection Refused

- Verify the peer has approved your routing request
- Check that the SOCKS5 proxy is running (should show "SOCKS5 proxy listening on...")

### Build Errors

If you encounter network errors when building:
```bash
# Try cleaning and rebuilding
cargo clean
cargo build --release
```

## Development

### Project Structure

```
dvpn/
├── src/
│   ├── main.rs           # CLI interface and application logic
│   ├── error.rs          # Error handling
│   ├── socks5.rs         # SOCKS5 proxy implementation
│   └── p2p/
│       ├── mod.rs        # P2P module exports
│       ├── network.rs    # libp2p network implementation
│       ├── peer_manager.rs # Peer state management
│       └── protocol.rs   # Message protocol definitions
├── Cargo.toml            # Dependencies and project metadata
└── README.md             # This file
```

### Key Dependencies

- `libp2p`: Peer-to-peer networking
- `tokio`: Async runtime
- `clap`: CLI argument parsing
- `serde`: Serialization
- `bincode`: Binary encoding for messages
- `tabled`: Table formatting
- `inquire`: Interactive CLI prompts

## Future Improvements

1. **Multi-hop Routing**: Route through multiple peers for better anonymity
2. **Bandwidth Metering**: Track and limit bandwidth usage
3. **Payment Integration**: Incentivize peers with cryptocurrency payments
4. **Mobile Support**: Create mobile apps for iOS/Android
5. **Web Interface**: Add a web UI for easier management
6. **Performance Metrics**: Track latency, bandwidth, and connection quality
7. **Peer Reputation**: Implement a reputation system for reliable peers
8. **NAT Traversal**: Better support for peers behind NAT

## Contributing

Contributions are welcome! Please feel free to submit pull requests or open issues.

## License

[Specify your license here]

## Disclaimer

This software is provided as-is for educational and research purposes. Users are responsible for ensuring compliance with all applicable laws and regulations in their jurisdiction. The authors assume no liability for misuse of this software.
