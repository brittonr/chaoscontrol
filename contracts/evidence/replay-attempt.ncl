# Replay attempt contract for reported bugs.
let AcceptanceStatus = std.contract.from_predicate (fun value =>
  value == "accepted" || value == "partial" || value == "known-gap" || value == "invalid" || value == "raw-log-only"
) in
let NonEmptyString = std.contract.from_predicate (fun value => std.is_string value && std.string.length value > 0) in
{
  command | NonEmptyString,
  exit_status | Number,
  message | NonEmptyString,
  status | AcceptanceStatus | optional,
}
