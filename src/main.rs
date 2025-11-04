mod error;
mod p2p;
mod socks5;

use error::Result;
use p2p::{NetworkCommand, NetworkEvent, P2PNetwork, PeerManager};
use socks5::Socks5Server;

use clap::{Parser, Subcommand};
use inquire::{Confirm, Select};
use std::net::SocketAddr;
use tabled::{Table, Tabled};
use tokio::sync::mpsc;

#[derive(Parser, Debug)]
#[command(version, about = "Decentralized VPN", long_about = None)]
struct Args {
    #[command(subcommand)]
    mode: Mode,
}

#[derive(Subcommand, Debug)]
enum Mode {
    /// Start the VPN node (join the network and serve)
    Start(Start),
    /// List available peers
    List,
    /// Connect to a peer (route traffic through them)
    Connect(Connect),
    /// View and manage routing requests
    Requests,
}

#[derive(Parser, Debug)]
struct Start {
    /// Port for P2P networking
    #[arg(short = 'p', long, default_value = "4202")]
    p2p_port: u16,

    /// Port for SOCKS5 proxy
    #[arg(short = 's', long, default_value = "1080")]
    socks_port: u16,

    /// Region identifier
    #[arg(short = 'r', long, default_value = "unknown")]
    region: String,
}

#[derive(Parser, Debug)]
struct Connect {
    /// Optional region filter
    #[arg(short, long)]
    region: Option<String>,
}

#[derive(Debug, Clone, Tabled)]
struct PeerDisplay {
    #[tabled(rename = "Peer ID")]
    peer_id: String,
    #[tabled(rename = "Region")]
    region: String,
    #[tabled(rename = "Available Slots")]
    slots: u32,
    #[tabled(rename = "Status")]
    status: String,
}

impl Start {
    async fn handle(&self) -> Result<()> {
        println!("🚀 Starting Decentralized VPN Node...");
        println!("   P2P Port: {}", self.p2p_port);
        println!("   SOCKS5 Port: {}", self.socks_port);
        println!("   Region: {}\n", self.region);

        let peer_manager = PeerManager::new();
        let peer_manager_clone = peer_manager.clone();

        // Create P2P network
        let (mut network, command_tx, mut event_rx) =
            P2PNetwork::new(self.region.clone(), peer_manager.clone())
                .await
                .map_err(|e| format!("Failed to create P2P network: {}", e))?;

        // Start listening on P2P port
        network
            .listen(self.p2p_port)
            .map_err(|e| format!("Failed to start P2P listener: {}", e))?;

        // Start SOCKS5 proxy
        let socks_addr = format!("127.0.0.1:{}", self.socks_port);
        let mut socks_server = Socks5Server::new(&socks_addr)
            .await
            .map_err(|e| format!("Failed to start SOCKS5 server: {:?}", e))?;

        println!("✅ Node is running!");
        println!("   Configure your applications to use SOCKS5 proxy: {}\n", socks_addr);
        println!("📡 Discovering peers...\n");

        // Spawn P2P network task
        tokio::spawn(async move {
            network.run().await;
        });

        // Spawn SOCKS5 proxy task
        tokio::spawn(async move {
            if let Err(e) = socks_server.run().await {
                eprintln!("SOCKS5 server error: {:?}", e);
            }
        });

        // Handle network events
        let command_tx_clone = command_tx.clone();
        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                match event {
                    NetworkEvent::PeerDiscovered { peer_id, region } => {
                        println!("🔍 Discovered peer: {} ({})", peer_id, region);
                    }
                    NetworkEvent::PeerDisconnected { peer_id } => {
                        println!("👋 Peer disconnected: {}", peer_id);
                    }
                    NetworkEvent::RouteRequestReceived {
                        request_id,
                        peer_id,
                        peer_name,
                    } => {
                        println!("\n📥 Routing request received!");
                        println!("   From: {} ({})", peer_name, peer_id);
                        println!("   Request ID: {}", request_id);
                        println!("   Use 'dvpn requests' to view and approve/deny\n");
                    }
                    NetworkEvent::RouteResponseReceived {
                        request_id,
                        approved,
                    } => {
                        if approved {
                            println!("✅ Routing request approved! (ID: {})", request_id);
                            println!("   Your traffic will now be routed through the peer.\n");
                        } else {
                            println!("❌ Routing request denied. (ID: {})\n", request_id);
                        }
                    }
                    NetworkEvent::MessageReceived { from, message } => {
                        println!("💬 Message from {}: {}", from, message);
                    }
                }
            }
        });

        // Keep the main thread alive
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;

            // Show peer count periodically
            let peers = peer_manager_clone.get_peers().await;
            if !peers.is_empty() {
                println!("📊 Connected peers: {}", peers.len());
            }
        }
    }
}

