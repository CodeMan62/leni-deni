use std::path::PathBuf;
use anyhow::Result;
use dialoguer::{theme::ColorfulTheme, Select, Input, Confirm};
use indicatif::{ProgressBar, ProgressStyle};
use uuid::Uuid;
use tracing::{info, error};
use crossterm::{
    terminal::{Clear, ClearType},
    ExecutableCommand,
};

use crate::cli::ui::CliInterface;
use crate::network::DiscoveryEvent;

pub struct InteractiveMenu {
    cli_interface: CliInterface,
}

impl InteractiveMenu {
    pub fn new(cli_interface: CliInterface) -> Self {
        Self {
            cli_interface,
        }
    }

    /// Clear the terminal screen
    fn clear_screen() {
        // Use crossterm to clear the screen
        let mut stdout = std::io::stdout();
        let _ = stdout.execute(Clear(ClearType::All));
        let _ = stdout.execute(crossterm::cursor::MoveTo(0, 0));
    }

    /// Get user input with screen clearing
    fn get_input(&self, prompt: &str) -> Result<String> {
        // Clear the screen first to remove any debug output
        Self::clear_screen();
        
        // Now get input
        let input: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt(prompt)
            .interact_text()?;
            
        Ok(input)
    }

    pub async fn run(&self) -> Result<()> {
        let theme = ColorfulTheme::default();
        
        loop {
            // Clear screen before showing menu
            Self::clear_screen();
            
            let menu_items = vec![
                "📤 Send File (Generate Share ID)",
                "📥 Receive File (Using Share ID)",
                "👥 Add Peer",
                "📋 List Peers",
                "📊 Show Transfer Progress",
                "❌ Quit",
            ];
            
            let selection = Select::with_theme(&theme)
                .with_prompt("Choose an option")
                .default(0)
                .items(&menu_items)
                .interact()?;
                
            match selection {
                0 => self.share_file_with_id().await?,
                1 => self.receive_file_with_id().await?,
                2 => self.add_peer().await?,
                3 => self.list_peers().await?,
                4 => self.show_progress().await?,
                5 => break,
                _ => unreachable!(),
            }
            
            println!("\nPress Enter to continue...");
            let _ = std::io::stdin().read_line(&mut String::new());
        }
        
        Ok(())
    }

    async fn add_peer(&self) -> Result<()> {
        println!("\n👥 Add New Peer");
        
        // Get peer ID with screen clearing
        let peer_id_str = self.get_input("Enter the peer's Share ID")?;
        
        let peer_id = match Uuid::parse_str(&peer_id_str) {
            Ok(id) => id,
            Err(_) => {
                Self::clear_screen();
                println!("❌ Invalid Share ID format");
                return Ok(());
            }
        };

        // Show spinner while attempting to connect
        Self::clear_screen();
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
                .template("{spinner} {msg}")?,
        );
        spinner.set_message("Attempting to connect to peer...");

        // Try to discover and connect to the peer
        match self.cli_interface.add_peer(peer_id).await {
            Ok(_) => {
                spinner.finish_with_message("✅ Successfully added peer!");
            }
            Err(e) => {
                spinner.finish_with_message(format!("❌ Failed to add peer: {}", e));
            }
        }

        Ok(())
    }
    
    async fn list_peers(&self) -> Result<()> {
        println!("\n📡 Discovering peers...");
        
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
                .template("{spinner} {msg}")?,
        );
        spinner.set_message("Searching for peers on the network...");
        
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        spinner.finish_and_clear();
        
        let peers = self.cli_interface.discovery_service.get_peers().await;
        
        if peers.is_empty() {
            println!("❌ No peers discovered on the network");
        } else {
            println!("✅ Connected peers:");
            for (i, peer) in peers.iter().enumerate() {
                println!("  {}. {} at {}", i + 1, peer.id, peer.addr);
            }
        }
        
        Ok(())
    }
    
    async fn share_file_with_id(&self) -> Result<()> {
        println!("\n📤 Share File");
        
        // Get file path from user with screen clearing
        let file_path = self.get_input("Enter the path to the file you want to share")?;
        let path = PathBuf::from(file_path);
        
        if !path.exists() {
            println!("❌ File does not exist: {:?}", path);
            return Ok(());
        }

        // Generate a new Share ID
        let share_id = Uuid::new_v4();
        
        // Register the share
        match self.cli_interface.register_share(path.clone(), share_id).await {
            Ok(_) => {
                Self::clear_screen();
                println!("\n✅ File ready to share!");
                println!("📋 Share ID: {}", share_id);
                println!("\nShare this ID with the person you want to send the file to.");
                println!("They should use the 'Receive File' option and enter this Share ID.");
            }
            Err(e) => {
                Self::clear_screen();
                println!("❌ Failed to prepare file sharing: {}", e);
            }
        }
        
        Ok(())
    }
    
    async fn receive_file_with_id(&self) -> Result<()> {
        println!("\n📥 Receive File");
        
        // Get Share ID from user with screen clearing
        let share_id_str = self.get_input("Enter the Share ID provided by the sender")?;
        
        let share_id = match Uuid::parse_str(&share_id_str) {
            Ok(id) => id,
            Err(_) => {
                Self::clear_screen();
                println!("❌ Invalid Share ID format");
                return Ok(());
            }
        };
        
        // Get save path from user with screen clearing
        let save_path = self.get_input("Enter the path to save the received file")?;
        let path = PathBuf::from(save_path);
        
        // Confirm before proceeding
        Self::clear_screen();
        let confirm = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("Receive file using Share ID {} and save to {:?}?", share_id, path))
            .default(true)
            .interact()?;
            
        if !confirm {
            println!("Operation cancelled");
            return Ok(());
        }
        
        println!("📥 Receiving file...");
        match self.cli_interface.receive_shared_file(share_id, path).await {
            Ok(_) => {
                Self::clear_screen();
                println!("✅ File reception initiated successfully");
            }
            Err(e) => {
                Self::clear_screen();
                println!("❌ Failed to receive file: {}", e);
            }
        }
        
        Ok(())
    }
    
    async fn show_progress(&self) -> Result<()> {
        // Get Share ID with screen clearing
        let transfer_id_str = self.get_input("Enter the Share ID to check progress")?;
        
        let transfer_id = match Uuid::parse_str(&transfer_id_str) {
            Ok(id) => id,
            Err(_) => {
                Self::clear_screen();
                println!("❌ Invalid Share ID format");
                return Ok(());
            }
        };
        
        Self::clear_screen();
        if let Some(progress) = self.cli_interface.progress_tracker.get_progress(transfer_id).await {
            let _percentage = (progress.transferred_bytes as f64 / progress.file_size as f64) * 100.0;
            
            match progress.status {
                crate::transfer::TransferStatus::InProgress => {
                    let pb = ProgressBar::new(progress.file_size as u64);
                    pb.set_style(ProgressStyle::default_bar()
                        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")?
                        .progress_chars("█▉▊▋▌▍▎▏  "));
                    
                    pb.set_position(progress.transferred_bytes as u64);
                    pb.finish_with_message(format!("Transfer of {} in progress", progress.file_name));
                }
                crate::transfer::TransferStatus::Completed => {
                    println!("✅ Transfer completed: {}", progress.file_name);
                }
                crate::transfer::TransferStatus::Failed(ref error) => {
                    println!("❌ Transfer failed: {} (Error: {})", 
                        progress.file_name, error);
                }
            }
        } else {
            println!("❌ Transfer not found for Share ID: {}", transfer_id);
        }
        
        Ok(())
    }
} 
