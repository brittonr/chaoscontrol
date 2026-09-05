use chaoscontrol_evidence::check_assertion_readiness_boundary;

const EXIT_ERROR: i32 = 1;
const EXIT_USAGE: i32 = 2;

fn usage() -> &'static str {
    "usage: check-assertion-readiness-boundary [ROOT]\n\nChecks exact assertion evidence classification without promoting diagnostic-only artifacts."
}

fn parse_root() -> Result<std::path::PathBuf, String> {
    let mut root = None;
    for arg in std::env::args_os().skip(1) {
        match arg.to_string_lossy().as_ref() {
            "-h" | "--help" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            _ if root.is_none() => root = Some(std::path::PathBuf::from(arg)),
            other => return Err(format!("unexpected argument: {other}\n{}", usage())),
        }
    }
    Ok(root.unwrap_or_else(|| std::path::PathBuf::from(".")))
}

fn main() {
    let root = match parse_root() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(EXIT_USAGE);
        }
    };
    match check_assertion_readiness_boundary(root) {
        Ok(lines) => {
            println!("assertion readiness boundary ok:");
            for line in lines {
                println!("  {line}");
            }
        }
        Err(error) => {
            eprintln!("assertion readiness boundary failed: {error}");
            std::process::exit(EXIT_ERROR);
        }
    }
}
