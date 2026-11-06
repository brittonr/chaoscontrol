//! One bounded KVM campaign exercising directional network, CPU throttle,
//! custom effect, and pause control in a single recorded run.
//!
//! Run with:
//! `CC_KERNEL=... CC_INITRD=... CC_RECEIPT=... cargo test -p chaoscontrol-vmm \
//!  --test fault_interaction_kvm -- --ignored --nocapture`

use chaoscontrol_fault::faults::Fault;
use chaoscontrol_fault::mechanisms::{
    CustomEffectAdapter, CustomEffectContext, CustomEffectError, CustomEffectIdentity,
    NetworkDirection, NetworkDisruption, PauseControlRequest,
};
use chaoscontrol_fault::outcomes::NANOSECONDS_PER_SIMULATION_TICK;
use chaoscontrol_fault::schedule::{FaultSchedule, ScheduledFault};
use chaoscontrol_vmm::controller::{SimulationConfig, SimulationController};
use chaoscontrol_vmm::vm::VmConfig;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct CountingAdapter {
    calls: Arc<AtomicUsize>,
}

impl CustomEffectAdapter for CountingAdapter {
    fn identity(&self) -> CustomEffectIdentity {
        CustomEffectIdentity {
            effect_id: "kvm-interaction-effect".to_string(),
            version: 1,
        }
    }

