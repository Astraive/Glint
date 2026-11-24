//! [`ShutdownSignal`]: one place every shutdown trigger converges (COR-002).

use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use crate::lifecycle::ShutdownReason;

#[derive(Debug, Default)]
struct Inner {
    reason: Mutex<Option<ShutdownReason>>,
    changed: Condvar,
}

/// A cloneable, thread-safe shutdown request. Any trigger (a `shutdown`
/// command, the control channel closing, a native session-end notification
/// forwarded by a host crate) calls [`trigger`](Self::trigger); the first
/// reason wins. [`Runtime::run_until`](crate::Runtime::run_until) waits on
/// it and then shuts down cleanly.
#[derive(Debug, Clone, Default)]
pub struct ShutdownSignal {
    inner: Arc<Inner>,
}

impl ShutdownSignal {
    /// A signal that has not fired.
    pub fn new() -> Self {
        Self::default()
    }

    /// Request shutdown. Returns `true` if this call fired the signal,
    /// `false` if it had already fired (the first reason is kept).
    pub fn trigger(&self, reason: ShutdownReason) -> bool {
        let mut slot = self
            .inner
            .reason
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if slot.is_some() {
            return false;
        }
        *slot = Some(reason);
        self.inner.changed.notify_all();
        true
    }

    /// The reason, if the signal has fired.
    pub fn reason(&self) -> Option<ShutdownReason> {
        self.inner
            .reason
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Block until the signal fires.
    pub fn wait(&self) -> ShutdownReason {
        let mut slot = self
            .inner
            .reason
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(reason) = slot.as_ref() {
                return reason.clone();
            }
            slot = self
                .inner
                .changed
                .wait(slot)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Block until the signal fires or `timeout` passes.
    pub fn wait_timeout(&self, timeout: Duration) -> Option<ShutdownReason> {
        let slot = self
            .inner
            .reason
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let (slot, _) = self
            .inner
            .changed
            .wait_timeout_while(slot, timeout, |reason| reason.is_none())
            .unwrap_or_else(PoisonError::into_inner);
        slot.clone()
    }
}
