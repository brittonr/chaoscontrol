use std::path::PathBuf;

use chaoscontrol_evidence::{render_replay_proof_coverage, validate_replay_proof_coverage};

fn main() {
    let root = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    match validate_replay_proof_coverage(&root) {
        Ok(lines) => {
            print!("{}", render_replay_proof_coverage(&lines));
        }
        Err(err) => {
            eprintln!("replay proof coverage check failed: {err}");
            std::process::exit(1);
        }
    }
}
