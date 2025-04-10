use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::sync::RwLock;
use uuid::Uuid;
use anyhow::Result;
use tracing::{info, debug, warn};

use crate::network::protocol::PeerId;

#[derive(Debug, Clone)]
pub struct SharedResourceInfo {
    pub resource_id: Uuid,
    pub owner_id: PeerId,
    pub path: String,
    pub is_directory: bool,
    pub size: u64,
    pub last_updated: SystemTime,
    pub replicas: HashSet<PeerId>,
}

pub struct DistributedTracker {
    resources: Arc<RwLock<HashMap<Uuid, SharedResourceInfo>>>,
    peer_resources: Arc<RwLock<HashMap<PeerId, HashSet<Uuid>>>>,
}

impl DistributedTracker {
    pub fn new() -> Self {
        Self {
            resources: Arc::new(RwLock::new(HashMap::new())),
            peer_resources: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn add_resource(&self, info: SharedResourceInfo) {
        let mut resources = self.resources.write().await;
        let mut peer_resources = self.peer_resources.write().await;

        resources.insert(info.resource_id, info.clone());
        
        // Update peer's resource set
        peer_resources
            .entry(info.owner_id)
            .or_default()
            .insert(info.resource_id);
            
        // Add to replica sets
        for peer_id in &info.replicas {
            peer_resources
                .entry(*peer_id)
                .or_default()
                .insert(info.resource_id);
        }

        info!(
            "Added resource {} owned by {} with {} replicas",
            info.resource_id,
            info.owner_id,
            info.replicas.len()
        );
    }

    pub async fn update_resource(&self, resource_id: Uuid, updates: SharedResourceInfo) -> Result<()> {
        let mut resources = self.resources.write().await;
        if let Some(existing) = resources.get(&resource_id) {
            let mut updated = existing.clone();
            updated.path = updates.path;
            updated.is_directory = updates.is_directory;
            updated.size = updates.size;
            updated.last_updated = SystemTime::now();
            updated.replicas = updates.replicas;
            
            resources.insert(resource_id, updated);
            info!("Updated resource {}", resource_id);
            Ok(())
        } else {
            warn!("Attempted to update non-existent resource {}", resource_id);
            Err(anyhow::anyhow!("Resource not found"))
        }
    }

    pub async fn remove_resource(&self, resource_id: Uuid) {
        let mut resources = self.resources.write().await;
        let mut peer_resources = self.peer_resources.write().await;

        if let Some(info) = resources.remove(&resource_id) {
            // Remove from owner's set
            if let Some(peer_set) = peer_resources.get_mut(&info.owner_id) {
                peer_set.remove(&resource_id);
            }
            
            // Remove from replica sets
            for peer_id in &info.replicas {
                if let Some(peer_set) = peer_resources.get_mut(peer_id) {
                    peer_set.remove(&resource_id);
                }
            }
            
            info!("Removed resource {}", resource_id);
        }
    }

    pub async fn get_resource(&self, resource_id: Uuid) -> Option<SharedResourceInfo> {
        let resources = self.resources.read().await;
        resources.get(&resource_id).cloned()
    }

    pub async fn get_peer_resources(&self, peer_id: PeerId) -> Vec<SharedResourceInfo> {
        let resources = self.resources.read().await;
        let peer_resources = self.peer_resources.read().await;
        
        peer_resources
            .get(&peer_id)
            .map(|resource_ids| {
                resource_ids
                    .iter()
                    .filter_map(|id| resources.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn cleanup_expired(&self, max_age: Duration) {
        let now = SystemTime::now();
        let mut resources = self.resources.write().await;
        let mut peer_resources = self.peer_resources.write().await;

        let expired: Vec<Uuid> = resources
            .iter()
            .filter(|(_, info)| {
                now.duration_since(info.last_updated)
                    .map(|d| d > max_age)
                    .unwrap_or(false)
            })
            .map(|(id, _)| *id)
            .collect();

        for resource_id in expired {
            if let Some(info) = resources.remove(&resource_id) {
                // Remove from owner's set
                if let Some(peer_set) = peer_resources.get_mut(&info.owner_id) {
                    peer_set.remove(&resource_id);
                }
                
                // Remove from replica sets
                for peer_id in &info.replicas {
                    if let Some(peer_set) = peer_resources.get_mut(peer_id) {
                        peer_set.remove(&resource_id);
                    }
                }
                
                debug!("Cleaned up expired resource {}", resource_id);
            }
        }
    }
}

impl Default for DistributedTracker {
    fn default() -> Self {
        Self::new()
    }
} 
