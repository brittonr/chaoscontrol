fn main() {
    let mut root = std::path::PathBuf::from(".");
    let mut write = false;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" => {
                root = args
                    .next()
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| fail("--root requires a path"));
            }
            "--write" => write = true,
            "--check" => write = false,
            _ => fail(&format!("unknown argument: {argument}")),
        }
    }
    if let Err(error) =
        chaoscontrol_evidence::snapshot_descriptor::contracts::check_snapshot_descriptor_contracts(
            &root, write,
        )
    {
        fail(&error.to_string());
    }
    println!("snapshot descriptor contracts ok");
}

fn fail(message: &str) -> ! {
    eprintln!("check-snapshot-descriptor-contracts: {message}");
    std::process::exit(1);
}
