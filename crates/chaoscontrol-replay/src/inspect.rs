//! Interactive debugger REPL for destructive analysis.
//!
//! Boots a live VM from a recording, then accepts commands to navigate
//! time, inspect state, and modify memory/registers ("what if" analysis).

use crate::debugger::{Debugger, EventFilter};
use crate::recording::Recording;
use crate::replay::{MemoryModification, RegisterModification, ReplayError, SimulationRunner};
use chaoscontrol_vmm::registers::Register;
use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};

/// Run an interactive debug session on a recording.
///
/// Reads commands from stdin, writes output to stdout. Returns when
/// the user types `quit` or stdin closes.
pub fn run_interactive<R: SimulationRunner>(recording: Recording) -> Result<(), ReplayError> {
    let mut debugger = Debugger::<R>::new(recording);
    let mut stdout = io::stdout();
    let stdin = io::stdin();

    println!("ChaosControl interactive debugger");
    println!(
        "Recording: seed={}, {} ticks, {} checkpoints, {} events",
        debugger.recording().seed,
        debugger.recording().total_ticks,
        debugger.recording().checkpoints.len(),
        debugger.recording().events.len(),
    );
    println!("Type 'help' for commands.\n");

    // Pending modifications for the next counterfactual run.
    let mut pending_mem: Vec<MemoryModification> = Vec::new();
    let mut pending_reg: Vec<RegisterModification> = Vec::new();

    loop {
        print!("(tick {}) > ", debugger.state().tick);
        stdout.flush().ok();

        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {
            break; // EOF
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        let cmd = parts[0];

        let result = match cmd {
            "help" | "h" | "?" => {
                print_help();
                Ok(())
            }
            "quit" | "q" | "exit" => break,

            "goto" | "g" => cmd_goto(&mut debugger, &parts),
            "step" | "s" => cmd_step(&mut debugger, &parts),
            "rewind" | "rw" => cmd_rewind(&mut debugger, &parts),
            "bug" => cmd_goto_bug(&mut debugger, &parts),

            "regs" | "r" => cmd_regs(&debugger, &parts),
            "mem" | "x" => cmd_mem(&debugger, &parts),
            "serial" => cmd_serial(&debugger, &parts),
            "events" | "ev" => cmd_events(&debugger, &parts),
            "checkpoints" | "cp" => cmd_checkpoints(&debugger),
            "state" => cmd_state(&debugger),
            "next" => cmd_next_event(&debugger, &parts),

            "poke" => cmd_poke(&mut debugger, &mut pending_mem, &parts),
            "setreg" | "sr" => cmd_setreg(&mut debugger, &mut pending_reg, &parts),
            "pending" => cmd_pending(&pending_mem, &pending_reg),
            "clear" => {
                pending_mem.clear();
                pending_reg.clear();
                println!("Pending modifications cleared.");
                Ok(())
            }
            "whatif" | "cf" => {
                cmd_whatif(&mut debugger, &mut pending_mem, &mut pending_reg, &parts)
            }

            _ => {
                println!("Unknown command: {cmd}. Type 'help' for commands.");
                Ok(())
            }
        };

        if let Err(e) = result {
            println!("Error: {e}");
        }
    }

    println!("Bye.");
    Ok(())
}

fn print_help() {
    println!("Navigation:");
    println!(
        "  goto <tick>           Jump to a tick (restores nearest checkpoint, replays forward)"
    );
    println!("  step [n]              Step forward n ticks (default: 1)");
    println!("  rewind [n]            Rewind n ticks (default: 1)");
    println!("  bug <id>              Jump to the tick where bug #id was detected");
    println!();
    println!("Inspection:");
    println!("  regs [vm] [vcpu]      Show registers (default: VM 0, vCPU 0)");
    println!("  mem <vm> <addr> [n]   Hex dump n bytes at guest phys addr (default: 64)");
    println!("  serial [vm]           Show serial output for VM (default: 0)");
    println!("  events [from] [to]    List events in tick range");
    println!("  next <filter>         Find next event (fault, assertion, fail, bug)");
    println!("  checkpoints           List all checkpoints");
    println!("  state                 Show current debug state");
    println!();
    println!("Destructive analysis:");
    println!("  poke <vm> <addr> <hex>   Write bytes to guest memory (live)");
    println!("  setreg <vm> <vcpu> <reg> <value>  Set a register (live)");
    println!("  pending               Show pending modifications for whatif");
    println!("  clear                 Clear pending modifications");
    println!("  whatif <ticks>         Run counterfactual: apply pending mods, step N ticks");
    println!();
    println!("  quit                  Exit the debugger");
    println!();
    println!("Addresses and values can be decimal or hex (0x prefix).");
    println!("Registers: rax rbx rcx rdx rsi rdi rbp rsp r8-r15 rip rflags");
}

fn parse_u64(s: &str) -> Result<u64, String> {
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).map_err(|e| format!("bad hex: {e}"))
    } else {
        s.parse::<u64>().map_err(|e| format!("bad number: {e}"))
    }
}

