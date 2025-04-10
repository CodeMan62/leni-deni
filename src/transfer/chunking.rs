use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};
use anyhow::Result;
use tracing::{info, debug};
use sha2::{Sha256, Digest};

const DEFAULT_CHUNK_SIZE: usize = 1024 * 1024; // 1MB chunks

pub struct FileChunker {
    chunk_size: usize,
}

impl FileChunker {
    pub fn new() -> Self {
        Self {
            chunk_size: DEFAULT_CHUNK_SIZE,
        }
    }

    pub fn with_chunk_size(chunk_size: usize) -> Self {
        Self { chunk_size }
    }

    pub fn calculate_checksum(&self, file: &mut File) -> Result<String> {
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; self.chunk_size];
        let mut total_read = 0;

        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
            total_read += bytes_read;
        }

        // Reset file position
        file.seek(SeekFrom::Start(0))?;

        let checksum = format!("{:x}", hasher.finalize());
        debug!("Calculated checksum for {} bytes: {}", total_read, checksum);
        Ok(checksum)
    }

    pub fn read_chunk(&self, file: &mut File, offset: u64) -> Result<Option<Vec<u8>>> {
        file.seek(SeekFrom::Start(offset))?;
        let mut buffer = vec![0u8; self.chunk_size];
        let bytes_read = file.read(&mut buffer)?;

        if bytes_read == 0 {
            Ok(None)
        } else {
            Ok(Some(buffer[..bytes_read].to_vec()))
        }
    }

    pub fn get_chunk_count(&self, file_size: u64) -> u64 {
        (file_size + self.chunk_size as u64 - 1) / self.chunk_size as u64
    }

    pub fn get_chunk_size(&self) -> usize {
        self.chunk_size
    }
}

impl Default for FileChunker {
    fn default() -> Self {
        Self::new()
    }
} 
