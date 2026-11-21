# Molten guest admission attempt

Date: 2026-11-20. Result: **blocked before the driver started**.

## Inputs

- Signed output selected from Aspen's qualification evidence:
  `/nix/store/wns0dg28g06ma0f8mba8yvk0ci2k4hj4-molten-node-source-binding-v1`
- Selection refs from
  `.cairn/changes/host-public-addressable-actor/evidence/2026-11-20-signed-counter-flow/run.json`:
  binding `blake3:54fe…`, policy `blake3:e045…`, profile `blake3:902c…`.
- Trust record reconstructed with the output's registered
  `britton-desktop-1` Nix public key and the pinned Nix 2.36.0 verifier.
- `molten node init` completed, then `molten node run` performed source admission.

## Result

`molten node run` denied startup:

```
error: invalid harness artifact: source build NAR identity mismatch: /nix/store/c8lnhlc329hn74i7yriknrnpykw4wxi7-python3.13-cbor2-5.8.0
```

A second candidate output (`bibp2…`) denied a different path:

```
error: invalid harness artifact: source build NAR identity mismatch: /nix/store/q2vpbpf8vqzizz6836vlw3n83cjr8bma-mold-unwrapped-wrapper-2.40.4
```

The measured build closure is 21,247 paths and about 85.05 GB. Copying it into an
initrd is not viable; a disk-backed guest store was staged, but admission cannot
pass while the signed closure records NAR identities that the current store paths
do not reproduce.

## Conclusion

The guest wrapper and initrd/disk machinery can be built, but no current signed
Aspen output on this host admits its own recorded closure. The campaign requires a
freshly built and independently signed output whose recorded NAR identities match
the actual store paths, or a guest-specific signed output with a bounded runtime
cohort. Neither is available in this workspace.

## Additional guest blocker

The admission verifier is not a pure byte check. `src/node/source-binding/process.rs`
runs the trusted `nix` binary with `--store daemon --offline`, then performs
`path-info`, `hash path`, and `store verify` against that daemon. The signed
output's bytes and closure alone are insufficient: a guest needs a running Nix
daemon and writable store database, or an Aspen source-verification profile that
does not use the daemon. The image builder now includes the selected Nix
closure and a database dump, and the wrapper loads the dump and starts the daemon.
That path remains unexecuted; the campaign is still blocked on a consistent signed
output.