fn parse_usize(s: &str) -> Result<usize, String> {
    parse_u64(s).map(|v| v as usize)
}

// ─── Navigation ────────────────────────────────────────────────

fn cmd_goto<R: SimulationRunner>(dbg: &mut Debugger<R>, parts: &[&str]) -> Result<(), ReplayError> {
    let tick = parts
        .get(1)
        .ok_or_else(|| ReplayError::InvalidState {
            message: "usage: goto <tick>".into(),
        })
        .and_then(|s| parse_u64(s).map_err(|e| ReplayError::InvalidState { message: e }))?;
    let state = dbg.goto(tick)?;
    println!(
        "At tick {}  (checkpoint #{})",
        state.tick, state.checkpoint_id
    );
    if !state.events_at_tick.is_empty() {
        for ev in &state.events_at_tick {
            println!("  {ev:?}");
        }
    }
    Ok(())
}

fn cmd_step<R: SimulationRunner>(dbg: &mut Debugger<R>, parts: &[&str]) -> Result<(), ReplayError> {
    let n = parts
        .get(1)
        .map(|s| parse_u64(s))
        .transpose()
        .map_err(|e| ReplayError::InvalidState { message: e })?
        .unwrap_or(1);
    let state = dbg.step_forward(n)?;
    println!("At tick {}", state.tick);
    Ok(())
}

fn cmd_rewind<R: SimulationRunner>(
    dbg: &mut Debugger<R>,
    parts: &[&str],
) -> Result<(), ReplayError> {
    let n = parts
        .get(1)
        .map(|s| parse_u64(s))
        .transpose()
        .map_err(|e| ReplayError::InvalidState { message: e })?
        .unwrap_or(1);
    let state = dbg.rewind(n)?;
    println!("At tick {}", state.tick);
    Ok(())
}

fn cmd_goto_bug<R: SimulationRunner>(
    dbg: &mut Debugger<R>,
    parts: &[&str],
) -> Result<(), ReplayError> {
    let id = parts
        .get(1)
        .ok_or_else(|| ReplayError::InvalidState {
            message: "usage: bug <id>".into(),
        })
        .and_then(|s| parse_u64(s).map_err(|e| ReplayError::InvalidState { message: e }))?;
    let state = dbg.goto_bug(id)?;
    println!("At bug #{id}, tick {}", state.tick);
    Ok(())
}

// ─── Inspection ────────────────────────────────────────────────

fn cmd_regs<R: SimulationRunner>(dbg: &Debugger<R>, parts: &[&str]) -> Result<(), ReplayError> {
    let vm = parts
        .get(1)
        .map(|s| parse_usize(s))
        .transpose()
        .map_err(|e| ReplayError::InvalidState { message: e })?
        .unwrap_or(0);
    let vcpu = parts
        .get(2)
        .map(|s| parse_usize(s))
        .transpose()
        .map_err(|e| ReplayError::InvalidState { message: e })?
        .unwrap_or(0);
    let state = dbg.read_registers(vm, vcpu)?;
    println!("VM{vm} vCPU{vcpu}:");
    println!("{state}");
    Ok(())
}

