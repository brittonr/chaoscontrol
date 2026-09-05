use chaoscontrol_evidence::{
    check_replay_readiness_status, render_replay_readiness_status, write_replay_readiness_status,
    REPLAY_READINESS_STATUS_DOC,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Print,
    Check,
    Write,
}

fn usage() -> &'static str {
    "usage: generate-replay-readiness-report [--check|--write] [ROOT]\n\nGenerate or check docs/replay-readiness-status.md from accepted workload proofs."
}

fn parse_args() -> Result<(Mode, std::path::PathBuf), String> {
    let mut mode = Mode::Print;
    let mut root: Option<std::path::PathBuf> = None;
    for arg in std::env::args_os().skip(1) {
        match arg.to_string_lossy().as_ref() {
            "-h" | "--help" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            "--check" => mode = Mode::Check,
            "--write" => mode = Mode::Write,
            _ if root.is_none() => root = Some(std::path::PathBuf::from(arg)),
            other => return Err(format!("unexpected argument: {other}\n{}", usage())),
        }
    }
    Ok((mode, root.unwrap_or_else(|| std::path::PathBuf::from("."))))
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
        Mode::Print => render_replay_readiness_status(&root).map(|report| print!("{report}")),
        Mode::Check => check_replay_readiness_status(&root)
            .map(|()| println!("replay readiness report ok: {REPLAY_READINESS_STATUS_DOC}")),
        Mode::Write => write_replay_readiness_status(&root)
            .map(|()| println!("wrote {REPLAY_READINESS_STATUS_DOC}")),
    };

    if let Err(err) = result {
        eprintln!("replay readiness report failed: {err}");
        std::process::exit(1);
    }
}
