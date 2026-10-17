let Contract = import "../../bug-report.ncl" in
let Base = import "bug-report.identity.valid.json" in
let Parent = std.record.update "replay_parent_depth" 1 Base in
(Parent & {
  replay_parent_snapshot_ref = {
    store = "file-content-addressed",
    digest = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    codec = "simulation-snapshot-cbor-zstd-v2",
    schema_version = 2,
    path = "snapshots/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.snapshot.bin",
  },
  marker_context = {
    marker_identity = "b3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    owner = "guest",
    state_ref = null,
    logical_position_ref = "term:1",
    run_id = 0,
    observed_tick = 1,
  },
}) | Contract