fn cmd_mem<R: SimulationRunner>(dbg: &Debugger<R>, parts: &[&str]) -> Result<(), ReplayError> {
    if parts.len() < 3 {
        return Err(ReplayError::InvalidState {
            message: "usage: mem <vm> <addr> [size]".into(),
        });
    }
    let vm = parse_usize(parts[1]).map_err(|e| ReplayError::InvalidState { message: e })?;
    let addr = parse_u64(parts[2]).map_err(|e| ReplayError::InvalidState { message: e })?;
    let size = parts
        .get(3)
        .map(|s| parse_usize(s))
        .transpose()
        .map_err(|e| ReplayError::InvalidState { message: e })?
        .unwrap_or(64);

    let data = dbg.read_memory(vm, addr, size)?;
    hex_dump(addr, &data);
    Ok(())
}

fn cmd_serial<R: SimulationRunner>(dbg: &Debugger<R>, parts: &[&str]) -> Result<(), ReplayError> {
    let vm = parts
        .get(1)
        .map(|s| parse_usize(s))
        .transpose()
        .map_err(|e| ReplayError::InvalidState { message: e })?
        .unwrap_or(0);
    let output = dbg.serial_output(vm);
    if output.is_empty() {
        println!("(no serial output for VM{vm} at this checkpoint)");
    } else {
        println!("{output}");
    }
    Ok(())
}

fn cmd_events<R: SimulationRunner>(dbg: &Debugger<R>, parts: &[&str]) -> Result<(), ReplayError> {
    let from = parts
        .get(1)
        .map(|s| parse_u64(s))
        .transpose()
        .map_err(|e| ReplayError::InvalidState { message: e })?
        .unwrap_or(0);
    let to = parts
        .get(2)
        .map(|s| parse_u64(s))
        .transpose()
        .map_err(|e| ReplayError::InvalidState { message: e })?
        .unwrap_or(u64::MAX);
    let events = dbg.events_between(from, to);
    if events.is_empty() {
        println!("(no events in range {from}..{to})");
    } else {
        for ev in events {
            println!("  {ev:?}");
        }
    }
    Ok(())
}

fn cmd_next_event<R: SimulationRunner>(
    dbg: &Debugger<R>,
    parts: &[&str],
) -> Result<(), ReplayError> {
    let filter_name = parts.get(1).unwrap_or(&"any");
    let filter = match *filter_name {
        "fault" | "faults" => EventFilter::AnyFault,
        "assertion" | "assertions" => EventFilter::AnyAssertion,
        "fail" | "failed" => EventFilter::FailedAssertion,
        "bug" | "bugs" => EventFilter::AnyBug,
        "status" => EventFilter::VmStatusChange,
        "serial" => EventFilter::SerialOutput,
        _ => {
            println!("Filters: fault, assertion, fail, bug, status, serial");
            return Ok(());
        }
    };
    match dbg.next_event(filter) {
        Some(ev) => println!("  {ev:?}"),
        None => println!("(no matching event after current tick)"),
    }
    Ok(())
}

fn cmd_checkpoints<R: SimulationRunner>(dbg: &Debugger<R>) -> Result<(), ReplayError> {
    let cps = dbg.checkpoints();
    if cps.is_empty() {
        println!("(no checkpoints in recording)");
    } else {
        println!("{} checkpoint(s):", cps.len());
        for cp in cps {
            let has_snap = if cp.snapshot.is_some() { "✓" } else { "○" };
            println!(
                "  #{:<4}  tick {:<10}  snapshot: {}  events: {}",
                cp.id,
                cp.tick,
                has_snap,
                cp.events_since_last.len()
            );
        }
    }
    Ok(())
}

