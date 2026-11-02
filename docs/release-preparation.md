# Public release and production preparation

## Decision and ownership

**Blocked; do not publish a production-ready claim.** Public source distribution
and bounded production acceptance are separate decisions. ChaosControl
maintainers own this checklist and the candidate's release decision; dependency
owners own publication of their sources. The selected route is to publish the
existing dependency closure, not remove functionality or replace pinned code.
Repository visibility and upstream publication require their owners' approval.

Immediate outcome of this preparation: repair native lifecycle linkage, remove
the available public Octet source's sibling-checkout dependency, correct denied
implicit Serde defaults without changing their values or admission rules, and
make remaining distribution and production prerequisites explicit. Repeatability
comes from existing Cairn, Cargo, dependency-policy, evidence and KVM rails, not a new
parallel readiness implementation.

## Public-source prerequisites

A warm developer cache is not evidence of a fresh public installation. Before
release, anonymously acquire every exact source in `flake.lock` and `Cargo.lock`
from a new checkout with empty source caches and without sibling checkouts,
credential helpers, SSH agents or private Radicle authorization.

| Source boundary | Exact selected revision | Observed anonymous availability and required action |
| --- | --- | --- |
| Root Octet | `9c7ba87bef2934d2b7b144167e13c8d18eac8958` | GitHub API returned the commit; the lock now pins public GitHub instead of `file:../octet` with the same NAR identity. Verify the complete transitive closure separately. |
| Root Trellis | `46ab2d92b9cfd2cfc4e631a56f3e667ee7263685` | Still `file:../trellis`; the exact GitHub commit endpoint returned anonymous HTTP 404. Its owner must publish the existing source or approve a public mirror before this locator can be migrated. |
| VM Cohort | `ab123e3673b6dd616b3df5d044026b5e85755149` | `rad://z2QJLUqyAZnnHPiZQ1BFjLsX9ush3` is documented as private. `git ls-remote` against the public Radicle seed returned HTTP 500, so no public copy was observed. Owner-approved public acquisition is required for both Cargo and Nix. |
| Campaign sources | `796fe3844c91c3060da5ffc573a6f801a214aba7`, `b3e08e19750f53bdbcae970cdf58a47a791ed20b` | Nix inputs now use `git+https://seed.radicle.garden/<id>.git` with the same revisions and NAR identities, so Nix acquisition no longer needs a Radicle client. Cargo still uses `rad://` for these crates; see the source-acquisition contract. |
| Transitive Octet | `09b2d863adf0b88b905f0ca62abf3e297e4988fa` | Root-level input override now pins `github:OnixResearch/octet/<rev>`; the lock NAR identity is unchanged. |
| Artifact source | `e41340bec587b6d049b5cc518ec7db925dde84be` | Root-level input override now pins `github:OnixResearch/onix-artifact/<rev>`; the lock NAR identity is unchanged. |
| Artifact authentication source | `c932138d880ddf4c2967f4c024b489b5c0022bf1` | Same public override; the lock NAR identity is unchanged. Source availability and authentication/authority remain distinct. |

These rows identify concrete nonportable/private boundaries, not an exhaustive
public-acquisition verdict. Other Git/HTTP dependencies, recursive flake inputs,
Cargo source repairs and build fetches must also pass the fresh-cache run. Do not
rewrite a lock entry to an unverified endpoint, expose private sources without
approval, use sibling paths as a public fallback or treat a cached build as proof.
See [source acquisition](campaign-source-acquisition.md) for the required patched
Cargo transport and current limitations.

Re-audit the whole lock graph after any refresh; this reports every node that
still needs SSH credentials, a sibling checkout, or a Radicle client:

```sh
python3 - <<'PY'
import json
nodes = json.load(open("flake.lock"))["nodes"]
for name, node in nodes.items():
    url = node.get("locked", {}).get("url", "")
    if url.startswith(("ssh://", "file:")) or "rad://" in url:
        print(name, node["locked"].get("rev"), url)
PY
```

At preparation time this printed only `trellis` (`file:../trellis`) and
`vm-cohort-src` (`rad://z2QJLUqyAZnnHPiZQ1BFjLsX9ush3`) out of 127 nodes. Treat any
new line as a release blocker until its owner supplies an anonymous locator for
the same revision.

### Cargo acquisition observation

With the repository dev shell's patched Cargo and an empty `CARGO_HOME`,
`cargo fetch --locked` completed in six seconds and populated 504 crate archives
plus every locked Git source, including the `rad://` Campaign and VM Cohort
entries and the `git.onix.computer` bounded-exec source. That observation ran on a
host where `git-remote-rad` is installed.

It does not establish anonymous public acquisition. A host without the Radicle
Git helper, without SSH credentials, or without access to the private VM Cohort
repository still cannot complete the same fetch. Migrating those Cargo sources to
anonymous locators, or documenting the required helper as a prerequisite, is an
owner decision and remains a public-release blocker.

## Candidate checks

Use the repository Nix development environment on x86_64 Linux. `/dev/kvm` must be
readable and writable for VM execution; Nix KVM builders need the `kvm` system
feature. Review [licensing](licensing.md) and [security policy](../SECURITY.md)
before distributing artifacts. Verify a working private vulnerability-reporting
channel; no support/backport promise is currently made.

