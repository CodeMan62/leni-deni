use std::path::PathBuf;
use uuid::Uuid;
use tracing::{info, error};
use anyhow::Result;
use std::collections::HashMap;
use tokio::sync::Mutex;
use std::sync::Arc;

use crate::network::{ConnectionManager, DiscoveryService};
use crate::transfer::{ProgressTracker, TransferStatus};
use crate::security::AuthenticationManager;

#[derive(Debug)]
struct PendingShare {
    file_path: PathBuf,
    share_id: Uuid,
}

pub struct CliInterface {
    pub connection_manager: ConnectionManager,
    pub discovery_service: DiscoveryService,
    pub progress_tracker: ProgressTracker,
    pub auth_manager: AuthenticationManager,
    pending_shares: Arc<Mutex<HashMap<Uuid, PendingShare>>>,
}

impl CliInterface {
    pub fn new(
        connection_manager: ConnectionManager,
        discovery_service: DiscoveryService,
        progress_tracker: ProgressTracker,
        auth_manager: AuthenticationManager,
    ) -> Self {
        Self {
            connection_manager,
            discovery_service,
            progress_tracker,
            auth_manager,
            pending_shares: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn add_peer(&self, peer_id: Uuid) -> Result<()> {
        info!("Attempting to add peer {}", peer_id);
        
        // First check if we already know this peer
        if self.discovery_service.has_peer(&peer_id).await {
            return Ok(());
        }
        
        // Broadcast discovery request with the specific peer ID
        self.discovery_service.discover_peer(peer_id).await?;
        
        // Wait a bit for discovery
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        
        // Check if peer was discovered
        if !self.discovery_service.has_peer(&peer_id).await {
            return Err(anyhow::anyhow!("Could not discover peer"));
        }
        
        Ok(())
    }

    pub async fn register_share(&self, file_path: PathBuf, share_id: Uuid) -> Result<()> {
        info!("Registering share for file {:?} with ID {}", file_path, share_id);
        
        // Verify file exists and is readable
        if !file_path.exists() {
            return Err(anyhow::anyhow!("File does not exist"));
        }
        
        // Store the pending share
        let mut shares = self.pending_shares.lock().await;
        shares.insert(share_id, PendingShare {
            file_path: file_path.clone(),
            share_id,
        });
        
        Ok(())
    }

    pub async fn receive_shared_file(&self, share_id: Uuid, save_path: PathBuf) -> Result<()> {
        info!("Attempting to receive shared file with ID {} to {:?}", share_id, save_path);
        
        // Try to discover the peer with this share ID
        self.add_peer(share_id).await?;
        
        // Get peer address
        let peer = self.discovery_service.get_peer(&share_id).await
            .ok_or_else(|| anyhow::anyhow!("Could not find peer with this Share ID"))?;
            
        // Connect to peer
        let _conn_id = self.connection_manager.connect_to(peer.addr).await?;
        
        // Request the file using the share ID
        let _conn = self.connection_manager.get_connection(_conn_id).await
            .ok_or_else(|| anyhow::anyhow!("Connection lost"))?;
            
        // TODO: Implement actual file transfer protocol
        // For now, we'll just create a progress tracker entry
        self.progress_tracker.create_transfer(
            share_id,
            save_path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            1000, // Dummy file size
        ).await;
        
        Ok(())
    }

    pub async fn share_file(&self, file_path: PathBuf, peer_id: Uuid) -> Result<()> {
        info!("Sharing file {:?} with peer {}", file_path, peer_id);
        
        // Verify peer exists
        if !self.discovery_service.has_peer(&peer_id).await {
            return Err(anyhow::anyhow!("Peer not found"));
        }
        
        // Get peer address
        let peer = self.discovery_service.get_peer(&peer_id).await
            .ok_or_else(|| anyhow::anyhow!("Peer not found"))?;
            
        // Connect to peer
        let _conn_id = self.connection_manager.connect_to(peer.addr).await?;
        
        // TODO: Implement file sharing logic
        
        Ok(())
    }

    pub async fn receive_file(&self, save_path: PathBuf, peer_id: Uuid) -> Result<()> {
        info!("Receiving file to {:?} from peer {}", save_path, peer_id);
        
        // Verify peer exists
        if !self.discovery_service.has_peer(&peer_id).await {
            return Err(anyhow::anyhow!("Peer not found"));
        }
        
        // Get peer address
        let peer = self.discovery_service.get_peer(&peer_id).await
            .ok_or_else(|| anyhow::anyhow!("Peer not found"))?;
            
        // Connect to peer
        let _conn_id = self.connection_manager.connect_to(peer.addr).await?;
        
        // TODO: Implement file receiving logic
        
        Ok(())
    }

    pub async fn list_peers(&self) -> Result<()> {
        let peers = self.discovery_service.get_peers().await;
        if peers.is_empty() {
            println!("No peers discovered");
            return Ok(());
        }
        
        println!("Discovered peers:");
        for peer in peers {
            println!("  - {} at {}", peer.id, peer.addr);
        }
        
        Ok(())
    }

    pub async fn show_progress(&self, transfer_id: Uuid) -> Result<()> {
        if let Some(progress) = self.progress_tracker.get_progress(transfer_id).await {
            match progress.status {
                TransferStatus::InProgress => {
                    let percentage = (progress.transferred_bytes as f64 / progress.file_size as f64) * 100.0;
                    println!(
                        "Transfer {}: {} ({:.2}%)",
                        transfer_id,
                        progress.file_name,
                        percentage
                    );
                }
                TransferStatus::Completed => {
                    println!("Transfer {}: {} (Completed)", transfer_id, progress.file_name);
                }
                TransferStatus::Failed(ref error) => {
                    println!("Transfer {}: {} (Failed: {})", transfer_id, progress.file_name, error);
                }
            }
        } else {
            println!("Transfer {} not found", transfer_id);
        }
        
        Ok(())
    }
} 
