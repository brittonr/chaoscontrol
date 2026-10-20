let Receipt = import "receipt.ncl" in
Receipt | { production_ready | std.contract.Equal true }
