use std::path::PathBuf;
use std::process::ExitCode;

use chaoscontrol_evidence::validate_snapshot_backed_replay_artifact;

const EXIT_USAGE: u8 = 2;

fn usage() -> &'static str {
    "usage: check-replay-verdict-artifact --verdict PATH --bug PATH"
}

fn parse_args() -> Result<(PathBuf, PathBuf), String> {
    let mut verdict = None;
    let mut bug = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_string_lossy().as_ref() {
            "--verdict" => verdict = args.next().map(PathBuf::from),
            "--bug" => bug = args.next().map(PathBuf::from),
            "-h" | "--help" => return Err(usage().to_string()),
            other => return Err(format!("unexpected argument {other:?}\n{}", usage())),
        }
    }
    let verdict = verdict.ok_or_else(|| format!("--verdict is required\n{}", usage()))?;
    let bug = bug.ok_or_else(|| format!("--bug is required\n{}", usage()))?;
    admit_path("--verdict", &verdict)?;
    admit_path("--bug", &bug)?;
    Ok((verdict, bug))
}

fn admit_path(label: &str, path: &std::path::Path) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err(format!("{label} must not be empty\n{}", usage()));
    }
    Ok(())
}

fn main() -> ExitCode {
    let (verdict, bug) = match parse_args() {
        Ok(paths) => paths,
        Err(message) if message == usage() => {
            println!("{message}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    match validate_snapshot_backed_replay_artifact(verdict, bug) {
        Ok(summary) => {
            println!(
                "replay verdict artifact ok: run={} bug={} assertion={} depth={}",
                summary.run_id, summary.bug_id, summary.assertion_id, summary.replay_parent_depth
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("replay verdict artifact failed: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_paths_are_rejected() {
        assert!(admit_path("--verdict", std::path::Path::new("")).is_err());
        assert!(admit_path("--bug", std::path::Path::new("bug.json")).is_ok());
    }
}
