//! Normalization and recovery for the closed KVM debug-exit boundary profile.
//!
//! A boundary is a settled guest-origin exit, never an instruction count.
use super::*;

pub(super) const UNSUPPORTED_CLOCK_MSRS: [u32; 7] = [
    0x11,
    0x12,
    0x6e0,
    0x4b56_4d00,
    0x4b56_4d01,
    0x4b56_4d03,
    0x4000_0021,
];

fn coarse_projection_runtime_admitted(
    descriptor: &crate::snapshot::SnapshotExecutionDescriptor,
    mode: ProgressMode,
    installed_filter_identity: Option<[u8; 32]>,
    healthy: bool,
) -> bool {
    healthy
        && mode == ProgressMode::KvmDebugBoundary
        && installed_filter_identity
            == Some(
                crate::snapshot::SnapshotMsrFilterProfile::KvmCoarseClockDenyV1.table_identity(),
            )
        && descriptor.permits_coarse_clock_projection()
}

impl DeterministicVm {
    pub(super) fn install_coarse_clock_msr_filter(vm: &VmFd) -> Result<[u8; 32], VmError> {
        use crate::snapshot::SnapshotMsrFilterProfile;
        use std::os::fd::AsRawFd;
        let profile = SnapshotMsrFilterProfile::KvmCoarseClockDenyV1;
        // KVM copies these bitmaps synchronously. Zero bits deny the selected
        // guest access. Host GET/SET_MSRS remains independent of this filter.
        let mut deny_bitmap = [0u8; 512];
        let mut filter = kvm_bindings::kvm_msr_filter {
            flags: kvm_bindings::KVM_MSR_FILTER_DEFAULT_ALLOW,
            ..Default::default()
        };
        for (slot, &(base, count, flags)) in filter.ranges.iter_mut().zip(profile.ranges()) {
            *slot = kvm_bindings::kvm_msr_filter_range {
                flags,
                nmsrs: count,
                base,
                bitmap: deny_bitmap.as_mut_ptr(),
            };
        }
        // Linux UAPI: _IOW(KVMIO, 0xc6, struct kvm_msr_filter). kvm-ioctls
        // 0.19 has the binding but no wrapper for KVM_X86_SET_MSR_FILTER.
        const KVM_X86_SET_MSR_FILTER: libc::c_ulong = (1 << 30)
            | ((std::mem::size_of::<kvm_bindings::kvm_msr_filter>() as libc::c_ulong) << 16)
            | (0xae << 8)
            | 0xc6;
        // SAFETY: valid VM descriptor and ABI filter; every referenced bitmap
        // is alive for this synchronous ioctl and covers the largest range.
        if unsafe { libc::ioctl(vm.as_raw_fd(), KVM_X86_SET_MSR_FILTER, &filter) } != 0 {
            return Err(VmError::ProgressCapability {
                message: format!(
                    "cannot enforce coarse guest clock MSR filter: {}",
                    io::Error::last_os_error()
                ),
            });
        }
        Ok(profile.table_identity())
    }

    pub(super) fn coarse_mode(&self) -> bool {
        self.scheduler.progress_mode() == ProgressMode::KvmDebugBoundary
    }

    /// The coarse profile does not admit NMI/nested-NMI injection semantics.
    pub(crate) fn supports_nmi_injection(&self) -> bool {
        !self.coarse_mode()
    }

    /// Live authority for guarded coarse clock samples, never descriptor-only admission.
    /// Every executable entry overwrites IA32_TSC before KVM_RUN; short writes fail closed.
    pub fn coarse_clock_projection_admitted(&self) -> bool {
        coarse_projection_runtime_admitted(
            &self.execution_descriptor,
            self.scheduler.progress_mode(),
            self.coarse_clock_filter_identity,
            self.schedule_execution_poison.is_none() && !self.schedule_entry_in_flight,
        )
    }

