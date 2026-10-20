use std::alloc::Layout;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};

use spacewasm::{
    AllocError, Allocator, CodeBuilder, CompilerOptions, ExportDesc, InnerVec, Interpreter,
    InterpreterResult, InterpreterRunner, MemoryStatistics, Module, ModuleRef, Rc, Ref, Store,
    ValType, WasmMemoryAllocator, WasmRef, WasmStream,
};

const SOURCE_REVISION: &str = "e24cf09355a90497148eb5029fdb8e3400bd63e3";
const REPORT_SCHEMA: &str = "chaoscontrol.spacewasm-resume-probe.v2";
const MAXIMUM_INPUT_BYTES: usize = 1_048_576;
const MAXIMUM_OUTPUT_BYTES: usize = 1_048_576;
const MAXIMUM_CODE_PAGES: usize = 64;
const MAXIMUM_CONTROL_FRAMES: usize = 64;
const MAXIMUM_STACK_WORDS: usize = 1_024;
const INITIALIZATION_FUEL: usize = 1_024;
const SEGMENT_FUEL: usize = 1;
const MAXIMUM_SEGMENTS: usize = 4_096;
const MAXIMUM_CHUNKS: usize = 4_096;
const MAXIMUM_MODULES: usize = 1;
const MAXIMUM_GLOBALS: usize = 64;
const MAXIMUM_ALLOCATION_BYTES: usize = 16_777_216;
const DEFAULT_SEED: u64 = 1_469_598_103_934_665_603;
const MAXIMUM_GENERATED_CHUNK_BYTES: usize = 64;

// This is the SpaceWasm allocator, not Rust's process-global allocator. Account for
// both producer allocation interfaces, including temporary storage during realloc.
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static DENIALS: AtomicUsize = AtomicUsize::new(0);
static SYSTEM_FAILURES: AtomicUsize = AtomicUsize::new(0);
static ALLOCATION_LIMIT: AtomicUsize = AtomicUsize::new(MAXIMUM_ALLOCATION_BYTES);

fn reserve(bytes: usize) -> Result<(), AllocError> {
    let mut current = LIVE.load(Ordering::SeqCst);
    loop {
        let Some(next) = current.checked_add(bytes).filter(|next| {
            *next <= ALLOCATION_LIMIT.load(Ordering::SeqCst)
        }) else {
            DENIALS.fetch_add(1, Ordering::SeqCst);
            return Err(AllocError::OutOfMemory);
        };
        match LIVE.compare_exchange(current, next, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return Ok(()),
            Err(actual) => current = actual,
        }
    }
}

#[derive(Clone, Copy)]
struct ProbeAllocator;

unsafe impl Allocator for ProbeAllocator {
    unsafe fn alloc(&self, layout: Layout) -> Result<*mut u8, AllocError> {
        reserve(layout.size())?;
        let pointer = unsafe { std::alloc::alloc(layout) };
        if pointer.is_null() {
            LIVE.fetch_sub(layout.size(), Ordering::SeqCst);
            SYSTEM_FAILURES.fetch_add(1, Ordering::SeqCst);
            Err(AllocError::AllocationFailed)
        } else {
            PEAK.fetch_max(LIVE.load(Ordering::SeqCst), Ordering::SeqCst);
            ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
            Ok(pointer)
        }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if !pointer.is_null() {
            unsafe { std::alloc::dealloc(pointer, layout) };
            LIVE.fetch_sub(layout.size(), Ordering::SeqCst);
        }
    }

    fn memory_statistics(&self) -> MemoryStatistics {
        MemoryStatistics {
            // The admitted cap is less than i32::MAX; no truncation or invented zero.
            total_bytes: LIVE.load(Ordering::SeqCst) as i32,
            pad_bytes: 0,
        }
    }
}

impl WasmMemoryAllocator for ProbeAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<u8>, AllocError> {
        if layout.size() == 0 {
            return NonNull::new(layout.align() as *mut u8).ok_or(AllocError::AllocationFailed);
        }
        NonNull::new(unsafe { Allocator::alloc(self, layout)? })
            .ok_or(AllocError::AllocationFailed)
    }

    fn reallocate(
        &self,
        pointer: NonNull<u8>,
        old_layout: Layout,
        layout: Layout,
    ) -> Result<NonNull<u8>, AllocError> {
        // Allocate-copy-free handles alignment changes and counts the actual
        // simultaneous old/new storage. Failure leaves the original intact.
        let new = self.allocate(layout)?;
        unsafe {
            std::ptr::copy_nonoverlapping(
                pointer.as_ptr(), new.as_ptr(), old_layout.size().min(layout.size()),
            );
        }
        self.deallocate(pointer, old_layout);
        Ok(new)
    }

    fn deallocate(&self, pointer: NonNull<u8>, layout: Layout) {
        if layout.size() != 0 {
            unsafe { Allocator::dealloc(self, pointer.as_ptr(), layout) };
        }
    }
}

spacewasm::global_allocator!(ProbeAllocator, ProbeAllocator);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Partition {
    offset: usize,
    length: usize,
}

// r[impl chaoscontrol.spacewasm_mvp.streaming]
fn validate_partition(parts: &[Partition], byte_count: usize) -> Result<(), &'static str> {
    if parts.is_empty() || parts.len() > MAXIMUM_CHUNKS {
        return Err("chunk-count-bound");
    }
    let mut next = 0usize;
    for part in parts {
        if part.length == 0 { return Err("empty-chunk"); }
        if part.offset != next { return Err("noncontiguous-or-reordered-partition"); }
        next = next.checked_add(part.length).ok_or("partition-overflow")?;
        if next > byte_count { return Err("partition-extends-input"); }
    }
    if next != byte_count { return Err("partition-omits-input"); }
    Ok(())
}

