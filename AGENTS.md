# Agent Notes

## Workflow

- Use native Cairn under `.cairn/` for planned product or architecture changes before implementation. Keep spec-only changes active with only foundation tasks checked; do not mark implementation tasks complete until evidence exists.
- `cairn-policy/` owns the Nickel policy sources. Regenerate the projection with `cairn policy export --source cairn-policy/default.ncl --output cairn-policy/generated/cairn-policy.json`. Keep the generated projection committed; the CLI discovers it by default.
- For configuration/evidence work, prefer Nickel contracts at review boundaries: human-authored run configs and receipts are Nickel-backed; runtime-emitted bug/checkpoint/assertion records remain Rust-owned and are validated by contracts or generated schemas.
- Dogfood evidence should include a concise validated receipt that binds commands, git rev, built artifacts, config digest, artifact hashes, assertion coverage, bug files, replay attempts, and known gaps. Raw `run.log`/`reproduce.log` files are debug aids and should stay local/ignored unless deliberately summarized.

## Generated agent guidance

Generated OpenSpec skills and prompts are legacy compatibility guidance. They do not replace the native Cairn workflow or its required gates.
Keep local workflow overrides here. Update shared templates through their owning generator.
For an approved legacy apply task, use these repository rules:

- Diagnose and correct recoverable errors within the approved scope before continuing.
- If an unresolved requirement changes the outcome, resolve it before implementation.
- If a lifecycle review or approval is required, complete it before the protected action.
- If a concrete blocker prevents progress, report its evidence and the next required decision.
- Preserve read-only exploration, ambiguous change selection, and required evidence or archive gates.

## Design references

- Read `docs/references/antithesis-documentation.md` when work concerns deterministic simulation, fuzzing, assertions, faults, exploration, replay, reports, or debugging.
- Use the Antithesis material as a comparison source. Do not treat it as a ChaosControl requirement or parity claim.

## Non-Goals

- **Container image intake**: No Docker/OCI/Compose workflow. Users write Rust guest binaries.
- **Language-agnostic SDKs**: Rust-only SDK is intentional. No Go/Java/Python/C SDK planned.
- **Nickel-owned runtime traces**: Do not hand-author high-volume checkpoints, raw logs, or VM execution traces as Nickel. Use Nickel for configs/contracts/receipts and Rust for runtime record serialization.
