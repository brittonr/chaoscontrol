# Human-authored cohort, ABI surface and bounds for the diagnostic SpaceWasm /
# Wasmtime full-ABI differential lane. The MVP profile stays separate and never
# admits these modules. Runtime observations stay Rust-owned.
let P = import "profile-primitives.ncl" in
let MaximumCases = 1024 in
let MaximumInputBytes = 16777216 in
let MaximumMemoryPages = 256 in
let WasmPageBytes = 65536 in
let ReplyOffset = 4096 in
let MaximumFuel = 1000000000 in
let MaximumHostcallBytes = 1048576 in
let MaximumHostcallsPerRun = 4096 in
let MaximumResumeSegments = 4096 in
let GitRevision = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^[0-9a-f]{40}$" value
) in
let Blake3Hex = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^[0-9a-f]{64}$" value
) in
let Hostcall = fun hostcall_name => {
  module = "molten:hostcall",
  name = hostcall_name,
  parameters = ["i32", "i32"],
  results = ["i64"],
} in
let ExactHostcalls = std.contract.from_predicate (fun value =>
  value == [Hostcall "assert", Hostcall "send"]
) in
let ExactExports = std.contract.from_predicate (fun value =>
  value == [
    { name = "memory", kind = "memory", parameters = [], results = [] },
    { name = "molten_alloc", kind = "function", parameters = ["i32"], results = ["i32"] },
    { name = "molten_dealloc", kind = "function", parameters = ["i32", "i32"], results = [] },
    { name = "molten_run", kind = "function", parameters = ["i32", "i32"], results = ["i64"] },
  ]
) in
let ExactScenarioClasses = std.contract.from_predicate (fun value =>
  value == ["abi", "cumulative-fuel", "hostcall", "memory", "stack", "trap"]
) in
let RequiredNonClaims = std.contract.from_predicate (fun value =>
  std.array.elem "not-spacewasm-correctness" value
  && std.array.elem "not-spacewasm-wasmtime-equivalence" value
  && std.array.elem "not-memory-safety" value
  && std.array.elem "not-webassembly-conformance" value
  && std.array.elem "not-molten-runtime-correctness" value
  && std.array.elem "not-flight-qualification" value
  && std.array.elem "not-sandbox-effectiveness" value
  && std.array.elem "not-production-readiness" value
  && std.array.elem "not-release-eligibility" value
) in
let Abi = {
  name | P.exact_string "molten.wasm.abi.v1",
  hostcalls | ExactHostcalls,
  exports | ExactExports,
  memories | P.exact_value 1,
  tables | P.exact_value 0,
  start_function | P.exact_string "not-admitted",
  memory_grow | P.exact_string "not-admitted",
  maximum_memory_pages | P.bounded_positive MaximumMemoryPages,
  reply_offset | P.exact_value ReplyOffset,
  reply_descriptor | P.exact_string "high-u32-offset-low-u32-length",
} in
let Cohort = {
  spacewasm_revision | GitRevision,
  wasmtime_version | P.exact_string "wasmtime 49.0.5",
  wasmtime_patch_line | P.exact_string "patched-wasmtime-49.0.5",
  wasm_tools_version | P.exact_string "wasm-tools 1.245.0",
  host_authority | P.exact_string "declared-hostcalls-only",
} in
let Bundle = {
  mantle_change | P.exact_string "materialize-molten-spacewasm-abi",
  mantle_revision | GitRevision,
  bundle_identity_blake3 | Blake3Hex,
  publication | P.exact_string "release-authority-receipt-required",
} in
let Bounds = {
  maximum_cases | P.bounded_positive MaximumCases,
  maximum_input_bytes | P.bounded_positive MaximumInputBytes,
  maximum_hostcall_bytes | P.bounded_positive MaximumHostcallBytes,
  maximum_hostcalls_per_run | P.bounded_positive MaximumHostcallsPerRun,
  wasmtime_fuel | P.bounded_positive MaximumFuel,
  spacewasm_run_fuel | P.bounded_positive MaximumFuel,
  maximum_resume_segments | P.bounded_positive MaximumResumeSegments,
  maximum_functions | P.bounded_positive 65536,
  maximum_types | P.bounded_positive 65536,
  maximum_globals | P.bounded_positive 64,
} in
let Shape = {
  schema | P.exact_string "chaoscontrol.spacewasm-full-abi-differential-profile.v1",
  profile_id | P.identifier,
  abi | Abi,
  cohort | Cohort,
  bundle | Bundle,
  scenario_classes | ExactScenarioClasses,
  bounds | Bounds,
  non_claims | RequiredNonClaims,
} in
# The reply must fit inside the smallest admitted memory after its offset.
let Coherent = fun value =>
  value.abi.reply_offset + value.bounds.maximum_hostcall_bytes
  <= value.abi.maximum_memory_pages * WasmPageBytes
in
std.contract.custom (fun label value =>
  std.contract.check Shape label value
  |> match {
    'Ok checked => if Coherent checked then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)
