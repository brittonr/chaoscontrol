use std::path::PathBuf;

use chaoscontrol_evidence::update_replay_readiness_readme_status_path;

fn usage() -> &'static str {
    "usage: replay-readiness-readme-status RECEIPT.json [--readme README.md]"
}

fn main() {
    let mut receipt: Option<PathBuf> = None;
    let mut readme = PathBuf::from("README.md");
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_string_lossy().as_ref() {
            "-h" | "--help" => {
                println!("{}", usage());
                return;
            }
            "--readme" => {
                readme = PathBuf::from(args.next().unwrap_or_else(|| {
                    eprintln!("--readme requires a path\n{}", usage());
                    std::process::exit(2);
                }));
            }
            _ if receipt.is_none() => receipt = Some(PathBuf::from(arg)),
            other => {
                eprintln!("unexpected argument: {other}\n{}", usage());
                std::process::exit(2);
            }
        }
    }
    let Some(receipt) = receipt else {
        eprintln!("{}", usage());
        std::process::exit(2);
    };
    match update_replay_readiness_readme_status_path(receipt, readme) {
        Ok(summary) => println!("{summary}"),
        Err(err) => {
            eprintln!("replay-readiness README status update failed: {err}");
            std::process::exit(2);
        }
    }
}
