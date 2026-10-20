let R = import "./receipt.ncl" in
R.production_ready | std.contract.Equal true
