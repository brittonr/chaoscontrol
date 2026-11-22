//! Opt-in, uninterrupted Wasmtime 49.0.5 reference execution. No WASI or effects.

use ::wasmtime::{
    Caller, Config, Engine, Extern, Instance, Linker, Module, Store, StoreLimits,
    StoreLimitsBuilder, Trap,
};

use super::case::span;
use super::host::{RequestTrap, Run};
use super::{
    AdmittedCase, DiagnosticObservation, Terminal, HOSTCALL_MODULE, HOSTCALL_NAMES, WASM_PAGE_BYTES,
};

// Explicitly retain the selected engine's standard 512 KiB Wasm stack ceiling.
const REFERENCE_STACK_BYTES: usize = 512 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum HarnessIssue {
    #[error("cannot initialize the reference engine")]
    Engine,
    #[error("reference engine could not compile admitted bytes")]
    Compile,
    #[error("reference engine could not link the exact admitted imports")]
    Link,
    #[error("reference engine could not instantiate the admitted actor")]
    Instantiate,
    #[error("reference engine did not expose the admitted ABI")]
    Exports,
    #[error("reference engine failed without a recognized guest outcome")]
    Invocation,
    #[error("reference engine did not retain the configured cumulative fuel")]
    Fuel,
}

struct Host {
    run: Run,
    limits: StoreLimits,
}

/// Runs an admitted diagnostic case. A pause request is observed but does not
/// suspend the uninterrupted reference run. There is one budget for every call.
// r[impl chaoscontrol.spacewasm_full_abi.host]
pub fn run(case: &AdmittedCase<'_>) -> Result<DiagnosticObservation, HarnessIssue> {
    let mut config = Config::new();
    config
        .consume_fuel(true)
        .max_wasm_stack(REFERENCE_STACK_BYTES)
        .wasm_backtrace_max_frames(None);
    let engine = Engine::new(&config).map_err(|_| HarnessIssue::Engine)?;
    let module = Module::new(&engine, case.module).map_err(|_| HarnessIssue::Compile)?;
    let memory_bytes = (case.admission.memory_maximum_pages * WASM_PAGE_BYTES) as usize;
    let limits = StoreLimitsBuilder::new()
        .memory_size(memory_bytes)
        .memories(1)
        .instances(1)
        .tables(0)
        .build();
    let mut store = Store::new(
        &engine,
        Host {
            run: case.script.start(),
            limits,
        },
    );
    store.limiter(|host| &mut host.limits);
    let fuel = case.profile.bounds.wasmtime_fuel;
    store.set_fuel(fuel).map_err(|_| HarnessIssue::Fuel)?;

    let mut linker = Linker::new(&engine);
    for name in HOSTCALL_NAMES {
        if case.admission.hostcalls.iter().any(|import| import == name) {
            linker
                .func_wrap(
                    HOSTCALL_MODULE,
                    name,
                    move |mut caller: Caller<'_, Host>, pointer: i32, length: i32| {
                        let memory = caller
                            .get_export("memory")
                            .and_then(Extern::into_memory)
                            .ok_or_else(|| {
                                ::wasmtime::format_err!("admitted guest memory is unavailable")
                            })?;
                        let (memory, host) = memory.data_and_store_mut(&mut caller);
                        host.run
                            .invoke(name, pointer, length, memory)
                            .map(|reply| reply.descriptor)
                            .map_err(::wasmtime::Error::new)
                    },
                )
                .map_err(|_| HarnessIssue::Link)?;
        }
    }
    let instance = linker
        .instantiate(&mut store, &module)
        .map_err(|_| HarnessIssue::Instantiate)?;
    let terminal = match invoke(case, &instance, &mut store) {
        Ok(terminal) | Err(InvocationIssue::Guest(terminal)) => terminal,
        Err(InvocationIssue::Harness(issue)) => return Err(issue),
    };
    let remaining = store.get_fuel().map_err(|_| HarnessIssue::Fuel)?;
    let fuel_consumed = fuel.checked_sub(remaining).ok_or(HarnessIssue::Fuel)?;
    Ok(DiagnosticObservation {
        terminal,
        fuel_consumed,
        transcript: store.data().run.transcript().copied().collect(),
    })
}

fn invoke(
    case: &AdmittedCase<'_>,
    instance: &Instance,
    store: &mut Store<Host>,
) -> Result<Terminal, InvocationIssue> {
    let missing_export = InvocationIssue::Harness(HarnessIssue::Exports);
    let memory = instance
        .get_memory(&mut *store, "memory")
        .ok_or(missing_export)?;
    let alloc = instance
        .get_typed_func::<i32, i32>(&mut *store, "molten_alloc")
        .map_err(|_| missing_export)?;
    let dealloc = instance
        .get_typed_func::<(i32, i32), ()>(&mut *store, "molten_dealloc")
        .map_err(|_| missing_export)?;
    let entry = instance
        .get_typed_func::<(i32, i32), i64>(&mut *store, "molten_run")
        .map_err(|_| missing_export)?;

    let input_length = case.input.len() as i32;
    let input_pointer = alloc.call(&mut *store, input_length)?;
    let range = span(input_pointer, case.input.len())
        .ok_or(InvocationIssue::Guest(Terminal::InputRange))?;
    memory
        .write(&mut *store, range.start, &case.input)
        .map_err(|_| InvocationIssue::Guest(Terminal::InputRange))?;
    let descriptor = entry.call(&mut *store, (input_pointer, input_length))?;
    dealloc.call(&mut *store, (input_pointer, input_length))?;
    let output = case
        .match_output(memory.data(&*store), descriptor)
        .map_err(InvocationIssue::Guest)?;
    dealloc.call(&mut *store, (output.pointer, output.length))?;
    if !store.data().run.is_complete() {
        return Ok(Terminal::IncompleteHostcalls);
    }
    Ok(Terminal::Completed {
        output_blake3: output.blake3,
        output_bytes: output.length as u32,
    })
}

#[derive(Clone, Copy)]
enum InvocationIssue {
    Guest(Terminal),
    Harness(HarnessIssue),
}

impl From<::wasmtime::Error> for InvocationIssue {
    fn from(error: ::wasmtime::Error) -> Self {
        if let Some(trap) = error.downcast_ref::<RequestTrap>() {
            return Self::Guest(Terminal::Hostcall(*trap));
        }
        let class = match error.downcast_ref::<Trap>() {
            Some(Trap::MemoryOutOfBounds) => Terminal::MemoryTrap,
            Some(Trap::StackOverflow) => Terminal::StackLimit,
            Some(Trap::OutOfFuel) => Terminal::FuelExhausted,
            Some(Trap::UnreachableCodeReached) => Terminal::GuestTrap,
            _ => return Self::Harness(HarnessIssue::Invocation),
        };
        Self::Guest(class)
    }
}
