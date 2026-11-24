//! Theme operations from IPC clients, run on the runtime's thread
//! (CLI-006).
//!
//! The IPC server answers on its own connection threads, but only the
//! runtime may touch the daemon (the single writer). [`operation_channel`]
//! connects the two: the server side ([`OperationSender`], held by
//! [`CoreIpc`](crate::CoreIpc)) queues a [`ThemeOperation`] and waits for
//! its result with a deadline; the runtime side ([`OperationInbox`]) is
//! drained by [`Runtime::run_serving`](crate::Runtime::run_serving), which
//! calls [`Runtime::perform`](crate::Runtime::perform) for each job between
//! checks of the shutdown signal. Jobs run one at a time, in arrival order.

use std::fmt;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::time::Duration;

use glint_daemon::{DaemonError, ThemeOperation, ThemeOutcome, ThemeSelection};

use crate::error::CoreError;

/// What a performed operation did, and what changed because of it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OperationReport {
    /// The daemon's result.
    pub outcome: ThemeOutcome,
    /// Installed releases after a state-changing operation.
    pub installed_packages: Option<usize>,
    /// The newly selected theme, when the operation changed the selection
    /// (the shell was re-presented with it).
    pub theme: Option<ThemeSelection>,
}

/// Why an operation was not performed.
#[derive(Debug)]
#[non_exhaustive]
pub enum OperationError {
    /// The runtime cannot perform operations now (not ready, stopping).
    Core(CoreError),
    /// The daemon refused or failed it; nothing was changed.
    Daemon(DaemonError),
}

impl fmt::Display for OperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OperationError::Core(error) => write!(f, "{error}"),
            OperationError::Daemon(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for OperationError {}

pub(crate) type OperationResult = Result<OperationReport, OperationError>;

/// One queued operation and where its result goes.
pub(crate) struct Job {
    pub(crate) operation: ThemeOperation,
    pub(crate) reply: SyncSender<OperationResult>,
}

/// The runtime side: jobs to perform.
#[derive(Debug)]
pub struct OperationInbox {
    jobs: Receiver<Job>,
}

impl fmt::Debug for Job {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Job")
            .field("operation", &self.operation)
            .finish_non_exhaustive()
    }
}

/// The IPC side: queues jobs for the runtime. Cloneable; one per server.
#[derive(Debug, Clone)]
pub struct OperationSender {
    jobs: Sender<Job>,
}

/// A connected sender and inbox.
pub fn operation_channel() -> (OperationSender, OperationInbox) {
    let (jobs, inbox) = mpsc::channel();
    (OperationSender { jobs }, OperationInbox { jobs: inbox })
}

/// Why a queued operation produced no result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Undelivered {
    /// The runtime is gone (stopped or stopping).
    Stopped,
    /// No result within the deadline; the job may still run.
    TimedOut,
}

impl OperationSender {
    /// Queue `operation` and wait up to `deadline` for its result.
    pub(crate) fn perform(
        &self,
        operation: ThemeOperation,
        deadline: Duration,
    ) -> Result<OperationResult, Undelivered> {
        let (reply, result) = mpsc::sync_channel(1);
        self.jobs
            .send(Job { operation, reply })
            .map_err(|_| Undelivered::Stopped)?;
        match result.recv_timeout(deadline) {
            Ok(result) => Ok(result),
            Err(RecvTimeoutError::Timeout) => Err(Undelivered::TimedOut),
            Err(RecvTimeoutError::Disconnected) => Err(Undelivered::Stopped),
        }
    }
}

impl OperationInbox {
    /// The next job, waiting up to `timeout`.
    pub(crate) fn next(&self, timeout: Duration) -> Option<Job> {
        self.jobs.recv_timeout(timeout).ok()
    }

    /// Refuse every queued job (the runtime is stopping).
    pub(crate) fn refuse_pending(&self, error: &CoreError) {
        while let Ok(job) = self.jobs.try_recv() {
            let _ = job.reply.send(Err(OperationError::Core(error.clone())));
        }
    }
}
