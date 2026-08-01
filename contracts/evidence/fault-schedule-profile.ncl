# Nickel-authored finite fault schedule. It describes intent, not observed effects.
let P = import "profile-primitives.ncl" in
let NullableLabel = std.contract.custom (fun label value =>
  if value == null then 'Ok value
  else std.contract.check P.non_empty_string label value
) in
let CommonTime = P.u64 in
let ProcessKill = {
  kind | P.exact_string "process-kill",
  time_ns | CommonTime,
  label | NullableLabel,
  target | P.u32,
} in
let NetworkPartition = {
  kind | P.exact_string "network-partition",
  time_ns | CommonTime,
  label | NullableLabel,
  side_a | Array P.u32,
  side_b | Array P.u32,
} in
let NetworkLatency = {
  kind | P.exact_string "network-latency",
  time_ns | CommonTime,
  label | NullableLabel,
  target | P.u32,
  latency_ns | P.positive_u64,
} in
let PacketLoss = {
  kind | P.exact_string "packet-loss",
  time_ns | CommonTime,
  label | NullableLabel,
  target | P.u32,
  rate_ppm | P.integer_between 1 1000000,
} in
let DiskWriteError = {
  kind | P.exact_string "disk-write-error",
  time_ns | CommonTime,
  label | NullableLabel,
  target | P.u32,
  offset | P.u64,
} in
let ClockSkew = {
  kind | P.exact_string "clock-skew",
  time_ns | CommonTime,
  label | NullableLabel,
  target | P.u32,
  offset_ns | P.integer_between (-86400000000000) 86400000000000,
} in
let Descriptor = std.contract.custom (fun label value =>
  if !(std.is_record value) || !(std.record.has_field "kind" value) then 'Error {}
  else if value.kind == "process-kill" then std.contract.check ProcessKill label value
  else if value.kind == "network-partition" then std.contract.check NetworkPartition label value
  else if value.kind == "network-latency" then std.contract.check NetworkLatency label value
  else if value.kind == "packet-loss" then std.contract.check PacketLoss label value
  else if value.kind == "disk-write-error" then std.contract.check DiskWriteError label value
  else if value.kind == "clock-skew" then std.contract.check ClockSkew label value
  else 'Error {}
) in
let Shape = {
  schema | P.exact_string "chaoscontrol.fault-schedule-profile.v1",
  schedule_id | P.identifier,
  source_identity | P.blake3,
  num_vms | P.bounded_positive 64,
  faults | Array Descriptor,
  scope | P.exact_string "finite pre-run fault intent; not attempted, applied, observed, or effective fault evidence",
} in
let rec StrictlyOrdered = fun index values =>
  if index + 1 >= std.array.length values then true
  else (std.array.at index values).time_ns < (std.array.at (index + 1) values).time_ns
    && StrictlyOrdered (index + 1) values
in
let PartitionValid = fun fault num_vms =>
  std.array.length fault.side_a > 0
  && std.array.length fault.side_b > 0
  && P.unique_by (fun target => target) fault.side_a
  && P.unique_by (fun target => target) fault.side_b
  && std.array.all (fun target => target < num_vms) fault.side_a
  && std.array.all (fun target => target < num_vms) fault.side_b
  && std.array.all (fun target =>
    std.array.length (std.array.filter (fun candidate => candidate == target) fault.side_b) == 0
  ) fault.side_a
in
let DescriptorValid = fun fault num_vms =>
  if fault.kind == "network-partition" then PartitionValid fault num_vms
  else fault.target < num_vms
in
let Coherent = fun value =>
  std.array.length value.faults <= 4096
  && StrictlyOrdered 0 value.faults
  && std.array.all (fun fault => DescriptorValid fault value.num_vms) value.faults
in
std.contract.custom (fun label value =>
  std.contract.check Shape label value
  |> match {
    'Ok checked => if Coherent checked then 'Ok checked else 'Error {},
    'Error error => 'Error error,
  }
)
