use std::path::PathBuf;

use chaoscontrol_evidence::write_replay_readiness_dashboard_path;

fn usage() -> &'static str {
    "usage: replay-readiness-dashboard RECEIPT.json --output PATH"
}

fn main() {
    let mut receipt: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_string_lossy().as_ref() {
            "-h" | "--help" => {
                println!("{}", usage());
                return;
            }
            "--output" | "-o" => {
                output = Some(PathBuf::from(args.next().unwrap_or_else(|| {
                    eprintln!("--output requires a path\n{}", usage());
                    std::process::exit(2);
                })));
            }
            _ if receipt.is_none() => receipt = Some(PathBuf::from(arg)),
            other => {
                eprintln!("unexpected argument: {other}\n{}", usage());
                std::process::exit(2);
            }
        }
    }
    let (Some(receipt), Some(output)) = (receipt, output) else {
        eprintln!("{}", usage());
        std::process::exit(2);
    };
    if let Err(err) = write_replay_readiness_dashboard_path(receipt, output) {
        eprintln!("replay-readiness dashboard failed: {err}");
        std::process::exit(2);
    }
}
