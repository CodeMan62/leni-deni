use std::path::PathBuf;
use uuid::Uuid;
use tracing::{info, error};
use anyhow::Result;

use crate::cli::ui::CliInterface;
use crate::cli::Commands;

pub async fn handle_command(cli: &CliInterface, command: Commands) -> Result<()> {
    match command {
        Commands::Share { file_path, peer_id } => {
            cli.share_file(file_path, peer_id).await?;
        }
        Commands::Receive { save_path, peer_id } => {
            cli.receive_file(save_path, peer_id).await?;
        }
        Commands::ListPeers => {
            cli.list_peers().await?;
        }
        Commands::Progress { transfer_id } => {
            cli.show_progress(transfer_id).await?;
        }
        Commands::Interactive => {
            // This is handled in main.rs
        }
    }
    
    Ok(())
} 
