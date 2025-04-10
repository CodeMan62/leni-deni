use tracing::Level;
use tracing_subscriber::FmtSubscriber;

pub fn setup_logging(level: Level) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_file(true)
        .with_line_number(true)
        .with_target(true)
        .with_ansi(true)
        .with_level(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;
    Ok(())
} 
