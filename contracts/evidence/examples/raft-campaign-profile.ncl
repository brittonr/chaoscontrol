let CampaignProfile = import "../campaign-profile.ncl" in
let run_profile = import "raft-run-config.ncl" in
{
  schema = "chaoscontrol.campaign-profile.v1",
  campaign_id = "raft-multi-seed-v1",
  seeds = [42, 43, 44],
  run = run_profile,
  workers = { count = 2, failure_policy = "fail-campaign" },
  mutation = {
    maximum_tick = 500,
    add_probability = 0.25,
    remove_probability = 0.25,
    shift_probability = 0.25,
    replace_probability = 0.25,
    schedule_mutation_ratio = 0.1,
    havoc_after_stale = 5,
    havoc_minimum_mutations = 4,
    havoc_maximum_mutations = 16,
  },
  scenario = null,
  metrics = {
    enabled = true,
    output = {
      kind = "relative-artifact",
      path = "metrics/rounds.jsonl",
      identity = "blake3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    },
  },
  output = {
    root = {
      kind = "relative-artifact",
      path = "campaign-results/raft-v1",
      identity = "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
    },
    seed_directory_template = "seed_{seed}",
    checkpoint_name = "campaign_progress.json",
  },
  bounds = {
    maximum_seeds = 1024,
    maximum_workers = 64,
    maximum_total_memory_mib = 384,
  },
  scope = "pre-run campaign intent; not thread start, VM start, completed seed, fault effect, replay, report, receipt, or accepted evidence",
} | CampaignProfile
