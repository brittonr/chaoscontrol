# A clock hostcall is ambient authority outside molten.wasm.abi.v1.
let Contract = import "../../spacewasm-full-abi-differential-profile.ncl" in
let Example = import "../../examples/spacewasm-full-abi-differential-profile.json" in
(Example & {
  abi.hostcalls | force = [
    { module = "molten:hostcall", name = "assert", parameters = ["i32", "i32"], results = ["i64"] },
    { module = "molten:hostcall", name = "clock", parameters = ["i32", "i32"], results = ["i64"] },
    { module = "molten:hostcall", name = "send", parameters = ["i32", "i32"], results = ["i64"] },
  ],
}) | Contract
