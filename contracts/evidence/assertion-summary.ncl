# Closed review contract for Rust-owned assertion summary evidence.
let MaxNamespaceBytes = 128 in
let MaxKeyBytes = 256 in
let MaxMessageBytes = 1024 in
let MaxSourceBytes = 512 in
let MaxGuestBytes = 128 in
let MaxCategoryBytes = 64 in
let MaxFailureDetailsBytes = 2048 in
let MaxCanonicalHexBytes = 4608 in
let MaxCatalogDescriptors = 4096 in
let U32Maximum = 4294967295 in
let U64Maximum = 18446744073709551615 in
let ExactString = fun expected => std.contract.from_predicate (fun value =>
  std.is_string value && value == expected
) in
let Utf8ByteLength = fun value =>
  let encoded = std.string.base64_encode 'NoPad value in
  std.number.floor ((std.string.length encoded * 3) / 4)
in
let HasNoAsciiControls = fun value =>
  !(std.string.is_match "[[:cntrl:]]" value)
in
let BoundedString = fun maximum => std.contract.from_predicate (fun value =>
  std.is_string value
  && Utf8ByteLength value > 0
  && Utf8ByteLength value <= maximum
  && HasNoAsciiControls value
) in
let U32 = std.contract.from_predicate (fun value =>
  std.is_number value
  && std.number.is_integer value
  && value >= 0
  && value <= U32Maximum
) in
let NullableU32 = std.contract.from_predicate (fun value =>
  value == null
  || (
    std.is_number value
    && std.number.is_integer value
    && value >= 0
    && value <= U32Maximum
  )
) in
let PositiveU32 = std.contract.from_predicate (fun value =>
  std.is_number value
  && std.number.is_integer value
  && value > 0
  && value <= U32Maximum
) in
let U64 = std.contract.from_predicate (fun value =>
  std.is_number value
  && std.number.is_integer value
  && value >= 0
  && value <= U64Maximum
) in
let FingerprintValue = fun value =>
  std.is_string value && std.string.is_match "^[0-9a-f]{64}$" value
in
let Fingerprint = std.contract.from_predicate FingerprintValue in
let CanonicalHex = std.contract.from_predicate (fun value =>
  std.is_string value
  && Utf8ByteLength value > 0
  && Utf8ByteLength value <= MaxCanonicalHexBytes
  && std.string.is_match "^([0-9a-f]{2})+$" value
) in
let FailureDetails = std.contract.from_predicate (fun value =>
  value == null
  || (std.is_string value && Utf8ByteLength value <= MaxFailureDetailsBytes)
) in
let Category = std.contract.from_predicate (fun value =>
  std.is_string value
  && Utf8ByteLength value > 0
  && Utf8ByteLength value <= MaxCategoryBytes
  && HasNoAsciiControls value
  && std.string.is_match "^[a-z0-9]([a-z0-9-]*[a-z0-9])?$" value
) in
let AssertionKind = std.contract.from_predicate (fun value =>
  value == "always"
  || value == "sometimes"
  || value == "reachable"
  || value == "unreachable"
) in
let Verdict = std.contract.from_predicate (fun value =>
  value == "passed" || value == "failed" || value == "unexercised"
) in
let OneCatalogToken = std.contract.from_predicate (fun value =>
  std.is_array value
  && std.array.length value == 1
  && FingerprintValue (std.array.at 0 value)
) in
let BoundedAssertionArray = std.contract.from_predicate (fun value =>
  std.is_array value
  && std.array.length value > 0
  && std.array.length value <= MaxCatalogDescriptors
) in
let LogicalKeyTag = std.contract.from_predicate (fun value =>
  value == "automatic" || value == "stable" || value == "legacy_u32"
) in
let LogicalKeyShape = {
  type | LogicalKeyTag,
  source_site | BoundedString MaxKeyBytes | optional,
  key | BoundedString MaxKeyBytes | optional,
  id | U32 | optional,
} in
let LogicalKeyClassification = fun value =>
  let has_source = std.record.has_field "source_site" value in
  let has_key = std.record.has_field "key" value in
  let has_id = std.record.has_field "id" value in
  if value.type == "automatic" then has_source && !has_key && !has_id
  else if value.type == "stable" then !has_source && has_key && !has_id
  else !has_source && !has_key && has_id
