//! Apply journal, snapshots and crash rollback (INS-005).
//!
//! [`Installer::apply`] follows the plan's apply transaction:
//!
//! 1. Validate: the target release is installed, every native change has an
//!    adapter, and changes whose adapter cannot snapshot **and** restore
//!    were explicitly approved ([`ApplyPlan::approve_irreversible`]).
//! 2. Snapshot every setting an adapter will change (`canSnapshot`).
//! 3. Commit the journal: the prior active profile and every planned native
//!    change with its snapshot and `canSnapshot`/`canRestore` semantics.
//! 4. Stage the new surfaces beside the old ones ([`SurfaceHost::stage`]);
//!    the known-good UI is not torn down.
//! 5. Apply native changes one by one, marking each `applying` (durably)
//!    before calling the adapter and `applied` after.
//! 6. In one transaction, activate the new profile and delete the journal.
//!    Only then are the old surfaces torn down ([`SurfaceHost::promote`]).
//!
//! On a live failure the staged surfaces are discarded (the old ones never
//! closed), every touched native setting is restored best-effort in reverse
//! order, and the [`RollbackReport`] says exactly what could not be
//! restored ([`ApplyErrorCode::RollbackIncomplete`]). After a crash the
//! journal survives: [`Installer::open`] reports it as
//! [`RecoveryReport::pending_apply`](crate::RecoveryReport::pending_apply)
//! (the previous theme is still active, because activation commits with
//! the journal's deletion), and [`Installer::recover_apply`] restores the
//! native settings with the adapters once the caller has them.

use std::fmt;

use glint_storage::rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use glint_storage::{ActiveTheme, ErrorCategory, activate_in, unix_now};

use crate::error::InstallError;
use crate::fault::FaultPoint;
use crate::install::Installer;

/// Whether an adapter can capture and put back a setting's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RestoreSemantics {
    /// The current value can be read before changing it.
    pub can_snapshot: bool,
    /// A captured value can be written back.
    pub can_restore: bool,
}

impl RestoreSemantics {
    /// Snapshot and restore both supported.
    pub const REVERSIBLE: Self = Self {
        can_snapshot: true,
        can_restore: true,
    };

    /// Neither supported: changes are one-way.
    pub const IRREVERSIBLE: Self = Self {
        can_snapshot: false,
        can_restore: false,
    };

    /// True when a change can be undone.
    pub const fn is_reversible(self) -> bool {
        self.can_snapshot && self.can_restore
    }
}

/// Broad class of a native adapter failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NativeErrorKind {
    /// Not supported on this host.
    Unsupported,
    /// The OS refused.
    PermissionDenied,
    /// The setting or service is unavailable right now.
    Unavailable,
    /// The adapter gave up waiting.
    Timeout,
    /// Anything else.
    Failure,
}

/// A native adapter failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeError {
    /// Its class.
    pub kind: NativeErrorKind,
    /// Human-readable detail.
    pub message: String,
}

impl NativeError {
    /// A new error.
    pub fn new(kind: NativeErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for NativeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for NativeError {}

/// A native OS preference adapter (wallpaper, cursor, accent colour ...).
/// Implemented by the native host crates; values are opaque strings the
/// adapter serializes itself.
pub trait NativeAdapter {
    /// Stable adapter ID, e.g. `windows.wallpaper`.
    fn id(&self) -> &str;
    /// What the adapter can snapshot and restore.
    fn semantics(&self) -> RestoreSemantics;
    /// Read the current value of `setting`.
    ///
    /// # Errors
    ///
    /// A [`NativeError`].
    fn snapshot(&mut self, setting: &str) -> Result<String, NativeError>;
    /// Change `setting` to `value`.
    ///
    /// # Errors
    ///
    /// A [`NativeError`]; the setting may be partly changed.
    fn apply(&mut self, setting: &str, value: &str) -> Result<(), NativeError>;
    /// Write a snapshot back.
    ///
    /// # Errors
    ///
    /// A [`NativeError`].
    fn restore(&mut self, setting: &str, snapshot: &str) -> Result<(), NativeError>;
}

/// The surface host, which opens theme surfaces (implemented by the shell).
pub trait SurfaceHost {
    /// Open the target's surfaces beside the current ones, without closing
    /// anything.
    ///
    /// # Errors
    ///
    /// A message when the surfaces cannot start (`renderer-start-failed`).
    fn stage(&mut self, profile_id: &str, digest: &str) -> Result<(), String>;
    /// After commit: close the old surfaces; the staged ones become current.
    fn promote(&mut self);
    /// After a failure: close the staged surfaces; the old ones stay.
    fn discard(&mut self);
}

/// One native preference change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeChange {
    /// Adapter ID.
    pub adapter: String,
    /// Setting name within the adapter.
    pub setting: String,
    /// New value (adapter-serialized).
    pub value: String,
}

impl NativeChange {
    /// A new change.
    pub fn new(
        adapter: impl Into<String>,
        setting: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self {
            adapter: adapter.into(),
            setting: setting.into(),
            value: value.into(),
        }
    }
}

/// A desired-state plan: the profile to activate and the native changes
/// the user approved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyPlan {
    /// Profile to activate.
    pub profile_id: String,
    /// Installed release to use as its base theme.
    pub digest: String,
    /// Native changes, applied in order.
    pub native: Vec<NativeChange>,
    /// The user approved changes that cannot be snapshotted or restored.
    pub approve_irreversible: bool,
}

