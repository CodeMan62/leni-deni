use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time;
use tracing::{debug, error, info};
use uuid::Uuid;
use std::fmt;

use super::protocol::{Protocol, ProtocolError};
use super::socket::SocketHandler;
use super::NetworkConfig;

/// Service type for mDNS discovery
const SERVICE_TYPE: &str = "_leni-deni._udp.local.";
/// Time between discovery broadcasts (in seconds)
const DISCOVERY_INTERVAL: u64 = 30;

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("mDNS error: {0}")]
    MdnsError(String),
    
    #[error("Socket error: {0}")]
    SocketError(#[from] super::socket::SocketError),
    
    #[error("Protocol error: {0}")]
    ProtocolError(#[from] ProtocolError),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Discovered peer information
#[derive(Debug, Clone)]
pub struct DiscoveredPeer {
    /// Unique identifier for this peer
    pub id: Uuid,
    /// Socket address for connecting to this peer
    pub addr: SocketAddr,
    /// Time when this peer was last seen
    pub last_seen: std::time::Instant,
}

/// Message type for discovery events
#[derive(Debug)]
pub enum DiscoveryEvent {
    /// A new peer was discovered
    PeerDiscovered(Uuid, SocketAddr),
    /// A known peer was lost
    PeerLost(Uuid),
}

/// Service for discovering peers on the local network
pub struct DiscoveryService {
    /// Node ID for this peer
    node_id: Uuid,
    /// mDNS service daemon (not exposed for Debug)
    mdns_daemon: Arc<Mutex<Option<ServiceDaemon>>>,
    /// Socket handler for UDP communication
    socket_handler: Arc<Mutex<SocketHandler>>,
    /// Protocol handler
    protocol: Arc<Protocol>,
    /// Network configuration
    config: NetworkConfig,
    /// Map of discovered peers
    peers: Arc<RwLock<HashMap<Uuid, DiscoveredPeer>>>,
    /// Channel for discovery events
    event_tx: Option<mpsc::Sender<DiscoveryEvent>>,
}

// Manual Debug implementation that skips ServiceDaemon
impl fmt::Debug for DiscoveryService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DiscoveryService")
            .field("node_id", &self.node_id)
            .field("mdns_daemon", &"<ServiceDaemon>")
            .field("socket_handler", &self.socket_handler)
            .field("protocol", &self.protocol)
            .field("config", &self.config)
            .field("peers", &self.peers)
            .field("event_tx", &self.event_tx)
            .finish()
    }
}

impl DiscoveryService {
    /// Create a new discovery service
    pub fn new(
        node_id: Uuid, 
        socket_handler: SocketHandler, 
        protocol: Protocol,
        config: NetworkConfig
    ) -> Self {
        Self {
            node_id,
            mdns_daemon: Arc::new(Mutex::new(None)),
            socket_handler: Arc::new(Mutex::new(socket_handler)),
            protocol: Arc::new(protocol),
            config,
            peers: Arc::new(RwLock::new(HashMap::new())),
            event_tx: None,
        }
    }
    
    /// Initialize the discovery service
    pub async fn init(&mut self) -> Result<(), DiscoveryError> {
        // Create the mDNS daemon
        let mdns = ServiceDaemon::new()
            .map_err(|e| DiscoveryError::MdnsError(e.to_string()))?;
            
        *self.mdns_daemon.lock().await = Some(mdns);
        
        Ok(())
    }
    
