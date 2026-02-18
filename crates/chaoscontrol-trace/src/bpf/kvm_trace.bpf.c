// SPDX-License-Identifier: GPL-2.0 OR BSD-3-Clause
// KVM tracing BPF program
// Attaches to KVM tracepoints and emits structured events to ring buffer

#include "vmlinux.h"
#include <bpf/bpf_helpers.h>

// Event type constants (must match Rust side)
#define EVENT_KVM_EXIT       1
#define EVENT_KVM_ENTRY      2
#define EVENT_KVM_PIO        3
#define EVENT_KVM_MMIO       4
#define EVENT_KVM_MSR        5
#define EVENT_KVM_INJ_VIRQ   6
#define EVENT_KVM_PIC_IRQ    7
#define EVENT_KVM_SET_IRQ    8
#define EVENT_KVM_PAGE_FAULT 9
#define EVENT_KVM_CR        10
#define EVENT_KVM_CPUID     11

// Event structure (64 bytes, cache-line aligned)
struct cc_trace_event {
    __u64 seq;           // monotonic per-CPU sequence number
    __u64 host_ns;       // bpf_ktime_get_ns()
    __u32 event_type;    // EVENT_* constant
    __u32 pid;           // current PID
    __u64 arg0;          // event-specific
    __u64 arg1;          // event-specific
    __u64 arg2;          // event-specific
    __u64 arg3;          // event-specific
};


// Tracepoint context structures
struct tp_kvm_exit {
    struct trace_entry ent;
    unsigned int exit_reason;
    unsigned long guest_rip;
    u32 isa;
    u64 info1;
    u64 info2;
    u32 intr_info;
    u32 error_code;
    unsigned int vcpu_id;
    u64 requests;
};

struct tp_kvm_entry {
    struct trace_entry ent;
    unsigned int vcpu_id;
    unsigned long rip;
    bool immediate_exit;
    u32 intr_info;
    u32 error_code;
};

struct tp_kvm_pio {
    struct trace_entry ent;
    unsigned int rw;
    unsigned int port;
    unsigned int size;
    unsigned int count;
    unsigned int val;
};

struct tp_kvm_mmio {
    struct trace_entry ent;
    u32 type;
    u32 len;
    u64 gpa;
    u64 val;
};

struct tp_kvm_msr {
    struct trace_entry ent;
    unsigned int write;
    u32 ecx;
    u64 data;
    u8 exception;
};

struct tp_kvm_inj_virq {
    struct trace_entry ent;
    unsigned int vector;
    bool soft;
    bool reinjected;
};

struct tp_kvm_pic_set_irq {
    struct trace_entry ent;
    __u8 chip;
    __u8 pin;
    __u8 elcr;
    __u8 imr;
    bool coalesced;
};

struct tp_kvm_set_irq {
    struct trace_entry ent;
    unsigned int gsi;
    int level;
    int irq_source_id;
};

struct tp_kvm_page_fault {
    struct trace_entry ent;
    unsigned int vcpu_id;
    unsigned long guest_rip;
    u64 fault_address;
    u64 error_code;
};

struct tp_kvm_cr {
    struct trace_entry ent;
    unsigned int rw;
    unsigned int cr;
    unsigned long val;
};

struct tp_kvm_cpuid {
    struct trace_entry ent;
    unsigned int function;
    unsigned int index;
    unsigned long rax;
    unsigned long rbx;
    unsigned long rcx;
    unsigned long rdx;
    bool found;
    bool used_max_basic;
};

// BPF maps
struct {
    __uint(type, BPF_MAP_TYPE_ARRAY);
    __uint(max_entries, 1);
    __type(key, __u32);
    __type(value, __u32);
} target_pid SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, 16 * 1024 * 1024); // 16MB
} events SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_PERCPU_ARRAY);
    __uint(max_entries, 1);
    __type(key, __u32);
    __type(value, __u64);
} event_seq SEC(".maps");

// Helper: Check if current PID matches target
static __always_inline bool is_target(void) {
    __u32 key = 0;
    __u32 *target = bpf_map_lookup_elem(&target_pid, &key);
    if (!target)
        return false;
    
    __u32 pid = bpf_get_current_pid_tgid() >> 32;
    return pid == *target;
}

// Helper: Get next sequence number for this CPU
static __always_inline __u64 next_seq(void) {
    __u32 key = 0;
    __u64 *seq = bpf_map_lookup_elem(&event_seq, &key);
    if (!seq)
        return 0;
    
    __u64 val = __sync_fetch_and_add(seq, 1);
    return val;
}