impl ApplyPlan {
    /// A plan without native changes.
    pub fn new(profile_id: impl Into<String>, digest: impl Into<String>) -> Self {
        Self {
            profile_id: profile_id.into(),
            digest: digest.into(),
            native: Vec::new(),
            approve_irreversible: false,
        }
    }

    /// Add a native change.
    pub fn with_native(mut self, change: NativeChange) -> Self {
        self.native.push(change);
        self
    }
}

/// A native setting, by adapter and name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SettingRef {
    /// Adapter ID.
    pub adapter: String,
    /// Setting name.
    pub setting: String,
}

/// Why a setting could not be restored.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnrestoredReason {
    /// The adapter cannot restore (`canRestore = false`).
    CannotRestore,
    /// No snapshot was taken (`canSnapshot = false`).
    NoSnapshot,
    /// The adapter was not provided at recovery.
    AdapterMissing,
    /// The adapter tried and failed.
    RestoreFailed(NativeError),
}

/// A setting left changed after rollback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unrestored {
    /// The setting.
    pub setting: SettingRef,
    /// Why.
    pub reason: UnrestoredReason,
}

/// What a rollback did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct RollbackReport {
    /// Settings put back to their snapshot.
    pub restored: Vec<SettingRef>,
    /// Settings that may still carry the new value, and why.
    pub not_restored: Vec<Unrestored>,
    /// The active theme after rollback (the previous one).
    pub active: Option<ActiveTheme>,
}

impl RollbackReport {
    /// True when every touched setting was restored.
    pub fn is_complete(&self) -> bool {
        self.not_restored.is_empty()
    }
}

/// A planned native change recorded in a pending journal.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PendingNative {
    /// The setting.
    pub setting: SettingRef,
    /// `planned`, `applying` or `applied`.
    pub state: String,
    /// The adapter could snapshot it.
    pub can_snapshot: bool,
    /// The adapter can restore it.
    pub can_restore: bool,
    /// A snapshot was recorded.
    pub has_snapshot: bool,
}

/// An interrupted apply found at startup, for recovery UI to present.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PendingApply {
    /// The active theme before the apply (still active).
    pub prior: Option<ActiveTheme>,
    /// The profile the apply was switching to.
    pub target_profile_id: String,
    /// The release the apply was switching to.
    pub target_digest: String,
    /// Every planned native change and how far it got.
    pub native: Vec<PendingNative>,
}

impl PendingApply {
    /// Settings the interrupted apply may have changed.
    pub fn touched(&self) -> impl Iterator<Item = &PendingNative> {
        self.native.iter().filter(|n| n.state != "planned")
    }
}

/// The result of a successful apply.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ApplyOutcome {
    /// The new active theme.
    pub active: ActiveTheme,
    /// The theme it replaced.
    pub previous: Option<ActiveTheme>,
}

/// Stable apply error codes (`GLINT_APPLY_*`), following the plan's error
/// taxonomy where it has a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ApplyErrorCode {
    /// An installer or storage error; see [`ApplyError::install_error`].
    Install,
    /// An interrupted apply must be recovered first.
    RecoveryRequired,
    /// A planned change names an adapter that was not provided.
    UnknownAdapter,
    /// Irreversible changes were not approved.
    IrreversibleNotApproved,
    /// Snapshotting a setting failed; nothing was changed.
    SnapshotFailed,
    /// The new surfaces failed to start (`renderer-start-failed`); rolled
    /// back completely.
    RendererStartFailed,
    /// A native change failed (`native-apply-failed`); rolled back
    /// completely.
    NativeApplyFailed,
    /// Rolled back, but some native settings could not be restored
    /// (`rollback-incomplete`); see the report.
    RollbackIncomplete,
}

