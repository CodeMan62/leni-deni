use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::sync::RwLock;
use uuid::Uuid;
use anyhow::Result;
use tracing::{info, debug};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub file_name: String,
    pub file_size: u64,
    pub checksum: String,
    pub created_at: SystemTime,
    pub modified_at: SystemTime,
    pub permissions: u32,
    pub shared_with: Vec<Uuid>,
}

pub struct MetadataManager {
    metadata: Arc<RwLock<HashMap<Uuid, FileMetadata>>>,
}

impl MetadataManager {
    pub fn new() -> Self {
        Self {
            metadata: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn add_metadata(&self, file_id: Uuid, metadata: FileMetadata) -> Result<()> {
        let mut metadata_map = self.metadata.write().await;
        metadata_map.insert(file_id, metadata);
        info!("Added metadata for file {}", file_id);
        Ok(())
    }

    pub async fn get_metadata(&self, file_id: Uuid) -> Option<FileMetadata> {
        let metadata_map = self.metadata.read().await;
        metadata_map.get(&file_id).cloned()
    }

    pub async fn update_metadata(&self, file_id: Uuid, metadata: FileMetadata) -> Result<()> {
        let mut metadata_map = self.metadata.write().await;
        if metadata_map.contains_key(&file_id) {
            metadata_map.insert(file_id, metadata);
            info!("Updated metadata for file {}", file_id);
            Ok(())
        } else {
            Err(anyhow::anyhow!("File not found"))
        }
    }

    pub async fn remove_metadata(&self, file_id: Uuid) -> Result<()> {
        let mut metadata_map = self.metadata.write().await;
        if metadata_map.remove(&file_id).is_some() {
            info!("Removed metadata for file {}", file_id);
            Ok(())
        } else {
            Err(anyhow::anyhow!("File not found"))
        }
    }

    pub async fn cleanup_expired(&self) -> Result<()> {
        let mut metadata_map = self.metadata.write().await;
        let now = SystemTime::now();
        metadata_map.retain(|_, metadata| {
            if let Ok(duration) = now.duration_since(metadata.modified_at) {
                duration < Duration::from_secs(3600)
            } else {
                false
            }
        });
        Ok(())
    }
}

impl Default for MetadataManager {
    fn default() -> Self {
        Self::new()
    }
} 