in
let LogicalKey = std.contract.custom (fun label value =>
  std.contract.check LogicalKeyShape label value
  |> match {
    'Ok checked => if LogicalKeyClassification checked then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
) in
let DescriptorShape = {
  identity_version | std.contract.from_predicate (fun value => value == 1),
  namespace | BoundedString MaxNamespaceBytes,
  logical_key | LogicalKey,
  compatibility_id | NullableU32 | optional,
  kind | AssertionKind,
  message | BoundedString MaxMessageBytes,
  source_file | BoundedString MaxSourceBytes,
  source_line | PositiveU32,
  source_column | PositiveU32,
  guest | BoundedString MaxGuestBytes,
  category | Category,
} in
let SourcePathSemantics = fun value =>
  !(std.string.is_match "^/" value)
  && !(std.string.is_match "\\\\" value)
  && std.array.all (fun component =>
    component != "" && component != "." && component != ".."
  ) (std.string.split "/" value)
in
let DescriptorSemantics = fun value =>
  let logical_key_valid =
    if value.logical_key.type == "automatic" then
      value.logical_key.source_site == "%{value.source_file}:%{value.source_line}:%{value.source_column}"
    else if value.logical_key.type == "legacy_u32" then
      std.record.has_field "compatibility_id" value
      && value.compatibility_id != null
      && value.logical_key.id == value.compatibility_id
    else
      true
  in
  logical_key_valid && SourcePathSemantics value.source_file
in
let Descriptor = std.contract.custom (fun label value =>
  std.contract.check DescriptorShape label value
  |> match {
    'Ok checked => if DescriptorSemantics checked then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
) in
let Identity = {
  descriptor | Descriptor,
  fingerprint | Fingerprint,
  canonical_descriptor | CanonicalHex,
  catalog_tokens | OneCatalogToken,
} in
let AssertionSemantics = std.contract.from_predicate (fun value =>
  let hits = value.true_count + value.false_count in
  hits == value.hit_count
  && (
    if value.kind == "always" then
      if hits == 0 then value.verdict == "unexercised"
      else if value.false_count > 0 then value.verdict == "failed"
      else value.verdict == "passed"
    else if value.kind == "sometimes" then
      if hits == 0 then value.verdict == "unexercised"
      else if value.true_count > 0 then value.verdict == "passed"
      else value.verdict == "failed"
    else if value.kind == "reachable" then
      value.false_count == 0
      && (if value.true_count > 0 then value.verdict == "passed" else value.verdict == "unexercised")
    else
      value.true_count == 0
      && (if value.false_count > 0 then value.verdict == "failed" else value.verdict == "passed")
  )
) in
let AssertionRecord = {
  id | U32,
  identity | Identity | optional,
  message | BoundedString MaxMessageBytes,
  kind | AssertionKind,
  guest | BoundedString MaxGuestBytes,
  category | Category,
  verdict | Verdict,
  hit_count | U64,
  true_count | U64,
  false_count | U64,
  last_failure_details | FailureDetails | optional,
} in
let CatalogStatus = std.contract.from_predicate (fun value =>
  value == "accepted" || value == "legacy-ambiguous" || value == "fatal-conflict"
) in
let SummaryShape = {
  schema | ExactString "chaoscontrol.assertion-summary.v2",
  catalog_status | CatalogStatus,
  collision_safe_evidence | Bool,
  assertions | Array AssertionRecord | Array AssertionSemantics | BoundedAssertionArray,
} in
let UniqueBy = fun select values =>
  std.array.all (fun item =>
    std.array.length (std.array.filter (fun candidate =>
      select candidate == select item
    ) values) == 1
  ) values
in
let StrictMetadataMatches = fun assertion =>
  let descriptor = assertion.identity.descriptor in
  let expected_id =
    if std.record.has_field "compatibility_id" descriptor
      && descriptor.compatibility_id != null
    then descriptor.compatibility_id
    else 0
  in
  assertion.id == expected_id
  && assertion.message == descriptor.message
  && assertion.kind == descriptor.kind
  && assertion.guest == descriptor.guest
  && assertion.category == descriptor.category
in
let SummaryClassification = fun value =>
  let all_identified = std.array.all (fun assertion =>
    std.record.has_field "identity" assertion
  ) value.assertions in
  let all_strict = std.array.all (fun assertion =>
    if std.record.has_field "identity" assertion then
      assertion.identity.descriptor.logical_key.type != "legacy_u32"
    else
      false
  ) value.assertions in
  let all_legacy = std.array.all (fun assertion =>
    !(std.record.has_field "identity" assertion)
  ) value.assertions in
  if value.catalog_status == "accepted" then
    let expected_token = std.array.at 0 (std.array.at 0 value.assertions).identity.catalog_tokens in
    value.collision_safe_evidence
    && all_identified
    && all_strict
    && std.array.all StrictMetadataMatches value.assertions
    && UniqueBy (fun assertion => assertion.identity.fingerprint) value.assertions
    && std.array.all (fun assertion =>
      std.array.at 0 assertion.identity.catalog_tokens == expected_token
    ) value.assertions
  else if value.catalog_status == "legacy-ambiguous" then
    !value.collision_safe_evidence
    && all_legacy
    && UniqueBy (fun assertion => assertion.id) value.assertions
  else
    !value.collision_safe_evidence
    && std.array.all (fun assertion =>
      if std.record.has_field "identity" assertion then
        StrictMetadataMatches assertion
      else
        true
    ) value.assertions
in
std.contract.custom (fun label value =>
  std.contract.check SummaryShape label value
  |> match {
    'Ok checked => if SummaryClassification checked then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)
