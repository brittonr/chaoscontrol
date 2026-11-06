let base = import "../valid/observation-receipt.ncl" in
std.record.update "stdout_bytes" 1025 base | (import "../../observation-receipt.ncl")
