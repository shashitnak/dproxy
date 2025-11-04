use super::peer_manager::PeerManager;
use super::protocol::{decode_message, encode_message, VpnMessage};
use futures::StreamExt;
use libp2p::{
    gossipsub, identify, kad,
    mdns,
    noise,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Swarm, SwarmBuilder,
};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;
use tokio::sync::mpsc;

/// Network behavior combining mDNS, Kademlia, GossipSub, and Identify
#[derive(NetworkBehaviour)]
pub struct VpnBehaviour {
    pub mdns: mdns::tokio::Behaviour,
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,
    pub gossipsub: gossipsub::Behaviour,
    pub identify: identify::Behaviour,
}

/// Events that can be sent from the network layer to the application
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    PeerDiscovered {
        peer_id: String,
        region: String,
    },
    PeerDisconnected {
        peer_id: String,
    },
    RouteRequestReceived {
        request_id: String,
        peer_id: String,
        peer_name: String,
    },
    RouteResponseReceived {
        request_id: String,
        approved: bool,
    },
    MessageReceived {
        from: String,
        message: String,
    },
}

/// Commands that can be sent to the network layer
#[derive(Debug, Clone)]
pub enum NetworkCommand {
    SendRouteRequest {
        target_peer: String,
        requester_name: String,
    },
    SendRouteResponse {
        request_id: String,
        approved: bool,
    },
    BroadcastPeerInfo {
        region: String,
        available_slots: u32,
    },
}

pub struct P2PNetwork {
    swarm: Swarm<VpnBehaviour>,
    peer_manager: PeerManager,
    event_tx: mpsc::UnboundedSender<NetworkEvent>,
    command_rx: mpsc::UnboundedReceiver<NetworkCommand>,
    region: String,
}

impl P2PNetwork {
    pub async fn new(
        region: String,
        peer_manager: PeerManager,
    ) -> Result<(Self, mpsc::UnboundedSender<NetworkCommand>, mpsc::UnboundedReceiver<NetworkEvent>), Box<dyn std::error::Error>> {
        // Create a new keypair for this peer
        let local_key = libp2p::identity::Keypair::generate_ed25519();
        let local_peer_id = local_key.public().to_peer_id();

        println!("Local peer id: {}", local_peer_id);

        // Create a Swarm
        let swarm = SwarmBuilder::with_existing_identity(local_key)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_behaviour(|key| {
                // mDNS for local network peer discovery
                let mdns = mdns::tokio::Behaviour::new(
                    mdns::Config::default(),
                    key.public().to_peer_id(),
                )?;

                // Kademlia DHT for global peer discovery
                let store = kad::store::MemoryStore::new(key.public().to_peer_id());
                let kademlia = kad::Behaviour::new(key.public().to_peer_id(), store);

                // GossipSub for message propagation
                let gossipsub_config = gossipsub::ConfigBuilder::default()
                    .heartbeat_interval(Duration::from_secs(10))
                    .validation_mode(gossipsub::ValidationMode::Strict)
                    .build()
                    .expect("Valid config");

                let mut gossipsub = gossipsub::Behaviour::new(
                    gossipsub::MessageAuthenticity::Signed(key.clone()),
                    gossipsub_config,
                )?;

                // Subscribe to the VPN topic
                let topic = gossipsub::IdentTopic::new("dvpn");
                gossipsub.subscribe(&topic)?;

                // Identify protocol
                let identify = identify::Behaviour::new(identify::Config::new(
                    "/dvpn/1.0.0".to_string(),
                    key.public(),
                ));

                Ok(VpnBehaviour {
                    mdns,
                    kademlia,
                    gossipsub,
                    identify,
                })
            })?
            .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
            .build();

        // Create channels for communication
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (command_tx, command_rx) = mpsc::unbounded_channel();

        Ok((
            Self {
                swarm,
                peer_manager,
                event_tx,
                command_rx,
                region,
            },
            command_tx,
            event_rx,
        ))
    }

    /// Start listening on all interfaces
    pub fn listen(&mut self, port: u16) -> Result<(), Box<dyn std::error::Error>> {
        let listen_addr = format!("/ip4/0.0.0.0/tcp/{}", port);
        self.swarm.listen_on(listen_addr.parse()?)?;
        Ok(())
    }

    /// Main event loop for the P2P network
    pub async fn run(&mut self) {
        // Start broadcasting peer info periodically
        let mut broadcast_interval = tokio::time::interval(Duration::from_secs(30));

        loop {
            tokio::select! {
                // Handle swarm events
                event = self.swarm.select_next_some() => {
                    self.handle_swarm_event(event).await;
                }

                // Handle commands from the application
                Some(command) = self.command_rx.recv() => {
                    self.handle_command(command).await;
                }

                // Broadcast peer info periodically
                _ = broadcast_interval.tick() => {
                    self.broadcast_peer_info().await;
                }

                // Cleanup stale peers periodically
                _ = tokio::time::sleep(Duration::from_secs(60)) => {
                    self.peer_manager.cleanup_stale_peers().await;
                }
            }
        }
    }

