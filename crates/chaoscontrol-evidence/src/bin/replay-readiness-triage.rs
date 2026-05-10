use std::path::PathBuf;

use chaoscontrol_evidence::{
    check_operator_triage_runbook_path, render_operator_triage_runbook_path,
    write_operator_triage_runbook_path, TriageReceiptSource,
};

fn usage() -> &'static str {
    "usage: replay-readiness-triage [RECEIPT.json|--sample-receipt] [--root PATH] [--output PATH|--check PATH]"
}

fn main() {
    let mut receipt: Option<PathBuf> = None;
    let mut sample_receipt = false;
    let mut root = PathBuf::from(".");
    let mut output: Option<PathBuf> = None;
    let mut check: Option<PathBuf> = None;

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
            "--output" | "-o" => {
                output = Some(PathBuf::from(args.next().unwrap_or_else(|| {
                    eprintln!("--output requires a path\n{}", usage());
                    std::process::exit(2);
                })));
            }
            "--check" => {
                check = Some(PathBuf::from(args.next().unwrap_or_else(|| {
                    eprintln!("--check requires a path\n{}", usage());
                    std::process::exit(2);
                })));
            }
            "--sample-receipt" => sample_receipt = true,
            _ if receipt.is_none() => receipt = Some(PathBuf::from(arg)),
            other => {
                eprintln!("unexpected argument: {other}\n{}", usage());
                std::process::exit(2);
            }
        }
    }

    if sample_receipt && receipt.is_some() {
        eprintln!(
            "choose either RECEIPT.json or --sample-receipt\n{}",
            usage()
        );
        std::process::exit(2);
    }
    let source = if sample_receipt {
        TriageReceiptSource::Sample
    } else if let Some(path) = receipt.as_deref() {
        TriageReceiptSource::Path(path)
    } else {
        eprintln!("{}", usage());
        std::process::exit(2);
    };
    if output.is_some() && check.is_some() {
        eprintln!("choose either --output or --check\n{}", usage());
        std::process::exit(2);
    }

    let result = if let Some(path) = output {
        write_operator_triage_runbook_path(&root, source, path).map(|_| None)
    } else if let Some(path) = check {
        check_operator_triage_runbook_path(&root, source, path).map(|_| None)
    } else {
        render_operator_triage_runbook_path(&root, source).map(Some)
    };

    match result {
        Ok(Some(rendered)) => print!("{rendered}"),
        Ok(None) => {}
        Err(err) => {
            eprintln!("replay-readiness triage failed: {err}");
            std::process::exit(2);
        }
    }
}
