use std::env;
use std::path::PathBuf;
use std::process::Command;

/// Minimal vmlinux.h stub for BPF compilation in sandboxed environments
/// (e.g. Nix build) where /sys/kernel/btf/vmlinux is unavailable.
/// Provides only the types referenced by kvm_trace.bpf.c.
const VMLINUX_STUB: &str = r#"
#ifndef __VMLINUX_H__
#define __VMLINUX_H__

typedef unsigned char       __u8;
typedef unsigned short      __u16;
typedef unsigned int        __u32;
typedef unsigned long long  __u64;
typedef signed char         __s8;
typedef signed short        __s16;
typedef signed int          __s32;
typedef signed long long    __s64;

typedef __u8  u8;
typedef __u16 u16;
typedef __u32 u32;
typedef __u64 u64;
typedef __s8  s8;
typedef __s16 s16;
typedef __s32 s32;
typedef __s64 s64;

typedef _Bool bool;

enum { false = 0, true = 1 };

struct trace_entry {
    unsigned short type;
    unsigned char  flags;
    unsigned char  preempt_count;
    int            pid;
};

#endif /* __VMLINUX_H__ */
"#;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Step 1: Generate vmlinux.h from kernel BTF
    //
    // This requires /sys/kernel/btf/vmlinux (available on kernels with
    // CONFIG_DEBUG_INFO_BTF=y) and bpftool in PATH.
    //
    // In the Nix sandbox, /sys is not mounted so we fall back to a
    // minimal vmlinux.h stub that provides enough type definitions
    // for the BPF program to compile.  The resulting binary will work
    // when loaded on a real host with BTF support.
    let vmlinux_h = out_dir.join("vmlinux.h");
    if !vmlinux_h.exists() {
        // Allow overriding the BTF source via env var (useful for CI/Nix)
        let btf_path =
            env::var("VMLINUX_BTF").unwrap_or_else(|_| "/sys/kernel/btf/vmlinux".to_string());

        if PathBuf::from(&btf_path).exists() {
            let output = Command::new("bpftool")
                .args(["btf", "dump", "file", &btf_path, "format", "c"])
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
            std::fs::write(&vmlinux_h, &output.stdout).expect("failed to write vmlinux.h");
            eprintln!("Generated vmlinux.h ({} bytes)", output.stdout.len());
        } else {
            // No BTF available (e.g. Nix sandbox) — write minimal stub
            // with the types our BPF program actually references.
            eprintln!(
                "WARNING: {} not found, generating minimal vmlinux.h stub",
                btf_path
            );
            std::fs::write(&vmlinux_h, VMLINUX_STUB).expect("failed to write vmlinux.h stub");
        }
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
    builder.source("src/bpf/kvm_trace.bpf.c").clang_args([
        format!("-I{}", out_dir.display()),
        "-D__TARGET_ARCH_x86".to_string(),
    ]);

    // Use unwrapped clang if CLANG env var is set (needed on NixOS)
    if let Ok(clang_path) = env::var("CLANG") {
        eprintln!("Using CLANG={}", clang_path);
        builder.clang(clang_path);
    }

    builder.build_and_generate(&skel_output).expect(
        "Failed to build BPF program. Ensure clang is available.\n\
             On NixOS, set CLANG to unwrapped clang path.",
    );

    println!("cargo:rerun-if-changed=src/bpf/kvm_trace.bpf.c");
}