fn cmd_state<R: SimulationRunner>(dbg: &Debugger<R>) -> Result<(), ReplayError> {
    let s = dbg.state();
    println!("tick: {}", s.tick);
    println!("checkpoint: #{}", s.checkpoint_id);
    println!("VMs: {:?}", s.vm_statuses);
    if !s.events_at_tick.is_empty() {
        println!("Events at this tick:");
        for ev in &s.events_at_tick {
            println!("  {ev:?}");
        }
    }
    Ok(())
}

// ─── Destructive analysis ──────────────────────────────────────

fn cmd_poke<R: SimulationRunner>(
    dbg: &mut Debugger<R>,
    pending_mem: &mut Vec<MemoryModification>,
    parts: &[&str],
) -> Result<(), ReplayError> {
    if parts.len() < 4 {
        return Err(ReplayError::InvalidState {
            message: "usage: poke <vm> <addr> <hex_bytes>".into(),
        });
    }
    let vm = parse_usize(parts[1]).map_err(|e| ReplayError::InvalidState { message: e })?;
    let addr = parse_u64(parts[2]).map_err(|e| ReplayError::InvalidState { message: e })?;
    let hex_str: String = parts[3..].join("");
    let data = parse_hex_bytes(&hex_str).map_err(|e| ReplayError::InvalidState { message: e })?;

    // Apply live
    dbg.poke_memory(vm, addr, &data)?;
    println!("Wrote {} byte(s) to VM{vm} @ {addr:#x}", data.len());

    // Also record for counterfactual
    pending_mem.push(MemoryModification {
        vm_index: vm,
        address: addr,
        data,
    });
    Ok(())
}

fn cmd_setreg<R: SimulationRunner>(
    dbg: &mut Debugger<R>,
    pending_reg: &mut Vec<RegisterModification>,
    parts: &[&str],
) -> Result<(), ReplayError> {
    if parts.len() < 5 {
        return Err(ReplayError::InvalidState {
            message: "usage: setreg <vm> <vcpu> <reg> <value>".into(),
        });
    }
    let vm = parse_usize(parts[1]).map_err(|e| ReplayError::InvalidState { message: e })?;
    let vcpu = parse_usize(parts[2]).map_err(|e| ReplayError::InvalidState { message: e })?;
    let reg: Register = parts[3]
        .parse()
        .map_err(|e: String| ReplayError::InvalidState { message: e })?;
    let value = parse_u64(parts[4]).map_err(|e| ReplayError::InvalidState { message: e })?;

    dbg.set_register(vm, vcpu, reg, value)?;
    println!("Set VM{vm} vCPU{vcpu} {reg} = {value:#x}");

    // Record for counterfactual
    let mut changes = BTreeMap::new();
    changes.insert(reg, value);
    pending_reg.push(RegisterModification {
        vm_index: vm,
        vcpu,
        changes,
    });
    Ok(())
}

fn cmd_pending(
    pending_mem: &[MemoryModification],
    pending_reg: &[RegisterModification],
) -> Result<(), ReplayError> {
    if pending_mem.is_empty() && pending_reg.is_empty() {
        println!("(no pending modifications)");
        return Ok(());
    }
    if !pending_mem.is_empty() {
        println!("Memory:");
        for m in pending_mem {
            println!(
                "  VM{} @ {:#x}: {} byte(s)",
                m.vm_index,
                m.address,
                m.data.len()
            );
        }
    }
    if !pending_reg.is_empty() {
        println!("Registers:");
        for r in pending_reg {
            for (reg, val) in &r.changes {
                println!("  VM{} vCPU{} {reg} = {val:#x}", r.vm_index, r.vcpu);
            }
        }
    }
    Ok(())
}

