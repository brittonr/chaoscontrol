use std::path::{Path, PathBuf};

fn main() {
    let mut arguments = std::env::args().skip(1);
    let Some(kind) = arguments.next() else {
        fail("usage: check-profile-admission <run|campaign|simulator|schedule> <projection> <receipt>");
    };
    let Some(projection) = arguments.next().map(PathBuf::from) else {
        fail("profile projection path is required");
    };
    let Some(receipt) = arguments.next().map(PathBuf::from) else {
        fail("profile receipt path is required");
    };
    if arguments.next().is_some() {
        fail("unexpected profile admission argument");
    }
    let profile_id = match kind.as_str() {
        "run" => "vm-run",
        "campaign" => "campaign",
        "simulator" => "in-process-simulator",
        "schedule" => "finite-fault-schedule",
        _ => fail(&format!("unknown profile kind: {kind}")),
    };
    let projection_json = chaoscontrol_evidence::profile_projection::verify_profile_projection(
        Path::new("."),
        &projection,
        &receipt,
        profile_id,
    )
    .unwrap_or_else(|error| fail(&format!("profile linkage failed: {}", error.message())));
    let result = admit_profile(&kind, &projection_json);
    if let Err(error) = result {
        fail(&format!("profile admission failed: {error}"));
    }
    println!("profile admission ok: {kind}");
}

fn admit_profile(kind: &str, input: &str) -> Result<(), String> {
    match kind {
        "run" => {
            let profile: chaoscontrol_explore::profile::RunProfile =
                serde_json::from_str(input).map_err(|error| error.to_string())?;
            let seed = profile.seed;
            profile.try_into_explorer_config(seed, None).map(|_| ())
        }
        "campaign" => {
            let profile: chaoscontrol_explore::profile::CampaignProfile =
                serde_json::from_str(input).map_err(|error| error.to_string())?;
            profile.try_into_campaign_config(None).map(|_| ())
        }
        "schedule" => {
            let profile: chaoscontrol_explore::profile::FaultScheduleProfile =
                serde_json::from_str(input).map_err(|error| error.to_string())?;
            profile.try_into_schedule().map(|_| ())
        }
        "simulator" => {
            let profile: chaoscontrol_evidence::simulator_profile::SimulatorProfile =
                serde_json::from_str(input).map_err(|error| error.to_string())?;
            profile
                .try_into_config()
                .map(|_| ())
                .map_err(|error| error.message().to_string())
        }
        _ => Err(format!("unknown profile kind: {kind}")),
    }
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1)
}