    /// Start the discovery service
    pub async fn start(&mut self) -> Result<mpsc::Receiver<DiscoveryEvent>, DiscoveryError> {
        let (tx, rx) = mpsc::channel::<DiscoveryEvent>(100);
        self.event_tx = Some(tx.clone());
        
        // Get local addresses for the service
        let socket_handler = self.socket_handler.lock().await;
        let udp_addr = socket_handler.local_udp_addr()?;
        let host_addr = match udp_addr.ip() {
            IpAddr::V4(addr) => addr,
            IpAddr::V6(_) => {
                // Fallback to localhost if we can't determine the address
                "127.0.0.1".parse().unwrap()
            }
        };
        
        // Create mDNS service info
        let service_info = ServiceInfo::new(
            SERVICE_TYPE,
            &self.node_id.to_string(),
            &format!("leni-deni-{}", self.node_id.as_simple()),
            host_addr,
            self.config.udp_port,
            None
        )
        .map_err(|e| DiscoveryError::MdnsError(e.to_string()))?;
        
        // Register our service
        if let Some(mdns) = &*self.mdns_daemon.lock().await {
            mdns.register(service_info.clone())
                .map_err(|e| DiscoveryError::MdnsError(e.to_string()))?;
            
            debug!("Registered mDNS service: {}", service_info.get_fullname());
            
            // Start browsing for other services
            let browse_handle = mdns.browse(SERVICE_TYPE)
                .map_err(|e| DiscoveryError::MdnsError(e.to_string()))?;
                
            // Clone for move into async task
            let peers = self.peers.clone();
            let event_tx = tx.clone();
            
            // Spawn task to handle mDNS events
            tokio::spawn(async move {
                loop {
                    match browse_handle.recv_timeout(Duration::from_secs(1)) {
                        Ok(event) => {
                            match event {
                                ServiceEvent::ServiceResolved(info) => {
                                    let addresses = info.get_addresses();
                                    if !addresses.is_empty() {
                                        let socket_addr = SocketAddr::new(
                                            IpAddr::V4(*addresses.iter().next().unwrap()),
                                            info.get_port()
                                        );
                                        
                                        // Try to parse the instance name as a UUID
                                        if let Ok(id) = Uuid::parse_str(info.get_fullname().split('.').next().unwrap_or("")) {
                                            let peer = DiscoveredPeer {
                                                id,
                                                addr: socket_addr,
                                                last_seen: std::time::Instant::now(),
                                            };
                                            
                                            // Add to peers if not already known
                                            let mut peers_guard = peers.write().await;
                                            if !peers_guard.contains_key(&id) {
                                                info!("Discovered peer: {} at {}", id, socket_addr);
                                                peers_guard.insert(id, peer);
                                                
                                                // Send discovery event
                                                if let Err(e) = event_tx.send(
                                                    DiscoveryEvent::PeerDiscovered(id, socket_addr)
                                                ).await {
                                                    error!("Failed to send discovery event: {}", e);
                                                }
                                            }
                                        }
                                    }
                                },
                                ServiceEvent::ServiceRemoved(fullname, _) => {
                                    // Extract the instance name (UUID) from the fullname
                                    let parts: Vec<&str> = fullname.split('.').collect();
                                    if parts.len() > 0 {
                                        if let Ok(id) = Uuid::parse_str(parts[0]) {
                                            let mut peers_guard = peers.write().await;
                                            if peers_guard.remove(&id).is_some() {
                                                info!("Peer lost: {}", id);
                                                
                                                // Send discovery event
                                                if let Err(e) = event_tx.send(
                                                    DiscoveryEvent::PeerLost(id)
                                                ).await {
                                                    error!("Failed to send discovery event: {}", e);
                                                }
                                            }
                                        }
                                    }
                                },
                                _ => {}
                            }
                        },
                        Err(_) => {
                            // Timeout is expected, continue
                        }
                    }
                }
            });
        }
        
        // Start UDP broadcast for discovery
        self.start_udp_broadcast().await?;
        
        // Start peer expiration task
        self.start_peer_expiration().await;
        
        Ok(rx)
    }
    
    /// Start UDP broadcast for discovery
    async fn start_udp_broadcast(&self) -> Result<(), DiscoveryError> {
        let socket_handler = self.socket_handler.clone();
        let protocol = self.protocol.clone();
        
        // Spawn task to periodically send discovery broadcasts
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(DISCOVERY_INTERVAL));
            
