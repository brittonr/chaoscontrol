use std::path::PathBuf;

use chaoscontrol_evidence::validate_accepted_dogfood_config;

fn usage() -> &'static str {
    "usage: check-accepted-dogfood-config --config PATH [--expectations PATH] [--manifest PATH]"
}

fn main() {
    let mut config: Option<PathBuf> = None;
    let mut expectations = PathBuf::from("dogfood-results/accepted-dogfood-expectations.json");
    let mut manifest = PathBuf::from("dogfood-results/accepted-workload-proofs.json");
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_string_lossy().as_ref() {
            "-h" | "--help" => {
                println!("{}", usage());
                return;
            }
            "--config" => {
                config = Some(PathBuf::from(args.next().unwrap_or_else(|| {
                    eprintln!("--config requires a path\n{}", usage());
                    std::process::exit(2);
                })));
            }
            "--expectations" => {
                expectations = PathBuf::from(args.next().unwrap_or_else(|| {
                    eprintln!("--expectations requires a path\n{}", usage());
                    std::process::exit(2);
                }));
            }
            "--manifest" => {
                manifest = PathBuf::from(args.next().unwrap_or_else(|| {
                    eprintln!("--manifest requires a path\n{}", usage());
                    std::process::exit(2);
                }));
            }
            other => {
                eprintln!("unexpected argument: {other}\n{}", usage());
                std::process::exit(2);
            }
        }
    }
    let Some(config) = config else {
        eprintln!("{}", usage());
        std::process::exit(2);
    };
    match validate_accepted_dogfood_config(config, expectations, manifest) {
        Ok(line) => println!("{line}"),
        Err(err) => {
            for line in err.message().lines() {
                eprintln!("{line}");
            }
            std::process::exit(1);
        }
    }
}
