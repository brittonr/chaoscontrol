## Verification

Implementation evidence captured for the assertion-readiness promotion gate:

- `python scripts/check-assertion-readiness-promotion-gate.py` — passed; validates accepted workload assertion rows, gap guidance, evidence paths, and anti-overclaim text.
- `python scripts/check-assertion-readiness-promotion-gate.py --selftest` — passed; exercises negative fixtures for missing anti-claim text, hidden gap guidance, weakened report counts, report-only workload drift, and explicit overclaim fragments.
- `python -m py_compile scripts/check-assertion-readiness-promotion-gate.py` — passed.
- `python scripts/check-readiness-surface-drift.py` — passed; confirms `assertion-readiness-promotion` is present in executed static gates and receipt metadata.
- `python scripts/generate-assertion-readiness-report.py --check` — passed.
- `nix build .#checks.x86_64-linux.nixfmt --no-link -L` — passed.
- `nix build .#checks.x86_64-linux.evidence-contracts --no-link -L` — passed; includes the new checker.
- `nix build .#checks.x86_64-linux.replay-readiness --no-link -L` — passed with `static_gates=11/11` and `dogfood=skipped`.
- `openspec validate add-assertion-readiness-promotion-gate --strict --json` — passed.
- `openspec validate --all --strict --json` — passed.
- `git diff --check` — passed.

The slow VM dogfood rail was intentionally not run; this change is a static anti-overclaiming gate over already-accepted evidence.
