use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use anyhow::Result;
use tracing::{info, debug};

#[derive(Debug, Clone)]
pub struct BandwidthStats {
    pub bytes_transferred: u64,
    pub start_time: Instant,
    pub current_rate: f64, // bytes per second
}

pub struct BandwidthManager {
    max_bandwidth: Arc<RwLock<Option<u64>>>, // bytes per second
    stats: Arc<RwLock<HashMap<Uuid, BandwidthStats>>>,
}

impl BandwidthManager {
    pub fn new() -> Self {
        Self {
            max_bandwidth: Arc::new(RwLock::new(None)),
            stats: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn set_max_bandwidth(&self, bytes_per_second: Option<u64>) {
        let mut max_bw = self.max_bandwidth.write().await;
        *max_bw = bytes_per_second;
        info!("Set max bandwidth to {:?} bytes/s", bytes_per_second);
    }

    pub async fn start_transfer(&self, transfer_id: Uuid) {
        let mut stats = self.stats.write().await;
        stats.insert(
            transfer_id,
            BandwidthStats {
                bytes_transferred: 0,
                start_time: Instant::now(),
                current_rate: 0.0,
            },
        );
    }

    pub async fn update_transfer(
        &self,
        transfer_id: Uuid,
        bytes_transferred: u64,
    ) -> Result<Duration> {
        let mut stats = self.stats.write().await;
        if let Some(stat) = stats.get_mut(&transfer_id) {
            stat.bytes_transferred += bytes_transferred;
            
            // Calculate current transfer rate
            let elapsed = stat.start_time.elapsed();
            stat.current_rate = stat.bytes_transferred as f64 / elapsed.as_secs_f64();
            
            // Check if we need to throttle
            if let Some(max_bw) = *self.max_bandwidth.read().await {
                if stat.current_rate > max_bw as f64 {
                    // Calculate sleep time to maintain max bandwidth
                    let target_time = (stat.bytes_transferred as f64 / max_bw as f64) * 1000.0;
                    let actual_time = elapsed.as_millis() as f64;
                    let sleep_time = (target_time - actual_time).max(0.0);
                    
                    debug!(
                        "Throttling transfer {}: current rate {:.2} MB/s, max {:.2} MB/s",
                        transfer_id,
                        stat.current_rate / 1_000_000.0,
                        max_bw as f64 / 1_000_000.0
                    );
                    
                    return Ok(Duration::from_millis(sleep_time as u64));
                }
            }
        }
        Ok(Duration::from_millis(0))
    }

    pub async fn get_transfer_stats(&self, transfer_id: Uuid) -> Option<BandwidthStats> {
        let stats = self.stats.read().await;
        stats.get(&transfer_id).cloned()
    }

    pub async fn end_transfer(&self, transfer_id: Uuid) {
        let mut stats = self.stats.write().await;
        stats.remove(&transfer_id);
    }
}

impl Default for BandwidthManager {
    fn default() -> Self {
        Self::new()
    }
} 
