use std::path::PathBuf;

use chaoscontrol_evidence::{check_dogfood_artifact_sizes, DEFAULT_MAX_DOGFOOD_ARTIFACT_BYTES};

fn usage() -> &'static str {
    "usage: check-dogfood-artifact-sizes [--root PATH] [--max-bytes N]"
}

fn main() {
    let mut root = PathBuf::from("dogfood-results");
    let mut max_bytes = DEFAULT_MAX_DOGFOOD_ARTIFACT_BYTES;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_string_lossy().as_ref() {
            "-h" | "--help" => {
                println!("{}", usage());
                return;
            }
            "--root" => {
                root = PathBuf::from(args.next().unwrap_or_else(|| {
                    eprintln!("--root requires a path\n{}", usage());
                    std::process::exit(2);
                }));
            }
            "--max-bytes" => {
                let raw = args.next().unwrap_or_else(|| {
                    eprintln!("--max-bytes requires a value\n{}", usage());
                    std::process::exit(2);
                });
                max_bytes = raw.to_string_lossy().parse().unwrap_or_else(|_| {
                    eprintln!("--max-bytes must be a positive integer");
                    std::process::exit(2);
                });
            }
            other => {
                eprintln!("unexpected argument: {other}\n{}", usage());
                std::process::exit(2);
            }
        }
    }
    match check_dogfood_artifact_sizes(root, max_bytes) {
        Ok(line) => println!("{line}"),
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}
