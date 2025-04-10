use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use anyhow::Result;
use tracing::{info, error};
use std::time::Duration;

use crate::network::{ConnectionManager, DiscoveryService, FileTransfer};
use crate::transfer::progress::TransferStatus;
use super::Commands;

pub struct CliHandler {
    connection_manager: Arc<Mutex<ConnectionManager>>,
    discovery_service: Arc<Mutex<DiscoveryService>>,
    file_transfer: Arc<Mutex<FileTransfer>>,
}

impl CliHandler {
    pub fn new(
        connection_manager: Arc<Mutex<ConnectionManager>>,
        discovery_service: Arc<Mutex<DiscoveryService>>,
        file_transfer: Arc<Mutex<FileTransfer>>,
    ) -> Self {
        Self {
            connection_manager,
            discovery_service,
            file_transfer,
        }
    }

    pub async fn handle_command(&self, command: Commands) -> Result<()> {
        match command {
            Commands::Share { file_path, peer_id } => {
                self.handle_share(file_path, peer_id).await?;
            }
            Commands::Receive { save_path, peer_id } => {
                self.handle_receive(save_path, peer_id).await?;
            }
            Commands::ListPeers => {
                self.handle_list_peers().await?;
            }
            Commands::Progress { transfer_id } => {
                self.handle_progress(transfer_id).await?;
            }
        }
        Ok(())
    }

    async fn handle_share(&self, file_path: PathBuf, peer_id: Uuid) -> Result<()> {
        info!("Sharing file {:?} with peer {}", file_path, peer_id);
        
        // Get peer address from discovery service
        let discovery = self.discovery_service.lock().await;
        let peer = discovery.get_peer(&peer_id).await
            .ok_or_else(|| anyhow::anyhow!("Peer not found"))?;
        
        // Connect to peer
        let conn_manager = self.connection_manager.lock().await;
        let conn_id = conn_manager.connect_to(peer.addr).await?;
        
        // Get connection
        let conn = conn_manager.get_connection(conn_id).await
            .ok_or_else(|| anyhow::anyhow!("Connection not found"))?;
        
        // Start file transfer
        let transfer = self.file_transfer.lock().await;
        let progress_tracker = transfer.get_progress_tracker();
        let transfer_id = uuid::Uuid::new_v4();
        
        // Spawn progress display task
        let display_tracker = progress_tracker.clone();
        tokio::spawn(async move {
            loop {
                if let Some(progress) = display_tracker.get_progress(transfer_id).await {
                    match progress.status {
                        TransferStatus::InProgress => {
                            let percentage = (progress.transferred_bytes as f64 / progress.file_size as f64) * 100.0;
                            println!(
                                "Transferring {}: {}/{} bytes ({:.2}%)",
                                progress.file_name,
                                progress.transferred_bytes,
                                progress.file_size,
                                percentage
                            );
                        }
                        TransferStatus::Completed => {
                            println!("Transfer completed successfully");
                            break;
                        }
                        TransferStatus::Failed(ref error) => {
                            println!("Transfer failed: {}", error);
                            break;
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        });
        
        transfer.send_file(&mut conn.stream, &file_path)?;
        
        info!("File transfer completed successfully");
        Ok(())
    }

    async fn handle_receive(&self, save_path: PathBuf, peer_id: Uuid) -> Result<()> {
        info!("Receiving file to {:?} from peer {}", save_path, peer_id);
        
        // Get peer address from discovery service
        let discovery = self.discovery_service.lock().await;
        let peer = discovery.get_peer(&peer_id).await
            .ok_or_else(|| anyhow::anyhow!("Peer not found"))?;
        
        // Connect to peer
        let conn_manager = self.connection_manager.lock().await;
        let conn_id = conn_manager.connect_to(peer.addr).await?;
        
        // Get connection
        let conn = conn_manager.get_connection(conn_id).await
            .ok_or_else(|| anyhow::anyhow!("Connection not found"))?;
        
        // Start file transfer
        let transfer = self.file_transfer.lock().await;
        let progress_tracker = transfer.get_progress_tracker();
        let transfer_id = uuid::Uuid::new_v4();
        
        // Spawn progress display task
        let display_tracker = progress_tracker.clone();
        tokio::spawn(async move {
            loop {
                if let Some(progress) = display_tracker.get_progress(transfer_id).await {
                    match progress.status {
                        TransferStatus::InProgress => {
                            let percentage = (progress.transferred_bytes as f64 / progress.file_size as f64) * 100.0;
                            println!(
                                "Receiving {}: {}/{} bytes ({:.2}%)",
                                progress.file_name,
                                progress.transferred_bytes,
                                progress.file_size,
                                percentage
                            );
                        }
                        TransferStatus::Completed => {
                            println!("Transfer completed successfully");
                            break;
                        }
                        TransferStatus::Failed(ref error) => {
                            println!("Transfer failed: {}", error);
                            break;
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        });
        
        transfer.receive_file(&mut conn.stream, &save_path)?;
        
        info!("File transfer completed successfully");
        Ok(())
    }

    async fn handle_list_peers(&self) -> Result<()> {
        let discovery = self.discovery_service.lock().await;
        let peers = discovery.get_peers().await;
        
        if peers.is_empty() {
            println!("No peers discovered");
        } else {
            println!("Discovered peers:");
            for peer in peers {
                println!("  - {} at {}", peer.id, peer.addr);
            }
        }
        
        Ok(())
    }

    async fn handle_progress(&self, transfer_id: Uuid) -> Result<()> {
        let transfer = self.file_transfer.lock().await;
        let progress_tracker = transfer.get_progress_tracker();
        
        if let Some(progress) = progress_tracker.get_progress(transfer_id).await {
            match progress.status {
                TransferStatus::InProgress => {
                    let percentage = (progress.transferred_bytes as f64 / progress.file_size as f64) * 100.0;
                    println!(
                        "Transfer {}: {}/{} bytes ({:.2}%)",
                        progress.file_name,
                        progress.transferred_bytes,
                        progress.file_size,
                        percentage
                    );
                }
                TransferStatus::Completed => {
                    println!("Transfer {} completed successfully", progress.file_name);
                }
                TransferStatus::Failed(ref error) => {
                    println!("Transfer {} failed: {}", progress.file_name, error);
                }
            }
        } else {
            println!("No transfer found with ID {}", transfer_id);
        }
        
        Ok(())
    }
} 
