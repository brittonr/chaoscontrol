use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Step 1: Generate vmlinux.h from kernel BTF
    //
    // This requires /sys/kernel/btf/vmlinux (available on kernels with
    // CONFIG_DEBUG_INFO_BTF=y) and bpftool in PATH.
    let vmlinux_h = out_dir.join("vmlinux.h");
    if !vmlinux_h.exists() {
        let output = Command::new("bpftool")
            .args([
                "btf", "dump", "file", "/sys/kernel/btf/vmlinux", "format", "c",
            ])
            .output()
            .expect(
                "bpftool is required to generate vmlinux.h.\n\
                 Install via: nix-shell -p bpftools\n\
                 Or add to your devShell.",
            );
        assert!(
            output.status.success(),
            "bpftool btf dump failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::write(&vmlinux_h, &output.stdout)
            .expect("failed to write vmlinux.h");
        eprintln!(
            "Generated vmlinux.h ({} bytes)",
            output.stdout.len()
        );
    }

    // Step 2: Compile BPF program and generate Rust skeleton
    //
    // libbpf-cargo handles:
    // - Locating BPF helper headers (bpf_helpers.h, etc.)
    // - Compiling to BPF ELF object
    // - Generating type-safe Rust skeleton
    //
    // On NixOS, the wrapped clang adds flags incompatible with BPF
    // target. Use the CLANG env var to point to unwrapped clang.
    let skel_output = out_dir.join("kvm_trace.skel.rs");
    let mut builder = libbpf_cargo::SkeletonBuilder::new();
    builder
        .source("src/bpf/kvm_trace.bpf.c")
        .clang_args([
            format!("-I{}", out_dir.display()),
            "-D__TARGET_ARCH_x86".to_string(),
        ]);

    // Use unwrapped clang if CLANG env var is set (needed on NixOS)
    if let Ok(clang_path) = env::var("CLANG") {
        eprintln!("Using CLANG={}", clang_path);
        builder.clang(clang_path);
    }

    builder
        .build_and_generate(&skel_output)
        .expect(
            "Failed to build BPF program. Ensure clang is available.\n\
             On NixOS, set CLANG to unwrapped clang path.",
        );

    println!("cargo:rerun-if-changed=src/bpf/kvm_trace.bpf.c");
}
