use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use rand::RngCore;
use std::sync::Arc;
use tokio::sync::Mutex;
use anyhow::Result;
use tracing::{info, debug};

pub struct EncryptionManager {
    key: Arc<Mutex<Option<Key>>>,
}

impl EncryptionManager {
    pub fn new() -> Self {
        Self {
            key: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn generate_key(&self) -> Result<Vec<u8>> {
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        let key = Key::from_slice(&key);
        
        let mut stored_key = self.key.lock().await;
        *stored_key = Some(*key);
        
        Ok(key.to_vec())
    }

    pub async fn set_key(&self, key_bytes: &[u8]) -> Result<()> {
        if key_bytes.len() != 32 {
            return Err(anyhow::anyhow!("Invalid key length"));
        }
        
        let key = Key::from_slice(key_bytes);
        let mut stored_key = self.key.lock().await;
        *stored_key = Some(*key);
        
        Ok(())
    }

    pub async fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        let key = self.key.lock().await;
        let key = key.as_ref().ok_or_else(|| anyhow::anyhow!("No encryption key set"))?;
        
        let cipher = ChaCha20Poly1305::new(key);
        let mut nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);
        let nonce = Nonce::from_slice(&nonce);
        
        let mut encrypted_data = cipher
            .encrypt(nonce, data)
            .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;
        
        // Prepend nonce to encrypted data
        let mut result = nonce.to_vec();
        result.append(&mut encrypted_data);
        
        Ok(result)
    }

    pub async fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < 12 {
            return Err(anyhow::anyhow!("Invalid encrypted data length"));
        }
        
        let key = self.key.lock().await;
        let key = key.as_ref().ok_or_else(|| anyhow::anyhow!("No encryption key set"))?;
        
        let cipher = ChaCha20Poly1305::new(key);
        let nonce = Nonce::from_slice(&data[..12]);
        let encrypted_data = &data[12..];
        
        let decrypted_data = cipher
            .decrypt(nonce, encrypted_data)
            .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;
        
        Ok(decrypted_data)
    }

    pub async fn encrypt_file_chunk(&self, chunk: &[u8]) -> Result<Vec<u8>> {
        self.encrypt(chunk).await
    }

    pub async fn decrypt_file_chunk(&self, chunk: &[u8]) -> Result<Vec<u8>> {
        self.decrypt(chunk).await
    }
}

impl Default for EncryptionManager {
    fn default() -> Self {
        Self::new()
    }
} 
