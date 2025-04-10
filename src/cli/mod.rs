use clap::{Parser, Subcommand};
use std::path::PathBuf;
use uuid::Uuid;

pub mod commands;
pub mod ui;
pub mod menu;

pub use commands::handle_command;
pub use menu::InteractiveMenu;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Share a file with a peer
    Share {
        /// Path to the file to share
        file_path: PathBuf,
        /// Peer ID to share with
        peer_id: Uuid,
    },
    /// Receive a file from a peer
    Receive {
        /// Path to save the received file
        save_path: PathBuf,
        /// Peer ID to receive from
        peer_id: Uuid,
    },
    /// List discovered peers
    ListPeers,
    /// Show transfer progress
    Progress {
        /// Transfer ID to show progress for
        transfer_id: Uuid,
    },
    /// Launch interactive menu
    Interactive,
} 
