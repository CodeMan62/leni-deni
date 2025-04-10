use anyhow::Result;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use thiserror::Error;
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use super::NetworkConfig;

#[derive(Debug, Error)]
pub enum SocketError {
    #[error("Failed to bind to address: {0}")]
    BindError(String),

    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Wrapper for UDP and TCP sockets
#[derive(Debug)]
pub struct SocketHandler {
    tcp_listener: Option<TcpListener>,
    udp_socket: Option<Arc<UdpSocket>>,
    config: NetworkConfig,
}

// Manually implement Clone for SocketHandler
impl Clone for SocketHandler {
    fn clone(&self) -> Self {
        // We clone the config and UDP socket if available
        Self {
            tcp_listener: None,
            udp_socket: self.udp_socket.clone(),
            config: self.config.clone(),
        }
    }
}

/// Message type for internal communication
#[derive(Debug)]
pub enum SocketMessage {
    TcpConnection(tokio::net::TcpStream, SocketAddr),
    UdpData(Vec<u8>, SocketAddr),
}

impl SocketHandler {
    /// Create a new socket handler with the given configuration
    pub fn new(config: NetworkConfig) -> Self {
        Self {
            tcp_listener: None,
            udp_socket: None,
            config,
        }
    }

    /// Initialize TCP listener
    pub async fn init_tcp(&mut self) -> Result<(), SocketError> {
        let addr = SocketAddr::new(IpAddr::from([0, 0, 0, 0]), self.config.tcp_port);

        debug!("Binding TCP listener to {}", addr);
        let listener = TcpListener::bind(addr).await?;
        info!("TCP listener bound to {}", listener.local_addr()?);

        self.tcp_listener = Some(listener);
        Ok(())
    }

    /// Initialize UDP socket
    pub async fn init_udp(&mut self) -> Result<(), SocketError> {
        let addr = SocketAddr::new(IpAddr::from([0, 0, 0, 0]), self.config.udp_port);

        debug!("Binding UDP socket to {}", addr);
        let socket = UdpSocket::bind(addr).await?;
        
        // Enable broadcast for this socket
        socket.set_broadcast(true)?;
        
        info!("UDP socket bound to {}", socket.local_addr()?);

        self.udp_socket = Some(Arc::new(socket));
        Ok(())
    }

    /// Initialize both TCP and UDP sockets
    pub async fn init(&mut self) -> Result<(), SocketError> {
        self.init_tcp().await?;
        self.init_udp().await?;
        Ok(())
    }

    /// Start listening for incoming connections and messages
    /// Returns a channel receiver for incoming messages
    pub fn start_listening(&self) -> Result<mpsc::Receiver<SocketMessage>, SocketError> {
        // Verify both sockets are initialized
        if self.tcp_listener.is_none() {
            return Err(SocketError::BindError(
                "TCP listener not initialized".to_string(),
            ));
        }

        if self.udp_socket.is_none() {
            return Err(SocketError::BindError(
                "UDP socket not initialized".to_string(),
            ));
        }

        let (tx, rx) = mpsc::channel::<SocketMessage>(100);

        // Clone the channel sender for each task
        let tcp_tx = tx.clone();
        let udp_tx = tx;

        // Clone for moving into tasks
        let tcp_addr = self.tcp_listener.as_ref().unwrap().local_addr()?;
        let udp_socket = Arc::clone(self.udp_socket.as_ref().unwrap());

        // Spawn TCP listener task - we'll need to connect to the same address
        tokio::spawn(async move {
            // Create a new listener in the task
            if let Ok(tcp_listener) = TcpListener::bind(tcp_addr).await {
                loop {
                    match tcp_listener.accept().await {
                        Ok((socket, addr)) => {
                            debug!("Accepted TCP connection from {}", addr);
                            if let Err(e) = tcp_tx
                                .send(SocketMessage::TcpConnection(socket, addr))
                                .await
                            {
                                error!("Failed to send TCP connection: {}", e);
                            }
                        }
                        Err(e) => {
                            error!("Failed to accept TCP connection: {}", e);
                        }
                    }
                }
            }
        });

        // Spawn UDP receiver task
        tokio::spawn(async move {
            let mut buf = [0u8; 65536]; // Max UDP packet size

            loop {
                match udp_socket.recv_from(&mut buf).await {
                    Ok((len, addr)) => {
                        let data = buf[..len].to_vec();
                        debug!("Received {} bytes from {} via UDP", len, addr);
                        if let Err(e) = udp_tx.send(SocketMessage::UdpData(data, addr)).await {
                            error!("Failed to send UDP data: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("Failed to receive UDP data: {}", e);
                    }
                }
            }
        });

        Ok(rx)
    }

    /// Send data over UDP to a specific address
    pub async fn send_udp(&self, data: &[u8], addr: SocketAddr) -> Result<usize, SocketError> {
        let socket = self
            .udp_socket
            .as_ref()
            .ok_or_else(|| SocketError::BindError("UDP socket not initialized".to_string()))?;

        let sent = socket.send_to(data, addr).await?;
        debug!("Sent {} bytes to {} via UDP", sent, addr);

        Ok(sent)
    }

    /// Get the local TCP address
    pub fn local_tcp_addr(&self) -> Result<SocketAddr, SocketError> {
        self.tcp_listener
            .as_ref()
            .ok_or_else(|| SocketError::BindError("TCP listener not initialized".to_string()))
            .and_then(|listener| listener.local_addr().map_err(|e| e.into()))
    }

    /// Get the local UDP address
    pub fn local_udp_addr(&self) -> Result<SocketAddr, SocketError> {
        self.udp_socket
            .as_ref()
            .ok_or_else(|| SocketError::BindError("UDP socket not initialized".to_string()))
            .and_then(|socket| socket.local_addr().map_err(|e| e.into()))
    }

    /// Check if the socket handler is initialized
    pub fn get_status(&self) -> Option<(bool, bool)> {
        if self.tcp_listener.is_none() && self.udp_socket.is_none() {
            None
        } else {
            Some((self.tcp_listener.is_some(), self.udp_socket.is_some()))
        }
    }
}
