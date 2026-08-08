use std::alloc::Layout;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::ptr::NonNull;

use spacewasm::AllocError;
use spacewasm::Allocator;
use spacewasm::CodeBuilder;
use spacewasm::CompilerOptions;
use spacewasm::ExportDesc;
use spacewasm::InnerVec;
use spacewasm::Interpreter;
use spacewasm::InterpreterResult;
use spacewasm::InterpreterRunner;
use spacewasm::MemoryStatistics;
use spacewasm::Module;
use spacewasm::ModuleRef;
use spacewasm::Rc;
use spacewasm::Ref;
use spacewasm::Store;
use spacewasm::WasmMemoryAllocator;
use spacewasm::WasmRef;
use spacewasm::WasmStream;

const SOURCE_REVISION: &str = "e24cf09355a90497148eb5029fdb8e3400bd63e3";
const REPORT_SCHEMA: &str = "chaoscontrol.spacewasm-resume-probe.v1";
const MAXIMUM_INPUT_BYTES: u64 = 1_048_576;
const MAXIMUM_CODE_PAGES: usize = 64;
const MAXIMUM_CONTROL_FRAMES: usize = 64;
const MAXIMUM_STACK_WORDS: usize = 1_024;
const INITIALIZATION_FUEL: usize = 1_024;
const SEGMENT_FUEL: usize = 1;
const MAXIMUM_SEGMENTS: usize = 4_096;
const MAXIMUM_MODULES: usize = 1;
const STREAM_CHUNK_BYTES: usize = 1;
const COMPLETE_CHUNK_BYTES: usize = usize::MAX;

#[derive(Clone, Copy)]
struct ProbeAllocator;

unsafe impl Allocator for ProbeAllocator {
    unsafe fn alloc(&self, layout: Layout) -> Result<*mut u8, AllocError> {
        let pointer = unsafe { std::alloc::alloc(layout) };
        if pointer.is_null() {
            Err(AllocError::AllocationFailed)
        } else {
            Ok(pointer)
        }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if !pointer.is_null() {
            unsafe { std::alloc::dealloc(pointer, layout) };
        }
    }

    fn memory_statistics(&self) -> MemoryStatistics {
        MemoryStatistics {
            total_bytes: 0,
            pad_bytes: 0,
        }
    }
}

impl WasmMemoryAllocator for ProbeAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<u8>, AllocError> {
        unsafe { NonNull::new(std::alloc::alloc(layout)).ok_or(AllocError::AllocationFailed) }
    }

    fn reallocate(
        &self,
        pointer: NonNull<u8>,
        old_layout: Layout,
        layout: Layout,
    ) -> Result<NonNull<u8>, AllocError> {
        unsafe {
            NonNull::new(std::alloc::realloc(
                pointer.as_ptr(),
                old_layout,
                layout.size(),
            ))
            .ok_or(AllocError::AllocationFailed)
        }
    }

    fn deallocate(&self, pointer: NonNull<u8>, layout: Layout) {
        unsafe { std::alloc::dealloc(pointer.as_ptr(), layout) };
    }
}

spacewasm::global_allocator!(ProbeAllocator, ProbeAllocator);

struct ChunkStream {
    chunks: Vec<Vec<u8>>,
    next: usize,
}

impl ChunkStream {
    fn new(bytes: &[u8], chunk_bytes: usize) -> Self {
        assert!(chunk_bytes > 0, "chunk size must be positive");
        Self {
            chunks: bytes.chunks(chunk_bytes).map(<[u8]>::to_vec).collect(),
            next: 0,
        }
    }
}

impl WasmStream for ChunkStream {
    fn read(&mut self) -> Result<Option<InnerVec<u8>>, u8> {
        let Some(chunk) = self.chunks.get_mut(self.next) else {
            return Ok(None);
        };
        self.next = self.next.saturating_add(1);
        Ok(Some(InnerVec {
            ptr: chunk.as_mut_ptr(),
            capacity: u32::try_from(chunk.len()).unwrap_or(u32::MAX),
            len: u32::try_from(chunk.len()).unwrap_or(u32::MAX),
        }))
    }

    fn return_(&mut self, _chunk: InnerVec<u8>) {}
}

#[derive(Clone, Copy)]
enum ExecutionMode {
    Uninterrupted,
    Segmented,
}

struct ExecutionObservation {
    result: InterpreterResult,
    segments: usize,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("spacewasm resume probe failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let arguments = std::env::args().collect::<Vec<_>>();
    if arguments.len() != 3 {
        return Err(String::from(
            "usage: spacewasm-resume-probe <module.wasm> <report.json>",
        ));
    }
    let module_path = Path::new(&arguments[1]);
    let report_path = Path::new(&arguments[2]);
    let metadata = fs::symlink_metadata(module_path)
        .map_err(|error| format!("inspect {}: {error}", module_path.display()))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(String::from("module must be one regular no-follow file"));
    }
    if metadata.len() > MAXIMUM_INPUT_BYTES {
        return Err(String::from("module exceeds the input bound"));
    }
    let bytes = fs::read(module_path)
        .map_err(|error| format!("read {}: {error}", module_path.display()))?;
    if bytes.is_empty() {
        return Err(String::from("module is empty"));
    }

