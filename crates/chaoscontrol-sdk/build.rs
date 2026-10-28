use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    if let Ok(output) = Command::new(rustc).arg("--version").output() {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Some(version) = text.split_whitespace().nth(1) {
                println!("cargo:rustc-env=CHAOSCONTROL_RUSTC_VERSION={version}");
            }
        }
    }
}