// kvm_exit: exit_reason, guest_rip, info1, info2
SEC("tracepoint/kvm/kvm_exit")
int trace_kvm_exit(struct tp_kvm_exit *ctx) {
    if (!is_target())
        return 0;
    
    struct cc_trace_event *e = bpf_ringbuf_reserve(&events, sizeof(*e), 0);
    if (!e)
        return 0;
    
    e->seq = next_seq();
    e->host_ns = bpf_ktime_get_ns();
    e->event_type = EVENT_KVM_EXIT;
    e->pid = bpf_get_current_pid_tgid() >> 32;
    e->arg0 = ctx->exit_reason;
    e->arg1 = ctx->guest_rip;
    e->arg2 = ctx->info1;
    e->arg3 = ctx->info2;
    
    bpf_ringbuf_submit(e, 0);
    return 0;
}

// kvm_entry: vcpu_id, rip
SEC("tracepoint/kvm/kvm_entry")
int trace_kvm_entry(struct tp_kvm_entry *ctx) {
    if (!is_target())
        return 0;
    
    struct cc_trace_event *e = bpf_ringbuf_reserve(&events, sizeof(*e), 0);
    if (!e)
        return 0;
    
    e->seq = next_seq();
    e->host_ns = bpf_ktime_get_ns();
    e->event_type = EVENT_KVM_ENTRY;
    e->pid = bpf_get_current_pid_tgid() >> 32;
    e->arg0 = ctx->vcpu_id;
    e->arg1 = ctx->rip;
    e->arg2 = 0;
    e->arg3 = 0;
    
    bpf_ringbuf_submit(e, 0);
    return 0;
}

// kvm_pio: rw, port, size, val
SEC("tracepoint/kvm/kvm_pio")
int trace_kvm_pio(struct tp_kvm_pio *ctx) {
    if (!is_target())
        return 0;
    
    struct cc_trace_event *e = bpf_ringbuf_reserve(&events, sizeof(*e), 0);
    if (!e)
        return 0;
    
    e->seq = next_seq();
    e->host_ns = bpf_ktime_get_ns();
    e->event_type = EVENT_KVM_PIO;
    e->pid = bpf_get_current_pid_tgid() >> 32;
    e->arg0 = ctx->rw;
    e->arg1 = ctx->port;
    e->arg2 = ctx->size;
    e->arg3 = ctx->val;
    
    bpf_ringbuf_submit(e, 0);
    return 0;
}

// kvm_mmio: type, len, gpa, val
SEC("tracepoint/kvm/kvm_mmio")
int trace_kvm_mmio(struct tp_kvm_mmio *ctx) {
    if (!is_target())
        return 0;
    
    struct cc_trace_event *e = bpf_ringbuf_reserve(&events, sizeof(*e), 0);
    if (!e)
        return 0;
    
    e->seq = next_seq();
    e->host_ns = bpf_ktime_get_ns();
    e->event_type = EVENT_KVM_MMIO;
    e->pid = bpf_get_current_pid_tgid() >> 32;
    e->arg0 = ctx->type;
    e->arg1 = ctx->len;
    e->arg2 = ctx->gpa;
    e->arg3 = ctx->val;
    
    bpf_ringbuf_submit(e, 0);
    return 0;
}

// kvm_msr: write, ecx, data
SEC("tracepoint/kvm/kvm_msr")
int trace_kvm_msr(struct tp_kvm_msr *ctx) {
    if (!is_target())
        return 0;
    
    struct cc_trace_event *e = bpf_ringbuf_reserve(&events, sizeof(*e), 0);
    if (!e)
        return 0;
    
    e->seq = next_seq();
    e->host_ns = bpf_ktime_get_ns();
    e->event_type = EVENT_KVM_MSR;
    e->pid = bpf_get_current_pid_tgid() >> 32;
    e->arg0 = ctx->write;
    e->arg1 = ctx->ecx;
    e->arg2 = ctx->data;
    e->arg3 = 0;
    
    bpf_ringbuf_submit(e, 0);
    return 0;
}

// kvm_inj_virq: vector, soft, reinjected
SEC("tracepoint/kvm/kvm_inj_virq")
int trace_kvm_inj_virq(struct tp_kvm_inj_virq *ctx) {
    if (!is_target())
        return 0;
    
    struct cc_trace_event *e = bpf_ringbuf_reserve(&events, sizeof(*e), 0);
    if (!e)
        return 0;
    
    e->seq = next_seq();
    e->host_ns = bpf_ktime_get_ns();
    e->event_type = EVENT_KVM_INJ_VIRQ;
    e->pid = bpf_get_current_pid_tgid() >> 32;
    e->arg0 = ctx->vector;
    e->arg1 = ctx->soft;
    e->arg2 = ctx->reinjected;
    e->arg3 = 0;
    
    bpf_ringbuf_submit(e, 0);
    return 0;
}

