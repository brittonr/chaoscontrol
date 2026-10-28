use std::env;
use std::fs;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chaoscontrol_evidence::rust_automation::bounded_input::validate_byte_length;
use chaoscontrol_evidence::rust_automation::storage_campaign::{
    encode_storage_campaign_receipt, is_storage_campaign_pass, run_storage_campaign,
    MAX_CAMPAIGN_INPUT_BYTES,
};

const USAGE_EXIT: u8 = 2;
const NON_PASS_EXIT: u8 = 1;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(is_pass) => ExitCode::from(if is_pass { 0 } else { NON_PASS_EXIT }),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(USAGE_EXIT)
        }
    }
}

fn run(args: Vec<String>) -> Result<bool, String> {
    let (input, out) = parse_args(&args)?;
    let bytes = read_bounded(&input)?;
    let receipt = run_storage_campaign(&bytes)?;
    let encoded = encode_storage_campaign_receipt(&receipt)?;
    write_bounded(&out, &encoded)?;
    println!(
        "storage-recovery-campaign verdict={:?} receipt={} input={} out={}",
        receipt.verdict,
        receipt.receipt_ref,
        receipt.input_blake3,
        out.display()
    );
    Ok(is_storage_campaign_pass(receipt.verdict))
}

fn parse_args(args: &[String]) -> Result<(PathBuf, PathBuf), String> {
    let mut input = None;
    let mut out = None;
    let mut index = 0;
    while index < args.len() {
        let flag = &args[index];
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("{flag} requires a path"))?;
        match flag.as_str() {
            "--input" => input = Some(PathBuf::from(value)),
            "--out" => out = Some(PathBuf::from(value)),
            other => return Err(format!("unknown argument: {other}")),
        }
        index += 2;
    }
    let input = input.ok_or_else(|| String::from("--input is required"))?;
    let out = out.ok_or_else(|| String::from("--out is required"))?;
    Ok((input, out))
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if !metadata.file_type().is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    validate_byte_length(
        &format!("{}: campaign request", path.display()),
        metadata.len(),
        MAX_CAMPAIGN_INPUT_BYTES,
    )?;
    let mut bytes = Vec::new();
    file.take(MAX_CAMPAIGN_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    validate_byte_length(
        &format!("{}: campaign request", path.display()),
        u64::try_from(bytes.len()).map_err(|_| String::from("request length"))?,
        MAX_CAMPAIGN_INPUT_BYTES,
    )?;
    Ok(bytes)
}

fn write_bounded(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err(String::from("output path must not be empty"));
    }
    if path.exists() {
        return Err(format!("{} already exists", path.display()));
    }
    chaoscontrol_evidence::bounded_file::write_new_bytes(path, bytes)
        .map_err(|error| error.message().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn campaign_output_is_atomic_and_no_replace() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("nested/campaign.json");
        write_bounded(&path, b"{}\n").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"{}\n");
        assert!(write_bounded(&path, b"replacement").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{}\n");
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }
}
