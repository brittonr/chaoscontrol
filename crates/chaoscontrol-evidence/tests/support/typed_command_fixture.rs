use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use chaoscontrol_evidence::typed_operator_command::EnvironmentEntry;

const ACTION_ENV: &str = "CHAOSCONTROL_TYPED_COMMAND_FIXTURE_ACTION";
const FIRST_ENV: &str = "CHAOSCONTROL_TYPED_COMMAND_FIXTURE_FIRST";
const SECOND_ENV: &str = "CHAOSCONTROL_TYPED_COMMAND_FIXTURE_SECOND";
const CHILD_TEST_NAME: &str = "typed_command_fixture_child";
const CHILD_ARGUMENT_COUNT: usize = 3;

pub(crate) struct FixtureSpec {
    pub(crate) executable: PathBuf,
    pub(crate) executable_blake3: String,
    pub(crate) args: Vec<String>,
    pub(crate) environment: Vec<EnvironmentEntry>,
}

pub(crate) fn fixture_spec(action: &str, first: &str, second: &str) -> FixtureSpec {
    let executable = std::env::current_exe().expect("resolve integration test executable");
    let bytes = std::fs::read(&executable).expect("read integration test executable");
    FixtureSpec {
        executable,
        executable_blake3: blake3::hash(&bytes).to_hex().to_string(),
        args: vec![
            "--exact".to_string(),
            CHILD_TEST_NAME.to_string(),
            "--nocapture".to_string(),
        ],
        environment: vec![
            EnvironmentEntry {
                name: ACTION_ENV.to_string(),
                value: action.to_string(),
            },
            EnvironmentEntry {
                name: FIRST_ENV.to_string(),
                value: first.to_string(),
            },
            EnvironmentEntry {
                name: SECOND_ENV.to_string(),
                value: second.to_string(),
            },
        ],
    }
}

pub(crate) fn run_child() {
    let Ok(action) = std::env::var(ACTION_ENV) else {
        return;
    };
    let first = std::env::var(FIRST_ENV).expect("fixture first value");
    let second = std::env::var(SECOND_ENV).expect("fixture second value");
    match action.as_str() {
        "copy" => {
            std::fs::copy(&first, &second).expect("fixture copies source to target");
        }
        "write-literal" => {
            std::fs::write(&second, first.as_bytes()).expect("fixture writes literal target");
        }
        "exit" => {
            let code = first.parse::<u8>().expect("fixture exit code");
            std::process::exit(i32::from(code));
        }
        "flood" => {
            let bytes = first.parse::<usize>().expect("fixture flood size");
            std::io::stdout()
                .write_all(&vec![b'x'; bytes])
                .expect("fixture writes flood");
        }
        "sleep" => {
            let milliseconds = first.parse::<u64>().expect("fixture sleep duration");
            std::thread::sleep(Duration::from_millis(milliseconds));
        }
        "abort" => std::process::abort(),
        other => panic!("unsupported typed command fixture action {other:?}"),
    }
}

#[test]
fn child_argument_shape_is_stable() {
    let spec = fixture_spec("copy", "source", "target");
    assert_eq!(spec.args.len(), CHILD_ARGUMENT_COUNT);
    assert_eq!(spec.args[1], CHILD_TEST_NAME);
}
