use std::path::PathBuf;

fn main() {
    let mut arguments = std::env::args().skip(1);
    let Some(kind) = arguments.next() else {
        fail("usage: check-profile-admission <run|campaign|simulator|schedule> <path>");
    };
    let Some(path) = arguments.next().map(PathBuf::from) else {
        fail("profile path is required");
    };
    if arguments.next().is_some() {
        fail("unexpected profile admission argument");
    }
    let result = match kind.as_str() {
        "run" => chaoscontrol_explore::profile::load_run_profile(&path)
            .map_err(|error| error.to_string())
            .and_then(|profile| {
                let seed = profile.seed;
                profile.try_into_explorer_config(seed, None).map(|_| ())
            }),
        "campaign" => chaoscontrol_explore::profile::load_campaign_profile(&path)
            .map_err(|error| error.to_string())
            .and_then(|profile| profile.try_into_campaign_config(None).map(|_| ())),
        "schedule" => chaoscontrol_explore::profile::load_fault_schedule_profile(&path)
            .map_err(|error| error.to_string())
            .and_then(|profile| profile.try_into_schedule().map(|_| ())),
        "simulator" => chaoscontrol_evidence::simulator_profile::load_simulator_profile(&path)
            .map_err(|error| error.message().to_string())
            .and_then(|profile| {
                profile
                    .try_into_config()
                    .map(|_| ())
                    .map_err(|error| error.message().to_string())
            }),
        _ => Err(format!("unknown profile kind: {kind}")),
    };
    if let Err(error) = result {
        fail(&format!("profile admission failed: {error}"));
    }
    println!("profile admission ok: {kind}");
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1)
}