impl ApplyErrorCode {
    /// The stable string form.
    pub const fn as_str(self) -> &'static str {
        match self {
            ApplyErrorCode::Install => "GLINT_APPLY_INSTALL",
            ApplyErrorCode::RecoveryRequired => "GLINT_APPLY_RECOVERY_REQUIRED",
            ApplyErrorCode::UnknownAdapter => "GLINT_APPLY_UNKNOWN_ADAPTER",
            ApplyErrorCode::IrreversibleNotApproved => "GLINT_APPLY_IRREVERSIBLE_NOT_APPROVED",
            ApplyErrorCode::SnapshotFailed => "GLINT_APPLY_SNAPSHOT_FAILED",
            ApplyErrorCode::RendererStartFailed => "GLINT_APPLY_RENDERER_START_FAILED",
            ApplyErrorCode::NativeApplyFailed => "GLINT_APPLY_NATIVE_APPLY_FAILED",
            ApplyErrorCode::RollbackIncomplete => "GLINT_APPLY_ROLLBACK_INCOMPLETE",
        }
    }
}

impl fmt::Display for ApplyErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Every way an apply can fail.
#[derive(Debug)]
#[non_exhaustive]
pub enum ApplyError {
    /// Installer or storage failure (including a simulated crash).
    Install(InstallError),
    /// A pending journal must be recovered first.
    RecoveryRequired,
    /// No adapter with this ID.
    UnknownAdapter(String),
    /// These changes cannot be undone and were not approved.
    IrreversibleNotApproved(Vec<SettingRef>),
    /// Snapshot failed before any change.
    SnapshotFailed {
        /// The setting.
        setting: SettingRef,
        /// The adapter error.
        error: NativeError,
    },
    /// The surfaces failed to start; rolled back.
    RendererStartFailed {
        /// The host's message.
        message: String,
        /// What the rollback did.
        rollback: Box<RollbackReport>,
    },
    /// A native change failed; rolled back.
    NativeApplyFailed {
        /// The failing setting.
        setting: SettingRef,
        /// The adapter error.
        error: NativeError,
        /// What the rollback did.
        rollback: Box<RollbackReport>,
    },
}

impl ApplyError {
    /// The stable code. A failure whose rollback left settings changed is
    /// always [`ApplyErrorCode::RollbackIncomplete`].
    pub fn code(&self) -> ApplyErrorCode {
        if self.rollback().is_some_and(|r| !r.is_complete()) {
            return ApplyErrorCode::RollbackIncomplete;
        }
        match self {
            ApplyError::Install(_) => ApplyErrorCode::Install,
            ApplyError::RecoveryRequired => ApplyErrorCode::RecoveryRequired,
            ApplyError::UnknownAdapter(_) => ApplyErrorCode::UnknownAdapter,
            ApplyError::IrreversibleNotApproved(_) => ApplyErrorCode::IrreversibleNotApproved,
            ApplyError::SnapshotFailed { .. } => ApplyErrorCode::SnapshotFailed,
            ApplyError::RendererStartFailed { .. } => ApplyErrorCode::RendererStartFailed,
            ApplyError::NativeApplyFailed { .. } => ApplyErrorCode::NativeApplyFailed,
        }
    }

    /// The broad category.
    pub fn category(&self) -> ErrorCategory {
        let native = |error: &NativeError| match error.kind {
            NativeErrorKind::Unsupported => ErrorCategory::Unsupported,
            NativeErrorKind::PermissionDenied => ErrorCategory::PermissionDenied,
            NativeErrorKind::Unavailable => ErrorCategory::Unavailable,
            NativeErrorKind::Timeout => ErrorCategory::Timeout,
            NativeErrorKind::Failure => ErrorCategory::Failure,
        };
        match self {
            ApplyError::Install(error) => error.category(),
            ApplyError::UnknownAdapter(_) => ErrorCategory::Unsupported,
            ApplyError::SnapshotFailed { error, .. }
            | ApplyError::NativeApplyFailed { error, .. } => native(error),
            _ => ErrorCategory::Failure,
        }
    }

    /// The rollback report of a failure that was rolled back.
    pub fn rollback(&self) -> Option<&RollbackReport> {
        match self {
            ApplyError::RendererStartFailed { rollback, .. }
            | ApplyError::NativeApplyFailed { rollback, .. } => Some(rollback.as_ref()),
            _ => None,
        }
    }

