## Why

ChaosControl migrated from the legacy lifecycle layout to native Cairn. Some accepted legacy specs predate Cairn's current requirement-shape rules.

## What Changed

- Preserve legacy accepted legacy specs inside this Cairn archive package.
- Keep `.cairn/specs/` limited to native Cairn specs that pass current validation.
- Remove the legacy `openspec/` tree and OpenSpec wrapper.

## Non-goals

- Do not rewrite old legacy requirements during the layout migration.
- Do not promote legacy spec shape as current Cairn style.
