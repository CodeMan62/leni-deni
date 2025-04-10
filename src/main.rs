mod network;
mod transfer;
mod storage;
mod security;
mod cli;
mod utils;

use tokio::time::{sleep, Duration};
use anyhow::Result;
use tracing::{info, Level};
use uuid::Uuid;
use clap::Parser;

use network::{ConnectionManager, DiscoveryService, NetworkConfig, Protocol, SocketHandler};
use transfer::{ProgressTracker, VerificationManager, FileChunker};
use storage::MetadataManager;
use security::{EncryptionManager, AuthenticationManager};
use cli::{Cli, Commands, handle_command};
use utils::setup_logging;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    setup_logging(Level::INFO).map_err(|e| anyhow::anyhow!("Logging setup failed: {}", e))?;
    
    info!("Starting leni-deni P2P application...");
    
    // Create unique node ID
    let node_id = Uuid::new_v4();
    info!("Node ID: {}", node_id);
    
    // Create network configuration
    let config = NetworkConfig::default();
    
    // Create protocol handler
    let protocol = Protocol::new(node_id);
    
    // Create and initialize the socket handler
    let mut socket_handler = SocketHandler::new(config.clone());
    socket_handler.init().await?;
    info!("Network sockets initialized");
    
    // Create and start the discovery service
    let mut discovery_service = DiscoveryService::new(
        node_id,
        socket_handler.clone(),
        protocol.clone(),
        config.clone()
    );
    
    discovery_service.init().await?;
    let mut discovery_rx = discovery_service.start().await?;
    info!("Peer discovery service started");
    
    // Create and start the connection manager
    let connection_manager = ConnectionManager::new(socket_handler, config.max_connections);
    connection_manager.start().await?;
    info!("Connection manager started");
    
    // Create transfer components
    let progress_tracker = ProgressTracker::new();
    let _verification_manager = VerificationManager::new();
    let _file_chunker = FileChunker::new();
    
    // Create storage components
    let _metadata_manager = MetadataManager::new();
    
    // Create security components
    let _encryption_manager = EncryptionManager::new();
    let auth_manager = AuthenticationManager::new()?;
    
    // Create CLI interface
    let cli_interface = cli::ui::CliInterface::new(
        connection_manager,
        discovery_service,
        progress_tracker,
        auth_manager,
    );
    
    // Parse CLI arguments
    let cli = Cli::parse();
    
    // Handle CLI command
    match cli.command {
        Commands::Interactive => {
            let menu = cli::InteractiveMenu::new(cli_interface);
            menu.run().await?;
        }
        _ => {
            handle_command(&cli_interface, cli.command).await?;
            
            // Spawn task to handle discovery events
            tokio::spawn(async move {
                while let Some(event) = discovery_rx.recv().await {
                    match event {
                        network::DiscoveryEvent::PeerDiscovered(id, addr) => {
                            info!("Discovered peer: {} at {}", id, addr);
                        },
                        network::DiscoveryEvent::PeerLost(id) => {
                            info!("Peer lost: {}", id);
                        }
                    }
                }
            });
            
            info!("P2P network ready! Press Ctrl+C to exit.");
            
            // Keep the application running
            loop {
                sleep(Duration::from_secs(1)).await;
            }
        }
    }
    
    Ok(())
}