fn generate_partition(kind: &str, seed: u64, bytes: usize) -> Result<Vec<Partition>, String> {
    let mut parts = Vec::new();
    let mut offset = 0;
    let mut random = seed;
    while offset < bytes {
        if parts.len() == MAXIMUM_CHUNKS {
            return Err(String::from("chunk-count-bound"));
        }
        let length = match kind {
            "complete" => bytes,
            "one-byte" | "omit" | "overlap" | "reorder" | "extend" => 1,
            "seeded" => {
                random = random.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                1 + ((random >> 32) as usize % MAXIMUM_GENERATED_CHUNK_BYTES)
            }
            _ => return Err(String::from("unknown partition generator")),
        }.min(bytes - offset);
        parts.push(Partition { offset, length });
        offset += length;
    }
    match kind {
        "omit" => { parts.pop(); }
        "overlap" if parts.len() >= 2 => parts[1].offset = 0,
        "reorder" if parts.len() >= 2 => parts.swap(0, 1),
        "extend" => {
            if let Some(last) = parts.last_mut() { last.length += 1; }
        }
        _ => {}
    }
    Ok(parts)
}

// Borrow the input buffer without per-run or per-chunk copies. An outstanding
// descriptor prevents its backing bytes from being reused before return_.
struct ChunkStream<'a> {
    bytes: &'a mut [u8],
    parts: &'a [Partition],
    next: usize,
    outstanding: Option<Partition>,
    returned: usize,
    returned_bytes: usize,
    violations: usize,
}

impl<'a> ChunkStream<'a> {
    fn new(bytes: &'a mut [u8], parts: &'a [Partition]) -> Self {
        Self { bytes, parts, next: 0, outstanding: None,
            returned: 0, returned_bytes: 0, violations: 0 }
    }

    fn contract_ok(&self) -> bool {
        self.violations == 0 && self.outstanding.is_none() && self.returned == self.next
    }
}

impl WasmStream for ChunkStream<'_> {
    fn read(&mut self) -> Result<Option<InnerVec<u8>>, u8> {
        if self.outstanding.is_some() {
            self.violations += 1;
            return Err(1);
        }
        let Some(&part) = self.parts.get(self.next) else { return Ok(None); };
        self.next += 1;
        self.outstanding = Some(part);
        Ok(Some(InnerVec {
            ptr: unsafe { self.bytes.as_mut_ptr().add(part.offset) },
            capacity: part.length as u32,
            len: part.length as u32,
        }))
    }

    fn return_(&mut self, chunk: InnerVec<u8>) {
        let Some(part) = self.outstanding else {
            self.violations += 1;
            return;
        };
        let expected = unsafe { self.bytes.as_mut_ptr().add(part.offset) };
        if chunk.ptr != expected || chunk.capacity as usize != part.length
            || chunk.len as usize != part.length {
            self.violations += 1;
            return;
        }
        self.outstanding = None;
        self.returned += 1;
        self.returned_bytes += part.length;
    }
}

#[derive(Clone, Copy)]
enum ExecutionMode { Uninterrupted, Segmented }

#[derive(Debug, Clone, PartialEq, Eq)]
struct Terminal {
    validation_link_class: &'static str,
    phase: &'static str,
    outcome: &'static str,
    trap_class: Option<&'static str>,
    resource_class: &'static str,
    // A run()->() call must have no value, but retain any actual producer value
    // as a mismatch rather than silently substituting the ABI's expected unit.
    return_bits: Option<u64>,
    globals: Vec<(u8, u64)>,
    state_observed: bool,
}

impl Terminal {
    fn failure(phase: &'static str, class: &'static str, outcome: &'static str) -> Self {
        Self { validation_link_class: class, phase, outcome, trap_class: None,
            resource_class: "none", return_bits: None, globals: Vec::new(), state_observed: false }
    }

    fn json(&self) -> String {
        let globals = self.globals.iter().enumerate().map(|(index, (ty, bits))| {
            format!("{{\"index\":{index},\"type\":{},\"bits\":\"{bits:016x}\"}}", quote(type_name(*ty)))
        }).collect::<Vec<_>>().join(",");
        let state = if self.state_observed { format!("{{\"globals\":[{globals}]}}") }
            else { String::from("null") };
        let returns = self.return_bits.map_or_else(|| String::from("[]"), |bits| {
            format!("[{{\"unexpected_raw_bits\":\"{bits:016x}\"}}]")
        });
        format!("{{\"validation_link_class\":{},\"phase\":{},\"outcome\":{},\"trap_class\":{},\"resource_class\":{},\"return_values\":{returns},\"canonical_preserves_output\":null,\"ordered_hostcall_transcript\":[],\"selected_final_state\":{state},\"final_linear_memory_blake3\":null}}",
            quote(self.validation_link_class), quote(self.phase), quote(self.outcome),
            optional_string(self.trap_class), quote(self.resource_class))
    }
}

fn type_name(ty: u8) -> &'static str {
    match ty { 0 => "i32", 1 => "i64", 2 => "f32", _ => "f64" }
}

