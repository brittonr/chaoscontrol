## Context

When a ProcessKill or NMI fault triggers a kernel panic, the kernel
attempts to reboot (`panic=-1`). With no BIOS, the reboot triggers a
triple fault, but before the triple fault fires, the panic handler
enters a recursive exception cascade: GPF → `irqentry_enter` stack
overflow → GPF → ... This generates unlimited serial I/O output
(register dumps), keeping `exits_since_last_sdk` advancing and
preventing the idle detector from ever triggering. The VM loops
forever.

The VMM already handles `VcpuExit::Hlt` (injects timer IRQ and
advances virtual TSC) and `VcpuExit::Shutdown` (returns halted). The
problem is that the recursive exception cascade never reaches either
of these clean exit states.

## Goals / Non-Goals

**Goals:**
- VMs that kernel-panic stop executing within a bounded number of
  exits
- Exploration runs with ProcessKill/NMI faults complete instead of
  hanging
- Zero performance cost on the non-panic path

**Non-Goals:**
- Guest-side changes (no SDK modifications)
- Recovering a panicked VM (it's dead — the controller already
  handles restart scheduling)
- Detecting application-level crashes vs kernel panics

## Decisions

### 1. Switch to `panic=0` (halt on panic)

**Choice:** Replace `panic=-1` with `panic=0` in both kernel cmdline
templates.

**Rationale:** `panic=0` tells the kernel to halt (HLT instruction)
after printing the panic message. HLT produces `VcpuExit::Hlt` which
the VMM already handles. The idle detector will fire naturally because
HLT in a loop with no SDK calls hits the threshold. This alone fixes
the common case.

**Alternatives:**
- `panic=1` (reboot after 1 second): still causes the triple-fault
  loop since there's no BIOS.
- Keep `panic=-1` and fix the VMM: more complex, doesn't address the
  root cause.

### 2. Serial panic detection as defense in depth

**Choice:** Add a `panic_detected: bool` flag to `DeterministicVm`.
In the serial I/O write path, scan for the byte sequence
`Kernel panic`. When found, set the flag. In `step()`, check
the flag after every exit — if set, return `Ok(true)` (halted).

**Why not just rely on `panic=0`?** The recursive GPF cascade can
start before the kernel reaches the HLT in the panic handler. The
serial output arrives earlier (the panic message is printed first),
so detecting it provides faster termination. Also covers edge cases
where the kernel's panic path itself faults before reaching HLT.

**Implementation:** Use a small ring buffer match (8 bytes of
`"Kernel p"` is enough). The serial write handler already processes
one byte at a time, so checking is O(1) per byte. No allocation
needed — just shift a u64 and compare.

### 3. Panic flag reset on restore

**Choice:** Clear `panic_detected` in `restore()`, same as the
existing `exits_since_last_sdk = 0` reset.

**Rationale:** A panic in branch A must not affect branch B which
restores from a pre-panic snapshot.

## Risks / Trade-offs

**[Risk] `panic=0` changes kernel behavior for all guests** →
All current guests (SDK, Raft, net) are designed to be killed by the
VMM. None rely on panic-reboot behavior. The only observable change
is that crashed VMs halt instead of looping.

**[Risk] Serial panic detection could match guest user-space output** →
If a guest program prints "Kernel panic" to the serial port, it would
trigger false detection. Mitigation: this string is unlikely in
normal output, and the consequence (VM marked halted) is benign for
exploration — the branch just ends early.

**[Trade-off] Two detection mechanisms (HLT + serial)** →
Belt and suspenders. Either alone is sufficient for most cases, but
together they cover the full space: serial catches the fast path
(panic message arrives before HLT), HLT catches the slow path (panic
handler completes normally).
