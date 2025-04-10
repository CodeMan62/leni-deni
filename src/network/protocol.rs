use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::SocketAddr;
use thiserror::Error;
use tracing::{debug, error};
use uuid::Uuid;

use super::connection::ConnectionId;

/// Protocol versioning
pub const PROTOCOL_VERSION: u8 = 1;

/// Error types for protocol operations
#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("Serialization error: {0}")]
    SerializationError(#[from] bincode::Error),
    
    #[error("Invalid message format")]
    InvalidFormat,
    
    #[error("Protocol version mismatch. Expected {0}, got {1}")]
    VersionMismatch(u8, u8),
    
    #[error("Unknown message type: {0}")]
    UnknownMessageType(u8),
}

/// Message types supported by the protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum MessageType {
    /// Ping request to check if peer is alive
    Ping = 0,
    /// Pong response to a ping
    Pong = 1,
    /// Discovery announcement
    Announce = 2,
    /// Discovery query
    DiscoveryQuery = 3,
    /// Peer list exchange
    PeerList = 4,
    /// File metadata
    FileMetadata = 5,
    /// Request for file content
    FileRequest = 6,
    /// File content chunk
    FileChunk = 7,
    /// File transfer acknowledgment
    Ack = 8,
    /// File transfer completion
    FileComplete = 9,
    /// Error message
    Error = 255,
}

impl TryFrom<u8> for MessageType {
    type Error = ProtocolError;
    
    fn try_from(value: u8) -> Result<Self, ProtocolError> {
        match value {
            0 => Ok(MessageType::Ping),
            1 => Ok(MessageType::Pong),
            2 => Ok(MessageType::Announce),
            3 => Ok(MessageType::DiscoveryQuery),
            4 => Ok(MessageType::PeerList),
            5 => Ok(MessageType::FileMetadata),
            6 => Ok(MessageType::FileRequest),
            7 => Ok(MessageType::FileChunk),
            8 => Ok(MessageType::Ack),
            9 => Ok(MessageType::FileComplete),
            255 => Ok(MessageType::Error),
            _ => Err(ProtocolError::UnknownMessageType(value)),
        }
    }
}

/// Message structure for communication between peers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Protocol version
    pub version: u8,
    /// Message type
    pub msg_type: MessageType,
    /// Message ID for correlating requests and responses
    pub message_id: Uuid,
    /// Sender ID
    pub sender_id: Uuid,
    /// Payload data
    pub payload: Vec<u8>,
}

/// Protocol handler for encoding/decoding messages
#[derive(Debug, Clone)]
pub struct Protocol {
    /// Unique identifier for this node
    pub node_id: Uuid,
}

impl Protocol {
    /// Create a new protocol handler
    pub fn new(node_id: Uuid) -> Self {
        Self { node_id }
    }
    
    /// Create a new message
    pub fn create_message(&self, msg_type: MessageType, payload: Vec<u8>) -> Message {
        Message {
            version: PROTOCOL_VERSION,
            msg_type,
            message_id: Uuid::new_v4(),
            sender_id: self.node_id,
            payload,
        }
    }
    
    /// Encode a message to bytes
    pub fn encode(&self, message: &Message) -> Result<Vec<u8>, ProtocolError> {
        Ok(bincode::serialize(message)?)
    }
    
    /// Decode bytes to a message
    pub fn decode(&self, data: &[u8]) -> Result<Message, ProtocolError> {
        let message: Message = bincode::deserialize(data)?;
        
        // Check protocol version
        if message.version != PROTOCOL_VERSION {
            return Err(ProtocolError::VersionMismatch(PROTOCOL_VERSION, message.version));
        }
        
        Ok(message)
    }
    
    /// Create a ping message
    pub fn create_ping(&self) -> Result<Vec<u8>, ProtocolError> {
        let message = self.create_message(MessageType::Ping, Vec::new());
        self.encode(&message)
    }
    
    /// Create a pong message in response to a ping
    pub fn create_pong(&self, ping_id: Uuid) -> Result<Vec<u8>, ProtocolError> {
        let mut message = self.create_message(MessageType::Pong, Vec::new());
        message.message_id = ping_id;
        self.encode(&message)
    }
    
    /// Create an announcement message
    pub fn create_announce(&self, addr: SocketAddr) -> Result<Vec<u8>, ProtocolError> {
        let addr_str = addr.to_string();
        let payload = addr_str.into_bytes();
        let message = self.create_message(MessageType::Announce, payload);
        self.encode(&message)
    }
    
    /// Create a discovery query message
    pub fn create_discovery_query(&self) -> Result<Vec<u8>, ProtocolError> {
        let message = self.create_message(MessageType::DiscoveryQuery, Vec::new());
        self.encode(&message)
    }
    
    /// Create a peer list message
    pub fn create_peer_list(&self, peers: &[(ConnectionId, SocketAddr)]) -> Result<Vec<u8>, ProtocolError> {
        let peer_data: Vec<String> = peers
            .iter()
            .map(|(id, addr)| format!("{}:{}", id, addr))
            .collect();
        
        let payload = bincode::serialize(&peer_data)?;
        let message = self.create_message(MessageType::PeerList, payload);
        self.encode(&message)
    }
    
    /// Create an error message
    pub fn create_error(&self, error_message: &str) -> Result<Vec<u8>, ProtocolError> {
        let payload = error_message.as_bytes().to_vec();
        let message = self.create_message(MessageType::Error, payload);
        self.encode(&message)
    }
    
    /// Handle an incoming message
    pub fn handle_message(&self, data: &[u8]) -> Result<(MessageType, Uuid, Vec<u8>), ProtocolError> {
        let message = self.decode(data)?;
        debug!("Received message: {:?}", message.msg_type);
        
        Ok((message.msg_type, message.message_id, message.payload))
    }
}

/// Implement display for MessageType for better logging
impl fmt::Display for MessageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageType::Ping => write!(f, "Ping"),
            MessageType::Pong => write!(f, "Pong"),
            MessageType::Announce => write!(f, "Announce"),
            MessageType::DiscoveryQuery => write!(f, "DiscoveryQuery"),
            MessageType::PeerList => write!(f, "PeerList"),
            MessageType::FileMetadata => write!(f, "FileMetadata"),
            MessageType::FileRequest => write!(f, "FileRequest"),
            MessageType::FileChunk => write!(f, "FileChunk"),
            MessageType::Ack => write!(f, "Ack"),
            MessageType::FileComplete => write!(f, "FileComplete"),
            MessageType::Error => write!(f, "Error"),
        }
    }
} 
