/* SPDX-License-Identifier: (LGPL-2.1 OR BSD-2-Clause) */
/*
 * Minimal vmlinux.h stub for BPF compilation in sandboxed environments
 * (e.g. Nix build) where /sys/kernel/btf/vmlinux is unavailable.
 *
 * Provides only the types referenced by kvm_trace.bpf.c.
 * On a real host, build.rs generates the full vmlinux.h from BTF.
 */

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

enum {
    false = 0,
    true  = 1,
};

/*
 * struct trace_entry — first 8 bytes of every tracepoint context.
 * Must match the kernel's struct trace_entry layout exactly.
 */
struct trace_entry {
    unsigned short type;
    unsigned char  flags;
    unsigned char  preempt_count;
    int            pid;
};

#endif /* __VMLINUX_H__ */
