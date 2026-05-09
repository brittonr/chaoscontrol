use std::path::PathBuf;

use chaoscontrol_evidence::{write_sdk_local_report, DEFAULT_SDK_LOCAL_EVIDENCE_CLASS};

fn usage() -> &'static str {
    "usage: summarize-sdk-local-output --input PATH --output PATH [--evidence-class NAME]"
}

fn main() {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut evidence_class = DEFAULT_SDK_LOCAL_EVIDENCE_CLASS.to_string();
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
            "--output" => {
                output = Some(PathBuf::from(args.next().unwrap_or_else(|| {
                    eprintln!("--output requires a path\n{}", usage());
                    std::process::exit(2);
                })));
            }
            "--evidence-class" => {
                evidence_class = args
                    .next()
                    .unwrap_or_else(|| {
                        eprintln!("--evidence-class requires a value\n{}", usage());
                        std::process::exit(2);
                    })
                    .to_string_lossy()
                    .into_owned();
            }
            other => {
                eprintln!("unexpected argument: {other}\n{}", usage());
                std::process::exit(2);
            }
        }
    }
    let (Some(input), Some(output)) = (input, output) else {
        eprintln!("{}", usage());
        std::process::exit(2);
    };
    match write_sdk_local_report(input, output, &evidence_class) {
        Ok(report) => println!(
            "{}",
            serde_json::to_string(&report).expect("serialize report")
        ),
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}