impl Connect {
    async fn handle(&self, peer_manager: PeerManager, command_tx: mpsc::UnboundedSender<NetworkCommand>) -> Result<()> {
        // Get list of peers
        let mut peers = peer_manager.get_peers().await;

        if peers.is_empty() {
            println!("❌ No peers available. Make sure a node is running with 'dvpn start'");
            return Ok(());
        }

        // Filter by region if specified
        if let Some(region) = &self.region {
            peers.retain(|p| p.region.contains(region));
            if peers.is_empty() {
                println!("❌ No peers found in region: {}", region);
                return Ok(());
            }
        }

        // Display peers
        let peer_displays: Vec<PeerDisplay> = peers
            .iter()
            .map(|p| PeerDisplay {
                peer_id: p.peer_id.to_string()[..16].to_string(),
                region: p.region.clone(),
                slots: p.available_slots,
                status: if p.is_routing_through {
                    "Active".to_string()
                } else {
                    "Available".to_string()
                },
            })
            .collect();

        println!("\n📋 Available Peers:\n");
        println!("{}\n", Table::new(&peer_displays));

        // Select peer
        let options: Vec<String> = peers
            .iter()
            .map(|p| format!("{} - {} ({})", &p.peer_id.to_string()[..16], p.region, p.available_slots))
            .collect();

        let selection = Select::new("Select a peer to route through:", options)
            .prompt()
            .map_err(|e| format!("Selection error: {}", e))?;

        let selected_idx = options
            .iter()
            .position(|x| x == &selection)
            .ok_or("Invalid selection")?;
        let selected_peer = &peers[selected_idx];

        println!("\n📤 Sending routing request to peer...");

        // Send route request
        command_tx
            .send(NetworkCommand::SendRouteRequest {
                target_peer: selected_peer.peer_id.to_string(),
                requester_name: "User".to_string(),
            })
            .map_err(|e| format!("Failed to send route request: {}", e))?;

        println!("✅ Request sent! Waiting for approval...");
        println!("   The peer will receive a notification and can approve/deny your request.\n");

        Ok(())
    }
}

async fn handle_requests(peer_manager: PeerManager, command_tx: mpsc::UnboundedSender<NetworkCommand>) -> Result<()> {
    let requests = peer_manager.get_pending_requests().await;

    if requests.is_empty() {
        println!("📭 No pending routing requests.\n");
        return Ok(());
    }

    println!("\n📬 Pending Routing Requests:\n");

    for request in &requests {
        println!("┌─────────────────────────────────────────────");
        println!("│ Request ID: {}", request.request_id);
        println!("│ From: {} ({})", request.peer_name, request.peer_id);
        println!("│ Time: {}", request.timestamp.format("%Y-%m-%d %H:%M:%S"));
        println!("└─────────────────────────────────────────────\n");

        let approve = Confirm::new("Approve this routing request?")
            .with_default(false)
            .prompt()
            .map_err(|e| format!("Prompt error: {}", e))?;

        // Send response
        command_tx
            .send(NetworkCommand::SendRouteResponse {
                request_id: request.request_id.clone(),
                approved: approve,
            })
            .map_err(|e| format!("Failed to send response: {}", e))?;

        // Remove from pending
        peer_manager.remove_routing_request(&request.request_id).await;

        if approve {
            println!("✅ Request approved!\n");
            // Set active route
            peer_manager.set_active_route(Some(request.peer_id)).await;
        } else {
            println!("❌ Request denied.\n");
        }
    }

    Ok(())
}

async fn handle_list(peer_manager: PeerManager) -> Result<()> {
    let peers = peer_manager.get_peers().await;

    if peers.is_empty() {
        println!("❌ No peers discovered yet. Start a node with 'dvpn start'\n");
        return Ok(());
    }

    let peer_displays: Vec<PeerDisplay> = peers
        .iter()
        .map(|p| PeerDisplay {
            peer_id: p.peer_id.to_string()[..16].to_string(),
            region: p.region.clone(),
            slots: p.available_slots,
            status: if p.is_routing_through {
                "Active".to_string()
            } else {
                "Available".to_string()
            },
        })
        .collect();

    println!("\n📋 Discovered Peers:\n");
    println!("{}\n", Table::new(&peer_displays));

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    match args.mode {
        Mode::Start(start) => start.handle().await,
        Mode::List => {
            println!("❌ This command requires a running node. Use 'dvpn start' first.\n");
            Ok(())
        }
        Mode::Connect(_) => {
            println!("❌ This command requires a running node. Use 'dvpn start' first.\n");
            Ok(())
        }
        Mode::Requests => {
            println!("❌ This command requires a running node. Use 'dvpn start' first.\n");
            Ok(())
        }
    }
}
