# Shared exact scheduler serde shapes for review boundaries.
let P = import "profile-primitives.ncl" in
let BudgetContract = std.contract.custom (fun label value =>
  if !(std.is_record value) then 'Error {}
  else if std.record.has_field "Instructions" value then
    std.contract.check { Instructions | P.positive_u64 } label value
  else
    std.contract.check {
      KvmBoundaries | {
        profile_id | P.exact_string "KvmDebugExitBoundaryV1",
        count | P.positive_u64,
      },
    } label value
) in
let IsInstructions = fun budget => std.record.has_field "Instructions" budget in
let BudgetCount = fun budget => if IsInstructions budget then budget.Instructions else budget.KvmBoundaries.count in
let BudgetsHaveSameUnit = fun left right => IsInstructions left == IsInstructions right in
let StrategyContract = std.contract.custom (fun label value =>
  if value == null || value == "RoundRobin" then 'Ok value
  else std.contract.check {
    Randomized | {
      min_budget | BudgetContract,
      max_budget | BudgetContract,
    },
  } label value
) in
{
  ProgressBudget = BudgetContract,
  Count = BudgetCount,
  SameUnit = BudgetsHaveSameUnit,
  Strategy = StrategyContract,
}
