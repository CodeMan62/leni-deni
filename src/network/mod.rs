mod connection;
mod socket;
mod protocol;
mod discovery;
mod nat;

pub use connection::{ConnectionManager, ConnectionError, ConnectionStatus, PeerConnection};
pub use socket::{SocketHandler, SocketError, SocketMessage};
pub use protocol::{Protocol, ProtocolError, Message, MessageType};
pub use discovery::{DiscoveryService, DiscoveryError, DiscoveryEvent, DiscoveredPeer};
pub use nat::NatTraversal;

/// Configuration for the network module
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// The port to listen on for TCP connections
    pub tcp_port: u16,
    /// The port to listen on for UDP messages
    pub udp_port: u16,
    /// Maximum number of concurrent connections
    pub max_connections: usize,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            tcp_port: 45678,
            udp_port: 45679,
            max_connections: 50,
        }
    }
}
