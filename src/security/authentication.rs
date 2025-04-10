use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::sync::RwLock;
use uuid::Uuid;
use anyhow::Result;
use tracing::{info, debug};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use rand::RngCore;

#[derive(Debug, Clone)]
pub struct PeerCredentials {
    pub id: Uuid,
    pub public_key: Vec<u8>,
    pub shared_secret: Option<Vec<u8>>,
    pub last_seen: SystemTime,
}

pub struct AuthenticationManager {
    credentials: Arc<RwLock<HashMap<Uuid, PeerCredentials>>>,
    encryption: Arc<ChaCha20Poly1305>,
}

impl AuthenticationManager {
    pub fn new() -> Result<Self> {
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        let key = Key::from_slice(&key);
        let encryption = ChaCha20Poly1305::new(key);
        
        Ok(Self {
            credentials: Arc::new(RwLock::new(HashMap::new())),
            encryption: Arc::new(encryption),
        })
    }

    pub async fn register_peer(&self, peer_id: Uuid, public_key: Vec<u8>) -> Result<()> {
        let mut credentials = self.credentials.write().await;
        credentials.insert(
            peer_id,
            PeerCredentials {
                id: peer_id,
                public_key,
                shared_secret: None,
                last_seen: SystemTime::now(),
            },
        );
        
        info!("Registered peer {}", peer_id);
        Ok(())
    }

    pub async fn authenticate_peer(&self, peer_id: Uuid, challenge: &[u8]) -> Result<bool> {
        let credentials = self.credentials.read().await;
        if let Some(peer) = credentials.get(&peer_id) {
            // Check if peer has been seen recently
            if peer.last_seen.elapsed()? > Duration::from_secs(300) {
                return Ok(false);
            }
            
            // Verify challenge response
            if let Some(secret) = &peer.shared_secret {
                let mut nonce = [0u8; 12];
                rand::thread_rng().fill_bytes(&mut nonce);
                let nonce = Nonce::from_slice(&nonce);
                
                let cipher = ChaCha20Poly1305::new(Key::from_slice(secret));
                let _response = cipher
                    .encrypt(nonce, challenge)
                    .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;
                
                // TODO: Send response to peer for verification
                debug!("Generated authentication response for peer {}", peer_id);
                return Ok(true);
            }
        }
        
        Ok(false)
    }

    pub async fn set_shared_secret(&self, peer_id: Uuid, secret: Vec<u8>) -> Result<()> {
        let mut credentials = self.credentials.write().await;
        if let Some(peer) = credentials.get_mut(&peer_id) {
            peer.shared_secret = Some(secret);
            peer.last_seen = SystemTime::now();
            info!("Set shared secret for peer {}", peer_id);
        }
        Ok(())
    }

    pub async fn get_peer_credentials(&self, peer_id: Uuid) -> Option<PeerCredentials> {
        let credentials = self.credentials.read().await;
        credentials.get(&peer_id).cloned()
    }

    pub async fn remove_peer(&self, peer_id: Uuid) -> Result<()> {
        let mut credentials = self.credentials.write().await;
        credentials.remove(&peer_id);
        info!("Removed peer {}", peer_id);
        Ok(())
    }

    pub async fn cleanup_expired_peers(&self) -> Result<()> {
        let mut credentials = self.credentials.write().await;
        let now = SystemTime::now();
        credentials.retain(|_, peer| {
            if let Ok(duration) = now.duration_since(peer.last_seen) {
                duration < Duration::from_secs(300)
            } else {
                false
            }
        });
        Ok(())
    }
} 
