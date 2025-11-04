use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Messages exchanged between peers for routing requests and approvals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VpnMessage {
    /// Request to route traffic through a peer
    RouteRequest {
        request_id: String,
        requester_peer_id: String,
        requester_name: String,
    },
    /// Response to a routing request
    RouteResponse {
        request_id: String,
        approved: bool,
    },
    /// Heartbeat to keep connection alive
    Heartbeat,
    /// Peer information broadcast
    PeerInfo {
        peer_id: String,
        region: String,
        available_slots: u32,
    },
}

impl VpnMessage {
    pub fn new_route_request(peer_id: String, name: String) -> Self {
        VpnMessage::RouteRequest {
            request_id: Uuid::new_v4().to_string(),
            requester_peer_id: peer_id,
            requester_name: name,
        }
    }

    pub fn new_route_response(request_id: String, approved: bool) -> Self {
        VpnMessage::RouteResponse {
            request_id,
            approved,
        }
    }

    pub fn new_heartbeat() -> Self {
        VpnMessage::Heartbeat
    }

    pub fn new_peer_info(peer_id: String, region: String, available_slots: u32) -> Self {
        VpnMessage::PeerInfo {
            peer_id,
            region,
            available_slots,
        }
    }
}

/// Serialize a message to bytes
pub fn encode_message(msg: &VpnMessage) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(bincode::serialize(msg)?)
}

/// Deserialize a message from bytes
pub fn decode_message(data: &[u8]) -> Result<VpnMessage, Box<dyn std::error::Error>> {
    Ok(bincode::deserialize(data)?)
}
