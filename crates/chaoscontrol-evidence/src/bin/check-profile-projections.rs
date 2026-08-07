use std::path::PathBuf;

fn main() {
    let mut root = PathBuf::from(".");
    let mut write = false;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--root" => {
                let Some(value) = arguments.next() else {
                    eprintln!("--root requires a path");
                    std::process::exit(2);
                };
                root = PathBuf::from(value);
            }
            "--write" => write = true,
            _ => {
                eprintln!("unexpected argument: {argument}");
                std::process::exit(2);
            }
        }
    }
    if let Err(error) =
        chaoscontrol_evidence::profile_projection::check_profile_projections(&root, write)
    {
        eprintln!("profile projection check failed: {}", error.message());
        std::process::exit(1);
    }
    println!("profile projections ok");
}
