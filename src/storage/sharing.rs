use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::RwLock;
use uuid::Uuid;
use anyhow::Result;
use tracing::{info, debug};

#[derive(Debug, Clone)]
pub struct SharedResource {
    pub path: PathBuf,
    pub is_directory: bool,
    pub permissions: Permissions,
    pub owner_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct Permissions {
    pub read: bool,
    pub write: bool,
    pub share: bool,
}

impl Default for Permissions {
    fn default() -> Self {
        Self {
            read: true,
            write: false,
            share: false,
        }
    }
}

pub struct SharingManager {
    shared_resources: Arc<RwLock<HashMap<Uuid, SharedResource>>>,
}

impl SharingManager {
    pub fn new() -> Self {
        Self {
            shared_resources: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn share_file(&self, path: &Path, owner_id: Uuid) -> Result<Uuid> {
        let resource_id = Uuid::new_v4();
        let resource = SharedResource {
            path: path.to_path_buf(),
            is_directory: false,
            permissions: Permissions::default(),
            owner_id,
        };

        let mut resources = self.shared_resources.write().await;
        resources.insert(resource_id, resource.clone());
        
        info!("Shared file {:?} with ID {}", path, resource_id);
        Ok(resource_id)
    }

    pub async fn share_directory(&self, path: &Path, owner_id: Uuid) -> Result<Uuid> {
        let resource_id = Uuid::new_v4();
        let resource = SharedResource {
            path: path.to_path_buf(),
            is_directory: true,
            permissions: Permissions::default(),
            owner_id,
        };

        let mut resources = self.shared_resources.write().await;
        resources.insert(resource_id, resource.clone());
        
        info!("Shared directory {:?} with ID {}", path, resource_id);
        Ok(resource_id)
    }

    pub async fn unshare(&self, resource_id: Uuid, owner_id: Uuid) -> Result<()> {
        let mut resources = self.shared_resources.write().await;
        if let Some(resource) = resources.get(&resource_id) {
            if resource.owner_id != owner_id {
                return Err(anyhow::anyhow!("Not authorized to unshare this resource"));
            }
            resources.remove(&resource_id);
            info!("Unshared resource {}", resource_id);
        }
        Ok(())
    }

    pub async fn update_permissions(
        &self,
        resource_id: Uuid,
        owner_id: Uuid,
        permissions: Permissions,
    ) -> Result<()> {
        let mut resources = self.shared_resources.write().await;
        if let Some(resource) = resources.get_mut(&resource_id) {
            if resource.owner_id != owner_id {
                return Err(anyhow::anyhow!("Not authorized to update permissions"));
            }
            resource.permissions = permissions;
            info!("Updated permissions for resource {}", resource_id);
        }
        Ok(())
    }

    pub async fn get_shared_resource(&self, resource_id: Uuid) -> Option<SharedResource> {
        let resources = self.shared_resources.read().await;
        resources.get(&resource_id).cloned()
    }

    pub async fn list_shared_resources(&self, owner_id: Uuid) -> Vec<SharedResource> {
        let resources = self.shared_resources.read().await;
        resources
            .values()
            .filter(|r| r.owner_id == owner_id)
            .cloned()
            .collect()
    }

    pub async fn check_permission(
        &self,
        resource_id: Uuid,
        user_id: Uuid,
        permission: &str,
    ) -> bool {
        let resources = self.shared_resources.read().await;
        if let Some(resource) = resources.get(&resource_id) {
            if resource.owner_id == user_id {
                return true; // Owners have all permissions
            }
            match permission {
                "read" => resource.permissions.read,
                "write" => resource.permissions.write,
                "share" => resource.permissions.share,
                _ => false,
            }
        } else {
            false
        }
    }
}

impl Default for SharingManager {
    fn default() -> Self {
        Self::new()
    }
} 
