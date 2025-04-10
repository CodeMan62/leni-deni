use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use tracing::{info, debug};

#[derive(Debug, Clone)]
pub struct TransferProgress {
    pub file_name: String,
    pub file_size: u64,
    pub transferred_bytes: u64,
    pub status: TransferStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TransferStatus {
    InProgress,
    Completed,
    Failed(String),
}

pub struct ProgressTracker {
    transfers: Arc<RwLock<HashMap<Uuid, TransferProgress>>>,
}

impl ProgressTracker {
    pub fn new() -> Self {
        Self {
            transfers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn start_transfer(&self, transfer_id: Uuid, file_name: String, file_size: u64) {
        let mut transfers = self.transfers.write().await;
        let file_name_clone = file_name.clone();
        transfers.insert(
            transfer_id,
            TransferProgress {
                file_name,
                file_size,
                transferred_bytes: 0,
                status: TransferStatus::InProgress,
            },
        );
        info!("Started tracking transfer {} for file {}", transfer_id, file_name_clone);
    }

    pub async fn update_progress(&self, transfer_id: Uuid, transferred_bytes: u64) {
        let mut transfers = self.transfers.write().await;
        if let Some(progress) = transfers.get_mut(&transfer_id) {
            progress.transferred_bytes = transferred_bytes;
            debug!(
                "Transfer {}: {}/{} bytes ({:.2}%)",
                transfer_id,
                transferred_bytes,
                progress.file_size,
                (transferred_bytes as f64 / progress.file_size as f64) * 100.0
            );
        }
    }

    pub async fn complete_transfer(&self, transfer_id: Uuid) {
        let mut transfers = self.transfers.write().await;
        if let Some(progress) = transfers.get_mut(&transfer_id) {
            progress.status = TransferStatus::Completed;
            progress.transferred_bytes = progress.file_size;
            info!("Transfer {} completed successfully", transfer_id);
        }
    }

    pub async fn fail_transfer(&self, transfer_id: Uuid, error: String) {
        let mut transfers = self.transfers.write().await;
        if let Some(progress) = transfers.get_mut(&transfer_id) {
            progress.status = TransferStatus::Failed(error.clone());
            info!("Transfer {} failed: {}", transfer_id, error);
        }
    }

    pub async fn get_progress(&self, transfer_id: Uuid) -> Option<TransferProgress> {
        let transfers = self.transfers.read().await;
        transfers.get(&transfer_id).cloned()
    }

    pub async fn cleanup(&self, transfer_id: Uuid) {
        let mut transfers = self.transfers.write().await;
        transfers.remove(&transfer_id);
        debug!("Cleaned up transfer {}", transfer_id);
    }

    pub async fn create_transfer(&self, transfer_id: Uuid, file_name: String, file_size: u64) {
        self.start_transfer(transfer_id, file_name, file_size).await;
    }
}

impl Default for ProgressTracker {
    fn default() -> Self {
        Self::new()
    }
} 
