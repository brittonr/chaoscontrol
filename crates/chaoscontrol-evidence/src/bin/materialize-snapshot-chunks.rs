use chaoscontrol_evidence::{
    materialize_snapshot_chunks, run_materialize_snapshot_chunks_selftest,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    manifest: Option<std::path::PathBuf>,
    force: bool,
    selftest: bool,
}

fn usage() -> &'static str {
    "usage: materialize-snapshot-chunks [--force] [--selftest] [MANIFEST]\n\nMaterializes a <snapshot>.chunks.json sidecar back to its raw .snapshot.bin file."
}

fn parse_args() -> Result<Args, String> {
    let mut parsed = Args {
        manifest: None,
        force: false,
        selftest: false,
    };
    for arg in std::env::args_os().skip(1) {
        match arg.to_string_lossy().as_ref() {
            "-h" | "--help" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            "--force" => parsed.force = true,
            "--selftest" => parsed.selftest = true,
            _ if parsed.manifest.is_none() => parsed.manifest = Some(std::path::PathBuf::from(arg)),
            other => return Err(format!("unexpected argument: {other}\n{}", usage())),
        }
    }
    if !parsed.selftest && parsed.manifest.is_none() {
        return Err(format!(
            "manifest is required unless --selftest is used\n{}",
            usage()
        ));
    }
    Ok(parsed)
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(2);
        }
    };

    let result = if args.selftest {
        run_materialize_snapshot_chunks_selftest()
            .map(|()| println!("materialize-snapshot-chunks selftest ok"))
    } else {
        materialize_snapshot_chunks(
            args.manifest
                .as_ref()
                .expect("manifest presence checked by parse_args"),
            args.force,
        )
        .map(|snapshot| println!("{}", snapshot.render()))
    };

    if let Err(err) = result {
        eprintln!("snapshot chunk materialization failed: {err}");
        std::process::exit(1);
    }
}