// kvm_pic_set_irq: chip, pin, elcr, imr, coalesced
SEC("tracepoint/kvm/kvm_pic_set_irq")
int trace_kvm_pic_set_irq(struct tp_kvm_pic_set_irq *ctx) {
    if (!is_target())
        return 0;
    
    struct cc_trace_event *e = bpf_ringbuf_reserve(&events, sizeof(*e), 0);
    if (!e)
        return 0;
    
    e->seq = next_seq();
    e->host_ns = bpf_ktime_get_ns();
    e->event_type = EVENT_KVM_PIC_IRQ;
    e->pid = bpf_get_current_pid_tgid() >> 32;
    e->arg0 = ((__u64)ctx->chip << 32) | ((__u64)ctx->pin << 16) | ((__u64)ctx->elcr << 8) | ctx->imr;
    e->arg1 = ctx->coalesced;
    e->arg2 = 0;
    e->arg3 = 0;
    
    bpf_ringbuf_submit(e, 0);
    return 0;
}

// kvm_set_irq: gsi, level
SEC("tracepoint/kvm/kvm_set_irq")
int trace_kvm_set_irq(struct tp_kvm_set_irq *ctx) {
    if (!is_target())
        return 0;
    
    struct cc_trace_event *e = bpf_ringbuf_reserve(&events, sizeof(*e), 0);
    if (!e)
        return 0;
    
    e->seq = next_seq();
    e->host_ns = bpf_ktime_get_ns();
    e->event_type = EVENT_KVM_SET_IRQ;
    e->pid = bpf_get_current_pid_tgid() >> 32;
    e->arg0 = ctx->gsi;
    e->arg1 = ctx->level;
    e->arg2 = 0;
    e->arg3 = 0;
    
    bpf_ringbuf_submit(e, 0);
    return 0;
}

// kvm_page_fault: vcpu_id, guest_rip, fault_address, error_code
SEC("tracepoint/kvm/kvm_page_fault")
int trace_kvm_page_fault(struct tp_kvm_page_fault *ctx) {
    if (!is_target())
        return 0;
    
    struct cc_trace_event *e = bpf_ringbuf_reserve(&events, sizeof(*e), 0);
    if (!e)
        return 0;
    
    e->seq = next_seq();
    e->host_ns = bpf_ktime_get_ns();
    e->event_type = EVENT_KVM_PAGE_FAULT;
    e->pid = bpf_get_current_pid_tgid() >> 32;
    e->arg0 = ctx->vcpu_id;
    e->arg1 = ctx->guest_rip;
    e->arg2 = ctx->fault_address;
    e->arg3 = ctx->error_code;
    
    bpf_ringbuf_submit(e, 0);
    return 0;
}

// kvm_cr: rw, cr, val
SEC("tracepoint/kvm/kvm_cr")
int trace_kvm_cr(struct tp_kvm_cr *ctx) {
    if (!is_target())
        return 0;
    
    struct cc_trace_event *e = bpf_ringbuf_reserve(&events, sizeof(*e), 0);
    if (!e)
        return 0;
    
    e->seq = next_seq();
    e->host_ns = bpf_ktime_get_ns();
    e->event_type = EVENT_KVM_CR;
    e->pid = bpf_get_current_pid_tgid() >> 32;
    e->arg0 = ctx->rw;
    e->arg1 = ctx->cr;
    e->arg2 = ctx->val;
    e->arg3 = 0;
    
    bpf_ringbuf_submit(e, 0);
    return 0;
}

// kvm_cpuid: function, index, rax, rbx, rcx, rdx
// Note: We pack rcx and rdx together since we only have 4 arg slots
SEC("tracepoint/kvm/kvm_cpuid")
int trace_kvm_cpuid(struct tp_kvm_cpuid *ctx) {
    if (!is_target())
        return 0;
    
    struct cc_trace_event *e = bpf_ringbuf_reserve(&events, sizeof(*e), 0);
    if (!e)
        return 0;
    
    e->seq = next_seq();
    e->host_ns = bpf_ktime_get_ns();
    e->event_type = EVENT_KVM_CPUID;
    e->pid = bpf_get_current_pid_tgid() >> 32;
    e->arg0 = ((__u64)ctx->function << 32) | ctx->index;
    e->arg1 = ctx->rax;
    e->arg2 = ctx->rbx;
    e->arg3 = (ctx->rcx << 32) | (ctx->rdx & 0xFFFFFFFF);
    
    bpf_ringbuf_submit(e, 0);
    return 0;
}

char LICENSE[] SEC("license") = "GPL";
