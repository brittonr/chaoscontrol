use std::path::PathBuf;

use chaoscontrol_evidence::check_product_scope;

fn usage() -> &'static str {
    "usage: check-product-scope [--root PATH] [--write]"
}

fn parse_args() -> Result<(PathBuf, bool), String> {
    let mut root = PathBuf::from(".");
    let mut write = false;
    let mut arguments = std::env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "-h" | "--help" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            "--root" => {
                root = PathBuf::from(
                    arguments
                        .next()
                        .ok_or_else(|| format!("--root requires a path\n{}", usage()))?,
                );
            }
            "--write" => write = true,
            other => return Err(format!("unexpected argument: {other}\n{}", usage())),
        }
    }
    admit_root(&root)?;
    Ok((root, write))
}

fn admit_root(root: &std::path::Path) -> Result<(), String> {
    if root.as_os_str().is_empty() {
        return Err(String::from("--root must not be empty"));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_root_is_rejected() {
        assert!(admit_root(std::path::Path::new("")).is_err());
    }
}
