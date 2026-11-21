# Guest initrd and block-image attempt (2026-11-21)

## Host baseline

The fresh signed output
`/nix/store/9kh84csrlczijhwr04gg0xkvg0dxrl9y-molten-node-source-binding-v1`
admitted on the host and the real driver retained:

```
baseline observations=blake3:de2c103b33c9e0420972903386822e2477450bfe1f1d5dc4d5e3ecc19707434a
```

## Guest attempts

### Block disk with cached closure

- ext4 image: 455 MB actual / 477 MB virtual, below the 1 GiB block-image limit.
- The guest kernel reported `EXT4-fs (vda): unable to read superblock` and repeated
  `I/O error, dev vda, sector 0` with an empty fault schedule (`Schedule: 0 faults`).
- The wrapper's mount retry failed; no driver evidence was produced.

### Self-contained initrd

- Built an uncompressed initrd containing the signed output, the Nix verifier
  closure, a guest Nix daemon, the database dump, the cache record, and the
  stripped baseline driver.
- The kernel unpacked the initrd and ran `/init`.
- Nix needed a home directory; `/etc/passwd`, `/root`, and `HOME=/root` were added.
- Nix then rejected a referenced store path:
  `path '/nix/store/3sj8msrsym648f35mk3gb3pp385hhk8r-gcc-16.2.0-libgcc' is not valid`.
  The database dump includes references that the initrd did not copy, so a
  loadable database is not the same as a valid local store.
- The driver assertion failed and no Molten campaign evidence was produced.

## Conclusion

The cache avoids the 85 GB NAR measurement but not Nix's local-store validity and
reference requirements. A guest-capable source binding or a complete local store
provisioning path is still required. No guest campaign claim is made.
