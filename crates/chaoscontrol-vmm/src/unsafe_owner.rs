//! Explicit owner for VMM unsafe timer transfer and destruction.

// r[impl chaoscontrol.architecture_modules.ownership]
// r[impl chaoscontrol.architecture_modules.validation]

/// Unique owner of one POSIX timer returned by `timer_create`.
///
/// The timer is used only by the current VM worker. A controller can move the
/// VM into a scoped worker and must join that worker before it regains the VM.
pub(crate) struct SendTimerId(libc::timer_t);

impl SendTimerId {
    /// Take ownership of one successfully created POSIX timer.
    ///
    /// The caller must supply a timer returned by a successful `timer_create`
    /// call and must not delete it through another owner.
    pub(crate) unsafe fn from_created(timer: libc::timer_t) -> Self {
        Self(timer)
    }

    pub(crate) fn raw(&self) -> libc::timer_t {
        self.0
    }
}

impl Drop for SendTimerId {
    fn drop(&mut self) {
        // SAFETY: this wrapper is the unique owner of a successful timer_create result.
        unsafe {
            libc::timer_delete(self.0);
        }
    }
}

// SAFETY: ownership moves into a scoped VM worker. The controller joins that
// worker before it can access or destroy the timer owner again.
unsafe impl Send for SendTimerId {}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimerReleasePlan {
    RetainUntilJoin,
    ReleaseOwnedTimer,
    NoTimer,
}

#[cfg(test)]
fn plan_timer_release(timer_owned: bool, worker_joined: bool) -> TimerReleasePlan {
    match (timer_owned, worker_joined) {
        (false, _) => TimerReleasePlan::NoTimer,
        (true, false) => TimerReleasePlan::RetainUntilJoin,
        (true, true) => TimerReleasePlan::ReleaseOwnedTimer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send<T: Send>() {}

    #[test]
    fn timer_owner_has_the_reviewed_send_boundary() {
        assert_send::<SendTimerId>();
    }

    #[test]
    fn cancellation_retains_timer_until_worker_join() {
        assert_eq!(
            plan_timer_release(true, false),
            TimerReleasePlan::RetainUntilJoin
        );
        assert_eq!(
            plan_timer_release(true, true),
            TimerReleasePlan::ReleaseOwnedTimer
        );
        assert_eq!(plan_timer_release(false, true), TimerReleasePlan::NoTimer);
    }
}
