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

#[derive(Debug)]
pub enum RuntimeError {
    Spawn(std::io::Error),
    Signal(std::io::Error),
    Wait(std::io::Error),
    UnknownProcess,
    Core(SupervisorError),
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
        let identities = self.state.processes.keys().cloned().collect::<Vec<_>>();
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