struct ExecutionObservation {
    terminal: Terminal,
    segments: usize,
    initialization_segments: usize,
    raw_result: String,
    diagnostic: Option<String>,
    chunks_read: usize,
    chunks_returned: usize,
    bytes_returned: usize,
    stream_ok: bool,
    allocation_peak: usize,
    allocation_live_after_drop: usize,
    allocations: usize,
    allocation_denials: usize,
    system_allocation_failures: usize,
}

fn first_difference(left: &Terminal, right: &Terminal) -> Option<&'static str> {
    if left.validation_link_class != right.validation_link_class { Some("validation-link-class") }
    else if left.phase != right.phase { Some("terminal-phase") }
    else if left.outcome != right.outcome { Some("outcome") }
    else if left.trap_class != right.trap_class { Some("trap-class") }
    else if left.return_bits != right.return_bits { Some("return-values") }
    else if left.globals != right.globals || left.state_observed != right.state_observed {
        Some("selected-final-state")
    }
    else if left.resource_class != right.resource_class { Some("resource-class") }
    else { None }
}

fn matching_verdict(terminal: &Terminal, segments: usize, initialization_segments: usize) -> &'static str {
    match terminal.outcome {
        "rejected" => "decode-match",
        "allocation-exhausted" => "allocation-denied",
        "fuel-exhausted" => "bound-match",
        _ if segments <= 1 && initialization_segments <= 1 => "terminal-match-without-resume-boundary",
        _ => "resume-match",
    }
}

fn observe_result(result: &InterpreterResult, phase: &'static str,
    state: &spacewasm::InterpreterState<'_>) -> Terminal {
    let mut terminal = Terminal::failure(phase, "accepted", "harness-failure");
    match result {
        InterpreterResult::Finished => terminal.outcome = "finished",
        InterpreterResult::Trap(reason) => {
            terminal.outcome = "trap";
            use spacewasm::TrapReason;
            terminal.trap_class = Some(match reason {
                TrapReason::Unreachable => "unreachable",
                TrapReason::Host => "host-trap",
                TrapReason::DivideByZero => "divide-by-zero",
                TrapReason::InvalidTableIndex => "indirect-call-out-of-bounds",
                TrapReason::InvalidTableFunctionType => "indirect-call-type-mismatch",
                TrapReason::UninitializedTableElement => "uninitialized-table-element",
                TrapReason::GlobalGetFailed => "global-get-failed",
                TrapReason::GlobalSetFailed => "global-set-failed",
                TrapReason::OutOfMemory => "out-of-memory",
                TrapReason::MemoryRefNotUnique => "memory-reference-not-unique",
                TrapReason::MemoryOutOfBounds => "memory-out-of-bounds",
                TrapReason::StackOverflow => "stack-overflow",
                TrapReason::UnrepresentableResult => "unrepresentable-result",
                TrapReason::IntegerOverflow => "integer-overflow",
                TrapReason::BadConversionToInteger => "bad-conversion-to-integer",
            });
            terminal.resource_class = match reason {
                spacewasm::TrapReason::OutOfMemory => "allocation-exhausted",
                spacewasm::TrapReason::StackOverflow => "stack-exhausted",
                _ => "none",
            };
        }
        InterpreterResult::OutOfFuel => {
            terminal.outcome = "fuel-exhausted";
            terminal.resource_class = "instruction-bound";
        }
        InterpreterResult::Pause => terminal.outcome = "host-pause",
        InterpreterResult::ReaderError(_) => terminal.outcome = "harness-failure",
    }
    terminal.return_bits = state.result.map(|value| value.read_64());
    terminal.state_observed = true;
    for module in state.store.modules() {
        for global in &module.globals {
            let ty = global.type_.ty;
            let (tag, bits) = match ty {
                ValType::I32 => (0, u64::from(global.value.read_32())),
                ValType::I64 => (1, global.value.read_64()),
                ValType::F32 => (2, u64::from(global.value.read_32())),
                ValType::F64 => (3, global.value.read_64()),
            };
            terminal.globals.push((tag, bits));
        }
    }
    terminal
}

fn spin(text: &[spacewasm::Box<spacewasm::TextPage>],
    state: &mut spacewasm::InterpreterState<'_>, mode: ExecutionMode,
    budget: usize) -> (InterpreterResult, usize) {
    let interpreter = Interpreter::default();
    match mode {
        ExecutionMode::Uninterrupted => (interpreter.run(text, state, budget), 1),
        ExecutionMode::Segmented => {
            for segment in 1..=budget {
                let result = interpreter.run(text, state, SEGMENT_FUEL);
                if result != InterpreterResult::OutOfFuel { return (result, segment); }
            }
            (InterpreterResult::OutOfFuel, budget)
        }
    }
}

