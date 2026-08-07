# Dogfood evidence

`accepted-workload-proofs.json` is an unchanged schema-v1 historical manifest. Its names and claim text record the classification used when those artifacts were created.

The manifest does not provide current promotion authority. Current validation classifies each retained workload as `blocked-assertion-identity`. See `../docs/replay-readiness-status.md` and `../docs/assertion-readiness-status.md`.

`accepted-dogfood-expectations.json` binds live wrapper attempts. Its live aliases can differ from the historical manifest. Each historical reference declares its blocked promotion status.

Generate fresh admitted v2 KVM evidence before you promote, reproduce, or minimize a retained ID-only carrier.
