use chaoscontrol_protocol::guest_process::{
    AdmittedManifest, ProcessLifecycleEvent, ProcessSpec, SupervisorEffect, SupervisorEffectKind,
    SupervisorError, SupervisorState,
};
use chaoscontrol_protocol::process::ProcessFaultCommand;
use std::collections::BTreeMap;
use std::process::{Child, Command};

pub const PROCESS_ID_ENV: &str = "CHAOSCONTROL_PROCESS_ID";
pub const PROCESS_ROLE_ENV: &str = "CHAOSCONTROL_PROCESS_ROLE";
pub const PROCESS_TRANSPORT_SLOT_ENV: &str = "CHAOSCONTROL_PROCESS_TRANSPORT_SLOT";
pub const PROCESS_TRANSPORT_LOCK_ENV: &str = "CHAOSCONTROL_SDK_TRANSPORT_LOCK";
pub const DEFAULT_PROCESS_TRANSPORT_LOCK: &str = "/run/chaoscontrol-sdk.transport.lock";

pub use chaoscontrol_protocol::guest_process::MountedSharedStorage;

/// Mount one tmpfs per admitted Memory directory, once, before children start.
/// Never run mount effects on an ordinary host process. Block storage requires
/// an explicit reviewed guest-device mapping, which this manifest does not carry.
pub fn prepare_guest_shared_storage(
    manifest: &AdmittedManifest,
) -> Result<Vec<MountedSharedStorage>, RuntimeError> {
    use chaoscontrol_protocol::guest_process::{shared_directory_identity, SharedDeviceKind};
    if manifest.shared_directories.is_empty() {
        return Ok(Vec::new());
    }
    if std::process::id() != 1 {
        return Err(RuntimeError::GuestInitRequired);
    }
    for (index, directory) in manifest.shared_directories.iter().enumerate() {
        if directory.device == SharedDeviceKind::Block {
            return Err(RuntimeError::UnsupportedSharedDevice(directory.id.clone()));
        }
        let path = std::path::Path::new(&directory.path);
        if directory.path == "/"
            || ["/dev", "/proc", "/sys", "/run"]
                .iter()
                .any(|reserved| path.starts_with(reserved))
        {
            return Err(RuntimeError::Storage(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "shared mount overlaps a guest control filesystem",
            )));
        }
        if manifest.shared_directories[..index].iter().any(|prior| {
            path.starts_with(&prior.path) || std::path::Path::new(&prior.path).starts_with(path)
        }) || manifest
            .processes
            .iter()
            .any(|process| std::path::Path::new(&process.spec.executable).starts_with(path))
        {
            return Err(RuntimeError::Storage(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "shared mount overlaps another admitted root",
            )));
        }
        if directory
            .path
            .split('/')
            .skip(1)
            .any(|component| component.is_empty() || matches!(component, "." | ".."))
        {
            return Err(RuntimeError::Storage(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "shared mount path is not canonical",
            )));
        }
    }
    let mut observed = Vec::with_capacity(manifest.shared_directories.len());
    for directory in &manifest.shared_directories {
        std::fs::create_dir_all(&directory.path).map_err(RuntimeError::Storage)?;
        if std::fs::canonicalize(&directory.path).map_err(RuntimeError::Storage)?
            != std::path::Path::new(&directory.path)
        {
            return Err(RuntimeError::Storage(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "shared mount path aliases another directory",
            )));
        }
        let target = std::ffi::CString::new(directory.path.as_bytes()).map_err(|error| {
            RuntimeError::Storage(std::io::Error::new(std::io::ErrorKind::InvalidInput, error))
        })?;
        let source = std::ffi::CString::new(format!("chaoscontrol-shared-{}", directory.id))
            .map_err(|error| {
                RuntimeError::Storage(std::io::Error::new(std::io::ErrorKind::InvalidInput, error))
            })?;
        let result = unsafe {
            libc::mount(
                source.as_ptr(),
                target.as_ptr(),
                c"tmpfs".as_ptr(),
                libc::MS_NODEV | libc::MS_NOSUID,
                std::ptr::null(),
            )
        };
        if result != 0 {
            return Err(RuntimeError::Storage(std::io::Error::last_os_error()));
        }
        let mut filesystem = std::mem::MaybeUninit::<libc::statfs>::uninit();
        if unsafe { libc::statfs(target.as_ptr(), filesystem.as_mut_ptr()) } != 0 {
            return Err(RuntimeError::Storage(std::io::Error::last_os_error()));
        }
        let filesystem = unsafe { filesystem.assume_init() };
        // musl exposes an unsigned word; GNU libc already uses the record's i64.
        #[cfg(target_env = "musl")]
        let filesystem_type = filesystem.f_type as i64;
        #[cfg(not(target_env = "musl"))]
        let filesystem_type = filesystem.f_type;
        if filesystem_type != libc::TMPFS_MAGIC {
            return Err(RuntimeError::Storage(std::io::Error::other(
                "shared Memory mount is not tmpfs",
            )));
        }
        observed.push(MountedSharedStorage {
            directory_id: directory.id.clone(),
            directory_identity: shared_directory_identity(directory),
            filesystem_type,
        });
    }
    Ok(observed)
}

