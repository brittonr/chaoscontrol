## MODIFIED Requirements

### Requirement: Exploration parameters
`mkChaosTest` SHALL accept optional parameters for the exploration run:
`vms` (default 3), `rounds` (default 50), `branches` (default 8),
`ticks` (default 1000), `seed` (default 42), `mode` (default "hybrid"),
`diskImage` (default null), `extraArgs` (default "").

#### Scenario: Custom parameters
- **WHEN** `mkChaosTest { ...; vms = 5; rounds = 200; seed = 1337; mode = "input-tree"; }` is evaluated
- **THEN** the explorer is invoked with `--vms 5 --rounds 200 --seed 1337 --mode input-tree`

#### Scenario: Disk image passthrough
- **WHEN** `mkChaosTest { ...; diskImage = redb-disk-image; }` is evaluated
- **THEN** the explorer is invoked with `--disk-image ${redb-disk-image}`

## ADDED Requirements

### Requirement: Tuned explore-redb wrapper
The `explore-redb` nix app SHALL use defaults appropriate for single-node storage testing: `--vms 1`, `--ticks 5000` (long enough for multiple crash/recovery cycles), `--rounds 100`, `--branches 8`, and `--mode hybrid`. The wrapper SHALL pass the `redb-disk-image` via `--disk-image`.

#### Scenario: Explore-redb defaults
- **WHEN** `nix run .#explore-redb` is invoked without arguments
- **THEN** the explorer starts with 1 VM, 5000 ticks per branch, and the redb disk image

#### Scenario: User override via extra args
- **WHEN** `nix run .#explore-redb -- --rounds 500 --seed 99`
- **THEN** the user-specified flags override the wrapper defaults

### Requirement: Adequate redb disk image size
The `redb-disk-image` nix derivation SHALL produce an ext4 filesystem image with at least 64 MB of usable space. This SHALL accommodate the redb guest's workload (MAX_KEY=1000, values up to 64 bytes) across multiple crash/recovery cycles with compaction and WAL growth.

#### Scenario: Image is large enough for workload
- **WHEN** the redb guest runs for 5000 ticks across 100 branches
- **THEN** no `ENOSPC` errors occur unless a `DiskFull` fault was explicitly injected
