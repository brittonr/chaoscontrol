## Why

The replay readiness command can already emit a JSON receipt and one-line summary, but CI needs a first-class check surface that preserves both artifacts for dashboard and operator review.

## What Changes

- Add a Nix check that runs replay readiness with receipt emission and stores both receipt and summary text in the check output.
- Wire GitHub Actions to build that check, print the summary line, and upload both artifacts.
- Document the CI/check artifact path.

## Impact

- Files: `flake.nix`, `.github/workflows/ci.yml`, `README.md`, replay-parent snapshot spec.
- Testing: build the new replay readiness check, run the existing summary app, validate the OpenSpec change.
