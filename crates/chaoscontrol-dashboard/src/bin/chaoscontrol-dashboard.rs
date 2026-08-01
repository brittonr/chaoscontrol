//! Standalone dashboard binary for reviewing exploration results.
//!
//! ```bash
//! chaoscontrol-dashboard serve --corpus results/ --port 8080
//! ```

use chaoscontrol_explore::checkpoint::load_checkpoint;
use chaoscontrol_explore::dashboard_types::DashboardState;
use clap::{Parser, Subcommand};
use std::path::Path;

#[derive(Parser)]
#[command(name = "chaoscontrol-dashboard")]
#[command(about = "Web dashboard for ChaosControl exploration results")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Serve a dashboard from saved exploration results.
    Serve {
        /// Path to corpus directory (containing checkpoint.json).
        #[arg(short, long)]
        corpus: String,

        /// Port to listen on.
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },
}

fn main() {
    env_logger::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { corpus, port } => cmd_serve(corpus, port),
    }
}

fn cmd_serve(corpus: String, port: u16) {
    // Validate corpus directory
    if !Path::new(&corpus).is_dir() {
        eprintln!("Error: corpus directory not found: {}", corpus);
        std::process::exit(1);
    }

    // Load checkpoint
    let checkpoint_path = format!("{}/checkpoint.json", corpus);
    if !Path::new(&checkpoint_path).exists() {
        eprintln!("Error: checkpoint.json not found in {}", corpus);
        std::process::exit(1);
    }

    let checkpoint = match load_checkpoint(&checkpoint_path) {
        Ok(cp) => cp,
        Err(e) => {
            eprintln!("Error: failed to load checkpoint: {}", e);
            std::process::exit(1);
        }
    };

    let state = match DashboardState::from_checkpoint(&checkpoint) {
        Ok(state) => state,
        Err(error) => {
            eprintln!("Error: checkpoint evidence is invalid: {error}");
            std::process::exit(1);
        }
    };

    eprintln!("═══════════════════════════════════════════════════════════════════════");
    eprintln!("  ChaosControl Dashboard");
    eprintln!("═══════════════════════════════════════════════════════════════════════");
    eprintln!();
    eprintln!("Loaded from: {}", corpus);
    eprintln!("  Rounds:    {}", state.rounds);
    eprintln!("  Branches:  {}", state.total_branches);
    eprintln!("  Edges:     {}", state.total_edges);
    eprintln!("  Bugs:      {}", state.bugs.len());
    eprintln!();

    if let Err(e) = chaoscontrol_explore::server::start_standalone(state, port) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