#[derive(Debug)]
pub enum RuntimeError {
    Spawn(std::io::Error),
    Signal(std::io::Error),
    Wait(std::io::Error),
    UnknownProcess,
    Core(SupervisorError),
    Storage(std::io::Error),
    GuestInitRequired,
    UnsupportedSharedDevice(String),
}

pub trait ProcessRuntime {
    fn spawn(
        &mut self,
        identity: &str,
        spec: &ProcessSpec,
        manifest: &AdmittedManifest,
    ) -> Result<(), RuntimeError>;
    fn signal(&mut self, identity: &str, signal: SupervisorEffectKind) -> Result<(), RuntimeError>;
    fn exited(&mut self, identity: &str) -> Result<Option<bool>, RuntimeError>;
}

// r[impl chaoscontrol.guest_processes.supervisor]
pub struct Supervisor<R> {
    manifest: AdmittedManifest,
    state: SupervisorState,
    runtime: R,
    events: Vec<ProcessLifecycleEvent>,
}

impl<R: ProcessRuntime> Supervisor<R> {
    pub fn new(manifest: AdmittedManifest, runtime: R) -> Self {
        Self {
            state: SupervisorState::new(&manifest),
            manifest,
            runtime,
            events: Vec::new(),
        }
    }

    pub fn state(&self) -> &SupervisorState {
        &self.state
    }

    pub fn start(&mut self) -> Result<(), RuntimeError> {
        let transition = self.state.start_all().map_err(RuntimeError::Core)?;
        self.execute(&transition.effects)?;
        self.state = transition.next;
        self.events.extend(transition.events);
        Ok(())
    }

    pub fn apply_fault(&mut self, command: &ProcessFaultCommand) -> Result<(), RuntimeError> {
        let transition = self
            .state
            .apply_fault(command)
            .map_err(RuntimeError::Core)?;
        self.execute(&transition.effects)?;
        self.state = transition.next;
        self.events.extend(transition.events);
        Ok(())
    }

    pub fn monitor(&mut self) -> Result<(), RuntimeError> {
        let identities = self
            .state
            .processes
            .iter()
            .filter(|(_, process)| {
                matches!(
                    process.status,
                    chaoscontrol_protocol::guest_process::ProcessStatus::Running
                        | chaoscontrol_protocol::guest_process::ProcessStatus::Paused
                )
            })
            .map(|(identity, _)| identity.clone())
            .collect::<Vec<_>>();
        for identity in identities {
            let Some(success) = self.runtime.exited(&identity)? else {
                continue;
            };
            let transition = self
                .state
                .observe_exit(&self.manifest, &identity, success)
                .map_err(RuntimeError::Core)?;
            self.execute(&transition.effects)?;
            self.state = transition.next;
            self.events.extend(transition.events);
        }
        Ok(())
    }

    pub fn advance_tick(&mut self) -> Result<(), RuntimeError> {
        let transition = self.state.advance_tick().map_err(RuntimeError::Core)?;
        self.execute(&transition.effects)?;
        self.state = transition.next;
        self.events.extend(transition.events);
        Ok(())
    }

    pub fn drain_events(&mut self) -> Vec<ProcessLifecycleEvent> {
        std::mem::take(&mut self.events)
    }

