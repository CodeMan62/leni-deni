use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex, RwLock};
use thiserror::Error;
use uuid::Uuid;
use tracing::{debug, error, info, warn};
use anyhow::Result;

use super::socket::{SocketHandler, SocketMessage};

/// Unique identifier for a connection
pub type ConnectionId = Uuid;

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error("Connection not found: {0}")]
    NotFound(ConnectionId),
    
    #[error("Connection already exists: {0}")]
    AlreadyExists(ConnectionId),
    
    #[error("Socket error: {0}")]
    SocketError(#[from] super::socket::SocketError),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Connection limit reached")]
    ConnectionLimitReached,
    
    #[error("Send error: {0}")]
    SendError(String),
}

/// Status of a peer connection
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connecting,
    Connected,
    Disconnecting,
    Disconnected,
}

/// Represents a connection to a peer
#[derive(Debug)]
pub struct PeerConnection {
    /// Unique identifier for this connection
    pub id: ConnectionId,
    /// Remote peer address
    pub addr: SocketAddr,
    /// Current connection status
    pub status: ConnectionStatus,
    /// TCP stream for reliable communication
    stream: Option<TcpStream>,
    /// Channel for outgoing messages
    tx: Option<mpsc::Sender<Vec<u8>>>,
}

impl PeerConnection {
    /// Create a new peer connection
    pub fn new(addr: SocketAddr, stream: Option<TcpStream>) -> Self {
        Self {
            id: Uuid::new_v4(),
            addr,
            status: ConnectionStatus::Connecting,
            stream,
            tx: None,
        }
    }
    
    /// Start the connection handler tasks
    pub async fn start(&mut self) -> Result<mpsc::Receiver<Vec<u8>>, ConnectionError> {
        if self.stream.is_none() {
            return Err(ConnectionError::IoError(
                std::io::Error::new(std::io::ErrorKind::NotConnected, "No TCP stream available")
            ));
        }
        
        let stream = self.stream.take().unwrap();
        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(100);
        let (incoming_tx, incoming_rx) = mpsc::channel::<Vec<u8>>(100);
        
        self.tx = Some(tx.clone());
        self.status = ConnectionStatus::Connected;
        
        // Split stream for reading and writing
        let (mut read_half, mut write_half) = stream.into_split();
        
        // Spawn task for handling outgoing messages
        tokio::spawn(async move {
            while let Some(data) = rx.recv().await {
                // Simple framing: 4-byte length prefix followed by data
                let len = data.len() as u32;
                let len_bytes = len.to_be_bytes();
                
                if let Err(e) = write_half.write_all(&len_bytes).await {
                    error!("Failed to write length prefix: {}", e);
                    break;
                }
                
                if let Err(e) = write_half.write_all(&data).await {
                    error!("Failed to write data: {}", e);
                    break;
                }
                
                if let Err(e) = write_half.flush().await {
                    error!("Failed to flush: {}", e);
                    break;
                }
            }
            
            debug!("Outgoing message handler exited");
        });
        
        // Spawn task for handling incoming messages
        tokio::spawn(async move {
            let mut len_buf = [0u8; 4];
            let mut data_buf = vec![0u8; 8192]; // Initial capacity
            
            loop {
                // Read length prefix
                match read_half.read_exact(&mut len_buf).await {
                    Ok(_) => {
                        let len = u32::from_be_bytes(len_buf) as usize;
                        
                        // Resize buffer if needed
                        if data_buf.len() < len {
                            data_buf.resize(len, 0);
                        }
                        
                        // Read data
                        match read_half.read_exact(&mut data_buf[..len]).await {
                            Ok(_) => {
                                let message = data_buf[..len].to_vec();
                                if let Err(e) = incoming_tx.send(message).await {
                                    error!("Failed to send incoming message: {}", e);
                                    break;
                                }
                            }
                            Err(e) => {
                                error!("Failed to read data: {}", e);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::UnexpectedEof {
                            debug!("Connection closed");
                        } else {
                            error!("Failed to read length prefix: {}", e);
                        }
                        break;
                    }
                }
            }
            
            debug!("Incoming message handler exited");
        });
        
        Ok(incoming_rx)
    }
    
    /// Send data to the peer
    pub async fn send(&self, data: Vec<u8>) -> Result<(), ConnectionError> {
        if let Some(tx) = &self.tx {
            tx.send(data).await.map_err(|e| {
                ConnectionError::SendError(format!("Failed to send: {}", e))
            })?;
            Ok(())
        } else {
            Err(ConnectionError::IoError(
                std::io::Error::new(std::io::ErrorKind::NotConnected, "Connection not started")
            ))
        }
    }
}

/// Manages connections to peers
#[derive(Debug)]
pub struct ConnectionManager {
    /// Map of connection IDs to connections
    connections: Arc<RwLock<HashMap<ConnectionId, Arc<Mutex<PeerConnection>>>>>,
    /// Socket handler for accepting new connections
    socket_handler: Arc<Mutex<SocketHandler>>,
    /// Maximum number of connections
    max_connections: usize,
}

impl ConnectionManager {
    /// Create a new connection manager
    pub fn new(socket_handler: SocketHandler, max_connections: usize) -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            socket_handler: Arc::new(Mutex::new(socket_handler)),
            max_connections,
        }
    }
    
    /// Initialize the connection manager and start listening for connections
    pub async fn init(&self) -> Result<(), ConnectionError> {
        let mut handler = self.socket_handler.lock().await;
        // Only initialize if not already initialized
        if handler.get_status().is_none() {
            handler.init().await?;
        }
        Ok(())
    }
    