    async fn handle_swarm_event(&mut self, event: SwarmEvent<VpnBehaviourEvent>) {
        match event {
            SwarmEvent::Behaviour(VpnBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                for (peer_id, addr) in list {
                    println!("mDNS discovered peer: {} at {}", peer_id, addr);
                    self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                    self.peer_manager
                        .add_peer(peer_id, "local".to_string(), 5)
                        .await;

                    let _ = self.event_tx.send(NetworkEvent::PeerDiscovered {
                        peer_id: peer_id.to_string(),
                        region: "local".to_string(),
                    });
                }
            }
            SwarmEvent::Behaviour(VpnBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                for (peer_id, _addr) in list {
                    println!("mDNS peer expired: {}", peer_id);
                    self.peer_manager.remove_peer(&peer_id).await;

                    let _ = self.event_tx.send(NetworkEvent::PeerDisconnected {
                        peer_id: peer_id.to_string(),
                    });
                }
            }
            SwarmEvent::Behaviour(VpnBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                propagation_source: _,
                message_id: _,
                message,
            })) => {
                // Decode and handle VPN messages
                if let Ok(vpn_msg) = decode_message(&message.data) {
                    self.handle_vpn_message(vpn_msg, message.source).await;
                }
            }
            SwarmEvent::Behaviour(VpnBehaviourEvent::Identify(identify::Event::Received {
                peer_id,
                info,
            })) => {
                println!("Identified peer: {} with protocols: {:?}", peer_id, info.protocols);
                // Add addresses to Kademlia
                for addr in info.listen_addrs {
                    self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                }
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("Listening on {}", address);
            }
            SwarmEvent::ConnectionEstablished {
                peer_id,
                endpoint,
                ..
            } => {
                println!("Connection established with {} at {:?}", peer_id, endpoint);
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                println!("Connection closed with {}", peer_id);
            }
            _ => {}
        }
    }

    async fn handle_vpn_message(&mut self, message: VpnMessage, source: Option<libp2p::PeerId>) {
        match message {
            VpnMessage::RouteRequest {
                request_id,
                requester_peer_id,
                requester_name,
            } => {
                println!(
                    "Received route request from {} ({})",
                    requester_name, requester_peer_id
                );

                if let Some(peer_id) = source {
                    self.peer_manager
                        .add_routing_request(request_id.clone(), peer_id, requester_name.clone())
                        .await;
                }

                let _ = self.event_tx.send(NetworkEvent::RouteRequestReceived {
                    request_id,
                    peer_id: requester_peer_id,
                    peer_name: requester_name,
                });
            }
            VpnMessage::RouteResponse {
                request_id,
                approved,
            } => {
                println!("Received route response: {} (approved: {})", request_id, approved);

                let _ = self.event_tx.send(NetworkEvent::RouteResponseReceived {
                    request_id,
                    approved,
                });
            }
            VpnMessage::PeerInfo {
                peer_id,
                region,
                available_slots,
            } => {
                if let Ok(pid) = peer_id.parse() {
                    self.peer_manager
                        .add_peer(pid, region.clone(), available_slots)
                        .await;

                    let _ = self.event_tx.send(NetworkEvent::PeerDiscovered {
                        peer_id,
                        region,
                    });
                }
            }
            VpnMessage::Heartbeat => {
                // Update last seen time for the peer
                if let Some(peer_id) = source {
                    if let Some(mut peer) = self.peer_manager.get_peer(&peer_id).await {
                        peer.update_last_seen();
                    }
                }
            }
        }
    }

    async fn handle_command(&mut self, command: NetworkCommand) {
        match command {
            NetworkCommand::SendRouteRequest {
                target_peer,
                requester_name,
            } => {
                let local_peer_id = *self.swarm.local_peer_id();
                let message = VpnMessage::new_route_request(
                    local_peer_id.to_string(),
                    requester_name,
                );
                self.broadcast_message(message).await;
            }
            NetworkCommand::SendRouteResponse {
                request_id,
                approved,
            } => {
                let message = VpnMessage::new_route_response(request_id, approved);
                self.broadcast_message(message).await;
            }
            NetworkCommand::BroadcastPeerInfo {
                region,
                available_slots,
            } => {
                let local_peer_id = *self.swarm.local_peer_id();
                let message = VpnMessage::new_peer_info(
                    local_peer_id.to_string(),
                    region,
                    available_slots,
                );
                self.broadcast_message(message).await;
            }
        }
    }

    async fn broadcast_message(&mut self, message: VpnMessage) {
        if let Ok(data) = encode_message(&message) {
            let topic = gossipsub::IdentTopic::new("dvpn");
            if let Err(e) = self.swarm.behaviour_mut().gossipsub.publish(topic, data) {
                eprintln!("Failed to publish message: {:?}", e);
            }
        }
    }

    async fn broadcast_peer_info(&mut self) {
        let command = NetworkCommand::BroadcastPeerInfo {
            region: self.region.clone(),
            available_slots: 5,
        };
        self.handle_command(command).await;
    }
}