fn execute_inner(stream: &mut ChunkStream<'_>, mode: ExecutionMode)
    -> (Terminal, usize, usize, String, Option<String>) {
    let mut phase = "allocate-store";
    let result = (|| -> Result<(Terminal, usize, usize, String), String> {
        let mut store = Store::new(MAXIMUM_MODULES, [])
            .map_err(|error| format!("{error:?}"))?;
        let mut builder = CodeBuilder::<MAXIMUM_CODE_PAGES>::default();
        let allocator = Rc::new(ProbeAllocator).map_err(|error| format!("{error:?}"))?
            .into_wasm_memory_allocator();
        phase = "decode";
        let module = Module::new::<MAXIMUM_CODE_PAGES, MAXIMUM_CONTROL_FRAMES, MAXIMUM_STACK_WORDS>(
            "resume-probe", stream, &mut store, &mut builder, allocator, CompilerOptions::default(),
        ).map_err(|error| format!("{error:?}"))?;
        phase = "profile-admission";
        if !module.imports.is_empty() || module.memory.is_some() || module.table.is_some() {
            return Err(String::from("hostless profile forbids imports, memory and table"));
        }
        if module.globals.len() > MAXIMUM_GLOBALS {
            return Err(format!("global-count-bound: maximum={MAXIMUM_GLOBALS}"));
        }
        let function = resolve_run_export(&module)?;
        phase = "finalize-code";
        let (text, _) = builder.finish().map_err(|error| format!("{error:?}"))?;
        phase = "allocate-state";
        let mut state = store.allocate(MAXIMUM_STACK_WORDS).map_err(|error| format!("{error:?}"))?;
        phase = "initialize";
        let initial_fuel = match mode {
            ExecutionMode::Uninterrupted => INITIALIZATION_FUEL,
            ExecutionMode::Segmented => SEGMENT_FUEL,
        };
        let mut initialization = state.initialize_module(module, &text, initial_fuel);
        let mut initialization_segments = 1;
        if matches!(mode, ExecutionMode::Segmented) {
            while initialization == InterpreterResult::OutOfFuel
                && initialization_segments < INITIALIZATION_FUEL {
                initialization = Interpreter::default().run(&text, &mut state, SEGMENT_FUEL);
                initialization_segments += 1;
            }
        }
        if initialization != InterpreterResult::Finished {
            return Ok((observe_result(&initialization, phase, &state), 0,
                initialization_segments, format!("{initialization:?}")));
        }
        phase = "invoke";
        state.invoke(function, &[]).map_err(|error| format!("{error:?}"))?;
        phase = "run";
        let (result, segments) = spin(&text, &mut state, mode, MAXIMUM_SEGMENTS);
        Ok((observe_result(&result, phase, &state), segments,
            initialization_segments, format!("{result:?}")))
    })();
    match result {
        Ok((terminal, segments, init_segments, raw)) => (terminal, segments, init_segments, raw, None),
        Err(error) => {
            let bound_denied = DENIALS.load(Ordering::SeqCst) > 0;
            let denied = bound_denied || SYSTEM_FAILURES.load(Ordering::SeqCst) > 0;
            let (class, outcome) = if denied { ("not-completed", "allocation-exhausted") }
                else if phase == "decode" { ("rejected", "rejected") }
                else if phase == "profile-admission" { ("profile-rejected", "profile-rejected") }
                else { ("not-completed", "harness-failure") };
            let mut terminal = Terminal::failure(phase, class, outcome);
            if denied {
                terminal.resource_class = if bound_denied { "allocation-bound" } else { "system-oom" };
            }
            (terminal, 0, 0, String::from("not-run"), Some(error))
        }
    }
}

// r[impl chaoscontrol.spacewasm_mvp.resume]
fn execute(bytes: &mut [u8], parts: &[Partition], mode: ExecutionMode, limit: usize)
    -> Result<ExecutionObservation, String> {
    validate_partition(parts, bytes.len()).map_err(String::from)?;
    if LIVE.load(Ordering::SeqCst) != 0 {
        return Ok(ExecutionObservation {
            terminal: Terminal::failure("allocate-store", "not-completed", "harness-failure"),
            segments: 0, initialization_segments: 0, raw_result: String::from("not-run"),
            diagnostic: Some(String::from("previous execution leaked allocations; runtime not invoked")),
            chunks_read: 0, chunks_returned: 0, bytes_returned: 0, stream_ok: true,
            allocation_peak: 0, allocation_live_after_drop: LIVE.load(Ordering::SeqCst),
            allocations: 0, allocation_denials: 0, system_allocation_failures: 0,
        });
    }
    ALLOCATION_LIMIT.store(limit, Ordering::SeqCst);
    PEAK.store(0, Ordering::SeqCst);
    ALLOCATIONS.store(0, Ordering::SeqCst);
    DENIALS.store(0, Ordering::SeqCst);
    SYSTEM_FAILURES.store(0, Ordering::SeqCst);
    let mut stream = ChunkStream::new(bytes, parts);
    let (terminal, segments, initialization_segments, raw_result, diagnostic) =
        execute_inner(&mut stream, mode);
    Ok(ExecutionObservation { terminal, segments, initialization_segments, raw_result, diagnostic,
        chunks_read: stream.next, chunks_returned: stream.returned, bytes_returned: stream.returned_bytes,
        stream_ok: stream.contract_ok(), allocation_peak: PEAK.load(Ordering::SeqCst),
        allocation_live_after_drop: LIVE.load(Ordering::SeqCst),
        allocations: ALLOCATIONS.load(Ordering::SeqCst), allocation_denials: DENIALS.load(Ordering::SeqCst),
        system_allocation_failures: SYSTEM_FAILURES.load(Ordering::SeqCst) })
}

fn resolve_run_export(module: &Module) -> Result<WasmRef, String> {
    let export = module.exports.iter().find(|export| export.name == "run")
        .ok_or_else(|| String::from("run export is missing"))?;
    let ExportDesc::Func(index) = export.desc else { return Err(String::from("run is not a function")); };
    let Ref::Module(index) = module.get_func_ref(index)
        .ok_or_else(|| String::from("run function is missing"))?
        else { return Err(String::from("run is not local")); };
    let function = &module.functions[index as usize];
    let ty = &module.types[function.ty.0 as usize];
    if !ty.params.is_empty() || !ty.returns.is_empty() {
        return Err(String::from("run must have signature () -> ()"));
    }
    Ok(WasmRef { module: ModuleRef(0), index })
}

fn quote(value: &str) -> String {
    let mut result = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => result.push_str("\\\""), '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"), '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c < ' ' => result.push_str(&format!("\\u{:04x}", c as u32)),
            c => result.push(c),
        }
    }
    result.push('"');
    result
}

