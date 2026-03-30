## 1. Kernel cmdline fix

- [ ] 1.1 Change `panic=-1` to `panic=0` in both cmdline templates in `vm.rs` (`build_cmdline` and the default const).
- [ ] 1.2 Verify the integration test suite still passes (Tests 1-33).

## 2. Serial panic detection

- [ ] 2.1 Add `panic_detected: bool` field to `DeterministicVm`.
- [ ] 2.2 In the serial I/O write path (`step()` IoOut for serial ports), scan each byte for the string `Kernel panic`. Use a sliding window match (shift a `u64` and compare against the target pattern).
- [ ] 2.3 In `step()`, after every exit, check `panic_detected`. If set, log a warning and return `Ok(true)` (halted).
- [ ] 2.4 Clear `panic_detected` in `restore()`.
- [ ] 2.5 Initialize `panic_detected = false` in `DeterministicVm::new`.
- [ ] 2.6 Unit test: write "Kernel panic" byte-by-byte to a mock serial path and verify detection triggers. Write normal output and verify no false positive.

## 3. VcpuExit::Shutdown handling

- [ ] 3.1 Verify `VcpuExit::Shutdown` in `step()` already returns `Ok(true)`. If it currently only logs, change it to also set `panic_detected = true` so the controller treats it as a crash.

## 4. Integration test

- [ ] 4.1 Add integration test: inject ProcessKill fault, verify the VM stops (run_bounded returns) within a bounded number of exits instead of hanging.
- [ ] 4.2 Re-run the `explore_validation` stress test and verify it completes without hanging.
