use std::{
    fs::File,
    io::{self, Read, Write, Seek, SeekFrom},
    net::TcpStream,
    path::Path,
    sync::Arc,
};

use crate::network::protocol::{Message, MessageType, ProtocolError};
use crate::transfer::progress::{ProgressTracker, TransferStatus};

const DEFAULT_CHUNK_SIZE: usize = 1024 * 1024; // 1MB chunks

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TransferState {
    pub file_name: String,
    pub file_size: u64,
    pub transferred_bytes: u64,
    pub checksum: String,
}

pub struct FileTransfer {
    chunk_size: usize,
    progress_tracker: Arc<ProgressTracker>,
}

impl FileTransfer {
    pub fn new() -> Self {
        Self {
            chunk_size: DEFAULT_CHUNK_SIZE,
            progress_tracker: Arc::new(ProgressTracker::new()),
        }
    }

    pub fn with_chunk_size(chunk_size: usize) -> Self {
        Self {
            chunk_size,
            progress_tracker: Arc::new(ProgressTracker::new()),
        }
    }

    pub fn get_progress_tracker(&self) -> Arc<ProgressTracker> {
        self.progress_tracker.clone()
    }

    pub fn send_file(&self, stream: &mut TcpStream, file_path: &Path) -> Result<(), ProtocolError> {
        let mut file = File::open(file_path)?;
        let file_size = file.metadata()?.len();
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ProtocolError::InvalidMessage("Invalid file name".into()))?
            .to_string();

        // Start tracking progress
        let transfer_id = uuid::Uuid::new_v4();
        self.progress_tracker.blocking_start_transfer(
            transfer_id,
            file_name.clone(),
            file_size,
        );

        // Calculate file checksum
        let checksum = self.calculate_checksum(&mut file)?;
        file.seek(SeekFrom::Start(0))?;

        // Send file metadata
        let metadata = Message {
            version: 1,
            message_type: MessageType::FileMetadata,
            id: 0,
            sender_id: 0,
            data: serde_json::to_vec(&(file_name.clone(), file_size, checksum))?,
        };
        metadata.write_to_stream(stream)?;

        // Wait for acknowledgment or resumption request
        let response = Message::read_from_stream(stream)?;
        match response.message_type {
            MessageType::Ack => {
                // Start fresh transfer
                self.send_file_chunks(stream, &mut file, file_size, 0, transfer_id)?;
            }
            MessageType::FileRequest => {
                // Parse resumption request
                let offset: u64 = serde_json::from_slice(&response.data)?;
                if offset >= file_size {
                    return Err(ProtocolError::InvalidMessage("Invalid resumption offset".into()));
                }
                file.seek(SeekFrom::Start(offset))?;
                self.send_file_chunks(stream, &mut file, file_size, offset, transfer_id)?;
            }
            _ => return Err(ProtocolError::InvalidMessage("Unexpected response type".into())),
        }

