use std::env;
use std::path::PathBuf;
use std::process::Command;

/// Minimal vmlinux.h stub for BPF compilation in sandboxed environments
/// (e.g. Nix build) where /sys/kernel/btf/vmlinux is unavailable.
/// Provides only the types referenced by kvm_trace.bpf.c.
const VMLINUX_STUB: &str = r#"
#ifndef __VMLINUX_H__
#define __VMLINUX_H__

/* Basic integer types */
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

/* Network byte-order types (referenced by bpf_helper_defs.h) */
typedef __u16 __be16;
typedef __u32 __be32;
typedef __u64 __be64;
typedef __u16 __le16;
typedef __u32 __le32;
typedef __u64 __le64;
typedef __u32 __wsum;
typedef __u32 __sum16;

/* BPF map types (referenced by __uint(type, ...) macros) */
enum bpf_map_type {
    BPF_MAP_TYPE_UNSPEC = 0,
    BPF_MAP_TYPE_HASH = 1,
    BPF_MAP_TYPE_ARRAY = 2,
    BPF_MAP_TYPE_PROG_ARRAY = 3,
    BPF_MAP_TYPE_PERF_EVENT_ARRAY = 4,
    BPF_MAP_TYPE_PERCPU_HASH = 5,
    BPF_MAP_TYPE_PERCPU_ARRAY = 6,
    BPF_MAP_TYPE_STACK_TRACE = 7,
    BPF_MAP_TYPE_CGROUP_ARRAY = 8,
    BPF_MAP_TYPE_LRU_HASH = 9,
    BPF_MAP_TYPE_LRU_PERCPU_HASH = 10,
    BPF_MAP_TYPE_LPM_TRIE = 11,
    BPF_MAP_TYPE_ARRAY_OF_MAPS = 12,
    BPF_MAP_TYPE_HASH_OF_MAPS = 13,
    BPF_MAP_TYPE_DEVMAP = 14,
    BPF_MAP_TYPE_SOCKMAP = 15,
    BPF_MAP_TYPE_CPUMAP = 16,
    BPF_MAP_TYPE_XSKMAP = 17,
    BPF_MAP_TYPE_SOCKHASH = 18,
    BPF_MAP_TYPE_CGROUP_STORAGE = 19,
    BPF_MAP_TYPE_REUSEPORT_SOCKARRAY = 20,
    BPF_MAP_TYPE_PERCPU_CGROUP_STORAGE = 21,
    BPF_MAP_TYPE_QUEUE = 22,
    BPF_MAP_TYPE_STACK = 23,
    BPF_MAP_TYPE_SK_STORAGE = 24,
    BPF_MAP_TYPE_DEVMAP_HASH = 25,
    BPF_MAP_TYPE_STRUCT_OPS = 26,
    BPF_MAP_TYPE_RINGBUF = 27,
    BPF_MAP_TYPE_INODE_STORAGE = 28,
    BPF_MAP_TYPE_TASK_STORAGE = 29,
    BPF_MAP_TYPE_BLOOM_FILTER = 30,
    BPF_MAP_TYPE_USER_RINGBUF = 31,
    BPF_MAP_TYPE_CGRP_STORAGE = 32,
    BPF_MAP_TYPE_ARENA = 33,
};

/* Tracepoint entry header */
struct trace_entry {
    unsigned short type;
    unsigned char  flags;
    unsigned char  preempt_count;
    int            pid;
};

/* Stub structures referenced by bpf_helper_defs.h */
struct __sk_buff { int len; };
struct bpf_sock { __u32 bound_dev_if; };
struct bpf_sock_addr { __u32 user_family; };
struct bpf_sock_ops { __u32 op; };
struct xdp_md { __u32 data; };
struct bpf_cgroup_dev_ctx { __u32 access_type; };
struct bpf_sysctl { __u32 write; };
struct bpf_sockopt { int optname; };
struct sk_msg_md { int family; };
struct bpf_perf_event_data { __u64 addr; };
struct bpf_perf_event_value { __u64 counter; };
struct bpf_pidns_info { __u32 pid; };
struct bpf_sk_lookup { __u32 family; };
struct bpf_ct_opts { __u16 l4proto; };
struct bpf_cpumask { unsigned long bits; };
struct bpf_dynptr { };
struct bpf_map { };
struct bpf_timer { };
struct bpf_spin_lock { int val; };
struct bpf_list_head { };
struct bpf_list_node { };
struct bpf_rb_root { };
struct bpf_rb_node { };
struct bpf_refcount { };
struct linux_binprm { };
struct pt_regs { unsigned long ip; };
struct bpf_tcp_sock { __u32 snd_cwnd; };
struct bpf_tunnel_key { __u32 tunnel_id; };
struct bpf_xfrm_state { __u32 reqid; };
struct tcp_timewait_sock { };
struct tcp_request_sock { };
struct bpf_fib_lookup { __u8 family; };
struct bpf_redir_neigh { __u32 nh_family; };
struct task_struct { int pid; };
struct inode { unsigned long i_ino; };
struct socket { };
struct file { };
struct bpf_flow_keys { __u16 proto; };
struct bpf_func_info { };
struct path { };
struct btf_ptr { };
struct inode_storage_ptr { };
struct task_storage_ptr { };
struct cgroup_storage_ptr { };
struct sk_storage_ptr { };
struct user_namespace { };
struct cgroup { };
struct unix_sock { };
struct mptcp_sock { };

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

    // The generated skeleton compares libbpf's internal object-builder default
    // to decide whether it must pass open options. The public libbpf API only
    // exposes `Default` for that internal state, so keep the generated code
    // intact and allow the lint at the generated-module boundary.

    println!("cargo:rerun-if-changed=src/bpf/kvm_trace.bpf.c");
}
