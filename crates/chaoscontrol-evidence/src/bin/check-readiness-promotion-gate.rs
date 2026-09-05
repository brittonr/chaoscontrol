use std::env;

use std::process::ExitCode;

use chaoscontrol_evidence::{
    default_readiness_promotion_paths, run_readiness_promotion_selftest,
    validate_readiness_promotion_files,
};

fn usage() -> &'static str {
    "usage: check-readiness-promotion-gate [--root DIR] [--manifest PATH] [--report PATH] [--selftest]"
}

fn parse_args() -> Result<
    (
        std::path::PathBuf,
        Option<std::path::PathBuf>,
        Option<std::path::PathBuf>,
        bool,
    ),
    String,
> {
    let mut root = std::path::PathBuf::from(".");
    let mut manifest = None;
    let mut report = None;
    let mut selftest = false;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--root requires a path".to_string())?;
                root = std::path::PathBuf::from(value);
            }
            "--manifest" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--manifest requires a path".to_string())?;
                manifest = Some(std::path::PathBuf::from(value));
            }
            "--report" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--report requires a path".to_string())?;
                report = Some(std::path::PathBuf::from(value));
            }
            "--selftest" => selftest = true,
            "-h" | "--help" => return Err(usage().to_string()),
            other => return Err(format!("unexpected argument {other:?}\n{}", usage())),
        }
    }
    Ok((root, manifest, report, selftest))
}

fn main() -> ExitCode {
    let (root, manifest, report, selftest) = match parse_args() {
        Ok(args) => args,
        Err(message) if message == usage() => {
            println!("{message}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };

    let (default_manifest, default_report) = default_readiness_promotion_paths(&root);
    let manifest = manifest.unwrap_or(default_manifest);
    let report = report.unwrap_or(default_report);

    if selftest {
        match run_readiness_promotion_selftest(&manifest, &report) {
            Ok(()) => {
                println!("readiness promotion gate selftest ok");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("readiness promotion gate failed: {err}");
                ExitCode::FAILURE
            }
        }
    } else {
        match validate_readiness_promotion_files(&manifest, &report) {
            Ok(summary) => {
                println!("readiness promotion gate ok:");
                for line in summary.lines {
                    println!("  {line}");
                }
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("readiness promotion gate failed: {err}");
                ExitCode::FAILURE
            }
        }
    }
}