fn optional_string(value: Option<&str>) -> String {
    value.map_or_else(|| String::from("null"), quote)
}

fn blake3(bytes: &[u8]) -> Result<String, String> {
    // Thin shell only: no floating producer source or extra Cargo dependency.
    // b3sum is supplied by the existing Nix differential check.
    let mut child = Command::new("b3sum").args(["--no-names"])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().map_err(|error| format!("start b3sum: {error}"))?;
    let write = child.stdin.take().ok_or("b3sum stdin unavailable")?
        .write_all(bytes);
    if let Err(error) = write {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("write b3sum input: {error}"));
    }
    let output = child.wait_with_output().map_err(|error| format!("wait b3sum: {error}"))?;
    let digest = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
    let digest = digest.trim();
    if !output.status.success() || digest.len() != 64
        || !digest.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
        return Err(String::from("b3sum did not produce one canonical digest"));
    }
    Ok(digest.to_owned())
}

fn partition_json(parts: &[Partition]) -> String {
    parts.iter().map(|part| format!("[{},{}]", part.offset, part.length))
        .collect::<Vec<_>>().join(",")
}

fn schedule_json(kind: &str, seed: u64, expected: &str, module_digest: &str,
    parts: &[Partition], denial: Option<&str>) -> Result<String, String> {
    let canonical = format!("{{\"generator\":\"chaoscontrol-ordered-partition-v1\",\"kind\":{},\"seed\":{seed},\"maximum_chunks\":{MAXIMUM_CHUNKS},\"maximum_generated_chunk_bytes\":{MAXIMUM_GENERATED_CHUNK_BYTES},\"module_blake3\":{},\"expected_class\":{},\"partitions\":[{}]}}",
        quote(kind), quote(module_digest), quote(expected), partition_json(parts));
    let identity = blake3(canonical.as_bytes())?;
    Ok(format!("{{\"identity_blake3\":{},\"inputs\":{canonical},\"admission_denial\":{}}}",
        quote(&identity), optional_string(denial)))
}

fn observation_json(observation: &ExecutionObservation) -> Result<String, String> {
    let terminal = observation.terminal.json();
    let identity = blake3(terminal.as_bytes())?;
    Ok(format!("{{\"normalized_terminal\":{terminal},\"normalized_identity_blake3\":{},\"raw\":{{\"result\":{},\"segments\":{},\"initialization_segments\":{},\"diagnostic\":{},\"chunks_read\":{},\"chunks_returned\":{},\"bytes_returned\":{},\"return_buffer_contract_ok\":{},\"allocation_peak_bytes\":{},\"allocation_live_after_drop_bytes\":{},\"allocation_count\":{},\"allocation_denials\":{},\"system_allocation_failures\":{}}}}}",
        quote(&identity), quote(&observation.raw_result), observation.segments,
        observation.initialization_segments, optional_string(observation.diagnostic.as_deref()),
        observation.chunks_read, observation.chunks_returned, observation.bytes_returned,
        observation.stream_ok, observation.allocation_peak, observation.allocation_live_after_drop,
        observation.allocations, observation.allocation_denials, observation.system_allocation_failures))
}

fn write_report(path: &Path, report: &str) -> Result<(), String> {
    if report.len() > MAXIMUM_OUTPUT_BYTES { return Err(String::from("report-output-bound")); }
    let mut output = fs::OpenOptions::new().write(true).create_new(true).open(path)
        .map_err(|error| format!("create {}: {error}", path.display()))?;
    output.write_all(report.as_bytes()).map_err(|error| format!("write report: {error}"))
}

