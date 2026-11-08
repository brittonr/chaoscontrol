let MakeQualification = import "../../examples/kvm-debug-boundary-qualification.ncl" in
# Relative runtime locations name the measured input bytes; this portable fixture
# checks intent, not artifact existence, KVM admission, or successful qualification.
MakeQualification {
  kernel = "qualification-inputs/kernel.bin",
  initrd = "qualification-inputs/initrd.bin",
  observation_profile = "qualification-inputs/profile.json",
  admitted_observation_profiles = "qualification-inputs/observation-profiles.json",
}
