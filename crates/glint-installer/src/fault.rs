//! Crash simulation hooks.
//!
//! Every durable step of an install, removal or apply has a named
//! [`FaultPoint`]. A [`FaultInjector`] installed with
//! [`Installer::set_fault_injector`](crate::Installer::set_fault_injector)
//! is asked at each point; answering `true` makes the operation return
//! [`InstallError::Interrupted`](crate::InstallError::Interrupted)
//! immediately, **without any cleanup**, which is what the filesystem and
//! the database look like after the process dies at that point (an open
//! SQLite transaction is rolled back, exactly as SQLite does after a crash).
//! Tests then drop the installer, reopen it and check what recovery does.
//!
//! Production code never installs an injector.

/// A named point between two durable steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FaultPoint {
    /// Install: the journal row is committed; nothing moved yet.
    InstallAfterJournal,
    /// Install: the staged package was renamed into the store; the database
    /// does not know it yet.
    InstallAfterMove,
    /// Install: inside the commit transaction, before `COMMIT`.
    InstallBeforeCommit,
    /// Install: committed.
    InstallAfterCommit,
    /// Remove: the journal row is committed; nothing moved yet.
    RemoveAfterJournal,
    /// Remove: the store entry was renamed into the trash.
    RemoveAfterMove,
    /// Remove: inside the commit transaction, before `COMMIT`.
    RemoveBeforeCommit,
    /// Remove: committed; the trash entry is not deleted yet.
    RemoveAfterCommit,
    /// Apply: the journal (prior state and native snapshots) is committed.
    ApplyAfterJournal,
    /// Apply: new surfaces are staged beside the old ones.
    ApplyAfterSurfaces,
    /// Apply: native change `n` is marked `applying`; the adapter has not
    /// been called.
    ApplyBeforeNative(usize),
    /// Apply: the adapter applied native change `n`; not yet marked
    /// `applied`.
    ApplyAfterNative(usize),
    /// Apply: inside the commit transaction, before `COMMIT`.
    ApplyBeforeCommit,
    /// Apply: committed; old surfaces not yet torn down.
    ApplyAfterCommit,
}

/// Decides whether to simulate a crash at a [`FaultPoint`].
pub trait FaultInjector: Send {
    /// `true` to stop here as if the process died.
    fn crash_at(&mut self, point: FaultPoint) -> bool;
}

impl<F: FnMut(FaultPoint) -> bool + Send> FaultInjector for F {
    fn crash_at(&mut self, point: FaultPoint) -> bool {
        self(point)
    }
}
