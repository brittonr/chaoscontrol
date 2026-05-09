use chaoscontrol_evidence::check_sdk_local_report_tracks;

fn main() {
    let mut args = std::env::args().skip(1);
    if let Some(arg) = args.next() {
        if arg == "-h" || arg == "--help" {
            println!("usage: check-sdk-local-report-tracks");
            return;
        }
        eprintln!("unexpected argument: {arg}\nusage: check-sdk-local-report-tracks");
        std::process::exit(2);
    }
    match check_sdk_local_report_tracks() {
        Ok(line) => println!("{line}"),
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}