    /// The installer error behind [`ApplyErrorCode::Install`].
    pub fn install_error(&self) -> Option<&InstallError> {
        match self {
            ApplyError::Install(error) => Some(error),
            _ => None,
        }
    }
}

impl From<InstallError> for ApplyError {
    fn from(error: InstallError) -> Self {
        ApplyError::Install(error)
    }
}

impl From<glint_storage::StorageError> for ApplyError {
    fn from(error: glint_storage::StorageError) -> Self {
        ApplyError::Install(InstallError::Storage(error))
    }
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: ", self.code())?;
        match self {
            ApplyError::Install(error) => write!(f, "{error}"),
            ApplyError::RecoveryRequired => {
                f.write_str("an interrupted apply must be recovered first")
            }
            ApplyError::UnknownAdapter(id) => write!(f, "no native adapter {id:?}"),
            ApplyError::IrreversibleNotApproved(settings) => write!(
                f,
                "{} change(s) cannot be undone and were not approved",
                settings.len()
            ),
            ApplyError::SnapshotFailed { setting, error } => write!(
                f,
                "snapshot of {}/{} failed: {error}",
                setting.adapter, setting.setting
            ),
            ApplyError::RendererStartFailed { message, rollback } => write!(
                f,
                "surfaces failed to start ({message}); {} setting(s) not restored",
                rollback.not_restored.len()
            ),
            ApplyError::NativeApplyFailed {
                setting,
                error,
                rollback,
            } => write!(
                f,
                "{}/{} failed: {error}; {} setting(s) not restored",
                setting.adapter,
                setting.setting,
                rollback.not_restored.len()
            ),
        }
    }
}

impl std::error::Error for ApplyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ApplyError::Install(error) => Some(error),
            ApplyError::SnapshotFailed { error, .. }
            | ApplyError::NativeApplyFailed { error, .. } => Some(error),
            _ => None,
        }
    }
}

struct JournalNative {
    seq: i64,
    setting: SettingRef,
    prior: Option<String>,
    can_snapshot: bool,
    can_restore: bool,
    state: String,
}

fn find<'a>(
    adapters: &'a mut [Box<dyn NativeAdapter>],
    id: &str,
) -> Option<&'a mut Box<dyn NativeAdapter>> {
    adapters.iter_mut().find(|adapter| adapter.id() == id)
}

