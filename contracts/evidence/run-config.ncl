# Nickel-authored run configuration contract.
let PositiveInteger = std.contract.from_predicate (fun value =>
  std.is_number value && std.number.is_integer value && value > 0
) in
let NonEmptyString = std.contract.from_predicate (fun value =>
  std.is_string value && std.string.length value > 0
) in
{
  schema_version | NonEmptyString,
  profile | NonEmptyString,
  mode | NonEmptyString,
  num_vms | PositiveInteger,
  kernel_path | NonEmptyString,
  initrd_path | NonEmptyString,
  seed | Number,
  branch_factor | PositiveInteger,
  ticks_per_branch | PositiveInteger,
  max_rounds | PositiveInteger,
  max_frontier | PositiveInteger,
  quantum | PositiveInteger,
  coverage_gpa | Number,
  bootstrap_budget | PositiveInteger,
  raw_log_policy | NonEmptyString,
}
