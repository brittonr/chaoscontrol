# Full-ABI differential profile for Mantle's materialize-molten-spacewasm-abi
# fixtures. The bundle identity is the one verified on 2026-10-30; evidence use
# still requires a release-authority publication receipt for it.
let Profile = import "../spacewasm-full-abi-differential-profile.ncl" in
let Hostcall = fun hostcall_name => {
  module = "molten:hostcall",
  name = hostcall_name,
  parameters = ["i32", "i32"],
  results = ["i64"],
} in
{
  schema = "chaoscontrol.spacewasm-full-abi-differential-profile.v1",
  profile_id = "molten-abi-v1-wasmtime-49-0-5",
  abi = {
    name = "molten.wasm.abi.v1",
    hostcalls = [Hostcall "assert", Hostcall "send"],
    exports = [
      { name = "memory", kind = "memory", parameters = [], results = [] },
      { name = "molten_alloc", kind = "function", parameters = ["i32"], results = ["i32"] },
      { name = "molten_dealloc", kind = "function", parameters = ["i32", "i32"], results = [] },
      { name = "molten_run", kind = "function", parameters = ["i32", "i32"], results = ["i64"] },
    ],
    memories = 1,
    tables = 0,
    start_function = "not-admitted",
    memory_grow = "not-admitted",
    # Mantle's memory-limit fixture expects denial under a one-page budget.
    maximum_memory_pages = 1,
    reply_offset = 4096,
    reply_descriptor = "high-u32-offset-low-u32-length",
  },
  cohort = {
    spacewasm_revision = "e24cf09355a90497148eb5029fdb8e3400bd63e3",
    wasmtime_version = "wasmtime 49.0.5",
    wasmtime_patch_line = "patched-wasmtime-49.0.5",
    wasm_tools_version = "wasm-tools 1.245.0",
    host_authority = "declared-hostcalls-only",
  },
  bundle = {
    mantle_change = "materialize-molten-spacewasm-abi",
    mantle_revision = "67fe04fe88edce20cc1bdf3faa7e51fcd8bfbeef",
    bundle_identity_blake3 = "774331f6c2defe75e30c354b2b3080838cf56438bbd99268779fe4590f8c9f39",
    publication = "release-authority-receipt-required",
  },
  scenario_classes = ["abi", "cumulative-fuel", "hostcall", "memory", "stack", "trap"],
  bounds = {
    maximum_cases = 64,
    maximum_input_bytes = 1048576,
    # One 64 KiB page less the 4 KiB reply offset.
    maximum_hostcall_bytes = 61440,
    maximum_hostcalls_per_run = 64,
    wasmtime_fuel = 10000000,
    spacewasm_run_fuel = 10000000,
    maximum_resume_segments = 4096,
    maximum_functions = 256,
    maximum_types = 256,
    maximum_globals = 16,
  },
  non_claims = [
    "not-spacewasm-correctness",
    "not-spacewasm-wasmtime-equivalence",
    "not-memory-safety",
    "not-webassembly-conformance",
    "not-molten-runtime-correctness",
    "not-flight-qualification",
    "not-sandbox-effectiveness",
    "not-production-readiness",
    "not-release-eligibility",
  ],
} | Profile