impl Installer {
    /// Apply a plan transactionally; see the [module documentation](self).
    ///
    /// # Errors
    ///
    /// [`ApplyError`]; on every error the previous theme stays active, and
    /// failures after native changes started carry a [`RollbackReport`].
    pub fn apply(
        &mut self,
        plan: &ApplyPlan,
        surfaces: &mut dyn SurfaceHost,
        adapters: &mut [Box<dyn NativeAdapter>],
    ) -> Result<ApplyOutcome, ApplyError> {
        // 1. Validate.
        if self.pending_apply()?.is_some() {
            return Err(ApplyError::RecoveryRequired);
        }
        glint_storage::validate_profile_id(&plan.profile_id)?;
        if self.package(&plan.digest)?.is_none() {
            return Err(InstallError::NotInstalled(plan.digest.clone()).into());
        }
        let mut irreversible = Vec::new();
        for change in &plan.native {
            let adapter = find(adapters, &change.adapter)
                .ok_or_else(|| ApplyError::UnknownAdapter(change.adapter.clone()))?;
            if !adapter.semantics().is_reversible() {
                irreversible.push(SettingRef {
                    adapter: change.adapter.clone(),
                    setting: change.setting.clone(),
                });
            }
        }
        if !irreversible.is_empty() && !plan.approve_irreversible {
            return Err(ApplyError::IrreversibleNotApproved(irreversible));
        }

        // 2. Snapshot.
        let mut snapshots = Vec::with_capacity(plan.native.len());
        for change in &plan.native {
            let adapter = find(adapters, &change.adapter).expect("validated above");
            let semantics = adapter.semantics();
            let prior = if semantics.can_snapshot {
                Some(adapter.snapshot(&change.setting).map_err(|error| {
                    ApplyError::SnapshotFailed {
                        setting: SettingRef {
                            adapter: change.adapter.clone(),
                            setting: change.setting.clone(),
                        },
                        error,
                    }
                })?)
            } else {
                None
            };
            snapshots.push((semantics, prior));
        }

        // 3. Journal.
        let previous = self.storage.active_theme()?;
        self.write_apply_journal(plan, previous.as_ref(), &snapshots)?;
        self.fault(FaultPoint::ApplyAfterJournal)?;

        // 4. Stage surfaces beside the current ones.
        if let Err(message) = surfaces.stage(&plan.profile_id, &plan.digest) {
            surfaces.discard();
            let rollback = self.roll_back_native(adapters)?;
            return Err(ApplyError::RendererStartFailed {
                message,
                rollback: Box::new(rollback),
            });
        }
        self.fault(FaultPoint::ApplyAfterSurfaces)?;

        // 5. Native changes.
        for (index, change) in plan.native.iter().enumerate() {
            let seq = i64::try_from(index).unwrap_or(i64::MAX);
            self.set_native_state(seq, "applying")?;
            self.fault(FaultPoint::ApplyBeforeNative(index))?;
            let adapter = find(adapters, &change.adapter).expect("validated above");
            if let Err(error) = adapter.apply(&change.setting, &change.value) {
                surfaces.discard();
                let rollback = self.roll_back_native(adapters)?;
                return Err(ApplyError::NativeApplyFailed {
                    setting: SettingRef {
                        adapter: change.adapter.clone(),
                        setting: change.setting.clone(),
                    },
                    error,
                    rollback: Box::new(rollback),
                });
            }
            self.fault(FaultPoint::ApplyAfterNative(index))?;
            self.set_native_state(seq, "applied")?;
        }

        // 6. Commit: activation and journal deletion are one transaction.
        let active = {
            let tx = self
                .storage
                .connection_mut()
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(InstallError::db("begin apply commit"))?;
            let active = activate_in(&tx, &plan.profile_id, &plan.digest)?;
            delete_apply_journal(&tx)?;
            if let Some(injector) = &mut self.faults
                && injector.crash_at(FaultPoint::ApplyBeforeCommit)
            {
                return Err(InstallError::Interrupted(FaultPoint::ApplyBeforeCommit).into());
            }
            tx.commit().map_err(InstallError::db("commit apply"))?;
            active
        };
        self.storage.write_active_record()?;
        self.fault(FaultPoint::ApplyAfterCommit)?;
        surfaces.promote();
        Ok(ApplyOutcome { active, previous })
    }

    /// The interrupted apply recorded in the journal, if any. The previous
    /// theme is still active; present this to the user and call
    /// [`recover_apply`](Self::recover_apply).
    ///
    /// # Errors
    ///
    /// Storage errors.
    pub fn pending_apply(&self) -> Result<Option<PendingApply>, InstallError> {
        let conn = self.storage.connection();
        let head = conn
            .query_row(
                "SELECT prior_profile_id, prior_digest, prior_revision, target_profile_id, target_digest
                   FROM apply_journal WHERE singleton = 1",
                [],
                |row| {
                    let prior = match (
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                    ) {
                        (Some(profile_id), Some(package_digest), Some(revision)) => {
                            Some(ActiveTheme {
                                profile_id,
                                package_digest,
                                revision: revision.try_into().unwrap_or(0),
                            })
                        }
                        _ => None,
                    };
                    Ok((prior, row.get::<_, String>(3)?, row.get::<_, String>(4)?))
                },
            )
            .optional()
            .map_err(InstallError::db("read apply journal"))?;
        let Some((prior, target_profile_id, target_digest)) = head else {
            return Ok(None);
        };
        let native = journal_native(conn)?
            .into_iter()
            .map(|n| PendingNative {
                setting: n.setting,
                state: n.state,
                can_snapshot: n.can_snapshot,
                can_restore: n.can_restore,
                has_snapshot: n.prior.is_some(),
            })
            .collect();
        Ok(Some(PendingApply {
            prior,
            target_profile_id,
            target_digest,
            native,
        }))
    }

    /// Roll back an interrupted apply: restore every native setting it may
    /// have changed (best-effort, reverse order) and clear the journal.
    /// Returns `None` when nothing was pending.
    ///
    /// The active profile is not touched: an interrupted apply never
    /// committed it. The caller re-opens the active profile's surfaces as
    /// on any startup.
    ///
    /// # Errors
    ///
    /// Storage errors (the journal is then kept for another attempt).
    pub fn recover_apply(
        &mut self,
        adapters: &mut [Box<dyn NativeAdapter>],
    ) -> Result<Option<RollbackReport>, InstallError> {
        if self.pending_apply()?.is_none() {
            return Ok(None);
        }
        let report = self.roll_back_native(adapters)?;
        self.startup.pending_apply = None;
        Ok(Some(report))
    }

