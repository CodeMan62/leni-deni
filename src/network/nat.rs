use std::{
    net::{SocketAddr, UdpSocket},
    time::Duration,
};
use anyhow::Result;
use tracing::{info, debug, error};
use tokio::net::UdpSocket as TokioUdpSocket;

pub struct NatTraversal {
    stun_servers: Vec<String>,
    hole_punching_enabled: bool,
}

impl NatTraversal {
    pub fn new(stun_servers: Vec<String>) -> Self {
        Self {
            stun_servers,
            hole_punching_enabled: true,
        }
    }

    pub async fn get_public_address(&self) -> Result<SocketAddr> {
        for server in &self.stun_servers {
            match self.query_stun_server(server).await {
                Ok(addr) => {
                    info!("Got public address from STUN server: {}", addr);
                    return Ok(addr);
                }
                Err(e) => {
                    error!("Failed to get address from STUN server {}: {}", server, e);
                    continue;
                }
            }
        }
        Err(anyhow::anyhow!("All STUN servers failed"))
    }

    async fn query_stun_server(&self, server: &str) -> Result<SocketAddr> {
        let socket = TokioUdpSocket::bind("0.0.0.0:0").await?;
        socket.connect(server).await?;

        // Send STUN binding request
        let request = [0x00, 0x01, 0x00, 0x00]; // STUN binding request
        socket.send(&request).await?;

        // Wait for response
        let mut buffer = [0u8; 1024];
        let len = tokio::time::timeout(Duration::from_secs(5), socket.recv(&mut buffer)).await??;

        // Parse STUN response
        if len >= 20 && buffer[0] == 0x01 && buffer[1] == 0x01 {
            let port = u16::from_be_bytes([buffer[2], buffer[3]]);
            let ip = [buffer[4], buffer[5], buffer[6], buffer[7]];
            Ok(SocketAddr::from((ip, port)))
        } else {
            Err(anyhow::anyhow!("Invalid STUN response"))
        }
    }

    pub async fn perform_hole_punching(
        &self,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
    ) -> Result<()> {
        if !self.hole_punching_enabled {
            return Ok(());
        }

        let socket = TokioUdpSocket::bind(local_addr).await?;
        
        // Send packets to create NAT mapping
        for _ in 0..3 {
            socket.send_to(&[0u8; 1], remote_addr).await?;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        info!("Performed hole punching between {} and {}", local_addr, remote_addr);
        Ok(())
    }

    pub fn enable_hole_punching(&mut self, enabled: bool) {
        self.hole_punching_enabled = enabled;
    }
}

impl Default for NatTraversal {
    fn default() -> Self {
        Self::new(vec![
            "stun.l.google.com:19302".to_string(),
            "stun1.l.google.com:19302".to_string(),
            "stun2.l.google.com:19302".to_string(),
        ])
    }
} 
