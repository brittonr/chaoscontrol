let base = import "../valid/observation-receipt.ncl" in
std.record.update "certifies_original_history" true base | (import "../../observation-receipt.ncl")
