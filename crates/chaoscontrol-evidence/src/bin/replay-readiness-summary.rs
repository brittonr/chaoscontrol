use std::path::PathBuf;

use chaoscontrol_evidence::summarize_replay_readiness_receipt_path;

fn main() {
    let mut args = std::env::args_os().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: replay-readiness-summary RECEIPT.json");
        std::process::exit(2);
    };
    if args.next().is_some() {
        eprintln!("usage: replay-readiness-summary RECEIPT.json");
        std::process::exit(2);
    }
    match summarize_replay_readiness_receipt_path(PathBuf::from(path)) {
        Ok(line) => println!("{line}"),
        Err(err) => {
            eprintln!("replay-readiness summary failed: {err}");
            std::process::exit(2);
        }
    }
}