    /// Start the connection manager
    pub async fn start(&self) -> Result<(), ConnectionError> {
        let handler = self.socket_handler.lock().await;
        
        // Make sure we have properly initialized sockets
        if handler.get_status().map_or(false, |(tcp, udp)| tcp && udp) {
            let mut socket_rx = handler.start_listening()?;
            
            // Clone for move into async task
            let connections = Arc::clone(&self.connections);
            let max_connections = self.max_connections;
            
            // Spawn task to handle socket messages
            tokio::spawn(async move {
                while let Some(message) = socket_rx.recv().await {
                    match message {
                        SocketMessage::TcpConnection(stream, addr) => {
                            debug!("Handling new TCP connection from {}", addr);
                            
                            // Check connection limit
                            let connection_count = connections.read().await.len();
                            if connection_count >= max_connections {
                                warn!("Connection limit reached ({}), rejecting connection from {}", max_connections, addr);
                                continue;
                            }
                            
                            // Create new connection
                            let mut peer_conn = PeerConnection::new(addr, Some(stream));
                            let conn_id = peer_conn.id;
                            
                            // Start connection handlers
                            match peer_conn.start().await {
                                Ok(_rx) => {
                                    info!("Connection established with {} (ID: {})", addr, conn_id);
                                    
                                    // Store connection
                                    let peer_conn = Arc::new(Mutex::new(peer_conn));
                                    connections.write().await.insert(conn_id, Arc::clone(&peer_conn));
                                    
                                    // Here you would typically handle incoming messages from rx
                                    // This would be integrated with a protocol handler
                                }
                                Err(e) => {
                                    error!("Failed to start connection with {}: {}", addr, e);
                                }
                            }
                        }
                        SocketMessage::UdpData(data, addr) => {
                            debug!("Received UDP data from {}: {} bytes", addr, data.len());
                            // Handle UDP data (discovery, etc.)
                        }
                    }
                }
            });
            
            Ok(())
        } else {
            Err(ConnectionError::SocketError(
                super::socket::SocketError::BindError("Sockets not fully initialized".to_string())
            ))
        }
    }
    
    /// Connect to a peer
    pub async fn connect_to(&self, addr: SocketAddr) -> Result<ConnectionId, ConnectionError> {
        // Check connection limit
        let connection_count = self.connections.read().await.len();
        if connection_count >= self.max_connections {
            return Err(ConnectionError::ConnectionLimitReached);
        }
        
        // Check if already connected
        for conn in self.connections.read().await.values() {
            let conn_guard = conn.lock().await;
            if conn_guard.addr == addr && conn_guard.status == ConnectionStatus::Connected {
                return Ok(conn_guard.id);
            }
        }
        
        // Connect to peer
        debug!("Connecting to {}", addr);
        let stream = TcpStream::connect(addr).await?;
        
        // Create new connection
        let mut peer_conn = PeerConnection::new(addr, Some(stream));
        let conn_id = peer_conn.id;
        
        // Start connection handlers
        match peer_conn.start().await {
            Ok(_rx) => {
                info!("Connection established with {} (ID: {})", addr, conn_id);
                
                // Store connection
                let peer_conn = Arc::new(Mutex::new(peer_conn));
                self.connections.write().await.insert(conn_id, Arc::clone(&peer_conn));
                
                Ok(conn_id)
            }
            Err(e) => {
                error!("Failed to start connection with {}: {}", addr, e);
                Err(e)
            }
        }
    }
    
    /// Disconnect from a peer
    pub async fn disconnect(&self, conn_id: ConnectionId) -> Result<(), ConnectionError> {
        // First, get the connection and update its status
        let conn_opt = {
            let connections = self.connections.read().await;
            connections.get(&conn_id).cloned()
        };
        
        if let Some(conn) = conn_opt {
            {
                let mut conn_guard = conn.lock().await;
                conn_guard.status = ConnectionStatus::Disconnecting;
                info!("Disconnecting from {} (ID: {})", conn_guard.addr, conn_id);
            }
            
            // Now remove it from the connections map
            let mut connections = self.connections.write().await;
            connections.remove(&conn_id);
            Ok(())
        } else {
            Err(ConnectionError::NotFound(conn_id))
        }
    }
    
    /// Send data to a peer
    pub async fn send_to(&self, conn_id: ConnectionId, data: Vec<u8>) -> Result<(), ConnectionError> {
        let connections = self.connections.read().await;
        
        if let Some(conn) = connections.get(&conn_id) {
            let conn_guard = conn.lock().await;
            conn_guard.send(data).await
        } else {
            Err(ConnectionError::NotFound(conn_id))
        }
    }
    
    /// Get a list of all connections
    pub async fn list_connections(&self) -> Vec<(ConnectionId, SocketAddr, ConnectionStatus)> {
        let connections = self.connections.read().await;
        
        let mut result = Vec::with_capacity(connections.len());
        for (id, conn) in connections.iter() {
            let conn_guard = conn.lock().await;
            result.push((*id, conn_guard.addr, conn_guard.status.clone()));
        }
        
        result
    }
    
    /// Get a connection by ID
    pub async fn get_connection(&self, conn_id: ConnectionId) -> Option<Arc<Mutex<PeerConnection>>> {
        let connections = self.connections.read().await;
        connections.get(&conn_id).cloned()
    }
} 
