## ADDED Requirements

### Requirement: Local multi-hypervisor dashboard [r[campaign-dashboard.local-multi-hypervisor]]

The campaign dashboard MUST render a static or local-only operator view for one-machine multi-hypervisor campaigns from validated queue, worker, run, bug, and receipt artifacts.

#### Scenario: Dashboard shows local worker and queue state [r[campaign-dashboard.local-multi-hypervisor.queue-workers]]

- GIVEN a valid local multi-hypervisor control-plane receipt and queue-state snapshot
- WHEN the dashboard renderer runs
- THEN it shows campaign status, worker IDs, resource budgets when present, leased/running/completed queue entries, run receipt summaries, bug counts, and reproduce/minimize follow-up status

#### Scenario: Dashboard preserves local-only scope [r[campaign-dashboard.local-multi-hypervisor.scope]]

- GIVEN the dashboard is rendered for a local multi-hypervisor campaign
- WHEN an operator reviews it
- THEN it states that the evidence covers one-machine local hypervisor orchestration only and does not claim SaaS, remote shared queues, cross-machine scheduling, or universal fleet throughput
