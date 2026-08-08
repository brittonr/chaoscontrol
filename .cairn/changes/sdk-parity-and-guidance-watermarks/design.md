## Context

`chaoscontrol-sdk` (0.1.0) mirrors the Antithesis Rust SDK surface:
`chaoscontrol_init`, `cc_assert_*` macros, `lifecycle::setup_complete`,
`send_event`, and `random::get_random`. The two SDKs differ in transport,
assertion identity, and coverage. Antithesis has guidance watermarks;
ChaosControl has `record_state` protocol coverage instead.

The AGENTS.md design reference treats Antithesis as a comparison source,
not a requirement or parity claim. This change produces the tracked
mapping and the guidance decision to keep that boundary explicit.

## Decisions

### 1. A tracked parity document lives in docs/references

**Choice:** Store the parity mapping at
`docs/references/sdk-antithesis-rust-parity.md`, organized by surface with
a status code per entry (equivalent, superset, subset, divergent, absent)
and a version note.

**Rationale:** The existing Antithesis index already lives in
`docs/references/`, and a single tracked document is easier to review and
diff than scattered notes.

### 2. Parity is a comparison aid, not a requirement

**Choice:** The mapping marks ChaosControl-only and Antithesis-only
surfaces without creating parity obligations. AGENTS.md keeps authority.

**Rationale:** Treating Antithesis presence as a requirement would
overclaim and contradict the documented non-goal (Rust-only guests, no
language-agnostic SDK).

### 3. Guidance watermarks are not a current requirement

**Choice:** Record that Antithesis numeric/boolean guidance watermarks
(atomic `fetch_min`/`fetch_max` guards, `maximize` flags) are a MAY-level
item, not a current requirement.

**Rationale:** ChaosControl captures `left`/`right` in failure details and
uses `record_state` for state-aware coverage. The Antithesis guidance
mechanism is one explorer signal, and the ChaosControl explorer does not
depend on it today.

### 4. The local-output schema is a documented superset

**Choice:** Document that ChaosControl local output emits the Antithesis
fallback records plus `chaoscontrol_*` extension records and identity
fields.

**Rationale:** Reusing `antithesis_assert`/`antithesis_setup` keeps
compatibility readable, but callers must know the output is a superset and
a strict validator sees extra fields.

## Risks / Trade-offs

- Mapping drift: the document can age if SDK surfaces change. The decision
  lists reviewed source versions and dates to bound this.
- Overclaim risk: a reader can mistake the map for a parity requirement.
  The status codes and the reference-only note control this.
- Guidance limitation: not implementing watermarks does not hurt today. A
  future explorer change can reopen the decision.
