## Tasks

- [ ] [serial] Add and build the `chaoscontrol-molten-guest` wrapper that selects the source trust from the cmdline and runs the exact Aspen collaboration-native driver case. r[chaoscontrol.molten_guest.wrapper]
- [ ] [depends:molten-guest-wrapper] Assemble a guest image that preserves the signed Aspen output, its closure, the driver binary, and the operator trust at absolute paths; the current 21,247-path, 83.9 GB build closure requires a bounded guest output or disk-backed store. r[chaoscontrol.molten_guest.initrd]
- [ ] [depends:molten-guest-initrd] Run the single-guest, no-fault campaign and retain the driver observation, SDK events, assertion results, and artifact identities. Blocked: no current signed Aspen output admits its recorded closure; see `evidence/2026-11-20-admission-attempt.md`. r[chaoscontrol.molten_guest.campaign]
- [ ] [depends:molten-guest-campaign] Add and run a three-guest cohort with one independently admitted node per guest, including crash/restart, partition, and recovery schedules. r[chaoscontrol.molten_guest.cohort]
- [ ] [depends:molten-guest-cohort] Classify and import the bounded evidence into Aspen without widening the driver's non-claims. r[chaoscontrol.molten_guest.evidence]
