use std::path::PathBuf;

fn main() {
    let mut output = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--out" => {
                output = Some(
                    args.next()
                        .map(PathBuf::from)
                        .unwrap_or_else(|| fail("--out requires a path")),
                );
            }
            _ => fail(&format!("unknown argument: {argument}")),
        }
    }
    let output = output.unwrap_or_else(|| fail("--out is required"));
    match chaoscontrol_evidence::snapshot_descriptor::fixture::write_fixture_bundle(&output) {
        Ok(bundle) => {
            println!("snapshot descriptor fixture written: {}", output.display());
            println!(
                "monolithic={:?}:{} chunked={:?}:{} restore_completed={}",
                bundle.monolithic_descriptor.algorithm,
                bundle.monolithic_descriptor.hex,
                bundle.chunked_descriptor.algorithm,
                bundle.chunked_descriptor.hex,
                bundle.restore_completed
            );
        }
        Err(error) => fail(&error.to_string()),
    }
}

fn fail(message: &str) -> ! {
    eprintln!("snapshot-descriptor-fixture: {message}");
    std::process::exit(1);
}
