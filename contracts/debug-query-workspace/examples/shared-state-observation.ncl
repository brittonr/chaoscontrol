# Merge executable_blake3 from the exact built supervisor before exporting.
# This file intentionally cannot export with an unbound executable identity.
{
  executable = "init",
  executable_blake3 | doc "Required BLAKE3 bytes from the built guest supervisor",
  argv = ["--observe-shared-state"],
  environment = {},
  working_directory = "data",
  deadline_tick = 5000,
  stdout_limit = 1024,
  stderr_limit = 1024,
  mutates_guest = false,
} | (import "../observation-profile.ncl")
