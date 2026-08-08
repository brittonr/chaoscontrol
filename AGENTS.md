# Agent Notes

## Workflow

- Use native Cairn under `.cairn/` for planned product or architecture changes before implementation. Keep spec-only changes active with only foundation tasks checked; do not mark implementation tasks complete until evidence exists.
- For configuration/evidence work, prefer Nickel contracts at review boundaries: human-authored run configs and receipts are Nickel-backed; runtime-emitted bug/checkpoint/assertion records remain Rust-owned and are validated by contracts or generated schemas.
- Dogfood evidence should include a concise validated receipt that binds commands, git rev, built artifacts, config digest, artifact hashes, assertion coverage, bug files, replay attempts, and known gaps. Raw `run.log`/`reproduce.log` files are debug aids and should stay local/ignored unless deliberately summarized.

## Design references

- Read `docs/references/antithesis-documentation.md` when work concerns deterministic simulation, fuzzing, assertions, faults, exploration, replay, reports, or debugging.
- Use the Antithesis material as a comparison source. Do not treat it as a ChaosControl requirement or parity claim.

## Non-Goals

- **Container image intake**: No Docker/OCI/Compose workflow. Users write Rust guest binaries.
- **Language-agnostic SDKs**: Rust-only SDK is intentional. No Go/Java/Python/C SDK planned.
- **Nickel-owned runtime traces**: Do not hand-author high-volume checkpoints, raw logs, or VM execution traces as Nickel. Use Nickel for configs/contracts/receipts and Rust for runtime record serialization.