    fn write_apply_journal(
        &mut self,
        plan: &ApplyPlan,
        previous: Option<&ActiveTheme>,
        snapshots: &[(RestoreSemantics, Option<String>)],
    ) -> Result<(), InstallError> {
        let tx = self
            .storage
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(InstallError::db("begin apply journal"))?;
        tx.execute(
            "INSERT INTO apply_journal (singleton, prior_profile_id, prior_digest, prior_revision,
                                        target_profile_id, target_digest, started_at)
                  VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                previous.map(|p| p.profile_id.as_str()),
                previous.map(|p| p.package_digest.as_str()),
                previous.map(|p| i64::try_from(p.revision).unwrap_or(i64::MAX)),
                plan.profile_id,
                plan.digest,
                unix_now(),
            ],
        )
        .map_err(InstallError::db("write apply journal"))?;
        for (index, (change, (semantics, prior))) in plan.native.iter().zip(snapshots).enumerate() {
            tx.execute(
                "INSERT INTO apply_native (seq, adapter, setting, prior_value, new_value,
                                           can_snapshot, can_restore, state)
                      VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'planned')",
                params![
                    i64::try_from(index).unwrap_or(i64::MAX),
                    change.adapter,
                    change.setting,
                    prior,
                    change.value,
                    semantics.can_snapshot,
                    semantics.can_restore,
                ],
            )
            .map_err(InstallError::db("write apply journal"))?;
        }
        tx.commit()
            .map_err(InstallError::db("commit apply journal"))
    }

    fn set_native_state(&self, seq: i64, state: &str) -> Result<(), InstallError> {
        self.storage
            .connection()
            .execute(
                "UPDATE apply_native SET state = ?2 WHERE seq = ?1",
                params![seq, state],
            )
            .map_err(InstallError::db("update apply journal"))?;
        Ok(())
    }

    /// Restore touched settings in reverse order and delete the journal.
    fn roll_back_native(
        &mut self,
        adapters: &mut [Box<dyn NativeAdapter>],
    ) -> Result<RollbackReport, InstallError> {
        let mut report = RollbackReport::default();
        let mut rows = journal_native(self.storage.connection())?;
        rows.retain(|row| row.state != "planned");
        rows.sort_by_key(|row| std::cmp::Reverse(row.seq));
        for row in rows {
            let reason = if !row.can_restore {
                Some(UnrestoredReason::CannotRestore)
            } else if let Some(prior) = row.prior.as_deref() {
                match find(adapters, &row.setting.adapter) {
                    None => Some(UnrestoredReason::AdapterMissing),
                    Some(adapter) => adapter
                        .restore(&row.setting.setting, prior)
                        .err()
                        .map(UnrestoredReason::RestoreFailed),
                }
            } else {
                debug_assert!(!row.can_snapshot);
                Some(UnrestoredReason::NoSnapshot)
            };
            match reason {
                None => report.restored.push(row.setting),
                Some(reason) => report.not_restored.push(Unrestored {
                    setting: row.setting,
                    reason,
                }),
            }
        }
        let tx = self
            .storage
            .connection_mut()
            .transaction()
            .map_err(InstallError::db("begin apply rollback"))?;
        delete_apply_journal(&tx)?;
        tx.commit()
            .map_err(InstallError::db("commit apply rollback"))?;
        report.active = self.storage.active_theme()?;
        Ok(report)
    }
}

fn journal_native(conn: &Connection) -> Result<Vec<JournalNative>, InstallError> {
    let mut stmt = conn
        .prepare(
            "SELECT seq, adapter, setting, prior_value, can_snapshot, can_restore, state
               FROM apply_native ORDER BY seq",
        )
        .map_err(InstallError::db("read apply journal"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(JournalNative {
                seq: row.get(0)?,
                setting: SettingRef {
                    adapter: row.get(1)?,
                    setting: row.get(2)?,
                },
                prior: row.get(3)?,
                can_snapshot: row.get(4)?,
                can_restore: row.get(5)?,
                state: row.get(6)?,
            })
        })
        .map_err(InstallError::db("read apply journal"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(InstallError::db("read apply journal"))
}

fn delete_apply_journal(conn: &Connection) -> Result<(), InstallError> {
    conn.execute_batch("DELETE FROM apply_native; DELETE FROM apply_journal;")
        .map_err(InstallError::db("clear apply journal"))
}