    fn apply(
        &mut self,
        _effect: &chaoscontrol_fault::mechanisms::CustomFaultEffect,
        _context: CustomEffectContext,
    ) -> Result<(), CustomEffectError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

fn tick_ns(tick: u64) -> u64 {
    tick * NANOSECONDS_PER_SIMULATION_TICK
}

#[test]
#[ignore = "requires CC_KERNEL and CC_INITRD guest artifacts"]
fn kvm_campaign_records_four_mechanisms_in_one_run() {
    let kernel = std::env::var("CC_KERNEL").expect("CC_KERNEL is required for an explicit KVM run");
    let initrd = std::env::var("CC_INITRD").expect("CC_INITRD is required for an explicit KVM run");

    let config = SimulationConfig {
        num_vms: 2,
        vm_config: VmConfig::single_vcpu(42, true),
        kernel_path: kernel,
        initrd_path: Some(initrd),
        seed: 42,
        quantum: 100,
        schedule: FaultSchedule::new(),
        disk_image_path: None,
        bootstrap_budget: Some(10_000),
        base_core: None,
        dlog_dir: None,
        random_faults: false,
    };
    let mut controller = SimulationController::new(config).expect("controller");
    controller
        .init_thread_timers()
        .expect("bind watchdogs to the executing test thread");
    const BOOTSTRAP_TICKS: u64 = 10_000;
    controller
        .run_until_setup_complete(BOOTSTRAP_TICKS)
        .expect("bootstrap runs");
    for vm in 0..2 {
        assert!(
            controller
                .vm_slot(vm)
                .expect("VM exists")
                .vm
                .fault_engine()
                .is_setup_complete(),
            "VM {vm} did not complete setup"
        );
    }
    let start_tick = controller.tick();
    eprintln!("campaign: all VMs completed setup at tick {start_tick}");

    let calls = Arc::new(AtomicUsize::new(0));
    controller.set_custom_effect_adapter(Box::new(CountingAdapter {
        calls: Arc::clone(&calls),
    }));

    let mut schedule = FaultSchedule::new();
    schedule.add(ScheduledFault::new(
        tick_ns(start_tick + 4),
        Fault::NetworkDirectional {
            node: 0,
            peer: 1,
            direction: NetworkDirection::Outbound,
            disruption: NetworkDisruption::Stopped,
            delay_ns: 0,
            queue_limit: 0,
        },
    ));
    schedule.add(ScheduledFault::new(
        tick_ns(start_tick + 5),
        Fault::CpuThrottle {
            target: 0,
            budget_units: 2,
            duration_ticks: 30,
        },
    ));
    schedule.add(ScheduledFault::new(
        tick_ns(start_tick + 6),
        Fault::CustomEffect {
            effect_id: "kvm-interaction-effect".to_string(),
            version: 1,
            parameters: BTreeMap::new(),
        },
    ));
    // A fourth fault is scheduled inside the later pause window; it must stay
    // unselected while the window is active.
    schedule.add(ScheduledFault::new(
        tick_ns(start_tick + 27),
        Fault::CpuThrottle {
            target: 1,
            budget_units: 1,
            duration_ticks: 10,
        },
    ));
    controller
        .begin_counterfactual_fault_run(schedule)
        .expect("bind schedule while preserving completed setup");
    controller.run(20).expect("campaign runs");
    let calls_after_apply = calls.load(Ordering::SeqCst);
    eprintln!(
        "campaign: three effects applied at tick {}",
        controller.tick()
    );

    controller
        .request_pause_control(PauseControlRequest {
            requester: "kvm-campaign".to_string(),
            start_tick: start_tick + 20,
            end_tick: start_tick + 30,
        })
        .expect("pause window admits");
    let selected_before = controller.fault_outcomes().counters.selected;
    controller.run(9).expect("pause window runs");
    let selected_after = controller.fault_outcomes().counters.selected;

    let ledger = controller.fault_outcomes();
    let mut mechanisms = BTreeMap::<String, u64>::new();
    for state in ledger.attempts.values() {
        assert!(
            matches!(
                state.stage,
                chaoscontrol_fault::outcomes::FaultAuthoritativeStage::Applied
                    | chaoscontrol_fault::outcomes::FaultAuthoritativeStage::Observed
            ),
            "campaign attempt did not apply: {:?}",
            state.stage
        );
        if let Some(effect) = state.applicable_effect.as_ref() {
            let name = format!("{:?}", effect.mechanism());
            *mechanisms.entry(name).or_default() += 1;
        }
    }
    let mechanism_receipt =
        chaoscontrol_fault::evidence::fault_mechanism_receipt(ledger, 42, "kvm-interaction-v1")
            .expect("valid mechanism evidence");
    chaoscontrol_fault::evidence::validate_fault_mechanism_receipt(
        ledger,
        42,
        "kvm-interaction-v1",
        &mechanism_receipt,
    )
    .expect("receipt agrees with ledger");
    let observations = ledger.counters.observed;
    let receipt = serde_json::json!({
        "schema": "chaoscontrol.fault-interaction-campaign.v1",
        "seed": 42,
        "vms": 2,
        "start_tick": start_tick,
        "end_tick": controller.tick(),
        "mechanism_receipt": mechanism_receipt,
        "mechanisms": mechanisms,
        "attempts": ledger.attempts.len(),
        "events": ledger.events.len(),
        "counters": ledger.counters,
        "observations": observations,
        "custom_effect_calls": calls_after_apply,
        "pause": {
            "requester": "kvm-campaign",
            "start_tick": start_tick + 20,
            "end_tick": start_tick + 30,
            "selected_before": selected_before,
            "selected_after": selected_after,
        },
        "network": {
            "packets_sent": controller.network().stats().packets_sent,
            "packets_dropped_partition": controller.network().stats().packets_dropped_partition,
            "packets_dropped_loss": controller.network().stats().packets_dropped_loss,
        },
        "non_claims": [
            "This one recorded run is not universal determinism or correctness evidence.",
            "A pause window suppresses new selection; it does not remove active effects.",
            "Custom effect execution proves adapter invocation, not effect correctness.",
        ],
    });

    assert!(mechanisms
        .keys()
        .any(|name| name.contains("NetworkDirectional")));
    assert!(mechanisms.keys().any(|name| name.contains("CpuThrottle")));
    assert!(mechanisms.keys().any(|name| name.contains("CustomEffect")));
    assert_eq!(calls_after_apply, 1);
    assert_eq!(ledger.counters.applied, 3);
    assert_eq!(ledger.counters.selected, 3);
    assert_eq!(
        mechanisms.get("CpuThrottle").copied().unwrap_or_default(),
        1,
        "the paused throttle must not be selected"
    );
    assert_eq!(
        selected_before, selected_after,
        "pause window must suppress new selection"
    );
    controller.run(3).expect("selection resumes after pause");
    assert_eq!(
        controller.fault_outcomes().counters.selected,
        selected_after + 1,
        "the deferred fault must be selected after release"
    );
    if let Ok(path) = std::env::var("CC_RECEIPT") {
        std::fs::write(&path, serde_json::to_vec_pretty(&receipt).unwrap()).expect("receipt");
    }
    println!("{}", serde_json::to_string_pretty(&receipt).unwrap());
}
