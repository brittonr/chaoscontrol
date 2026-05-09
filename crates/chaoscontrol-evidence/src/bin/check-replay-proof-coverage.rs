use std::path::PathBuf;

use chaoscontrol_evidence::{
    check_replay_proof_coverage_doc, render_replay_proof_coverage,
    render_replay_proof_coverage_doc, validate_replay_proof_coverage,
    write_replay_proof_coverage_doc, REPLAY_PROOF_COVERAGE_DOC,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Coverage,
    CheckDoc,
    PrintDoc,
    WriteDoc,
}

fn usage() -> &'static str {
    "usage: check-replay-proof-coverage [--check-doc|--print-doc|--write-doc] [ROOT]\n\nValidates committed replay proof coverage. Doc modes derive docs/replay-proof-coverage.md from the accepted workload proof manifest."
}

fn parse_args() -> Result<(Mode, PathBuf), String> {
    let mut mode = Mode::Coverage;
    let mut root: Option<PathBuf> = None;
    for arg in std::env::args_os().skip(1) {
        match arg.to_string_lossy().as_ref() {
            "-h" | "--help" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            "--check-doc" => mode = Mode::CheckDoc,
            "--print-doc" => mode = Mode::PrintDoc,
            "--write-doc" => mode = Mode::WriteDoc,
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
        Mode::Coverage => validate_replay_proof_coverage(&root)
            .map(|lines| print!("{}", render_replay_proof_coverage(&lines))),
        Mode::CheckDoc => check_replay_proof_coverage_doc(&root)
            .map(|()| println!("replay proof coverage doc ok: {REPLAY_PROOF_COVERAGE_DOC}")),
        Mode::PrintDoc => render_replay_proof_coverage_doc(&root).map(|doc| print!("{doc}")),
        Mode::WriteDoc => write_replay_proof_coverage_doc(&root)
            .map(|()| println!("wrote {REPLAY_PROOF_COVERAGE_DOC}")),
    };

    if let Err(err) = result {
        eprintln!("replay proof coverage check failed: {err}");
        std::process::exit(1);
    }
}
