//! Local Git/Cargo transport controls. Run under an outer process-group timeout.
//! The fixture creates no public listener and never accesses a real Radicle store.
use std::{
    env, fs,
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

const NAMESPACE: &str = "fixture";
const SERVER_READY_LIMIT: Duration = Duration::from_secs(5);
const SERVER_POLL_DELAY: Duration = Duration::from_millis(20);
const GIT_SHA1_HEX_LENGTH: usize = 40;

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn git(root: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .expect("start Git fixture command")
}
fn checked_git(root: &Path, args: &[&str]) -> String {
    let output = git(root, args);
    assert!(
        output.status.success(),
        "Git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("Git ASCII output")
        .trim()
        .to_owned()
}
fn consumer(root: &Path, name: &str, url: &str, selector: &str) -> PathBuf {
    let path = root.join(name);
    fs::create_dir_all(path.join("src")).unwrap();
    fs::create_dir(path.join(".cargo")).unwrap();
    fs::write(
        path.join(".cargo/config.toml"),
        format!("[net]\ngit-fetch-exact-revisions = [\"{url}\"]\n"),
    )
    .unwrap();
    fs::write(path.join("Cargo.toml"), format!(
        "[package]\nname = \"consumer\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\nfixture = {{ git = \"{url}\"{selector} }}\n"
    )).unwrap();
    fs::write(
        path.join("src/lib.rs"),
        "#[test] fn exact_payload() { assert!(fixture::VALUE); }\n",
    )
    .unwrap();
    path
}
fn cargo(cargo: &Path, root: &Path, home: &Path, locked: bool) -> Output {
    assert!(
        !home.exists(),
        "every acquisition requires an empty Cargo home"
    );
    fs::create_dir(home).unwrap();
    let mut command = Command::new(cargo);
    command
        .current_dir(root)
        .args(["test", "--quiet"])
        .env("CARGO_HOME", home)
        .env("CARGO_TARGET_DIR", root.join("target"))
        .env("CARGO_NET_GIT_FETCH_WITH_CLI", "true")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER");
    if locked {
        command.arg("--locked");
    }
    command.output().expect("start Cargo fixture command")
}
fn accepted(output: Output) {
    assert!(
        output.status.success(),
        "Cargo failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn rejected(output: Output, diagnostic: &str) {
    assert!(
        !output.status.success(),
        "Cargo accepted a negative fixture"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(diagnostic),
        "wrong rejection: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn main() {
    let mut args = env::args_os().skip(1);
    let cargo_path = PathBuf::from(args.next().expect("Cargo executable"));
    let mode = args.next().expect("baseline or candidate");
    assert!(mode == "baseline" || mode == "candidate");
    assert!(args.next().is_none());
    let root = env::temp_dir().join(format!("cargo-revision-fixture-{}", std::process::id()));
    fs::create_dir(&root).expect("create a fresh owned fixture root");
    let repository = root.join("source");
    fs::create_dir(&repository).unwrap();
    checked_git(&repository, &["init", "--initial-branch=main"]);
    checked_git(&repository, &["config", "user.name", "Fixture"]);
    checked_git(
        &repository,
        &["config", "user.email", "fixture@example.invalid"],
    );
    checked_git(&repository, &["config", "commit.gpgsign", "false"]);
    fs::create_dir(repository.join("src")).unwrap();
    fs::write(
        repository.join("Cargo.toml"),
        "[package]\nname=\"fixture\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
    )
    .unwrap();
    fs::write(
        repository.join("src/lib.rs"),
        "pub const VALUE: bool = false;\n",
    )
    .unwrap();
    checked_git(&repository, &["add", "."]);
    checked_git(
        &repository,
        &["commit", "-m", "root payload must not satisfy the consumer"],
    );
    let original = checked_git(&repository, &["rev-parse", "HEAD"]);
    fs::write(
        repository.join("src/lib.rs"),
        "pub const VALUE: bool = true;\n",
    )
    .unwrap();
    checked_git(&repository, &["commit", "-am", "exact revision payload"]);
    let revision = checked_git(&repository, &["rev-parse", "HEAD"]);
    checked_git(
        &repository,
        &[
            "update-ref",
            &format!("refs/namespaces/{NAMESPACE}/refs/heads/candidate"),
            &revision,
        ],
    );
    checked_git(&repository, &["update-ref", "refs/heads/main", &original]);

    let reservation = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = reservation.local_addr().unwrap();
    drop(reservation);
    let mut server = Server(
        Command::new("git")
            .args(["daemon", "--export-all", "--listen=127.0.0.1"])
            .arg(format!("--port={}", address.port()))
            .arg(format!("--base-path={}", root.display()))
            .env("GIT_NAMESPACE", NAMESPACE)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let started = Instant::now();
    while TcpStream::connect(address).is_err() {
        assert!(
            server.0.try_wait().unwrap().is_none(),
            "fixture Git daemon stopped"
        );
        assert!(
            started.elapsed() < SERVER_READY_LIMIT,
            "fixture Git daemon readiness timeout"
        );
        thread::sleep(SERVER_POLL_DELAY);
    }
    let url = format!("git://{address}/source/.git");
    let explicit = consumer(&root, "explicit", &url, &format!(", rev = \"{revision}\""));
    let result = cargo(&cargo_path, &explicit, &root.join("explicit-home"), false);
    if mode == "baseline" {
        rejected(result, "couldn't find remote ref HEAD");
        println!("baseline_exact_revision=missing-head-reproduced");
    } else {
        accepted(result);
        accepted(cargo(
            &cargo_path,
            &explicit,
            &root.join("locked-home"),
            true,
        ));
        let lock = fs::read_to_string(explicit.join("Cargo.lock")).unwrap();
        assert!(lock.contains(&format!("?rev={revision}#{revision}")));
        println!("candidate_exact_revision=accepted; fresh_locked=accepted; payload=exact");
        let unadmitted = consumer(
            &root,
            "unadmitted",
            &url,
            &format!(", rev = \"{revision}\""),
        );
        fs::write(
            unadmitted.join(".cargo/config.toml"),
            "[net]\ngit-fetch-exact-revisions = [\"git://foreign.invalid/source\"]\n",
        )
        .unwrap();
        rejected(
            cargo(
                &cargo_path,
                &unadmitted,
                &root.join("unadmitted-home"),
                false,
            ),
            "HEAD",
        );
        let malformed = consumer(&root, "malformed", &url, &format!(", rev = \"{revision}\""));
        fs::write(
            malformed.join(".cargo/config.toml"),
            "[net]\ngit-fetch-exact-revisions = true\n",
        )
        .unwrap();
        let malformed_result = cargo(&cargo_path, &malformed, &root.join("malformed-home"), false);
        rejected(malformed_result, "git-fetch-exact-revisions");
        println!("unadmitted_remote=unchanged; malformed_admission=rejected");
    }
    let branch = consumer(&root, "branch", &url, ", branch = \"candidate\"");
    accepted(cargo(
        &cargo_path,
        &branch,
        &root.join("branch-home"),
        false,
    ));
    let default = consumer(&root, "default", &url, "");
    rejected(
        cargo(&cargo_path, &default, &root.join("default-home"), false),
        "HEAD",
    );
    let missing = "0".repeat(GIT_SHA1_HEX_LENGTH);
    let invalid = consumer(&root, "invalid", &url, &format!(", rev = \"{missing}\""));
    let result = cargo(&cargo_path, &invalid, &root.join("invalid-home"), false);
    assert!(
        !result.status.success(),
        "unknown revision must not fall back to another commit"
    );
    println!("named_branch=accepted; missing_default_head=rejected; unknown_revision=rejected");
    assert_eq!(
        checked_git(&repository, &["rev-parse", "refs/heads/main"]),
        original
    );
    assert!(
        !git(
            &repository,
            &[
                "show-ref",
                "--verify",
                &format!("refs/namespaces/{NAMESPACE}/HEAD")
            ]
        )
        .status
        .success()
    );
    drop(server);
    fs::remove_dir_all(root).expect("remove the owned fixture root");
}
