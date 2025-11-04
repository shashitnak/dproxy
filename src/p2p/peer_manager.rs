use chrono::{DateTime, Utc};
use libp2p::PeerId;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    pub region: String,
    pub available_slots: u32,
    pub last_seen: DateTime<Utc>,
    pub is_routing_through: bool,
}

impl PeerInfo {
    pub fn new(peer_id: PeerId, region: String, available_slots: u32) -> Self {
        Self {
            peer_id,
            region,
            available_slots,
            last_seen: Utc::now(),
            is_routing_through: false,
        }
    }

    pub fn update_last_seen(&mut self) {
        self.last_seen = Utc::now();
    }
}

#[derive(Debug, Clone)]
pub struct RoutingRequest {
    pub request_id: String,
    pub peer_id: PeerId,
    pub peer_name: String,
    pub timestamp: DateTime<Utc>,
}

/// Manages the list of discovered peers and routing requests
#[derive(Clone)]
pub struct PeerManager {
    peers: Arc<RwLock<HashMap<PeerId, PeerInfo>>>,
    pending_requests: Arc<RwLock<HashMap<String, RoutingRequest>>>,
    active_route: Arc<RwLock<Option<PeerId>>>,
}

impl PeerManager {
    pub fn new() -> Self {
        Self {
            peers: Arc::new(RwLock::new(HashMap::new())),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            active_route: Arc::new(RwLock::new(None)),
        }
    }

    /// Add or update a peer
    pub async fn add_peer(&self, peer_id: PeerId, region: String, available_slots: u32) {
        let mut peers = self.peers.write().await;
        peers
            .entry(peer_id)
            .and_modify(|p| {
                p.region = region.clone();
                p.available_slots = available_slots;
                p.update_last_seen();
            })
            .or_insert_with(|| PeerInfo::new(peer_id, region, available_slots));
    }

    /// Remove a peer
    pub async fn remove_peer(&self, peer_id: &PeerId) {
        let mut peers = self.peers.write().await;
        peers.remove(peer_id);
    }

    /// Get all peers
    pub async fn get_peers(&self) -> Vec<PeerInfo> {
        let peers = self.peers.read().await;
        peers.values().cloned().collect()
    }

    /// Get a specific peer
    pub async fn get_peer(&self, peer_id: &PeerId) -> Option<PeerInfo> {
        let peers = self.peers.read().await;
        peers.get(peer_id).cloned()
    }

    /// Add a pending routing request
    pub async fn add_routing_request(
        &self,
        request_id: String,
        peer_id: PeerId,
        peer_name: String,
    ) {
        let mut requests = self.pending_requests.write().await;
        requests.insert(
            request_id.clone(),
            RoutingRequest {
                request_id,
                peer_id,
                peer_name,
                timestamp: Utc::now(),
            },
        );
    }

    /// Get all pending routing requests
    pub async fn get_pending_requests(&self) -> Vec<RoutingRequest> {
        let requests = self.pending_requests.read().await;
        requests.values().cloned().collect()
    }

    /// Remove a routing request
    pub async fn remove_routing_request(&self, request_id: &str) -> Option<RoutingRequest> {
        let mut requests = self.pending_requests.write().await;
        requests.remove(request_id)
    }

    /// Set the active routing peer
    pub async fn set_active_route(&self, peer_id: Option<PeerId>) {
        let mut active = self.active_route.write().await;
        *active = peer_id;

        // Update the peer's routing status
        if let Some(pid) = peer_id {
            let mut peers = self.peers.write().await;
            if let Some(peer) = peers.get_mut(&pid) {
                peer.is_routing_through = true;
            }
        }
    }

    /// Get the active routing peer
    pub async fn get_active_route(&self) -> Option<PeerId> {
        let active = self.active_route.read().await;
        *active
    }

    /// Clean up old peers (not seen in last 5 minutes)
    pub async fn cleanup_stale_peers(&self) {
        let mut peers = self.peers.write().await;
        let now = Utc::now();
        peers.retain(|_, peer| {
            let duration = now.signed_duration_since(peer.last_seen);
            duration.num_minutes() < 5
        });
    }
}

impl Default for PeerManager {
    fn default() -> Self {
        Self::new()
    }
}