    fn expected_boundary_debug(debug: &kvm_bindings::kvm_debug_exit_arch) -> bool {
        const DR6_CAUSE_MASK: u64 = 0xf | (1 << 13) | (1 << 14) | (1 << 15);
        debug.exception == 1 && debug.dr6 & DR6_CAUSE_MASK == 1 << 14
    }

    pub(super) fn normalize_coarse_exit(
        result: &Result<VcpuExit<'_>, kvm_ioctls::Error>,
        vcpu: usize,
    ) -> Result<KvmBoundaryKind, VmError> {
        match result {
            Ok(VcpuExit::Debug(debug)) if Self::expected_boundary_debug(debug) => {
                Ok(KvmBoundaryKind::DebugSingleStep)
            }
            Ok(VcpuExit::IoIn(port, data)) if matches!(data.len(), 1 | 2 | 4) => {
                Ok(KvmBoundaryKind::Pio { port: *port, size: data.len() as u8, direction: BoundaryIoDirection::Read })
            }
            Ok(VcpuExit::IoOut(port, data)) if matches!(data.len(), 1 | 2 | 4) => {
                Ok(KvmBoundaryKind::Pio { port: *port, size: data.len() as u8, direction: BoundaryIoDirection::Write })
            }
            Ok(VcpuExit::MmioRead(address, data)) if matches!(data.len(), 1 | 2 | 4 | 8) => {
                Ok(KvmBoundaryKind::Mmio { address: *address, len: data.len() as u8, direction: BoundaryIoDirection::Read })
            }
            Ok(VcpuExit::MmioWrite(address, data)) if matches!(data.len(), 1 | 2 | 4 | 8) => {
                Ok(KvmBoundaryKind::Mmio { address: *address, len: data.len() as u8, direction: BoundaryIoDirection::Write })
            }
            Ok(VcpuExit::Hlt) => Ok(KvmBoundaryKind::Halt),
            // A watchdog or unrelated host signal can leave arbitrary guest work
            // unverified. Neither is a retryable zero-progress observation.
            Ok(VcpuExit::Intr) => Err(VmError::ProgressCapability {
                message: format!("vCPU {vcpu} was host-interrupted before a verified boundary; live retry is forbidden"),
            }),
            other => Err(VmError::ProgressCapability {
                message: format!("unverified exit outside kvm-debug-exit-boundary-v1: {other:?}"),
            }),
        }
    }

    pub(super) fn validate_coarse_clock_devices(&self) -> Result<(), VmError> {
        if !self.coarse_mode() {
            return Ok(());
        }
        // HPET_LEGACY inhibits IRQ generation, not in-kernel counter/speaker
        // reads. Absence is required so guest programming and reads always
        // reach the virtual-time PIT, even inside TF-suppressed handlers.
        match self.vm.get_pit2() {
            Err(error) if error.errno() == libc::ENXIO => {}
            Err(error) => {
                return Err(VmError::ProgressCapability {
                    message: format!("cannot establish absent coarse kernel PIT: {error}"),
                })
            }
            Ok(_) => {
                return Err(VmError::ProgressCapability {
                    message: "coarse profile requires the in-kernel host-clock PIT device absent"
                        .into(),
                })
            }
        }
        for (vcpu, fd) in self.vcpus.iter().enumerate() {
            let lapic = fd.get_lapic().context(SetRegistersSnafu)?;
            let register = |offset: usize| {
                u32::from_le_bytes(std::array::from_fn(|i| lapic.regs[offset + i] as u8))
            };
            let lvtt = register(0x320);
            if lvtt & (1 << 16) == 0 && (register(0x380) != 0 || lvtt & (3 << 17) == 2 << 17) {
                return Err(VmError::ProgressCapability {
                    message: format!(
                        "vCPU {vcpu} enabled an unsupported host-clock LAPIC/deadline timer"
                    ),
                });
            }
        }
        Ok(())
    }

