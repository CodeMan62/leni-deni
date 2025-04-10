use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::sync::RwLock;
use uuid::Uuid;
use anyhow::Result;
use tracing::{info, debug, error};
use sha2::{Sha256, Digest};

#[derive(Debug, Clone)]
pub struct TransferVerification {
    pub file_size: u64,
    pub checksum: String,
    pub chunks_verified: Vec<bool>,
    pub last_verified: SystemTime,
}

pub struct VerificationManager {
    transfers: Arc<RwLock<HashMap<Uuid, TransferVerification>>>,
}

impl VerificationManager {
    pub fn new() -> Self {
        Self {
            transfers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn start_verification(
        &self,
        transfer_id: Uuid,
        file_size: u64,
        initial_checksum: String,
    ) -> Result<()> {
        let mut transfers = self.transfers.write().await;
        transfers.insert(
            transfer_id,
            TransferVerification {
                file_size,
                checksum: initial_checksum,
                chunks_verified: vec![false; ((file_size + 1024 * 1024 - 1) / (1024 * 1024)) as usize],
                last_verified: SystemTime::now(),
            },
        );
        
        info!("Started verification for transfer {}", transfer_id);
        Ok(())
    }

    pub async fn verify_chunk(
        &self,
        transfer_id: Uuid,
        chunk_index: usize,
        chunk_data: &[u8],
    ) -> Result<bool> {
        let mut transfers = self.transfers.write().await;
        if let Some(verification) = transfers.get_mut(&transfer_id) {
            if chunk_index >= verification.chunks_verified.len() {
                return Err(anyhow::anyhow!("Invalid chunk index"));
            }

            // Calculate chunk checksum
            let mut hasher = Sha256::new();
            hasher.update(chunk_data);
            let _chunk_checksum = format!("{:x}", hasher.finalize());

            // TODO: Compare with expected chunk checksum
            verification.chunks_verified[chunk_index] = true;
            verification.last_verified = SystemTime::now();

            debug!("Verified chunk {} for transfer {}", chunk_index, transfer_id);
            Ok(true)
        } else {
            Err(anyhow::anyhow!("Transfer not found"))
        }
    }

    pub async fn verify_complete(&self, transfer_id: Uuid, final_checksum: &str) -> Result<bool> {
        let transfers = self.transfers.read().await;
        if let Some(verification) = transfers.get(&transfer_id) {
            let all_chunks_verified = verification.chunks_verified.iter().all(|&v| v);
            let checksum_matches = verification.checksum == final_checksum;
            
            if all_chunks_verified && checksum_matches {
                info!("Transfer {} verified successfully", transfer_id);
                Ok(true)
            } else {
                error!(
                    "Transfer {} verification failed: chunks_verified={}, checksum_matches={}",
                    transfer_id, all_chunks_verified, checksum_matches
                );
                Ok(false)
            }
        } else {
            Err(anyhow::anyhow!("Transfer not found"))
        }
    }

    pub async fn cleanup_expired(&self) -> Result<()> {
        let mut transfers = self.transfers.write().await;
        let now = SystemTime::now();
        transfers.retain(|_, verification| {
            if let Ok(duration) = now.duration_since(verification.last_verified) {
                duration < Duration::from_secs(3600)
            } else {
                false
            }
        });
        Ok(())
    }
}

impl Default for VerificationManager {
    fn default() -> Self {
        Self::new()
    }
} 
