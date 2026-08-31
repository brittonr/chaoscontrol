# Typed process manifest for one deterministic guest image.
let P = import "../evidence/profile-primitives.ncl" in
let MaxProcesses = 32 in
let MaxSharedDirectories = 16 in
let MaxArguments = 64 in
let MaxEnvironmentFields = 64 in
let AbsolutePath = std.contract.from_predicate (fun value =>
  std.is_string value
  && std.string.length value > 0
  && std.string.length value <= 1024
  && std.string.is_match "^/" value
  && !(std.string.is_match "(^|/)\\.\\.($|/)" value)
  && !(std.string.is_match "[[:cntrl:]]" value)
) in
let BoundedText = std.contract.from_predicate (fun value =>
  std.is_string value
  && std.string.length value <= 256
  && !(std.string.is_match "[[:cntrl:]]" value)
) in
let Environment = std.contract.from_predicate (fun value =>
  std.is_record value
  && std.array.length (std.record.fields value) <= MaxEnvironmentFields
  && std.record.to_array value
    |> std.array.all (fun entry =>
      std.string.is_match "^[A-Z_][A-Z0-9_]*$" entry.field
      && std.is_string entry.value
      && std.string.length entry.value <= 256
      && !(std.string.is_match "[[:cntrl:]]" entry.value)
    )
) in
let SharedDirectory = {
  id | P.identifier,
  path | AbsolutePath,
  device | P.closed_enum ["memory", "block"],
} in
let RestartPolicy = {
  mode | P.closed_enum ["never", "on_failure", "always"],
  max_restarts | P.integer_between 0 64,
} in
let Process = {
  role | P.identifier,
  executable | AbsolutePath,
  arguments | Array BoundedText,
  environment | Environment,
  shared_directories | Array P.identifier,
  restart | RestartPolicy,
  instrumented | Bool,
  transport_slot | P.nullable_u32,
} in
let Shape = {
  schema | P.exact_string "chaoscontrol.process-manifest.v1",
  guest | P.identifier,
  shared_directories | Array SharedDirectory,
  processes | Array Process,
} in
let Coherent = fun value =>
  std.array.length value.processes <= MaxProcesses
  && std.array.length value.shared_directories <= MaxSharedDirectories
  && P.unique_by (fun directory => directory.id) value.shared_directories
  && P.unique_by (fun process => process.role) value.processes
  && std.array.all (fun process =>
    std.array.length process.arguments <= MaxArguments
    && std.array.all (fun directory_id =>
      std.array.any (fun directory => directory.id == directory_id) value.shared_directories
    ) process.shared_directories
    && ((process.instrumented && process.transport_slot != null)
      || (!process.instrumented && process.transport_slot == null))
  ) value.processes
  && P.unique_by (fun process =>
    if process.transport_slot == null then "none:%{process.role}"
    else "slot:%{std.to_string process.transport_slot}"
  ) value.processes
in
std.contract.custom (fun label value =>
  std.contract.check Shape label value
  |> match {
    'Ok checked => if Coherent checked then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)
