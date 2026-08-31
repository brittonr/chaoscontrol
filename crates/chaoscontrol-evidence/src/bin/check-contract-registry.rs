use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use chaoscontrol_evidence::validate_contract_registry_json;

fn usage() -> &'static str {
    "usage: check-contract-registry [ROOT]"
}

fn main() -> ExitCode {
    let root = parse_root();
    let registry = root.join("contracts/evidence/registry.ncl");

    let command = match nickel_export_command(&registry) {
        Some(command) => command,
        None => {
            eprintln!("error: neither `nickel` nor `nix` is available for registry validation");
            return ExitCode::from(127);
        }
    };

    let output = match Command::new(&command[0])
        .args(&command[1..])
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

// r[impl chaoscontrol.nickel_toolchain.cohort]
fn nickel_export_command(registry: &Path) -> Option<Vec<String>> {
    which("nickel").map(|_| {
        vec![
            "nickel".to_string(),
            "export".to_string(),
            registry.display().to_string(),
        ]
    })
}

fn which(program: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}