    pub(super) fn coarse_watchdog_deadline_elapsed(
        timer: Option<&SendTimerId>,
    ) -> Result<bool, VmError> {
        let timer = timer.ok_or_else(|| VmError::ProgressCapability {
            message: "coarse entry lost its admitted thread watchdog".into(),
        })?;
        let mut spec = std::mem::MaybeUninit::<libc::itimerspec>::uninit();
        // SAFETY: timer_gettime initializes the ABI output on success.
        if unsafe { libc::timer_gettime(timer.raw(), spec.as_mut_ptr()) } != 0 {
            return Err(VmError::ProgressCapability {
                message: format!(
                    "cannot inspect coarse recovery deadline: {}",
                    io::Error::last_os_error()
                ),
            });
        }
        let remaining = unsafe { spec.assume_init() }.it_value;
        Ok(remaining.tv_sec == 0 && remaining.tv_nsec == 0)
    }

    pub(super) fn disarm_coarse_watchdog(timer: Option<&SendTimerId>) -> Result<(), VmError> {
        let timer = timer.ok_or_else(|| VmError::ProgressCapability {
            message: "coarse entry lost its admitted thread watchdog".into(),
        })?;
        let spec = libc::itimerspec {
            it_interval: libc::timespec {
                tv_sec: 0,
                tv_nsec: 0,
            },
            it_value: libc::timespec {
                tv_sec: 0,
                tv_nsec: 0,
            },
        };
        // SAFETY: valid uniquely owned POSIX timer and initialized ABI value.
        if unsafe { libc::timer_settime(timer.raw(), 0, &spec, std::ptr::null_mut()) } != 0 {
            return Err(VmError::ProgressCapability {
                message: format!(
                    "coarse watchdog disarm failed: {}",
                    io::Error::last_os_error()
                ),
            });
        }
        Ok(())
    }

    pub(super) fn settle_coarse_io(&mut self, vcpu: usize) -> Result<(), VmError> {
        self.sync_tsc_to_guest()?;
        self.arm_coarse_watchdog()?;
        self.vcpus[vcpu].set_kvm_immediate_exit(1);
        // KVM completes the pending emulated operation before honoring immediate
        // exit. A completion #DB is absorbed into this same I/O boundary; with
        // immediate_exit set, this entry must not execute the next instruction.
        let result = self.vcpus[vcpu].run();
        let settled = match &result {
            Ok(VcpuExit::Intr) => true,
            Err(error) if error.errno() == libc::EINTR => true,
            Ok(VcpuExit::Debug(debug)) => Self::expected_boundary_debug(debug),
            _ => false,
        };
        let detail = (!settled).then(|| format!("ambiguous pending-I/O settlement: {result:?}"));
        drop(result);
        self.vcpus[vcpu].set_kvm_immediate_exit(0);
        Self::disarm_coarse_watchdog(self.thread_timer.as_ref())?;
        if let Some(message) = detail {
            return Err(VmError::ProgressCapability { message });
        }
        self.validate_coarse_clock_devices()
    }

    pub(super) fn snapshot_boundary_state(&self) -> crate::snapshot::SnapshotBoundaryState {
        crate::snapshot::SnapshotBoundaryState {
            pending_io: crate::snapshot::SnapshotPendingIoState::Settled,
            delivery_state: crate::snapshot::SnapshotDeliveryState::CapturedKernelAndUserspace,
            injection_ledger: self.injection_ledger.clone(),
        }
    }

    /// Drain only verified injection facts. Poisoned attempts remain diagnostics.
    pub fn take_injection_trace(
        &mut self,
    ) -> Result<Vec<crate::snapshot::InjectionRecord>, VmError> {
        self.ensure_schedule_execution_healthy()?;
        Ok(std::mem::take(&mut self.injection_records))
    }

