use chaoscontrol_evidence::check_product_scope;

fn usage() -> &'static str {
    "usage: check-product-scope [--root PATH] [--write]"
}

fn parse_args() -> Result<(std::path::PathBuf, bool), String> {
    let mut root = std::path::PathBuf::from(".");
    let mut write = false;
    let mut arguments = std::env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "-h" | "--help" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            "--root" => {
                root = std::path::PathBuf::from(
                    arguments
                        .next()
                        .ok_or_else(|| format!("--root requires a path\n{}", usage()))?,
                );
            }
            "--write" => write = true,
            other => return Err(format!("unexpected argument: {other}\n{}", usage())),
        }
    }
    Ok((root, write))
}

fn main() {
    let (root, write) = match parse_args() {
        Ok(arguments) => arguments,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    match check_product_scope(root, write) {
        Ok(summary) => println!("{summary}"),
        Err(error) => {
            eprintln!("product scope failed: {error}");
            std::process::exit(1);
        }
    }
}