            loop {
                interval.tick().await;
                
                let handler = socket_handler.lock().await;
                let udp_addr = match handler.local_udp_addr() {
                    Ok(addr) => addr,
                    Err(e) => {
                        error!("Failed to get UDP address: {}", e);
                        continue;
                    }
                };
                
                // Create and send discovery announcement
                match protocol.create_announce(udp_addr) {
                    Ok(data) => {
                        // Try different broadcast approaches
                        // First try local subnet broadcast (replace last octet with 255)
                        let mut success = false;
                        
                        if let IpAddr::V4(local_ip) = udp_addr.ip() {
                            let octets = local_ip.octets();
                            if octets[0] != 127 { // Not loopback
                                // Create subnet broadcast address by setting last octet to 255
                                let subnet_broadcast = SocketAddr::new(
                                    IpAddr::V4(std::net::Ipv4Addr::new(
                                        octets[0], octets[1], octets[2], 255
                                    )),
                                    udp_addr.port()
                                );
                                
                                if let Ok(_) = handler.send_udp(&data, subnet_broadcast).await {
                                    debug!("Sent discovery broadcast to subnet {}", subnet_broadcast);
                                    success = true;
                                }
                            }
                        }
                        
                        // Fallback to global broadcast if subnet broadcast failed
                        if !success {
                            let broadcast_addr = SocketAddr::new(
                                IpAddr::V4("255.255.255.255".parse().unwrap()),
                                udp_addr.port()
                            );
                            
                            if let Err(e) = handler.send_udp(&data, broadcast_addr).await {
                                error!("Failed to send discovery broadcast: {}", e);
                            } else {
                                debug!("Sent global discovery broadcast");
                            }
                        }
                    },
                    Err(e) => {
                        error!("Failed to create discovery announcement: {}", e);
                    }
                }
            }
        });
        
        Ok(())
    }
    
    /// Start peer expiration task
    async fn start_peer_expiration(&self) {
        let peers = self.peers.clone();
        let event_tx = self.event_tx.clone();
        
        // Expiration timeout (3x discovery interval)
        let expiration_timeout = Duration::from_secs(DISCOVERY_INTERVAL * 3);
        
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(DISCOVERY_INTERVAL));
            
            loop {
                interval.tick().await;
                
                let now = std::time::Instant::now();
                let mut peers_to_remove = Vec::new();
                
                // Find expired peers
                {
                    let peers_guard = peers.read().await;
                    for (id, peer) in peers_guard.iter() {
                        if now.duration_since(peer.last_seen) > expiration_timeout {
                            peers_to_remove.push(*id);
                        }
                    }
                }
                
                // Remove expired peers
                if !peers_to_remove.is_empty() {
                    let mut peers_guard = peers.write().await;
                    for id in &peers_to_remove {
                        if peers_guard.remove(id).is_some() {
                            info!("Peer expired: {}", id);
                            
                            // Send discovery event
                            if let Some(tx) = &event_tx {
                                if let Err(e) = tx.send(DiscoveryEvent::PeerLost(*id)).await {
                                    error!("Failed to send discovery event: {}", e);
                                }
                            }
                        }
                    }
                }
            }
        });
    }
    
    /// Get a list of discovered peers
    pub async fn get_peers(&self) -> Vec<DiscoveredPeer> {
        let peers = self.peers.read().await;
        peers.values().cloned().collect()
    }
    
    /// Check if a peer is known
    pub async fn has_peer(&self, peer_id: &Uuid) -> bool {
        let peers = self.peers.read().await;
        peers.contains_key(peer_id)
    }
    
    /// Get a specific peer by ID
    pub async fn get_peer(&self, peer_id: &Uuid) -> Option<DiscoveredPeer> {
        let peers = self.peers.read().await;
        peers.get(peer_id).cloned()
    }
    
    /// Manually add a peer
    pub async fn add_peer(&self, id: Uuid, addr: SocketAddr) -> bool {
        let mut peers = self.peers.write().await;
        
        if !peers.contains_key(&id) {
            peers.insert(id, DiscoveredPeer {
                id,
                addr,
                last_seen: std::time::Instant::now(),
            });
            
            // Send discovery event
            if let Some(tx) = &self.event_tx {
                if let Err(e) = tx.send(DiscoveryEvent::PeerDiscovered(id, addr)).await {
                    error!("Failed to send discovery event: {}", e);
                }
            }
            
            true
        } else {
            false
        }
    }
    
    /// Update a peer's last seen time
    pub async fn update_peer(&self, id: &Uuid) {
        let mut peers = self.peers.write().await;
        
        if let Some(peer) = peers.get_mut(id) {
            peer.last_seen = std::time::Instant::now();
        }
    }

    pub async fn discover_peer(&self, peer_id: Uuid) -> Result<(), DiscoveryError> {
        // Create discovery query message
        let query = self.protocol.create_discovery_query()?;
        
        // Get socket handler and broadcast to subnet
        let handler = self.socket_handler.lock().await;
        
        // Create broadcast address
        let broadcast_addr = SocketAddr::new(
            IpAddr::V4("255.255.255.255".parse().unwrap()),
            self.config.udp_port
        );
        
        // Send the discovery query
        handler.send_udp(&query, broadcast_addr).await?;
        debug!("Sent discovery broadcast looking for peer {}", peer_id);
        
        Ok(())
    }
} 
