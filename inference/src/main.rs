use anyhow::Result;
use clap::Parser;
use std::sync::Arc;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use claw_pen_inference::{ModelLoader, InferenceEngine, InferenceApi};

/// Claw Pen Inference Service
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the GGUF model file
    #[arg(long)]
    model_path: String,

    /// Port to listen on
    #[arg(short, long, default_value_t = 8765)]
    port: u16,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize tracing
    let log_level = match args.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");

    info!("Starting Claw Pen Inference Service");
    info!("Model path: {}", args.model_path);
    info!("Port: {}", args.port);

    // Load the model
    let model = Arc::new(ModelLoader::new(&args.model_path));
    info!("Model loader initialized (lazy loading enabled)");

    // Create inference engine
    let engine = Arc::new(InferenceEngine::new(model));
    info!("Inference engine created");

    // Create and run the API server
    let api = InferenceApi::new(engine, args.port);
    api.run().await?;

    Ok(())
}
