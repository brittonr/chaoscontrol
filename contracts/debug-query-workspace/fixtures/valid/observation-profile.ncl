# Lexical fixture only; these bytes do not identify an executable to run.
{
  executable = "init",
  executable_blake3 = [1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],
  argv = ["--observe-shared-state"],
  environment = {},
  working_directory = "data",
  deadline_tick = 5000,
  stdout_limit = 1024,
  stderr_limit = 1024,
  mutates_guest = false,
} | (import "../../observation-profile.ncl")
