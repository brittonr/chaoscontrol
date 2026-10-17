use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use chaoscontrol_evidence::validate_contract_registry_json;

fn usage() -> &'static str {
    "usage: check-contract-registry [ROOT]"
}

fn main() -> ExitCode {
    let root = parse_root();
    if let Err(error) = chaoscontrol_evidence::profile_projection::admit_nickel_evaluator() {
        eprintln!("error: {error}");
        return ExitCode::FAILURE;
    }
    let registry = root.join("contracts/evidence/registry.ncl");

    let output = match Command::new("nickel")
        .arg("export")
        .arg(&registry)
        .current_dir(&root)
        .output()
    {
        Ok(output) => output,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(1);
        }
    };

    if !output.status.success() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        return ExitCode::from(output.status.code().unwrap_or(1) as u8);
    }

    match validate_contract_registry_json(&String::from_utf8_lossy(&output.stdout)) {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            for line in err.message().lines() {
                eprintln!("error: {line}");
            }
            ExitCode::from(1)
        }
    }
}

fn parse_root() -> PathBuf {
    let mut root = PathBuf::from(".");
    let args = env::args_os().skip(1);
    for arg in args {
        match arg.to_string_lossy().as_ref() {
            "-h" | "--help" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            other if root == Path::new(".") => root = PathBuf::from(other),
            other => {
                eprintln!("unexpected argument: {other}\n{}", usage());
                std::process::exit(2);
            }
        }
    }
    root
}
