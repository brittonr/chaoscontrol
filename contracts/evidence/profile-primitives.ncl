# Shared authoring contracts for bounded ChaosControl profiles.
let U32Maximum = 4294967295 in
let U64Maximum = 18446744073709551615 in
let MaxIdentifierBytes = 128 in
let MaxPathBytes = 4096 in
let ExactValue = fun expected => std.contract.from_predicate (fun value => value == expected) in
let ExactString = fun expected => std.contract.from_predicate (fun value =>
  std.is_string value && value == expected
) in
let ClosedEnum = fun alternatives => std.contract.from_predicate (fun value =>
  std.is_string value
  && std.array.length (std.array.filter (fun candidate => candidate == value) alternatives) == 1
) in
let IntegerBetween = fun minimum maximum => std.contract.from_predicate (fun value =>
  std.is_number value
  && std.number.is_integer value
  && value >= minimum
  && value <= maximum
) in
let U32 = IntegerBetween 0 U32Maximum in
let U64 = IntegerBetween 0 U64Maximum in
let PositiveU32 = IntegerBetween 1 U32Maximum in
let PositiveU64 = IntegerBetween 1 U64Maximum in
let BoundedPositive = fun maximum => IntegerBetween 1 maximum in
let Identifier = std.contract.from_predicate (fun value =>
  std.is_string value
  && std.string.length value > 0
  && std.string.length value <= MaxIdentifierBytes
  && std.string.is_match "^[a-z0-9]([a-z0-9._-]*[a-z0-9])?$" value
) in
let NonEmptyString = std.contract.from_predicate (fun value =>
  std.is_string value
  && std.string.length value > 0
  && std.string.length value <= 1024
  && !(std.string.is_match "[[:cntrl:]]" value)
) in
let NullableNonEmptyString = std.contract.custom (fun label value =>
  if value == null then 'Ok value
  else std.contract.check NonEmptyString label value
) in
let NullableU32 = std.contract.from_predicate (fun value =>
  value == null
  || (std.is_number value
    && std.number.is_integer value
    && value >= 0
    && value <= U32Maximum)
) in
let Blake3 = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^blake3:[0-9a-f]{64}$" value
) in
let Sha256 = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.is_match "^sha256:[0-9a-f]{64}$" value
) in
let SafePath = fun value =>
  std.is_string value
  && std.string.length value > 0
  && std.string.length value <= MaxPathBytes
  && !(std.string.is_match "\\\\" value)
  && !(std.string.is_match "(^|/)\\.($|/)" value)
  && !(std.string.is_match "(^|/)\\.\\.($|/)" value)
in
let ArtifactReferenceShape = {
  kind | ClosedEnum ["absolute-path", "relative-artifact"],
  path | std.contract.from_predicate SafePath,
  identity | Blake3,
} in
let ArtifactReference = std.contract.custom (fun label value =>
  std.contract.check ArtifactReferenceShape label value
  |> match {
    'Ok checked =>
      let absolute = std.string.is_match "^/" checked.path in
      if (checked.kind == "absolute-path" && absolute)
        || (checked.kind == "relative-artifact" && !absolute)
      then 'Ok checked
      else 'Error {},
    'Error error => 'Error error,
  }
) in
let NullableArtifactReference = std.contract.custom (fun label value =>
  if value == null then 'Ok value
  else std.contract.check ArtifactReference label value
) in
let UniqueBy = fun select values =>
  std.array.all (fun item =>
    std.array.length (std.array.filter (fun candidate =>
      select candidate == select item
    ) values) == 1
  ) values
in
{
  exact_value = ExactValue,
  exact_string = ExactString,
  closed_enum = ClosedEnum,
  integer_between = IntegerBetween,
  u32 = U32,
  u64 = U64,
  positive_u32 = PositiveU32,
  positive_u64 = PositiveU64,
  bounded_positive = BoundedPositive,
  identifier = Identifier,
  non_empty_string = NonEmptyString,
  nullable_non_empty_string = NullableNonEmptyString,
  nullable_u32 = NullableU32,
  blake3 = Blake3,
  sha256 = Sha256,
  artifact_reference = ArtifactReference,
  nullable_artifact_reference = NullableArtifactReference,
  unique_by = UniqueBy,
}