fn cmd_whatif<R: SimulationRunner>(
    dbg: &mut Debugger<R>,
    pending_mem: &mut Vec<MemoryModification>,
    pending_reg: &mut Vec<RegisterModification>,
    parts: &[&str],
) -> Result<(), ReplayError> {
    let ticks = parts
        .get(1)
        .ok_or_else(|| ReplayError::InvalidState {
            message: "usage: whatif <ticks>".into(),
        })
        .and_then(|s| parse_u64(s).map_err(|e| ReplayError::InvalidState { message: e }))?;

    let mem_mods = std::mem::take(pending_mem);
    let reg_mods = std::mem::take(pending_reg);
    let n_mem = mem_mods.len();
    let n_reg = reg_mods.len();

    println!("Running counterfactual: {n_mem} memory mod(s), {n_reg} register mod(s), {ticks} tick(s)...");

    let result = dbg.counterfactual(mem_mods, reg_mods, ticks)?;

    println!("Executed {} ticks.", result.ticks_executed);
    println!(
        "Oracle: {} passed, {} failed, {} unexercised",
        result.oracle_report.passed, result.oracle_report.failed, result.oracle_report.unexercised,
    );

    if result.oracle_report.failed > 0 {
        println!("\n⚠  Failures:");
        for (identity, info) in result.oracle_report.all_records() {
            if info.verdict() == chaoscontrol_fault::oracle::Verdict::Failed {
                println!(
                    "  ✗ Assertion {identity:?} ({:?}): {}",
                    info.kind, info.message
                );
            }
        }
    }

    for (i, output) in result.serial_output.iter().enumerate() {
        if !output.is_empty() {
            let tail = if output.len() > 300 {
                &output[output.len() - 300..]
            } else {
                output.as_str()
            };
            println!("\nVM{i} serial (tail):\n{tail}");
        }
    }

    Ok(())
}

// ─── Hex helpers ───────────────────────────────────────────────

fn parse_hex_bytes(s: &str) -> Result<Vec<u8>, String> {
    let s = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    if !s.len().is_multiple_of(2) {
        return Err("hex string must have even length".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|e| format!("bad hex byte at offset {i}: {e}"))
        })
        .collect()
}

fn hex_dump(base_addr: u64, data: &[u8]) {
    for (i, chunk) in data.chunks(16).enumerate() {
        let addr = base_addr + (i * 16) as u64;
        print!("{addr:016x}  ");

        // Hex bytes
        for (j, byte) in chunk.iter().enumerate() {
            if j == 8 {
                print!(" ");
            }
            print!("{byte:02x} ");
        }
        // Pad if short line
        let padding = 16 - chunk.len();
        for j in 0..padding {
            if chunk.len() + j == 8 {
                print!(" ");
            }
            print!("   ");
        }

        // ASCII
        print!(" |");
        for byte in chunk {
            if byte.is_ascii_graphic() || *byte == b' ' {
                print!("{}", *byte as char);
            } else {
                print!(".");
            }
        }
        println!("|");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_bytes_simple() {
        assert_eq!(
            parse_hex_bytes("deadbeef").unwrap(),
            vec![0xde, 0xad, 0xbe, 0xef]
        );
    }

    #[test]
    fn parse_hex_bytes_with_prefix() {
        assert_eq!(parse_hex_bytes("0xCAFE").unwrap(), vec![0xca, 0xfe]);
    }

    #[test]
    fn parse_hex_bytes_odd_length() {
        assert!(parse_hex_bytes("abc").is_err());
    }

    #[test]
    fn parse_hex_bytes_empty() {
        assert_eq!(parse_hex_bytes("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn parse_u64_decimal() {
        assert_eq!(parse_u64("42").unwrap(), 42);
    }

    #[test]
    fn parse_u64_hex() {
        assert_eq!(parse_u64("0x1000").unwrap(), 0x1000);
    }

    #[test]
    fn parse_u64_hex_upper() {
        assert_eq!(parse_u64("0XFF").unwrap(), 255);
    }

    #[test]
    fn parse_u64_bad() {
        assert!(parse_u64("xyz").is_err());
    }
}
