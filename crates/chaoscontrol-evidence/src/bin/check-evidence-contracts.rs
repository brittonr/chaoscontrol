use std::path::PathBuf;

use chaoscontrol_evidence::check_evidence_contracts;

fn usage() -> &'static str {
    "usage: check-evidence-contracts [--root PATH]"
}

fn main() {
    let mut root = PathBuf::from(".");
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
            other => {
                eprintln!("unexpected argument: {other}\n{}", usage());
                std::process::exit(2);
            }
        }
    }

    match check_evidence_contracts(root) {
        Ok(line) => println!("{line}"),
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    }
}
