use std::path::PathBuf;

use chaoscontrol_evidence::{
    check_assertion_readiness_status, render_assertion_readiness_status,
    write_assertion_readiness_status, ASSERTION_READINESS_STATUS_DOC,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Print,
    Check,
    Write,
}

fn usage() -> &'static str {
    "usage: generate-assertion-readiness-report [--check|--write] [ROOT]\n\nGenerate or check docs/assertion-readiness-status.md from accepted workload proofs and assertions."
}

fn parse_args() -> Result<(Mode, PathBuf), String> {
    let mut mode = Mode::Print;
    let mut root: Option<PathBuf> = None;
    for arg in std::env::args_os().skip(1) {
        match arg.to_string_lossy().as_ref() {
            "-h" | "--help" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            "--check" => mode = Mode::Check,
            "--write" => mode = Mode::Write,
            _ if root.is_none() => root = Some(PathBuf::from(arg)),
            other => return Err(format!("unexpected argument: {other}\n{}", usage())),
        }
    }
    Ok((mode, root.unwrap_or_else(|| PathBuf::from("."))))
}

fn main() {
    let (mode, root) = match parse_args() {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(2);
        }
    };

    let result = match mode {
        Mode::Print => render_assertion_readiness_status(&root).map(|report| print!("{report}")),
        Mode::Check => check_assertion_readiness_status(&root)
            .map(|()| println!("assertion readiness report ok: {ASSERTION_READINESS_STATUS_DOC}")),
        Mode::Write => write_assertion_readiness_status(&root)
            .map(|()| println!("wrote {ASSERTION_READINESS_STATUS_DOC}")),
    };

    if let Err(err) = result {
        eprintln!("assertion readiness report failed: {err}");
        std::process::exit(1);
    }
}
