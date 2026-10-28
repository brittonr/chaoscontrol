use std::io::Write;
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
    write_report(&options.output, &encoded)
        .map_err(|error| format!("failed to write {}: {error}", options.output.display()))?;
    println!(
        "property lane {} passed; receipt={}",
        options.lane.id(),
        options.output.display()
    );
    Ok(())
}

fn write_report(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "receipt output is not a regular file",
            ))
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let directory = std::fs::File::open(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    directory.sync_all()
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
    fn report_writer_replaces_an_existing_regular_file() {
        let root = tempfile::tempdir().expect("temporary directory");
        let path = root.path().join("receipt.json");
        std::fs::write(&path, b"old").unwrap();
        write_report(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
    }

    #[test]
    fn report_writer_rejects_symlink_targets() {
        let root = tempfile::tempdir().expect("temporary directory");
        let target = root.path().join("target.json");
        std::fs::write(&target, b"original").unwrap();
        let link = root.path().join("receipt.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert!(write_report(&link, b"receipt").is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"original");
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
