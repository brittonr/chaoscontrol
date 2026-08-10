use std::path::PathBuf;

use chaoscontrol_property_suite::framework::Lane;

const USAGE: &str = "usage: run-property-lane --lane <fast|deep> --output <path>";
const OPTION_PAIR_WIDTH: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Options {
    lane: Lane,
    output: PathBuf,
}

fn main() {
    if let Err(error) = run(std::env::args().skip(1).collect()) {
        eprintln!("run-property-lane: {error}");
        std::process::exit(1);
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let options = parse_options(&args)?;
    let report = chaoscontrol_property_suite::run_lane(options.lane).map_err(|failure| {
        serde_json::to_string_pretty(&failure).unwrap_or_else(|_| {
            format!(
                "property suite {} found a counterexample",
                failure.counterexample.suite
            )
        })
    })?;
    let encoded = serde_json::to_vec_pretty(&report)
        .map_err(|error| format!("failed to encode the property report: {error}"))?;
    if let Some(parent) = options.output.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    std::fs::write(&options.output, encoded)
        .map_err(|error| format!("failed to write {}: {error}", options.output.display()))?;
    println!(
        "property lane {} passed; receipt={}",
        options.lane.id(),
        options.output.display()
    );
    Ok(())
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut lane = None;
    let mut output = None;
    let mut index = 0_usize;
    while index < args.len() {
        match args[index].as_str() {
            "--lane" => {
                let value = args.get(index + 1).ok_or(USAGE)?;
                lane = Some(match value.as_str() {
                    "fast" => Lane::Fast,
                    "deep" => Lane::Deep,
                    _ => return Err(format!("unknown property lane {value:?}; {USAGE}")),
                });
                index += OPTION_PAIR_WIDTH;
            }
            "--output" => {
                let value = args.get(index + 1).ok_or(USAGE)?;
                output = Some(PathBuf::from(value));
                index += OPTION_PAIR_WIDTH;
            }
            unknown => return Err(format!("unknown argument {unknown:?}; {USAGE}")),
        }
    }
    Ok(Options {
        lane: lane.ok_or(USAGE)?,
        output: output.ok_or(USAGE)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_complete_options() {
        let options = parse_options(&[
            "--lane".to_string(),
            "fast".to_string(),
            "--output".to_string(),
            "receipt.json".to_string(),
        ])
        .expect("complete options must parse");
        assert_eq!(options.lane, Lane::Fast);
        assert_eq!(options.output, PathBuf::from("receipt.json"));
    }

    #[test]
    fn rejects_unknown_lane() {
        let result = parse_options(&[
            "--lane".to_string(),
            "unbounded".to_string(),
            "--output".to_string(),
            "receipt.json".to_string(),
        ]);
        assert!(result.is_err());
    }
}