fn main() {
    if let Err(error) = run() {
        eprintln!("spacewasm resume probe failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let arguments = std::env::args().collect::<Vec<_>>();
    if arguments.len() < 3 {
        return Err(String::from("usage: spacewasm-resume-probe <module.wasm> <report.json> [--expected finished|trap|fuel-exhausted|rejected|allocation-exhausted] [--partition complete|one-byte|seeded|omit|overlap|reorder|extend] [--seed u64] [--maximum-allocation-bytes usize] [--expect-partition-rejection]"));
    }
    let mut expected = String::from("finished");
    let mut kind = String::from("one-byte");
    let mut seed = DEFAULT_SEED;
    let mut allocation_limit = MAXIMUM_ALLOCATION_BYTES;
    let mut expect_denial = false;
    let mut index = 3;
    while index < arguments.len() {
        let option = arguments[index].as_str();
        if option == "--expect-partition-rejection" {
            expect_denial = true;
            index += 1;
            continue;
        }
        let value = arguments.get(index + 1).ok_or("option needs a value")?;
        match option {
            "--expected" => expected = value.clone(),
            "--partition" => kind = value.clone(),
            "--seed" => seed = value.parse().map_err(|_| "invalid seed")?,
            "--maximum-allocation-bytes" => allocation_limit = value.parse().map_err(|_| "invalid allocation bound")?,
            _ => return Err(format!("unknown option {option}")),
        }
        index += 2;
    }
    if !["complete", "one-byte", "seeded", "omit", "overlap", "reorder", "extend"].contains(&kind.as_str()) {
        return Err(String::from("unknown partition generator"));
    }
    if !["finished", "trap", "fuel-exhausted", "rejected", "allocation-exhausted"].contains(&expected.as_str()) {
        return Err(String::from("unknown expected class"));
    }
    if allocation_limit == 0 || allocation_limit > MAXIMUM_ALLOCATION_BYTES {
        return Err(String::from("allocation bound outside admitted range"));
    }
    let module_path = Path::new(&arguments[1]);
    let report_path = Path::new(&arguments[2]);
    let metadata = fs::symlink_metadata(module_path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(String::from("module must be one regular no-follow file"));
    }
    // Read at most bound+1 even if the file changes after metadata inspection.
    let mut bytes = Vec::new();
    fs::File::open(module_path).map_err(|error| error.to_string())?
        .take((MAXIMUM_INPUT_BYTES + 1) as u64).read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.is_empty() || bytes.len() > MAXIMUM_INPUT_BYTES {
        return Err(String::from("empty module or input-byte-bound"));
    }
    let module_digest = blake3(&bytes)?;
    let generated = generate_partition(&kind, seed, bytes.len());
    let (parts, denial) = match generated {
        Ok(parts) => {
            let denial = validate_partition(&parts, bytes.len()).err().map(String::from);
            (parts, denial)
        }
        Err(denial) => (Vec::new(), Some(denial)),
    };
    let schedule_expected = if expect_denial { "partition-rejected" } else { expected.as_str() };
    let schedule = schedule_json(&kind, seed, schedule_expected, &module_digest, &parts, denial.as_deref())?;
    if let Some(denial) = denial {
        let report = format!("{{\"schema\":\"{REPORT_SCHEMA}\",\"source_revision\":\"{SOURCE_REVISION}\",\"module_blake3\":{},\"verdict\":\"partition-rejected\",\"runtime_invoked\":false,\"schedule\":{schedule},\"diagnostic\":{}}}\n", quote(&module_digest), quote(&denial));
        write_report(report_path, &report)?;
        return if expect_denial { Ok(()) } else { Err(format!("partition rejected: {denial}")) };
    }
    if expect_denial {
        let report = format!("{{\"schema\":\"{REPORT_SCHEMA}\",\"verdict\":\"unexpected-partition-admission\",\"runtime_invoked\":false,\"schedule\":{schedule}}}\n");
        write_report(report_path, &report)?;
        return Err(String::from("partition unexpectedly admitted"));
    }
    let complete = generate_partition("complete", seed, bytes.len())?;
    let uninterrupted = execute(&mut bytes, &complete, ExecutionMode::Uninterrupted, allocation_limit)?;
    let segmented = execute(&mut bytes, &complete, ExecutionMode::Segmented, allocation_limit)?;
    let streaming = execute(&mut bytes, &parts, ExecutionMode::Uninterrupted, allocation_limit)?;
    let streaming_segmented = execute(&mut bytes, &parts, ExecutionMode::Segmented, allocation_limit)?;
    let observations = [&uninterrupted, &segmented, &streaming, &streaming_segmented];
    let resume_diff = first_difference(&uninterrupted.terminal, &segmented.terminal);
    let stream_diff = first_difference(&uninterrupted.terminal, &streaming.terminal);
    let stream_resume_diff = first_difference(&streaming.terminal, &streaming_segmented.terminal);
    let harness_failure = observations.iter().any(|observation| !observation.stream_ok
        || observation.allocation_live_after_drop != 0 || observation.terminal.outcome == "harness-failure"
        || observation.terminal.outcome == "host-pause");
    let expected_matches = observations.iter().all(|observation| observation.terminal.outcome == expected);
    let verdict = if harness_failure { "harness-failure" }
        else if resume_diff.is_some() || stream_resume_diff.is_some() { "resume-mismatch" }
        else if stream_diff.is_some() { "streaming-mismatch" }
        else if !expected_matches { "unexpected-class" }
        else { matching_verdict(&segmented.terminal, segmented.segments, segmented.initialization_segments) };
    let report = format!("{{\"schema\":\"{REPORT_SCHEMA}\",\"source_revision\":\"{SOURCE_REVISION}\",\"module_blake3\":{},\"verdict\":{},\"runtime_invoked\":true,\"expected_class\":{},\"segment_fuel\":{SEGMENT_FUEL},\"maximum_segments\":{MAXIMUM_SEGMENTS},\"maximum_allocation_bytes\":{allocation_limit},\"maximum_globals\":{MAXIMUM_GLOBALS},\"maximum_chunks\":{MAXIMUM_CHUNKS},\"schedule\":{schedule},\"complete_schedule\":{},\"comparisons\":{{\"resume_first_difference\":{},\"streaming_first_difference\":{},\"streaming_resume_first_difference\":{}}},\"observations\":{{\"uninterrupted\":{},\"segmented\":{},\"streaming\":{},\"streaming_segmented\":{}}},\"surfaces\":{{\"selected_final_state\":\"all-defined-globals-in-module-order-typed-raw-bits\",\"canonical_preserves_output\":\"unavailable-no-producer-Preserves-emitter\",\"hostcall_transcript\":\"empty-admitted-import-count-zero\",\"linear_memory\":\"absent-admitted-memory-count-zero\",\"host_pause\":\"producer-supports-Pause-but-admitted-host-function-count-zero\",\"allocation_scope\":\"SpaceWasm-Allocator-and-WasmMemoryAllocator-requested-layout-bytes-not-process-RSS\"}},\"evidence_role\":\"diagnostic-only\"}}\n",
        quote(&module_digest), quote(verdict), quote(&expected),
        schedule_json("complete", seed, &expected, &module_digest, &complete, None)?,
        optional_string(resume_diff), optional_string(stream_diff), optional_string(stream_resume_diff),
        observation_json(&uninterrupted)?, observation_json(&segmented)?,
        observation_json(&streaming)?, observation_json(&streaming_segmented)?);
    // Never discard a mismatch by returning an error before its observations exist.
    write_report(report_path, &report)?;
    println!("spacewasm resume probe: {verdict}; report={}", report_path.display());
    if !harness_failure && expected_matches && resume_diff.is_none() && stream_diff.is_none()
        && stream_resume_diff.is_none() { Ok(()) }
    else { Err(format!("{verdict}; retained diagnostic report")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    // (module (global (mut i32) (i32.const 0))
    //   (func (export "run") i32.const 7 global.set 0))
    const GLOBAL_MODULE: &[u8] = &[
        0,97,115,109,1,0,0,0, 1,4,1,96,0,0, 3,2,1,0,
        6,6,1,127,1,65,0,11, 7,7,1,3,114,117,110,0,0,
        10,8,1,6,0,65,7,36,0,11,
    ];

    #[test]
    // r[verify chaoscontrol.spacewasm_mvp.streaming]
    fn malformed_partitions_are_denied_before_stream_construction() {
        for kind in ["omit", "overlap", "reorder", "extend"] {
            let parts = generate_partition(kind, DEFAULT_SEED, GLOBAL_MODULE.len()).unwrap();
            assert!(validate_partition(&parts, GLOBAL_MODULE.len()).is_err(), "{kind}");
        }
        assert_eq!(validate_partition(&[Partition { offset: 0, length: 0 }], 1), Err("empty-chunk"));
        assert_eq!(validate_partition(&[Partition { offset: 0, length: usize::MAX },
            Partition { offset: usize::MAX, length: 1 }], usize::MAX), Err("partition-overflow"));
    }

    #[test]
    // r[verify chaoscontrol.spacewasm_mvp.streaming]
    fn ordered_partition_covers_every_byte_and_regenerates() {
        for kind in ["complete", "one-byte", "seeded"] {
            let first = generate_partition(kind, DEFAULT_SEED, GLOBAL_MODULE.len()).unwrap();
            assert_eq!(first, generate_partition(kind, DEFAULT_SEED, GLOBAL_MODULE.len()).unwrap());
            assert_eq!(validate_partition(&first, GLOBAL_MODULE.len()), Ok(()));
            let reconstructed = first.iter().flat_map(|part| {
                GLOBAL_MODULE[part.offset..part.offset + part.length].iter().copied()
            }).collect::<Vec<_>>();
            assert_eq!(reconstructed, GLOBAL_MODULE);
        }
        assert!(generate_partition("one-byte", 0, MAXIMUM_CHUNKS + 1).is_err());
    }

    #[test]
    fn stream_requires_exactly_one_matching_return_before_reuse() {
        let parts = generate_partition("one-byte", 0, 2).unwrap();
        let mut bytes = [1, 2];
        let mut stream = ChunkStream::new(&mut bytes, &parts);
        let chunk = stream.read().unwrap().unwrap();
        assert!(stream.read().is_err());
        stream.return_(chunk);
        assert_eq!(stream.returned, 1);
        let mut chunk = stream.read().unwrap().unwrap();
        chunk.capacity += 1;
        stream.return_(chunk);
        assert!(!stream.contract_ok());
        assert_eq!(stream.returned, 1);
    }

    #[test]
    // r[verify chaoscontrol.spacewasm_mvp.resume]
    fn globals_and_traps_survive_real_streaming_resume_boundaries() {
        allocation_limit_is_real_and_failed_reallocation_preserves_old_buffer();
        let complete = generate_partition("complete", 0, GLOBAL_MODULE.len()).unwrap();
        let chunks = generate_partition("one-byte", 0, GLOBAL_MODULE.len()).unwrap();
        let mut bytes = GLOBAL_MODULE.to_vec();
        let left = execute(&mut bytes, &complete, ExecutionMode::Uninterrupted, MAXIMUM_ALLOCATION_BYTES).unwrap();
        let right = execute(&mut bytes, &chunks, ExecutionMode::Segmented, MAXIMUM_ALLOCATION_BYTES).unwrap();
        assert_eq!(left.terminal.outcome, "finished");
        assert_eq!(left.terminal.globals, vec![(0, 7)]);
        assert_eq!(first_difference(&left.terminal, &right.terminal), None);
        assert!(right.segments > 1);
        assert!(right.stream_ok);
        assert_eq!(right.chunks_read, chunks.len());
        assert_eq!(right.chunks_returned, chunks.len());
        assert_eq!(right.bytes_returned, GLOBAL_MODULE.len());
        assert_eq!(right.allocation_live_after_drop, 0);
        assert!(right.allocation_peak > 0 && right.allocation_peak <= MAXIMUM_ALLOCATION_BYTES);

        let mut trap = GLOBAL_MODULE.to_vec();
        // Replace global.set 0 with unreachable/nop, preserving body size.
        let n = trap.len();
        trap[n - 3] = 0;
        trap[n - 2] = 1;
        let left = execute(&mut trap, &complete, ExecutionMode::Uninterrupted, MAXIMUM_ALLOCATION_BYTES).unwrap();
        let right = execute(&mut trap, &chunks, ExecutionMode::Segmented, MAXIMUM_ALLOCATION_BYTES).unwrap();
        assert_eq!(left.terminal.outcome, "trap");
        assert_eq!(left.terminal.trap_class, Some("unreachable"));
        assert_eq!(first_difference(&left.terminal, &right.terminal), None);

        let mut started = GLOBAL_MODULE[..GLOBAL_MODULE.len() - 10].to_vec();
        started.extend_from_slice(&[8, 1, 0]); // start function index 0
        started.extend_from_slice(&GLOBAL_MODULE[GLOBAL_MODULE.len() - 10..]);
        let full = generate_partition("complete", 0, started.len()).unwrap();
        let small = generate_partition("one-byte", 0, started.len()).unwrap();
        let left = execute(&mut started, &full, ExecutionMode::Uninterrupted, MAXIMUM_ALLOCATION_BYTES).unwrap();
        let right = execute(&mut started, &small, ExecutionMode::Segmented, MAXIMUM_ALLOCATION_BYTES).unwrap();
        assert_eq!(right.terminal.outcome, "finished");
        assert!(right.initialization_segments > 1);
        assert_eq!(first_difference(&left.terminal, &right.terminal), None);

        let mut malformed = GLOBAL_MODULE.to_vec();
        malformed[0] = 1;
        let rejected = execute(&mut malformed, &chunks, ExecutionMode::Segmented, MAXIMUM_ALLOCATION_BYTES).unwrap();
        assert_eq!(rejected.terminal.validation_link_class, "rejected");
        assert_eq!(rejected.terminal.outcome, "rejected");
        assert!(rejected.stream_ok);
        assert_eq!(matching_verdict(&rejected.terminal, 0, 0), "decode-match");

        let mut looping = vec![
            0,97,115,109,1,0,0,0, 1,4,1,96,0,0, 3,2,1,0,
            7,7,1,3,114,117,110,0,0, 10,9,1,7,0,3,64,12,0,11,11,
        ];
        let full = generate_partition("complete", 0, looping.len()).unwrap();
        let small = generate_partition("one-byte", 0, looping.len()).unwrap();
        let left = execute(&mut looping, &full, ExecutionMode::Uninterrupted, MAXIMUM_ALLOCATION_BYTES).unwrap();
        let right = execute(&mut looping, &small, ExecutionMode::Segmented, MAXIMUM_ALLOCATION_BYTES).unwrap();
        assert_eq!(right.terminal.outcome, "fuel-exhausted");
        assert_eq!(right.terminal.resource_class, "instruction-bound");
        assert_eq!(right.segments, MAXIMUM_SEGMENTS);
        assert_eq!(first_difference(&left.terminal, &right.terminal), None);
        assert_eq!(matching_verdict(&right.terminal, right.segments, 1), "bound-match");
    }

    fn allocation_limit_is_real_and_failed_reallocation_preserves_old_buffer() {
        assert_eq!(LIVE.load(Ordering::SeqCst), 0);
        ALLOCATION_LIMIT.store(32, Ordering::SeqCst);
        PEAK.store(0, Ordering::SeqCst);
        let old_layout = Layout::from_size_align(16, 8).unwrap();
        let new_layout = Layout::from_size_align(24, 16).unwrap();
        let pointer = ProbeAllocator.allocate(old_layout).unwrap();
        unsafe { pointer.as_ptr().write(93); }
        assert!(ProbeAllocator.reallocate(pointer, old_layout, new_layout).is_err());
        assert_eq!(unsafe { pointer.as_ptr().read() }, 93);
        assert_eq!(LIVE.load(Ordering::SeqCst), 16);
        assert_eq!(ProbeAllocator.memory_statistics().total_bytes, 16);
        ALLOCATION_LIMIT.store(64, Ordering::SeqCst);
        let pointer = ProbeAllocator.reallocate(pointer, old_layout, new_layout).unwrap();
        assert_eq!(unsafe { pointer.as_ptr().read() }, 93);
        assert_eq!(pointer.as_ptr() as usize % new_layout.align(), 0);
        assert_eq!(LIVE.load(Ordering::SeqCst), 24);
        assert_eq!(PEAK.load(Ordering::SeqCst), 40);
        ProbeAllocator.deallocate(pointer, new_layout);
        assert_eq!(LIVE.load(Ordering::SeqCst), 0);
        ALLOCATION_LIMIT.store(MAXIMUM_ALLOCATION_BYTES, Ordering::SeqCst);

        let parts = generate_partition("complete", 0, GLOBAL_MODULE.len()).unwrap();
        let mut bytes = GLOBAL_MODULE.to_vec();
        let denied = execute(&mut bytes, &parts, ExecutionMode::Uninterrupted, 1).unwrap();
        assert_eq!(denied.terminal.outcome, "allocation-exhausted");
        assert_eq!(denied.terminal.resource_class, "allocation-bound");
        assert_eq!(matching_verdict(&denied.terminal, 0, 0), "allocation-denied");
        assert!(denied.allocation_denials > 0);
        assert_eq!(denied.allocation_live_after_drop, 0);
        ALLOCATION_LIMIT.store(MAXIMUM_ALLOCATION_BYTES, Ordering::SeqCst);
    }

    #[test]
    // r[verify chaoscontrol.spacewasm_mvp.resume]
    fn changed_observable_state_is_a_retained_resume_predicate() {
        let mut left = Terminal::failure("run", "accepted", "finished");
        left.state_observed = true;
        left.globals.push((0, 7));
        let mut right = left.clone();
        right.globals[0].1 = 8;
        assert_eq!(first_difference(&left, &right), Some("selected-final-state"));
    }
}
