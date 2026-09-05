use chaoscontrol_evidence::{
    sample_simulator_run_evidence, validate_in_process_simulator_receipt_path,
    write_sample_in_process_simulator_receipt_path,
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Mode {
    Sample { output: std::path::PathBuf },
    Check { path: std::path::PathBuf },
}

fn usage() -> &'static str {
    "usage: in-process-simulator-receipt --sample --output PATH\n       in-process-simulator-receipt --check PATH\n\nEmit or validate bounded in-process simulator receipts. These receipts are adapter-simulator evidence only: not VM replay proof and not full FoundationDB parity."
}

fn parse_args() -> Result<Mode, String> {
    let mut args = std::env::args_os().skip(1);
    let mut sample = false;
    let mut output: Option<std::path::PathBuf> = None;
    let mut check: Option<std::path::PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.to_string_lossy().as_ref() {
            "-h" | "--help" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            "--sample" => sample = true,
            "--output" => {
                let path = args
                    .next()
                    .ok_or_else(|| format!("--output requires a path\n{}", usage()))?;
                output = Some(std::path::PathBuf::from(path));
            }
            "--check" => {
                let path = args
                    .next()
                    .ok_or_else(|| format!("--check requires a path\n{}", usage()))?;
                check = Some(std::path::PathBuf::from(path));
            }
            other => return Err(format!("unexpected argument: {other}\n{}", usage())),
        }
    }
    match (sample, output, check) {
        (true, Some(output), None) => Ok(Mode::Sample { output }),
        (false, None, Some(path)) => Ok(Mode::Check { path }),
        _ => Err(usage().to_string()),
    }
}

fn main() {
    let mode = match parse_args() {
        Ok(mode) => mode,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(2);
        }
    };
    let result = match mode {
        Mode::Sample { output } => write_sample_in_process_simulator_receipt_path(&output)
            .and_then(|()| {
                let evidence = sample_simulator_run_evidence()?;
                println!("{}", evidence.summary.receipt_summary);
                Ok(())
            }),
        Mode::Check { path } => {
            validate_in_process_simulator_receipt_path(path).map(|summary| println!("{summary}"))
        }
    };
    if let Err(err) = result {
        eprintln!("in-process simulator receipt failed: {err}");
        std::process::exit(1);
    }
}
