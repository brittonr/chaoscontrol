use std::path::PathBuf;

use chaoscontrol_evidence::{
    check_sdk_assertion_quality_fixtures, check_sdk_assertion_quality_path,
};

fn usage() -> &'static str {
    "usage: check-sdk-assertion-quality [--input REPORT.json]"
}

fn main() {
    let mut input: Option<PathBuf> = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_string_lossy().as_ref() {
            "-h" | "--help" => {
                println!("{}", usage());
                return;
            }
            "--input" => {
                input = Some(PathBuf::from(args.next().unwrap_or_else(|| {
                    eprintln!("--input requires a path\n{}", usage());
                    std::process::exit(2);
                })));
            }
            other => {
                eprintln!("unexpected argument: {other}\n{}", usage());
                std::process::exit(2);
            }
        }
    }

    let result = if let Some(input) = input {
        check_sdk_assertion_quality_path(input).map(|gate| {
            println!(
                "{}",
                serde_json::to_string_pretty(&gate.to_json()).expect("serialize gate")
            );
            if gate.passed {
                "sdk-assertion-quality: ok".to_string()
            } else {
                for blocker in &gate.blockers {
                    eprintln!("blocker: {blocker}");
                }
                std::process::exit(1);
            }
        })
    } else {
        check_sdk_assertion_quality_fixtures()
    };

    match result {
        Ok(line) => println!("{line}"),
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}
