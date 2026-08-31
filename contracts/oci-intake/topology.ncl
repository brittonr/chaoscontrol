# Typed image-sourced guest topology.
let P = import "../evidence/profile-primitives.ncl" in
let B3 = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^b3:[0-9a-f]{64}$" value
) in
let AbsolutePath = std.contract.from_predicate (fun value =>
  std.is_string value
  && std.string.length value > 0
  && std.string.length value <= 1024
  && std.string.is_match "^/" value
  && !(std.string.is_match "(^|/)\\.\\.($|/)" value)
) in
let RelativePath = std.contract.from_predicate (fun value =>
  std.is_string value
  && std.string.length value > 0
  && std.string.length value <= 1024
  && !(std.string.is_match "^/" value)
  && !(std.string.is_match "(^|/)\\.\\.?($|/)" value)
) in
let BoundedText = std.contract.from_predicate (fun value =>
  std.is_string value
  && std.string.length value <= 256
  && !(std.string.is_match "[[:cntrl:]]" value)
) in
let Environment = std.contract.from_predicate (fun value =>
  std.is_record value
  && std.array.length (std.record.fields value) <= 64
  && std.record.to_array value |> std.array.all (fun entry =>
    std.string.is_match "^[A-Z_][A-Z0-9_]*$" entry.field
    && std.is_string entry.value
    && std.string.length entry.value <= 256
  )
) in
let Layer = {
  path | RelativePath,
  identity | B3,
} in
let Source = {
  format | P.non_empty_string,
  path | AbsolutePath,
  image_identity | B3,
  layers | Array Layer,
} in
let Restart = {
  mode | P.closed_enum ["never", "on_failure", "always"],
  max_restarts | P.integer_between 0 64,
} in
let Service = {
  role | P.identifier,
  source | Source,
  entrypoint | RelativePath,
  arguments | Array BoundedText,
  environment | Environment,
  shared_directories | Array P.identifier,
  restart | Restart,
  instrumented | Bool,
  transport_slot | P.nullable_u32,
} in
let SharedDirectory = {
  id | P.identifier,
  path | AbsolutePath,
  device | P.closed_enum ["memory", "block"],
} in
let Shape = {
  schema | P.exact_string "chaoscontrol.oci-topology.v1",
  topology_id | P.identifier,
  shared_directories | Array SharedDirectory,
  services | Array Service,
} in
let Coherent = fun value =>
  std.array.length value.services > 0
  && std.array.length value.services <= 32
  && std.array.length value.shared_directories <= 16
  && P.unique_by (fun service => service.role) value.services
  && P.unique_by (fun directory => directory.id) value.shared_directories
  && std.array.all (fun service =>
    std.array.length service.source.layers <= 64
    && std.array.length service.arguments <= 64
    && P.unique_by (fun layer => layer.path) service.source.layers
    && std.array.all (fun directory_id =>
      std.array.any (fun directory => directory.id == directory_id) value.shared_directories
    ) service.shared_directories
    && ((service.instrumented && service.transport_slot != null)
      || (!service.instrumented && service.transport_slot == null))
  ) value.services
in
std.contract.custom (fun label value =>
  std.contract.check Shape label value
  |> match {
    'Ok checked => if Coherent checked then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)
