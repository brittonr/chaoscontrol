use std::path::PathBuf;

fn main() {
    let mut root = PathBuf::from(".");
    let mut write = false;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" => {
                root = args
                    .next()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| fail("--root requires a path"));
            }
            "--write" => write = true,
            "--check" => write = false,
            _ => fail(&format!("unknown argument: {argument}")),
        }
    }
    if admit_root(&root).is_err() {
        fail("--root must not be empty");
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

fn admit_root(root: &std::path::Path) -> Result<(), ()> {
    if root.as_os_str().is_empty() {
        return Err(());
    }
    Ok(())
}

fn fail(message: &str) -> ! {
    eprintln!("check-snapshot-descriptor-contracts: {message}");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_root_is_rejected() {
        assert!(admit_root(std::path::Path::new("")).is_err());
    }
}
