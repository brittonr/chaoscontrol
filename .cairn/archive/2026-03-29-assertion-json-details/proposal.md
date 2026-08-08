# Proposal: Assertion JSON Details

## Problem

ChaosControl's SDK assertion functions accept `details: &serde_json::Value` for structured metadata, but the current usage is stringly-typed with ad-hoc key names across different guests. This makes assertion details inconsistent, hard to analyze, and poorly integrated with oracle/triage tooling.

Current issues:
- No standardized schema for assertion details across call sites
- Ad-hoc key naming (`"node_id"` vs `"peer"` vs `"target"`)
- Oracle reports serialize details but don't display them meaningfully
- No helper functions to build well-structured details consistently

## Proposed Solution

**New Capability: `assertion-json-details`**

Standardize assertion detail structures through:

1. **Helper API**: New `chaoscontrol_sdk::assert::details` module with builder functions for common assertion patterns
2. **Standard Keys**: Consistent naming via constants/enum for detail fields  
3. **Oracle Integration**: Include structured details in triage reports and failure analysis
4. **Call Site Updates**: Migrate existing assertions to use standardized helpers

This improves assertion quality, makes oracle output more actionable, and provides better debugging context during fault injection campaigns.

## Benefits

- **Consistency**: Uniform detail schemas across all assertion types
- **Discoverability**: Helper functions guide developers toward well-structured assertions  
- **Debuggability**: Oracle reports include meaningful assertion context
- **Backward Compatible**: Existing `json!({})` usage continues to work

## Success Criteria

- All existing assertion call sites use standardized detail helpers
- Oracle triage reports display assertion details for failed/interesting cases
- New assertions follow consistent schema patterns
- API remains backward compatible with raw JSON usage