    fn execute(&mut self, effects: &[SupervisorEffect]) -> Result<(), RuntimeError> {
        for effect in effects {
            if effect.kind == SupervisorEffectKind::Spawn {
                let process = self
                    .manifest
                    .processes
                    .iter()
                    .find(|process| process.identity == effect.process_identity)
                    .ok_or(RuntimeError::UnknownProcess)?;
                self.runtime
                    .spawn(&process.identity, &process.spec, &self.manifest)?;
            } else {
                self.runtime.signal(&effect.process_identity, effect.kind)?;
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct StdProcessRuntime {
    children: BTreeMap<String, Child>,
}

impl ProcessRuntime for StdProcessRuntime {
    fn spawn(
        &mut self,
        identity: &str,
        spec: &ProcessSpec,
        manifest: &AdmittedManifest,
    ) -> Result<(), RuntimeError> {
        for directory in &manifest.shared_directories {
            std::fs::create_dir_all(&directory.path).map_err(RuntimeError::Spawn)?;
        }
        let mut command = Command::new(&spec.executable);
        command.args(&spec.arguments);
        command.envs(&spec.environment);
        command.env(PROCESS_ID_ENV, identity);
        command.env(PROCESS_ROLE_ENV, &spec.role);
        if let Some(slot) = spec.transport_slot {
            command.env(PROCESS_TRANSPORT_SLOT_ENV, slot.to_string());
            command.env(PROCESS_TRANSPORT_LOCK_ENV, DEFAULT_PROCESS_TRANSPORT_LOCK);
        }
        if let Some(directory_id) = spec.shared_directories.first() {
            let directory = manifest
                .shared_directories
                .iter()
                .find(|directory| &directory.id == directory_id)
                .ok_or(RuntimeError::UnknownProcess)?;
            command.current_dir(&directory.path);
        }
        let child = command.spawn().map_err(RuntimeError::Spawn)?;
        if self.children.insert(identity.to_string(), child).is_some() {
            return Err(RuntimeError::UnknownProcess);
        }
        Ok(())
    }

    fn signal(&mut self, identity: &str, signal: SupervisorEffectKind) -> Result<(), RuntimeError> {
        let child = self
            .children
            .get_mut(identity)
            .ok_or(RuntimeError::UnknownProcess)?;
        match signal {
            SupervisorEffectKind::Kill => {
                child.kill().map_err(RuntimeError::Signal)?;
                child.wait().map_err(RuntimeError::Wait)?;
                self.children.remove(identity);
            }
            SupervisorEffectKind::Pause | SupervisorEffectKind::Resume => {
                let signal_number = if signal == SupervisorEffectKind::Pause {
                    libc::SIGSTOP
                } else {
                    libc::SIGCONT
                };
                let pid = i32::try_from(child.id()).map_err(|_| RuntimeError::UnknownProcess)?;
                let result = unsafe { libc::kill(pid, signal_number) };
                if result != 0 {
                    return Err(RuntimeError::Signal(std::io::Error::last_os_error()));
                }
            }
            SupervisorEffectKind::Spawn => return Err(RuntimeError::UnknownProcess),
        }
        Ok(())
    }

    fn exited(&mut self, identity: &str) -> Result<Option<bool>, RuntimeError> {
        let child = self
            .children
            .get_mut(identity)
            .ok_or(RuntimeError::UnknownProcess)?;
        let Some(status) = child.try_wait().map_err(RuntimeError::Wait)? else {
            return Ok(None);
        };
        self.children.remove(identity);
        Ok(Some(status.success()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chaoscontrol_protocol::guest_process::{
        admit_manifest, ProcessManifest, ProcessStatus, RestartMode, RestartPolicy,
        SharedDeviceKind, SharedDirectorySpec, PROCESS_MANIFEST_SCHEMA,
    };
    use chaoscontrol_protocol::process::ProcessFaultAction;
    use std::collections::BTreeSet;

    const RESTART_LIMIT: u32 = 2;

    #[derive(Default)]
    struct FakeRuntime {
        running: BTreeSet<String>,
        failed: BTreeSet<String>,
    }

    impl ProcessRuntime for FakeRuntime {
        fn spawn(
            &mut self,
            identity: &str,
            _spec: &ProcessSpec,
            _manifest: &AdmittedManifest,
        ) -> Result<(), RuntimeError> {
            self.running.insert(identity.to_string());
            Ok(())
        }

        fn signal(
            &mut self,
            identity: &str,
            signal: SupervisorEffectKind,
        ) -> Result<(), RuntimeError> {
            if signal == SupervisorEffectKind::Kill {
                self.running.remove(identity);
            }
            Ok(())
        }

        fn exited(&mut self, identity: &str) -> Result<Option<bool>, RuntimeError> {
            if self.failed.remove(identity) {
                self.running.remove(identity);
                Ok(Some(false))
            } else {
                Ok(None)
            }
        }
    }

    fn admitted() -> AdmittedManifest {
        admit_manifest(&ProcessManifest {
            schema: PROCESS_MANIFEST_SCHEMA.to_string(),
            guest: "fixture".to_string(),
            shared_directories: vec![SharedDirectorySpec {
                id: "data".to_string(),
                path: "/tmp/chaoscontrol-supervisor-fixture".to_string(),
                device: SharedDeviceKind::Memory,
            }],
            processes: vec![ProcessSpec {
                role: "writer".to_string(),
                executable: "/bin/true".to_string(),
                arguments: Vec::new(),
                environment: BTreeMap::new(),
                shared_directories: vec!["data".to_string()],
                restart: RestartPolicy {
                    mode: RestartMode::OnFailure,
                    max_restarts: RESTART_LIMIT,
                },
                instrumented: true,
                transport_slot: Some(0),
            }],
        })
        .unwrap()
    }

    #[test]
    // r[verify chaoscontrol.guest_processes.supervisor]
    fn shell_executes_spawn_fault_and_restart_effects() {
        let manifest = admitted();
        let identity = manifest.processes[0].identity.clone();
        let mut supervisor = Supervisor::new(manifest, FakeRuntime::default());
        supervisor.start().unwrap();
        assert_eq!(
            supervisor.state().processes[&identity].status,
            ProcessStatus::Running
        );
        let command =
            ProcessFaultCommand::new("request-1", "writer", ProcessFaultAction::Restart, None)
                .unwrap();
        supervisor.apply_fault(&command).unwrap();
        assert_eq!(
            supervisor.state().processes[&identity].status,
            ProcessStatus::Running
        );
        assert!(!supervisor.drain_events().is_empty());
    }
}