    let uninterrupted = execute(&bytes, COMPLETE_CHUNK_BYTES, ExecutionMode::Uninterrupted)?;
    let segmented = execute(&bytes, COMPLETE_CHUNK_BYTES, ExecutionMode::Segmented)?;
    let streaming = execute(&bytes, STREAM_CHUNK_BYTES, ExecutionMode::Uninterrupted)?;
    if uninterrupted.result != InterpreterResult::Finished {
        return Err(format!("uninterrupted execution was {:?}", uninterrupted.result));
    }
    if segmented.result != uninterrupted.result {
        return Err(format!(
            "segmented execution diverged: uninterrupted={:?} segmented={:?}",
            uninterrupted.result, segmented.result
        ));
    }
    if segmented.segments <= 1 {
        return Err(String::from(
            "segmented execution did not cross an out-of-fuel boundary",
        ));
    }
    if streaming.result != uninterrupted.result {
        return Err(format!(
            "streaming decode diverged: uninterrupted={:?} streaming={:?}",
            uninterrupted.result, streaming.result
        ));
    }

    let report = format!(
        "{{\"schema\":\"{REPORT_SCHEMA}\",\"source_revision\":\"{SOURCE_REVISION}\",\"uninterrupted\":\"finished\",\"segmented\":\"finished\",\"segment_fuel\":{SEGMENT_FUEL},\"segments\":{},\"maximum_segments\":{MAXIMUM_SEGMENTS},\"streaming\":\"finished\",\"stream_chunk_bytes\":{STREAM_CHUNK_BYTES},\"state_claim\":\"not-observable-in-hostless-profile\"}}\n",
        segmented.segments
    );
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(report_path)
        .map_err(|error| format!("create {}: {error}", report_path.display()))?;
    output
        .write_all(report.as_bytes())
        .map_err(|error| format!("write {}: {error}", report_path.display()))?;
    println!(
        "spacewasm resume equivalence ok: segments={} streaming=finished",
        segmented.segments
    );
    Ok(())
}

fn execute(
    bytes: &[u8],
    chunk_bytes: usize,
    mode: ExecutionMode,
) -> Result<ExecutionObservation, String> {
    let mut stream = ChunkStream::new(bytes, chunk_bytes);
    let mut store = Store::new(MAXIMUM_MODULES, [])
        .map_err(|error| format!("allocate module store: {error:?}"))?;
    let mut code_builder = CodeBuilder::<MAXIMUM_CODE_PAGES>::default();
    let allocator = Rc::new(ProbeAllocator)
        .map_err(|error| format!("allocate allocator handle: {error:?}"))?
        .into_wasm_memory_allocator();
    let module = Module::new::<MAXIMUM_CODE_PAGES, MAXIMUM_CONTROL_FRAMES, MAXIMUM_STACK_WORDS>(
        "resume-probe",
        &mut stream,
        &mut store,
        &mut code_builder,
        allocator,
        CompilerOptions::default(),
    )
    .map_err(|error| format!("decode module: {error:?}"))?;
    let (text, _) = code_builder
        .finish()
        .map_err(|error| format!("finalize code: {error:?}"))?;
    let mut state = store
        .allocate(MAXIMUM_STACK_WORDS)
        .map_err(|error| format!("allocate interpreter state: {error:?}"))?;
    let initialization = state.initialize_module(module, &text, INITIALIZATION_FUEL);
    if initialization != InterpreterResult::Finished {
        return Err(format!("module initialization was {initialization:?}"));
    }
    let function = resolve_run_export(&state)?;
    state
        .invoke(function, &[])
        .map_err(|error| format!("invoke run export: {error:?}"))?;
    let interpreter = Interpreter::default();
    match mode {
        ExecutionMode::Uninterrupted => Ok(ExecutionObservation {
            result: interpreter.run(&text, &mut state, usize::MAX),
            segments: 1,
        }),
        ExecutionMode::Segmented => {
            for segment in 1..=MAXIMUM_SEGMENTS {
                let result = interpreter.run(&text, &mut state, SEGMENT_FUEL);
                if result != InterpreterResult::OutOfFuel {
                    return Ok(ExecutionObservation {
                        result,
                        segments: segment,
                    });
                }
            }
            Err(String::from("segmented execution exceeded its segment bound"))
        }
    }
}

fn resolve_run_export(state: &spacewasm::InterpreterState<'_>) -> Result<WasmRef, String> {
    let module = state
        .store
        .modules()
        .last()
        .ok_or_else(|| String::from("decoded module is missing"))?;
    let export = module
        .exports
        .iter()
        .find(|export| export.name == "run")
        .ok_or_else(|| String::from("run export is missing"))?;
    let ExportDesc::Func(index) = export.desc else {
        return Err(String::from("run export is not a function"));
    };
    let Ref::Module(index) = module
        .get_func_ref(index)
        .ok_or_else(|| String::from("run function is missing"))?
    else {
        return Err(String::from("run function is not local"));
    };
    Ok(WasmRef {
        module: ModuleRef(0),
        index,
    })
}
