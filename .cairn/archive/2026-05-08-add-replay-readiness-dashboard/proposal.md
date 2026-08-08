## Why

The replay-readiness receipt gives CI a machine-readable status, but operators still need a directly viewable artifact that can be opened from a CI run without scraping logs or raw JSON. An HTML dashboard artifact closes the visibility gap while preserving the bounded anti-claim around committed replay evidence.

## What Changes

- Add a deterministic receipt-to-HTML dashboard renderer for replay-readiness receipts.
- Package the renderer as a Nix app and include the generated dashboard in the replay-readiness check output.
- Upload the dashboard artifact from GitHub Actions and document the local workflow.

## Impact

- **Files**: `scripts/`, `flake.nix`, `.github/workflows/ci.yml`, `README.md`, OpenSpec files.
- **APIs**: New `replay-readiness-dashboard <receipt.json> --output <dashboard.html>` command/app.
- **Testing**: Script self-tests, Nix replay-readiness check, OpenSpec validation, whitespace check.