The consumer-owned portable command authority is
[`fixtures/agent-completion/registry.ncl`](../fixtures/agent-completion/registry.ncl):

```sh
nix develop
cargo test --workspace --all-targets --locked --no-fail-fast
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
nix flake check -L
```

For release completion, execute those full commands through the genuine bounded
Cairn producer and original consumer, with measured source/tool/dependency
identities and unchanged bounds. Follow the
[verified cohort procedure](../cairn-policy/UPSTREAM.md#measured-export-and-execution-sequence).
Ordinary terminal success is useful diagnostic evidence, not fabricated producer
completion. Select the verified compatible Cairn CLI explicitly and pass an
absolute `--policy` path; an arbitrary installed CLI is not cohort evidence.

Independent obligations remain strict native validation, native traceability,
policy projection freshness, original completion/MCP verdicts and native release
readiness. `nix run .#replay-readiness` exercises bounded static evidence gates;
it does not establish successful guest replay. The full flake check includes
dependency advisory and source/license policy checks. Preserve all failures.

On one clean candidate revision, run the existing KVM matrix and independent
Rust receipt validator, with `CANDIDATE_REVISION` set to that exact revision and
`MATRIX_OUT` set to a fresh output directory:

```sh
nix run .#kvm-release-matrix -- --root . \
  --matrix contracts/kvm-release/matrix.json \
  --out "$MATRIX_OUT" --expected-revision "$CANDIDATE_REVISION"
nix develop -c cargo run -p chaoscontrol-evidence \
  --bin check-kvm-release-matrix -- --root . \
  --receipt "$MATRIX_OUT/release-receipt.json" \
  --expected-revision "$CANDIDATE_REVISION"
```

Use pueue for long builds/campaigns. Keep source, matrix, executable, config,
artifacts and verdict identities together in the existing Nickel-backed evidence
format. Retain raw logs locally. Do not silently substitute a rebuilt initrd,
increase watchdog bounds, relabel an operational timeout as a guest finding or
promote `onix_release_ready=false` into a successful authenticated-release claim.

## Preparation evidence and limits

Baseline source: `1caeda2334f1d93e53ce22afbfa012b05458f606`. Preparation commits
`3db8a19c` and `fa6f3256` are local and are not a published release. The local
command observations are under `target/release-preparation-20261029/`; that
ignored path is a diagnostic locator, not a portable dependency or published
evidence store. The tracked [preparation record](../.cairn/changes/prepare-public-production-release/evidence/verification.md)
records exercised checks and outstanding prerequisites.

On commit `3db8a19c`, `nix flake check -L --keep-going` evaluated 106 checks and
failed four. Two were repaired and now pass on `fa6f3256`:

- `evidence-contracts` and `replay-readiness` failed through
  `check-accepted-dogfood-config`, which derived an empty repository root from the
  default relative manifest path and then failed to canonicalize it. The root is
  now the current directory for relative manifests, with a regression test.
  Re-running both checks on `fa6f3256` succeeded.

Two KVM checks still fail operationally, and the base revision fails the same
snapshot rail:

- `snapshot-replay-smoke` fails with `HostWatchdogTimeout { vcpu: 0 }` in
  isolation. The same check on base `1caeda23` fails with `rc=124` and
  `missing checkpoint.json after run`.
- `local-multi-hypervisor-kvm-smoke` fails in isolation: the Raft attempt hits a
  240-second `accepted-snapshot-verdict-dogfood` timeout and all six
  Rust-workload attempts report `HostWatchdogTimeout { vcpu: 0 }`.

A runnable single-vCPU guest now retries across consecutive host watchdog
intervals; only an uninterrupted series with no KVM exit (about five seconds) is
an operational timeout, and the smoke harness force-kills a blocked VMM after its
bound. Both changes are unit-tested, but the KVM rails were not re-run to
completion because the host was oversubscribed (load average 57–86 from unrelated
processes). Re-run them on a quiescent host before any release claim.

These are operational watchdog/timeout outcomes in the KVM rails, not reproduced
guest findings. They are retained as failures; nothing is relabeled as passing. A bounded timing probe
showed the same Raft workload still progressing at tick `15600` when a 180-second
outer bound fired, and a five-second watchdog interval variant stalled without
completing. The rails are therefore wall-clock sensitive: the single-vCPU 100 ms
no-exit watchdog can interrupt a runnable vCPU and become a permanent
`HostWatchdogTimeout`, and the enclosing script bound did not reliably terminate a
stalled process. Resolving that is a design decision about operational versus
evidence bounds; no bound or signal semantic was changed here. A clean-candidate
KVM pass, genuine completion-producer results, public acquisition of the remaining
private or SSH-only sources, and the maintainer's release decision are still
required.

Native baseline validation found an unlinked native-process validation task;
traceability had ten dangling IDs. The repair synchronizes existing native-process,
next-witness and storage requirements and binds the storage implementation/tests
to their actual obligations. Synchronization does not close incomplete tasks:
the Molten candidate campaign and existing change closeout remain open. No
runtime behavior, required policy, historical verdict or guest evidence is
changed by those linkage repairs.

Historical MCP correction and replacement planted-finding receipts retain their
original source revisions. The accepted replacement disposition does not turn
missing historical bytes into recovered artifacts. Current-source completion,
clean-candidate KVM evidence, public dependency acquisition and the maintainer's
release decision remain necessary before promotion.