    fn reserve_injection_records(&mut self, count: usize) -> Result<(), VmError> {
        self.ensure_schedule_execution_healthy()?;
        if !self.coarse_mode() {
            return Ok(());
        }
        if self
            .injection_records
            .len()
            .checked_add(count)
            .filter(|length| *length <= self.injection_record_limit)
            .is_none()
            || self
                .injection_ledger
                .sequence
                .checked_add(count as u64)
                .is_none()
        {
            return Err(VmError::ProgressCapability {
                message: "bounded injection evidence capacity exhausted before delivery".into(),
            });
        }
        self.injection_records
            .try_reserve(count)
            .map_err(|error| VmError::ProgressCapability {
                message: format!("cannot reserve injection evidence before delivery: {error}"),
            })
    }

    fn record_irq_fact(
        &mut self,
        irq: u32,
        level: bool,
        source: crate::snapshot::InjectionSource,
        accepted: bool,
    ) -> Result<(), VmError> {
        if !self.coarse_mode() {
            return Ok(());
        }
        let record = crate::snapshot::InjectionRecord {
            target_vcpu: None,
            kind: crate::snapshot::InjectionKind::Irq { irq, level },
            source,
            requested_state_id: self.scheduler.state_id().0,
            accepted,
        };
        self.injection_ledger
            .record(&record)
            .map_err(|error| VmError::Snapshot {
                message: format!("cannot commit reserved injection evidence: {error:?}"),
            })?;
        self.injection_records.push(record);
        Ok(())
    }

    pub(super) fn inject_irq_from(
        &mut self,
        irq: u32,
        source: crate::snapshot::InjectionSource,
    ) -> Result<(), VmError> {
        self.reserve_injection_records(4)?;
        for level in [true, false] {
            self.record_irq_fact(irq, level, source, false)?;
            if let Err(error) = self.vm.set_irq_line(irq, level) {
                if self.coarse_mode() {
                    self.latch_schedule_execution_poison(
                        SchedulePoisonStage::ExitHandling,
                        error.to_string(),
                    );
                }
                return Err(VmError::CreateIrqChip { source: error });
            }
            self.record_irq_fact(irq, level, source, true)?;
        }
        Ok(())
    }

