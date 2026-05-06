# Contract registry implementation evidence

Status: captured

## Commands / Oracles

- command: `nix run nixpkgs#nickel -- export contracts/evidence/registry.ncl >/tmp/chaoscontrol-registry.json`
- command: `python scripts/check-contract-registry.py`

## Outcomes

- result: pass — Nickel exported `contracts/evidence/registry.ncl` successfully.
- result: pass — Registry checker reported `contract registry ok: 8 families, ownership=excluded,nickel-authored,rust-derived`.

## Notes

The registry lands before detailed contracts to prevent source-of-truth drift. It classifies human-authored run config and dogfood receipt surfaces as `nickel-authored`; runtime-emitted bug reports, assertion summaries, checkpoint references, and campaign progress as `rust-derived`; and raw logs plus secrets/crypto internals as `excluded`.
