let OperatorCommand = import "../operator-command.ncl" in
{
  schema = "chaoscontrol.typed-command-plan.v1",
  mechanism_revision = "29dac88ecded94457572db3fdfaaaab95fa91525",
  executable = {
    path = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-chaoscontrol/bin/replay-readiness",
    blake3 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    maximum_bytes = 16777216,
  },
  args = ["--receipt", "target/replay-readiness.json"],
  working_directory = ".",
  environment = {mode = "clear", entries = []},
  stdin = {mode = "null"},
  limits = {
    timeout_ms = 30000,
    stdin_max_bytes = 1024,
    stdout_max_bytes = 1048576,
    stderr_max_bytes = 1048576,
    poll_interval_ms = 10,
    teardown_timeout_ms = 1000,
  },
  accepted_exit_codes = [0],
  reject_stdout_truncation = true,
  reject_stderr_truncation = true,
  termination_scope = "process-group",
  evidence_eligible = true,
} | OperatorCommand