    pub(super) fn deliver_coarse_virtio_irq(
        &mut self,
        device: usize,
        queue: usize,
    ) -> Result<(), VmError> {
        self.reserve_injection_records(4)?;
        let irq = self.virtio_devices[device].irq();
        for asserted in VIRTIO_IRQ_LEVELS {
            self.record_irq_fact(
                irq,
                asserted,
                crate::snapshot::InjectionSource::Virtio,
                false,
            )?;
            if let Err(source) = self.vm.set_irq_line(irq, asserted) {
                self.virtio_devices[device].record_interrupt_failure(queue, irq, asserted);
                return Err(VmError::VirtioInterrupt {
                    irq,
                    asserted,
                    source,
                });
            }
            self.record_irq_fact(
                irq,
                asserted,
                crate::snapshot::InjectionSource::Virtio,
                true,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_requires_live_filter_health_mode_and_eligible_measured_descriptor() {
        use crate::snapshot::{
            SnapshotCpuidEntry, SnapshotExecutionDescriptor, SnapshotHostOrigin,
            SnapshotMsrFilterProfile, SnapshotSafeProjection,
        };
        let mut descriptor = SnapshotExecutionDescriptor::portable_fixture(&VmConfig {
            num_vcpus: 2,
            smp_progress_mode: ProgressMode::KvmDebugBoundary,
            smp_progress_budget: ProgressBudget::KvmBoundaries {
                profile_id: crate::scheduler::core::BoundaryProfileId::KvmDebugExitBoundaryV1,
                count: 7,
            },
            ..Default::default()
        })
        .unwrap();
        let filter = Some(SnapshotMsrFilterProfile::KvmCoarseClockDenyV1.table_identity());
        // Even an apparently installed filter cannot elevate a portable fixture.
        assert!(!coarse_projection_runtime_admitted(
            &descriptor,
            ProgressMode::KvmDebugBoundary,
            filter,
            true,
        ));

        // Synthetic retained bytes exercise the pure guard, not actual host measurement.
        let leaf = |function| SnapshotCpuidEntry {
            function,
            index: 0,
            flags: 0,
            eax: 0,
            ebx: 0,
            ecx: 0,
            edx: 0,
        };
        descriptor.host.origin = SnapshotHostOrigin::Measured;
        descriptor.host.kernel_release = "synthetic-not-measured".into();
        descriptor.host.kernel_version = "synthetic-not-measured".into();
        descriptor.host.machine = "x86_64".into();
        descriptor.host.kvm_api_version = 12;
        descriptor.host.native_cpuid = [0, 1, 7, 0x8000_0000, 0x8000_0001, 0x8000_0007]
            .into_iter()
            .map(leaf)
            .collect();
        descriptor.host.kvm_supported_cpuid = vec![leaf(0)];
        descriptor.host.kvm_msr_indices = vec![0x10];
        descriptor.host.kvm_capabilities = [
            kvm_bindings::KVM_CAP_IRQCHIP,
            kvm_bindings::KVM_CAP_PIT2,
            kvm_bindings::KVM_CAP_ADJUST_CLOCK,
            kvm_bindings::KVM_CAP_VCPU_EVENTS,
            kvm_bindings::KVM_CAP_SET_GUEST_DEBUG,
            kvm_bindings::KVM_CAP_IMMEDIATE_EXIT,
            kvm_bindings::KVM_CAP_XSAVE,
            kvm_bindings::KVM_CAP_XCRS,
            kvm_bindings::KVM_CAP_TSC_CONTROL,
            kvm_bindings::KVM_CAP_GET_TSC_KHZ,
            kvm_bindings::KVM_CAP_X86_MSR_FILTER,
        ]
        .into_iter()
        .map(|capability| (capability, 1))
        .collect();
        descriptor.host.kvm_capabilities.sort_unstable();
        assert_eq!(descriptor.validate(), Ok(()));
        assert!(descriptor.permits_coarse_clock_projection());
        assert!(coarse_projection_runtime_admitted(
            &descriptor,
            ProgressMode::KvmDebugBoundary,
            filter,
            true,
        ));
        for (mode, installed, healthy) in [
            (ProgressMode::KvmDebugBoundary, None, true),
            (ProgressMode::KvmDebugBoundary, Some([0; 32]), true),
            (ProgressMode::ExactSingleStep, filter, true),
            (ProgressMode::KvmDebugBoundary, filter, false),
        ] {
            assert!(!coarse_projection_runtime_admitted(
                &descriptor,
                mode,
                installed,
                healthy,
            ));
        }
        let corruptions: &[fn(&mut SnapshotExecutionDescriptor)] = &[
            |state| state.host.origin = SnapshotHostOrigin::PortableFixture,
            |state| state.progress_mode = ProgressMode::ExactSingleStep,
            |state| state.coarse_msr_filter = None,
            |state| state.cpu_policy.hide_tsc = false,
            |state| state.cpu_policy.hide_hypervisor = false,
            |state| state.cpu_policy.tsc_mode = crate::cpu::TscAdvanceMode::InstructionProgress,
            |state| state.safe_projection = SnapshotSafeProjection::FullState,
            |state| state.boundary_profile.as_mut().unwrap().table_identity[0] ^= 1,
            |state| {
                state
                    .host
                    .kvm_capabilities
                    .iter_mut()
                    .find(|(id, _)| *id == kvm_bindings::KVM_CAP_X86_MSR_FILTER)
                    .unwrap()
                    .1 = 0;
            },
        ];
        for corrupt in corruptions {
            let mut invalid = descriptor.clone();
            corrupt(&mut invalid);
            assert!(!coarse_projection_runtime_admitted(
                &invalid,
                ProgressMode::KvmDebugBoundary,
                filter,
                true,
            ));
        }
    }

    #[test]
    fn debug_boundary_requires_single_step_cause_only() {
        let debug = kvm_bindings::kvm_debug_exit_arch {
            exception: 1,
            dr6: 1 << 14,
            ..Default::default()
        };
        assert_eq!(
            DeterministicVm::normalize_coarse_exit(&Ok(VcpuExit::Debug(debug)), 0).unwrap(),
            KvmBoundaryKind::DebugSingleStep
        );
        for cause in [0, 1, 1 << 13, 1 << 15, (1 << 14) | 1] {
            let invalid = kvm_bindings::kvm_debug_exit_arch {
                dr6: cause,
                ..debug
            };
            assert!(
                DeterministicVm::normalize_coarse_exit(&Ok(VcpuExit::Debug(invalid)), 0).is_err()
            );
        }
        let invalid = kvm_bindings::kvm_debug_exit_arch {
            exception: 3,
            ..debug
        };
        assert!(DeterministicVm::normalize_coarse_exit(&Ok(VcpuExit::Debug(invalid)), 0).is_err());
    }

    #[test]
    fn normalization_preserves_exact_exit_kind_direction_and_geometry() {
        let mut pio = [0u8; 2];
        assert_eq!(
            DeterministicVm::normalize_coarse_exit(&Ok(VcpuExit::IoIn(0x10, &mut pio)), 0).unwrap(),
            KvmBoundaryKind::Pio {
                port: 0x10,
                size: 2,
                direction: BoundaryIoDirection::Read
            }
        );
        assert_eq!(
            DeterministicVm::normalize_coarse_exit(&Ok(VcpuExit::IoOut(0x10, &pio)), 0).unwrap(),
            KvmBoundaryKind::Pio {
                port: 0x10,
                size: 2,
                direction: BoundaryIoDirection::Write
            }
        );
        let mut mmio = [0u8; 8];
        assert_eq!(
            DeterministicVm::normalize_coarse_exit(
                &Ok(VcpuExit::MmioRead(0xd0000000, &mut mmio)),
                0
            )
            .unwrap(),
            KvmBoundaryKind::Mmio {
                address: 0xd0000000,
                len: 8,
                direction: BoundaryIoDirection::Read
            }
        );
        assert_eq!(
            DeterministicVm::normalize_coarse_exit(&Ok(VcpuExit::MmioWrite(0xd0000000, &mmio)), 0)
                .unwrap(),
            KvmBoundaryKind::Mmio {
                address: 0xd0000000,
                len: 8,
                direction: BoundaryIoDirection::Write
            }
        );
        assert_eq!(
            DeterministicVm::normalize_coarse_exit(&Ok(VcpuExit::Hlt), 0).unwrap(),
            KvmBoundaryKind::Halt
        );
    }

    #[test]
    fn unverified_or_out_of_table_returns_never_normalize_into_progress() {
        for exit in [
            VcpuExit::Intr,
            VcpuExit::Unknown,
            VcpuExit::Shutdown,
            VcpuExit::IrqWindowOpen,
            VcpuExit::InternalError,
        ] {
            assert!(DeterministicVm::normalize_coarse_exit(&Ok(exit), 0).is_err());
        }
        assert!(
            DeterministicVm::normalize_coarse_exit(&Err(kvm_ioctls::Error::new(libc::EIO)), 0)
                .is_err()
        );
        assert!(
            DeterministicVm::normalize_coarse_exit(&Ok(VcpuExit::IoOut(0x10, &[0; 3])), 0).is_err()
        );
        assert!(DeterministicVm::normalize_coarse_exit(
            &Ok(VcpuExit::MmioWrite(0xd0000000, &[0; 3])),
            0
        )
        .is_err());
    }
}
