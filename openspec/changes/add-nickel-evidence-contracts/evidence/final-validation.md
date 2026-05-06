# Final validation — Nickel evidence contracts

Captured: 2026-05-06T14:48:07Z

## Implemented surfaces

- `contracts/evidence/*.ncl` defines run config, dogfood receipt, bug report, assertion summary, checkpoint reference, artifact hash, and replay attempt contracts.
- `contracts/evidence/examples/*.ncl` imports committed Raft fixtures through the contracts.
- `contracts/evidence/fixtures/valid/*.json` and `contracts/evidence/fixtures/invalid/*.json` cover positive and negative contract cases.
- `scripts/materialize-dogfood-receipt.py` deterministically materializes `run-config.json` and `receipt.json` for the Raft dogfood corpus.
- `scripts/check-evidence-contracts.py` validates Nickel examples, committed dogfood receipt hashes, positive fixtures, and required negative failures.
- `flake.nix` exposes `checks.x86_64-linux.evidence-contracts`.
- Rust round-trip tests deserialize and reserialize the Nickel-validated bug, checkpoint, and assertion fixtures through the Rust-owned DTOs.

## Commands

```bash
python -m py_compile scripts/check-contract-registry.py scripts/check-evidence-contracts.py scripts/materialize-dogfood-receipt.py
python scripts/materialize-dogfood-receipt.py dogfood-results/raft-20260506-095025 --git-revision bd8fb21 --replay-status known-gap --replay-message 'Bug NOT reproduced — assertion 1205943209 did not fail' --replay-exit-status 1
python scripts/check-contract-registry.py
python scripts/check-evidence-contracts.py
CARGO_TARGET_DIR=target RUSTC_WRAPPER= RUSTC_BOOTSTRAP=1 cargo test -p chaoscontrol-explore --lib nickel_ -- --nocapture
nix build .#checks.x86_64-linux.evidence-contracts --no-link -L
openspec validate add-nickel-evidence-contracts --strict --json
git diff --check
```

## Outcomes

- `check-contract-registry.py`: pass — `contract registry ok: 8 families, ownership=excluded,nickel-authored,rust-derived`
- `check-evidence-contracts.py`: pass — `evidence contracts ok: nickel examples, dogfood receipt, positive fixtures, negative fixtures`
- Rust focused tests: pass — 3 `nickel_` round-trip tests passed.
- Nix check: pass — `evidence-contracts-check` ran registry and evidence validators.
- OpenSpec strict validation: pass.
- Whitespace check: pass.

## Known gap retained

The Raft dogfood receipt remains `known-gap` because `bug_0.json` did not reproduce standalone:

```text
Bug NOT reproduced — assertion 1205943209 did not fail
```

That failure is now explicit machine-checkable receipt data rather than an implicit Markdown-only caveat.
