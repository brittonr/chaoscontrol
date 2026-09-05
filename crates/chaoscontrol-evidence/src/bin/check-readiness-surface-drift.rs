use chaoscontrol_evidence::{check_readiness_surface_drift, run_readiness_surface_drift_selftest};

fn usage() -> &'static str {
    "usage: check-readiness-surface-drift [--flake PATH] [--selftest] [ROOT]"
}

fn main() {
    let mut flake: Option<std::path::PathBuf> = None;
    let mut selftest = false;
    let mut root: Option<std::path::PathBuf> = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_string_lossy().as_ref() {
            "-h" | "--help" => {
                println!("{}", usage());
                return;
            }
            "--selftest" => selftest = true,
            "--flake" => {
                flake = Some(std::path::PathBuf::from(args.next().unwrap_or_else(|| {
                    eprintln!("--flake requires a path\n{}", usage());
                    std::process::exit(2);
                })));
            }
            _ if root.is_none() => root = Some(std::path::PathBuf::from(arg)),
            other => {
                eprintln!("unexpected argument: {other}\n{}", usage());
                std::process::exit(2);
            }
        }
    }
    let root = root.unwrap_or_else(|| std::path::PathBuf::from("."));
    let result = if selftest {
        run_readiness_surface_drift_selftest(&root).map(|()| {
            println!("readiness surface drift selftest ok");
        })
    } else {
        let flake = flake.unwrap_or_else(|| root.join("flake.nix"));
        check_readiness_surface_drift(&root, flake).map(|lines| {
            println!("readiness surface drift ok:");
            for line in lines {
                println!("  {line}");
            }
        })
    };
    if let Err(err) = result {
        eprintln!("readiness surface drift failed: {err}");
        std::process::exit(1);
    }
}
