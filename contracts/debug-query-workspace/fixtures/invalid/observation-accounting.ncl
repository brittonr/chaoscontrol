let base = import "../valid/observation-receipt.ncl" in
std.record.update "executed_rounds" 3831 base | (import "../../observation-receipt.ncl")