        // Mark transfer as completed
        self.progress_tracker.blocking_complete_transfer(transfer_id);
        Ok(())
    }

    fn send_file_chunks(
        &self,
        stream: &mut TcpStream,
        file: &mut File,
        file_size: u64,
        start_offset: u64,
        transfer_id: uuid::Uuid,
    ) -> Result<(), ProtocolError> {
        let mut buffer = vec![0u8; self.chunk_size];
        let mut offset = start_offset;

        while offset < file_size {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }

            let chunk = Message {
                version: 1,
                message_type: MessageType::FileChunk,
                id: 0,
                sender_id: 0,
                data: buffer[..bytes_read].to_vec(),
            };
            chunk.write_to_stream(stream)?;

            // Update progress
            offset += bytes_read as u64;
            self.progress_tracker.blocking_update_progress(transfer_id, offset);

            // Wait for acknowledgment
            let ack = Message::read_from_stream(stream)?;
            if ack.message_type != MessageType::Ack {
                return Err(ProtocolError::InvalidMessage("Expected acknowledgment".into()));
            }
        }

        // Send completion message
        let complete = Message {
            version: 1,
            message_type: MessageType::FileComplete,
            id: 0,
            sender_id: 0,
            data: vec![],
        };
        complete.write_to_stream(stream)?;

        Ok(())
    }

    pub fn receive_file(&self, stream: &mut TcpStream, save_path: &Path) -> Result<(), ProtocolError> {
        // Receive file metadata
        let metadata = Message::read_from_stream(stream)?;
        if metadata.message_type != MessageType::FileMetadata {
            return Err(ProtocolError::InvalidMessage("Expected file metadata".into()));
        }

        let (file_name, file_size, expected_checksum): (String, u64, String) = serde_json::from_slice(&metadata.data)?;

        // Start tracking progress
        let transfer_id = uuid::Uuid::new_v4();
        self.progress_tracker.blocking_start_transfer(
            transfer_id,
            file_name.clone(),
            file_size,
        );

        let mut file = if save_path.exists() {
            // Check if we can resume
            let existing_size = save_path.metadata()?.len();
            if existing_size < file_size {
                // Calculate checksum of existing file
                let mut existing_file = File::open(save_path)?;
                let existing_checksum = self.calculate_checksum(&mut existing_file)?;
                
                if existing_checksum == expected_checksum {
                    // Send resumption request
                    let request = Message {
                        version: 1,
                        message_type: MessageType::FileRequest,
                        id: 0,
                        sender_id: 0,
                        data: serde_json::to_vec(&existing_size)?,
                    };
                    request.write_to_stream(stream)?;
                    
                    // Open file in append mode
                    File::options().append(true).open(save_path)?
                } else {
                    // Checksum mismatch, start fresh
                    let ack = Message {
                        version: 1,
                        message_type: MessageType::Ack,
                        id: 0,
                        sender_id: 0,
                        data: vec![],
                    };
                    ack.write_to_stream(stream)?;
                    File::create(save_path)?
                }
            } else {
                // File already complete
                let ack = Message {
                    version: 1,
                    message_type: MessageType::Ack,
                    id: 0,
                    sender_id: 0,
                    data: vec![],
                };
                ack.write_to_stream(stream)?;
                return Ok(());
            }
        } else {
            // New file
            let ack = Message {
                version: 1,
                message_type: MessageType::Ack,
                id: 0,
                sender_id: 0,
                data: vec![],
            };
            ack.write_to_stream(stream)?;
            File::create(save_path)?
        };

        // Receive file chunks
        let mut received_size = file.metadata()?.len();
        while received_size < file_size {
            let chunk = Message::read_from_stream(stream)?;
            if chunk.message_type != MessageType::FileChunk {
                return Err(ProtocolError::InvalidMessage("Expected file chunk".into()));
            }

            file.write_all(&chunk.data)?;
            received_size += chunk.data.len() as u64;

            // Update progress
            self.progress_tracker.blocking_update_progress(transfer_id, received_size);

            // Send acknowledgment
            let ack = Message {
                version: 1,
                message_type: MessageType::Ack,
                id: 0,
                sender_id: 0,
                data: vec![],
            };
            ack.write_to_stream(stream)?;
        }

        // Wait for completion message
        let complete = Message::read_from_stream(stream)?;
        if complete.message_type != MessageType::FileComplete {
            return Err(ProtocolError::InvalidMessage("Expected completion message".into()));
        }

        // Verify final checksum
        file.seek(SeekFrom::Start(0))?;
        let final_checksum = self.calculate_checksum(&mut file)?;
        if final_checksum != expected_checksum {
            return Err(ProtocolError::InvalidMessage("File checksum mismatch".into()));
        }

        // Mark transfer as completed
        self.progress_tracker.blocking_complete_transfer(transfer_id);
        Ok(())
    }

    fn calculate_checksum(&self, file: &mut File) -> Result<String, ProtocolError> {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; self.chunk_size];
        
        file.seek(SeekFrom::Start(0))?;
        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }
        file.seek(SeekFrom::Start(0))?;
        
        Ok(format!("{:x}", hasher.finalize()))
    }
}

impl Default for FileTransfer {
    fn default() -> Self {
        Self::new()
    }
} 
