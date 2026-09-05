use chaoscontrol_evidence::{
    check_assertion_readiness_promotion, check_assertion_readiness_promotion_paths,
    run_assertion_readiness_promotion_selftest,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    manifest: Option<std::path::PathBuf>,
    report: Option<std::path::PathBuf>,
    selftest: bool,
    root: std::path::PathBuf,
}

fn usage() -> &'static str {
    "usage: check-assertion-readiness-promotion-gate [--manifest PATH] [--report PATH] [--selftest] [ROOT]\n\nFail-closed promotion gate for assertion-readiness workload claims."
}

fn parse_args() -> Result<Args, String> {
    let mut manifest = None;
    let mut report = None;
    let mut selftest = false;
    let mut root = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_string_lossy().as_ref() {
            "-h" | "--help" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            "--selftest" => selftest = true,
            "--manifest" => {
                manifest =
                    Some(std::path::PathBuf::from(args.next().ok_or_else(|| {
                        format!("--manifest requires a path\n{}", usage())
                    })?));
            }
            "--report" => {
                report =
                    Some(std::path::PathBuf::from(args.next().ok_or_else(|| {
                        format!("--report requires a path\n{}", usage())
                    })?));
            }
            _ if root.is_none() => root = Some(std::path::PathBuf::from(arg)),
            other => return Err(format!("unexpected argument: {other}\n{}", usage())),
        }
    }
    Ok(Args {
        manifest,
        report,
        selftest,
        root: root.unwrap_or_else(|| std::path::PathBuf::from(".")),
    })
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(2);
        }
    };

    if args.selftest {
        match run_assertion_readiness_promotion_selftest(&args.root) {
            Ok(()) => println!("assertion readiness promotion gate selftest ok"),
            Err(err) => {
                eprintln!("assertion readiness promotion gate failed: {err}");
                std::process::exit(1);
            }
        }
        return;
    }

    let result = if args.manifest.is_some() || args.report.is_some() {
        let manifest = args.manifest.unwrap_or_else(|| {
            args.root
                .join("dogfood-results/accepted-workload-proofs.json")
        });
        let report = args
            .report
            .unwrap_or_else(|| args.root.join("docs/assertion-readiness-status.md"));
        check_assertion_readiness_promotion_paths(&args.root, manifest, report)
    } else {
        check_assertion_readiness_promotion(&args.root)
    };

    match result {
        Ok(lines) => {
            println!("assertion readiness promotion gate ok:");
            for line in lines {
                println!("  {line}");
            }
        }
        Err(err) => {
            eprintln!("assertion readiness promotion gate failed: {err}");
            std::process::exit(1);
        }
    }
}
