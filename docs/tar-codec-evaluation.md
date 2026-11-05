# tar-codec evaluation

Evaluation for issue [#3](https://github.com/brittonr/chaoscontrol/issues/3).

`crates/chaoscontrol-evidence/Cargo.toml` uses `tar = "0.4"` to apply OCI layers
and tar archives in the bounded intake path
([`crates/chaoscontrol-evidence/src/oci_intake.rs`](../crates/chaoscontrol-evidence/src/oci_intake.rs)).
A parser differential between the scanner and the extractor would break the
"reviewed layer == executed layer" claim, so a second parser was compared on the
same archives.

Evaluated crate: [`tar-codec`](https://crates.io/crates/tar-codec) 0.0.16
(MIT OR Apache-2.0), part of [`astral-sh/astral-codec`](https://github.com/astral-sh/astral-codec).
Its own README states the project is in a "very early" state and not ready for
production use.

## Method

Harness: [`tools/tar-codec-differential`](../tools/tar-codec-differential)
(a detached Cargo workspace, excluded from the ChaosControl workspace). It defines
a small `TarLister` trait with two implementations — `tar 0.4` and
`tar-codec 0.0.16` — lists the same bytes with both, and applies the intake
admission rule (relative normal path, file or directory only; whiteouts are
regular files) to each entry list.

`tar-codec` is asynchronous and needs a Tokio current-thread runtime; the harness
adapts it with `block_on`. This is the same adapter a product integration would
need, and one reason a product trait was not added in this evaluation.

Corpus:

- Benign: a short-path layer with a directory, a GNU long-name entry, and OCI
  whiteout entries (`.wh.*`, `.wh..wh..opq`).
- Adversarial: absolute path, parent traversal, duplicate entry, symlink,
  hardlink with an out-of-root target, device node, truncated archive, PAX/ustar
  name override, GNU long-name, and a PAX-then-ustar mixed stream.
- Benchmark: 200 files of 4 KiB (901 KiB) listed 30 times; release build.

Run:

```bash
cd tools/tar-codec-differential
cargo run --release      # differential report and benchmark
cargo test --release     # the same checks as assertions
```

## Results

Benign corpus: both parsers produced identical entry lists (path, kind, size,
link target) for all three layers.

Adversarial corpus: both parsers agreed on every case; no divergence was recorded.

| Case | `tar 0.4` | `tar-codec 0.0.16` | Admission |
|---|---|---|---|
| absolute path | 1 entry | 1 entry | rejected by intake rule |
| parent traversal | 1 entry | 1 entry | rejected |
| duplicate entry | 2 entries | 2 entries | both admitted (last write wins in intake) |
| symlink | 1 entry | 1 entry | rejected (unsupported entry type) |
| hardlink out of root | 1 entry | 1 entry | rejected (unsupported entry type) |
| device node | 1 entry | 1 entry | rejected (unsupported entry type) |
| truncated archive | `unexpected EOF during skip` | `incomplete tar block` | rejected by both |
| PAX name override | 1 entry | 1 entry | agreed on the resolved path |
| GNU long name | 1 entry | 1 entry | agreed |
| PAX then ustar | 1 entry | 1 entry | agreed |

Benchmark (four repeated runs, 901 KiB layer, listing only):

| Parser | Per-run range | Typical |
|---|---|---|
| `tar 0.4` | 129–185 µs | ~150–165 µs |
| `tar-codec 0.0.16` | 99–163 µs | ~115–130 µs |

`tar-codec` is roughly 1.15–1.5× faster for listing in these runs, less than the
~2× figure in the issue's source post and workload-dependent. Listing does not
include extraction, policy checks, whiteout application, or hashing.

## Recommendation

Do not switch the intake parser now.

- `tar-codec` is 0.0.x and explicitly not production-ready; a parser swap in the
  evidence path needs a maintained dependency.
- Its extraction model differs from intake: extraction policies, mode
  normalization, hardlink opt-in, and streamwise partial extraction are its own
  semantics, so listing parity is not extraction parity.
- Intake is synchronous and streams into a destination directory; `tar-codec`
  requires an async executor, and a Tokio runtime inside the intake shell would
  be a new runtime dependency for evidence production.
- The measured speedup on a small layer does not offset the above.

Keep the harness as the differential check and revisit when `tar-codec` reaches a
stable release with a synchronous or blocking adapter and documented extraction
policy parity. If it is adopted later, add the `TarLister` trait behind the intake
seam with both implementations and keep the adversarial corpus as permanent
regressions.

## Non-claims

Listing parity does not prove extraction parity, archive safety, or intake
correctness. The corpus is small and synthetic; it does not cover every PAX/GNU
edge case, sparse files, large layers, compression, or malicious metadata. The
benchmark measures listing only and is not a layer-intake benchmark. No product
dependency, parser, or intake behavior changed in this evaluation.
