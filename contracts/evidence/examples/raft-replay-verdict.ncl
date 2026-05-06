let ReplayVerdict = import "../replay-verdict.ncl" in
(import "../fixtures/valid/replay-verdict.snapshot-backed.valid.json") | ReplayVerdict
