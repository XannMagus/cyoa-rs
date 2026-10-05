//! Atomic local saves. Blocking methods run only off the presentation loop.
use super::codec;
use cyoa_application::{cancellation::CancellationToken, persistence::*};
use std::{
    io,
    path::{Path, PathBuf},
};

pub fn data_directory(explicit: Option<PathBuf>) -> io::Result<PathBuf> {
    if let Some(path) = explicit {
        return absolute(path);
    }
    if let Some(path) = std::env::var_os("XDG_DATA_HOME") {
        absolute(PathBuf::from(path))?;
    }
    directories::ProjectDirs::from("", "", "cyoa")
        .map(|d| d.data_dir().to_path_buf())
        .ok_or_else(|| io::Error::other("no data location; provide an absolute data directory"))
}
fn absolute(path: PathBuf) -> io::Result<PathBuf> {
    if !path.is_absolute() {
        return Err(io::Error::other("data directory must be absolute"));
    }
    Ok(path)
}
fn failure(
    op: StorageOperation,
    stage: StorageStage,
    kind: StorageFailureKind,
    message: impl Into<String>,
) -> StorageFailure {
    StorageFailure {
        operation: op,
        stage,
        kind,
        message: message.into().into(),
        visibility: WriteVisibility::Unchanged,
        pending: None,
        cleanup_errors: Box::default(),
    }
}
fn io_failure(op: StorageOperation, stage: StorageStage, e: io::Error) -> StorageFailure {
    let kind = match e.kind() {
        io::ErrorKind::NotFound => StorageFailureKind::NotFound,
        io::ErrorKind::WouldBlock => StorageFailureKind::Busy,
        io::ErrorKind::FileTooLarge => StorageFailureKind::TooLarge,
        _ => StorageFailureKind::Io,
    };
    failure(op, stage, kind, e.to_string())
}
pub(super) fn decode_failure(op: StorageOperation, e: codec::SaveCodecError) -> StorageFailure {
    let kind = match e.kind() {
        codec::SaveCodecErrorKind::Invalid => StorageFailureKind::Corrupt {
            location: e.location().into(),
        },
        codec::SaveCodecErrorKind::TooLarge => StorageFailureKind::TooLarge,
        codec::SaveCodecErrorKind::FutureVersion { found, supported } => {
            StorageFailureKind::FutureVersion {
                found: *found,
                supported: *supported,
            }
        }
    };
    failure(op, StorageStage::Decode, kind, e.to_string())
}
#[cfg(target_os = "linux")]
mod linux {
    include!("repository_tests.rs");
    use super::super::filesystem::Directory;
    use super::*;
    use std::{collections::BTreeSet, fs::File, io::Write};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(super) enum Point {
        Open,
        Read,
        Encode,
        NewWrite,
        NewFlush,
        NewSync,
        BackupWrite,
        BackupFlush,
        BackupSync,
        BackupPersist,
        BackupDirSync,
        Recheck,
        PrimaryPersist,
        FinalSync,
        Cleanup,
    }
    trait FileOps: Send {
        fn check(&mut self, _point: Point) -> io::Result<()> {
            Ok(())
        }
        fn now(&mut self) -> io::Result<SavedAt> {
            let now = time::OffsetDateTime::now_utc();
            SavedAt::new(now.unix_timestamp(), now.nanosecond()).map_err(io::Error::other)
        }
        fn random(&mut self) -> io::Result<[u8; 16]> {
            let mut bytes = [0; 16];
            getrandom::fill(&mut bytes).map_err(io::Error::other)?;
            Ok(bytes)
        }
    }
    struct RealOps;
    impl FileOps for RealOps {}
    pub struct LocalRepository {
        app: PathBuf,
        ops: Box<dyn FileOps>,
    }
    impl LocalRepository {
        pub fn new(app: PathBuf) -> io::Result<Self> {
            Ok(Self {
                app: absolute(app)?,
                ops: Box::new(RealOps),
            })
        }
        pub fn data_dir(&self) -> &Path {
            &self.app
        }
        /// Validated exact file bytes for the private helper protocol; the slot
        /// stays locked through both bounded reading and codec validation.
        pub fn read_document(
            &mut self,
            id: &SaveId,
            copy: SaveCopy,
            cancel: &CancellationToken,
        ) -> Result<Vec<u8>, StorageFailure> {
            let op = StorageOperation::Load;
            Self::admitted(cancel, op)?;
            let dir = self.directory(false, op)?;
            let _lock = Self::lock(&dir, id, op)?;
            let bytes = dir
                .read(&name(id, copy))
                .map_err(|e| io_failure(op, StorageStage::Read, e))?;
            codec::decode(&bytes, id, copy).map_err(|e| decode_failure(op, e))?;
            Self::admitted(cancel, op)?;
            Ok(bytes)
        }
        fn directory(
            &mut self,
            create: bool,
            op: StorageOperation,
        ) -> Result<Directory, StorageFailure> {
            self.ops
                .check(Point::Open)
                .map_err(|e| io_failure(op, StorageStage::Read, e))?;
            Directory::open(&self.app, create).map_err(|e| io_failure(op, StorageStage::Read, e))
        }
        fn admitted(
            cancel: &CancellationToken,
            op: StorageOperation,
        ) -> Result<(), StorageFailure> {
            if cancel.is_cancelled() {
                Err(StorageFailure::cancelled(op))
            } else {
                Ok(())
            }
        }
        fn lock(
            dir: &Directory,
            id: &SaveId,
            op: StorageOperation,
        ) -> Result<File, StorageFailure> {
            dir.lock(&format!("{}.lock", id.as_str()))
                .map_err(|e| io_failure(op, StorageStage::Lock, e))
        }
        fn read(
            &mut self,
            dir: &Directory,
            id: &SaveId,
            copy: SaveCopy,
            op: StorageOperation,
        ) -> Result<StoredGame, StorageFailure> {
            self.ops
                .check(Point::Read)
                .map_err(|e| io_failure(op, StorageStage::Read, e))?;
            let bytes = dir
                .read(&name(id, copy))
                .map_err(|e| io_failure(op, StorageStage::Read, e))?;
            codec::decode(&bytes, id, copy).map_err(|e| decode_failure(op, e))
        }
        /// Reserve and encode exactly once, before a mutating helper is dispatched.
        pub fn prepare_create(
            &mut self,
            snapshot: &SaveSnapshot,
            cancel: &CancellationToken,
        ) -> Result<PendingWrite, StorageFailure> {
            let op = StorageOperation::Create;
            Self::admitted(cancel, op)?;
            let dir = self.directory(true, op)?;
            for _ in 0..8 {
                let random = self
                    .ops
                    .random()
                    .map_err(|e| io_failure(op, StorageStage::Prepare, e))?;
                let suffix = random
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>();
                let id = SaveId::new(format!(
                    "{}-{suffix}",
                    slug(snapshot.game().world().outline().title().as_str())
                ))
                .expect("generated grammar");
                let _lock = Self::lock(&dir, &id, op)?;
                if occupied(&dir, &id).map_err(|e| io_failure(op, StorageStage::Prepare, e))? {
                    continue;
                }
                return self.prepare(snapshot, id, None, SaveRevision::new(1).unwrap(), op);
            }
            Err(failure(
                op,
                StorageStage::Prepare,
                StorageFailureKind::Conflict,
                "save ID collisions exhausted eight attempts",
            ))
        }
        pub fn prepare_replace(
            &mut self,
            target: &SaveTarget,
            snapshot: &SaveSnapshot,
            cancel: &CancellationToken,
        ) -> Result<PendingWrite, StorageFailure> {
            let op = StorageOperation::Replace;
            Self::admitted(cancel, op)?;
            let dir = self.directory(false, op)?;
            let _lock = Self::lock(&dir, &target.id, op)?;
            let old = self
                .read(&dir, &target.id, SaveCopy::Primary, op)
                .map_err(|mut e| {
                    if e.kind == StorageFailureKind::NotFound {
                        e.kind = StorageFailureKind::Conflict;
                    }
                    e
                })?;
            if old.stamp != target.expected_stamp {
                return Err(conflict(op));
            }
            let revision = old.metadata.revision.next().map_err(|e| {
                failure(
                    op,
                    StorageStage::Prepare,
                    StorageFailureKind::Conflict,
                    e.to_string(),
                )
            })?;
            self.prepare(snapshot, target.id.clone(), Some(old.stamp), revision, op)
        }
        fn prepare(
            &mut self,
            snapshot: &SaveSnapshot,
            id: SaveId,
            previous: Option<ContentStamp>,
            revision: SaveRevision,
            op: StorageOperation,
        ) -> Result<PendingWrite, StorageFailure> {
            self.ops
                .check(Point::Encode)
                .map_err(|e| io_failure(op, StorageStage::Prepare, e))?;
            let metadata = SaveMetadata {
                id: id.clone(),
                revision,
                saved_at: self
                    .ops
                    .now()
                    .map_err(|e| io_failure(op, StorageStage::Prepare, e))?,
            };
            let bytes = codec::encode(snapshot, &metadata).map_err(|e| decode_failure(op, e))?;
            PendingWrite::new(id, previous, codec::stamp(&bytes), bytes).map_err(|e| {
                failure(
                    op,
                    StorageStage::Prepare,
                    StorageFailureKind::TooLarge,
                    e.to_string(),
                )
            })
        }
        /// Apply a prepared write, or acknowledge matching bytes without rotation.
        pub fn execute_pending(
            &mut self,
            attempt: PendingWrite,
            cancel: &CancellationToken,
            op: StorageOperation,
        ) -> Result<SaveReceipt, StorageFailure> {
            let mut visibility = WriteVisibility::Unchanged;
            let result = self.apply(&attempt, cancel, op, &mut visibility);
            result.map_err(|mut e| {
                e.visibility = visibility;
                e.pending = Some(Box::new(attempt));
                e
            })
        }
        fn apply(
            &mut self,
            attempt: &PendingWrite,
            cancel: &CancellationToken,
            op: StorageOperation,
            visibility: &mut WriteVisibility,
        ) -> Result<SaveReceipt, StorageFailure> {
            Self::admitted(cancel, op)?;
            if codec::stamp(attempt.bytes()) != attempt.intended_stamp() {
                return Err(conflict(op));
            }
            let intended = codec::decode(attempt.bytes(), attempt.target(), SaveCopy::Primary)
                .map_err(|e| decode_failure(op, e))?;
            let dir = self.directory(attempt.previous_stamp().is_none(), op)?;
            let _lock = Self::lock(&dir, attempt.target(), op)?;
            sweep_orphaned_temps(&dir, attempt.target());
            let primary = name(attempt.target(), SaveCopy::Primary);
            let backup = name(attempt.target(), SaveCopy::Backup);
            dir.checked_optional(&backup)
                .map_err(|e| io_failure(op, StorageStage::Backup, e))?;
            let existing = match dir.read(&primary) {
                Ok(bytes) => Some(bytes),
                Err(e) if e.kind() == io::ErrorKind::NotFound => None,
                Err(e) => return Err(io_failure(op, StorageStage::Read, e)),
            };
            if existing.as_deref() == Some(attempt.bytes()) {
                *visibility = WriteVisibility::Replaced {
                    stamp: attempt.intended_stamp(),
                };
                self.ops
                    .check(Point::NewSync)
                    .and_then(|()| dir.open_file(&primary, false)?.sync_all())
                    .map_err(|e| io_failure(op, StorageStage::Sync, e))?;
                self.ops
                    .check(Point::FinalSync)
                    .and_then(|()| dir.sync())
                    .map_err(|e| io_failure(op, StorageStage::Sync, e))?;
                return Ok(SaveReceipt {
                    metadata: intended.metadata,
                    stamp: intended.stamp,
                });
            }
            let old = match (attempt.previous_stamp(), existing) {
                (Some(stamp), Some(bytes)) if codec::stamp(&bytes) == stamp => {
                    let checked = codec::decode(&bytes, attempt.target(), SaveCopy::Primary)
                        .map_err(|e| decode_failure(op, e))?;
                    if checked.metadata.revision.next().ok() != Some(intended.metadata.revision) {
                        return Err(conflict(op));
                    }
                    Some(bytes)
                }
                (None, None)
                    if !occupied(&dir, attempt.target())
                        .map_err(|e| io_failure(op, StorageStage::Prepare, e))? =>
                {
                    if intended.metadata.revision.get() != 1 {
                        return Err(conflict(op));
                    }
                    None
                }
                _ => return Err(conflict(op)),
            };
            let mut new = self.temp(&dir, attempt.target(), attempt.bytes(), false, op)?;
            let write_result = (|| {
                Self::admitted(cancel, op)?;
                if let Some(old) = &old {
                    let mut tmp = self.temp(&dir, attempt.target(), old, true, op)?;
                    let result = (|| {
                        self.ops
                            .check(Point::BackupPersist)
                            .map_err(|e| io_failure(op, StorageStage::Backup, e))?;
                        persist(&mut tmp, &dir.path.join(&backup), false)
                            .map_err(|e| io_failure(op, StorageStage::Backup, e))?;
                        self.ops
                            .check(Point::BackupDirSync)
                            .and_then(|()| dir.sync())
                            .map_err(|e| io_failure(op, StorageStage::Sync, e))
                    })();
                    let cleanup = close(tmp);
                    combine_cleanup(result, cleanup, op)?;
                }
                Self::admitted(cancel, op)?;
                self.ops
                    .check(Point::Recheck)
                    .map_err(|e| io_failure(op, StorageStage::Read, e))?;
                dir.checked_optional(&backup)
                    .map_err(|e| io_failure(op, StorageStage::Backup, e))?;
                match &old {
                    Some(bytes) => {
                        if dir
                            .read(&primary)
                            .map_err(|e| io_failure(op, StorageStage::Read, e))?
                            != *bytes
                        {
                            return Err(conflict(op));
                        }
                    }
                    None => {
                        if occupied(&dir, attempt.target())
                            .map_err(|e| io_failure(op, StorageStage::Read, e))?
                        {
                            return Err(conflict(op));
                        }
                    }
                }
                self.ops
                    .check(Point::PrimaryPersist)
                    .map_err(|e| io_failure(op, StorageStage::Replace, e))?;
                persist(&mut new, &dir.path.join(&primary), old.is_none())
                    .map_err(|e| io_failure(op, StorageStage::Replace, e))?;
                *visibility = WriteVisibility::Replaced {
                    stamp: attempt.intended_stamp(),
                };
                self.ops
                    .check(Point::FinalSync)
                    .and_then(|()| dir.sync())
                    .map_err(|e| io_failure(op, StorageStage::Sync, e))?;
                Ok(SaveReceipt {
                    metadata: intended.metadata,
                    stamp: intended.stamp,
                })
            })();
            let injected_cleanup = self.ops.check(Point::Cleanup);
            let actual_cleanup = close(new);
            let cleanup = injected_cleanup.and(actual_cleanup);
            combine_cleanup(write_result, cleanup, op)
        }
        fn temp(
            &mut self,
            dir: &Directory,
            id: &SaveId,
            bytes: &[u8],
            backup: bool,
            op: StorageOperation,
        ) -> Result<Option<tempfile::NamedTempFile>, StorageFailure> {
            let mut temp = Some(
                tempfile::Builder::new()
                    .prefix(&temp_prefix(id))
                    .permissions(std::fs::Permissions::from_mode(0o600))
                    .tempfile_in(&dir.path)
                    .map_err(|e| io_failure(op, StorageStage::Write, e))?,
            );
            let result = (|| {
                let file = temp.as_mut().unwrap();
                self.ops
                    .check(if backup {
                        Point::BackupWrite
                    } else {
                        Point::NewWrite
                    })
                    .and_then(|()| file.write_all(bytes))
                    .map_err(|e| io_failure(op, StorageStage::Write, e))?;
                self.ops
                    .check(if backup {
                        Point::BackupFlush
                    } else {
                        Point::NewFlush
                    })
                    .and_then(|()| file.flush())
                    .map_err(|e| io_failure(op, StorageStage::Write, e))?;
                self.ops
                    .check(if backup {
                        Point::BackupSync
                    } else {
                        Point::NewSync
                    })
                    .and_then(|()| file.as_file().sync_all())
                    .map_err(|e| io_failure(op, StorageStage::Sync, e))
            })();
            if let Err(error) = result {
                return combine_cleanup(Err(error), close(temp), op);
            }
            Ok(temp)
        }
    }
    use std::os::unix::fs::PermissionsExt;
    fn close(temp: Option<tempfile::NamedTempFile>) -> io::Result<()> {
        match temp {
            Some(t) => t.close(),
            None => Ok(()),
        }
    }
    fn persist(
        temp: &mut Option<tempfile::NamedTempFile>,
        path: &Path,
        noclobber: bool,
    ) -> io::Result<()> {
        persist_with(temp, path, noclobber, rename_noreplace)
    }
    fn rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
        use rustix::fs::{CWD, RenameFlags, renameat_with};
        Ok(renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE)?)
    }
    /// A save is always published by one rename, so it keeps exactly one link.
    /// `tempfile`'s `persist_noclobber` falls back to hard-link-then-unlink when
    /// the filesystem rejects `RENAME_NOREPLACE`, ignoring unlink errors; a crash
    /// or failed unlink there would leave a two-link primary that `open_file`
    /// rejects. Callers re-check slot occupancy under the slot lock immediately
    /// before publishing, so an unsupported no-replace rename falls back to a
    /// plain atomic rename instead.
    fn persist_with(
        temp: &mut Option<tempfile::NamedTempFile>,
        path: &Path,
        noclobber: bool,
        noreplace: fn(&Path, &Path) -> io::Result<()>,
    ) -> io::Result<()> {
        let file = temp.take().expect("owned temp");
        if noclobber {
            match noreplace(file.path(), path) {
                Ok(()) => {
                    // The rename already moved the file; only forget the old name.
                    file.into_temp_path().keep().map_err(|e| e.error)?;
                    return Ok(());
                }
                Err(e) if !unsupported_noreplace(&e) => {
                    *temp = Some(file);
                    return Err(e);
                }
                Err(_) => {}
            }
        }
        match file.persist(path) {
            Ok(_) => Ok(()),
            Err(e) => {
                *temp = Some(e.file);
                Err(e.error)
            }
        }
    }
    fn unsupported_noreplace(error: &io::Error) -> bool {
        use rustix::io::Errno;
        [Errno::INVAL, Errno::NOSYS, Errno::OPNOTSUPP]
            .iter()
            .any(|errno| error.raw_os_error() == Some(errno.raw_os_error()))
    }
    fn combine_cleanup<T>(
        result: Result<T, StorageFailure>,
        cleanup: io::Result<()>,
        op: StorageOperation,
    ) -> Result<T, StorageFailure> {
        match cleanup {
            Ok(()) => result,
            Err(e) => {
                let mut failure = match result {
                    Err(e) => e,
                    Ok(_) => io_failure(
                        op,
                        StorageStage::Cleanup,
                        io::Error::other("save cleanup failed"),
                    ),
                };
                let mut errors = failure.cleanup_errors.into_vec();
                errors.push(e.to_string());
                failure.cleanup_errors = errors.into();
                Err(failure)
            }
        }
    }
    fn name(id: &SaveId, copy: SaveCopy) -> String {
        format!(
            "{}.json{}",
            id.as_str(),
            if copy == SaveCopy::Backup { ".bak" } else { "" }
        )
    }
    /// Temps carry their slot ID (IDs never contain `.`), so a slot's leftovers
    /// can be told apart from another slot's live writes.
    fn temp_prefix(id: &SaveId) -> String {
        format!(".cyoa-{}.", id.as_str())
    }
    /// Removes this slot's temps left behind by a killed helper. Only the
    /// holder of the slot lock writes the slot's temps, so under the lock every
    /// match is an orphan. Best effort: leftovers are inert, unreferenced
    /// files and must never block a save.
    fn sweep_orphaned_temps(dir: &Directory, id: &SaveId) {
        let prefix = temp_prefix(id);
        let Ok(entries) = dir.names() else { return };
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str()
                && name.starts_with(&prefix)
            {
                let _ = dir.remove(name);
            }
        }
    }
    fn occupied(dir: &Directory, id: &SaveId) -> io::Result<bool> {
        for suffix in [".json", ".json.bak", ".assets"] {
            if dir.exists(&format!("{}{suffix}", id.as_str()))? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn conflict(op: StorageOperation) -> StorageFailure {
        failure(
            op,
            StorageStage::Read,
            StorageFailureKind::Conflict,
            "save changed or slot is occupied; create a save copy",
        )
    }
    fn slug(title: &str) -> String {
        let mut result = String::new();
        for c in title.chars() {
            if c.is_ascii_alphanumeric() {
                if result.len() == 40 {
                    break;
                }
                result.push(c.to_ascii_lowercase());
            } else if !result.is_empty() && !result.ends_with('-') && result.len() < 40 {
                result.push('-');
            }
        }
        let result = result.trim_end_matches('-');
        if result.is_empty() {
            "story".into()
        } else {
            result.into()
        }
    }
    impl GameRepository for LocalRepository {
        fn create(
            &mut self,
            snapshot: SaveSnapshot,
            prepared: &PreparedWriteEvidence,
            cancel: &CancellationToken,
        ) -> Result<SaveReceipt, StorageFailure> {
            let pending = self.prepare_create(&snapshot, cancel)?;
            prepared.publish(pending.clone());
            self.execute_pending(pending, cancel, StorageOperation::Create)
        }
        fn replace(
            &mut self,
            target: SaveTarget,
            snapshot: SaveSnapshot,
            prepared: &PreparedWriteEvidence,
            cancel: &CancellationToken,
        ) -> Result<SaveReceipt, StorageFailure> {
            let pending = self.prepare_replace(&target, &snapshot, cancel)?;
            prepared.publish(pending.clone());
            self.execute_pending(pending, cancel, StorageOperation::Replace)
        }
        fn reconcile(
            &mut self,
            attempt: PendingWrite,
            prepared: &PreparedWriteEvidence,
            cancel: &CancellationToken,
        ) -> Result<SaveReceipt, StorageFailure> {
            prepared.publish(attempt.clone());
            self.execute_pending(attempt, cancel, StorageOperation::Reconcile)
        }
        fn load(
            &mut self,
            id: &SaveId,
            copy: SaveCopy,
            cancel: &CancellationToken,
        ) -> Result<StoredGame, StorageFailure> {
            let op = StorageOperation::Load;
            Self::admitted(cancel, op)?;
            let dir = self.directory(false, op)?;
            let _lock = Self::lock(&dir, id, op)?;
            let stored = self.read(&dir, id, copy, op)?;
            Self::admitted(cancel, op)?;
            Ok(stored)
        }
        fn list(
            &mut self,
            page: SavePage,
            cancel: &CancellationToken,
        ) -> Result<SavePageResult, StorageFailure> {
            let op = StorageOperation::List;
            Self::admitted(cancel, op)?;
            let dir = match self.directory(false, op) {
                Ok(d) => d,
                Err(e) if e.kind == StorageFailureKind::NotFound => {
                    return Ok(SavePageResult {
                        entries: vec![],
                        next: None,
                    });
                }
                Err(e) => return Err(e),
            };
            let mut ids = BTreeSet::new();
            for entry in dir
                .names()
                .map_err(|e| io_failure(op, StorageStage::Read, e))?
            {
                Self::admitted(cancel, op)?;
                let entry = entry.map_err(|e| io_failure(op, StorageStage::Read, e))?;
                let fname = entry.file_name();
                let Some(fname) = fname.to_str() else {
                    continue;
                };
                let Some(raw) = fname
                    .strip_suffix(".json")
                    .or_else(|| fname.strip_suffix(".json.bak"))
                else {
                    continue;
                };
                if let Ok(id) = SaveId::new(raw)
                    && page.after().is_none_or(|after| id > *after)
                {
                    ids.insert(id);
                    if ids.len() > page.limit() as usize + 1 {
                        ids.pop_last();
                    }
                }
            }
            let more = ids.len() > page.limit() as usize;
            if more {
                ids.pop_last();
            }
            let mut entries = vec![];
            for id in ids {
                Self::admitted(cancel, op)?;
                let backup_available = dir.open_file(&name(&id, SaveCopy::Backup), false).is_ok();
                let status = match Self::lock(&dir, &id, op) {
                    Err(e) if e.kind == StorageFailureKind::Busy => SaveListingStatus::Busy,
                    Err(_) => SaveListingStatus::Unreadable,
                    Ok(_lock) => match self.read(&dir, &id, SaveCopy::Primary, op) {
                        Ok(g) => SaveListingStatus::Valid {
                            title: g.snapshot.game().world().outline().title().as_str().into(),
                            saved_at: g.metadata.saved_at,
                            turn_count: g.snapshot.game().turns().len(),
                            source: g.snapshot.source(),
                        },
                        Err(e) => match e.kind {
                            StorageFailureKind::FutureVersion { found, .. } => {
                                SaveListingStatus::FutureVersion { version: found }
                            }
                            StorageFailureKind::Corrupt { .. } | StorageFailureKind::TooLarge => {
                                SaveListingStatus::Corrupt
                            }
                            StorageFailureKind::NotFound if backup_available => {
                                SaveListingStatus::BackupOnly
                            }
                            _ => SaveListingStatus::Unreadable,
                        },
                    },
                };
                entries.push(SaveListing {
                    id,
                    status,
                    backup_available,
                });
            }
            let next = if more {
                entries.last().map(|e| e.id.clone())
            } else {
                None
            };
            Ok(SavePageResult { entries, next })
        }
    }
}
#[cfg(target_os = "linux")]
pub use linux::LocalRepository;

#[cfg(not(target_os = "linux"))]
pub struct LocalRepository;
#[cfg(not(target_os = "linux"))]
impl LocalRepository {
    pub fn new(app: PathBuf) -> io::Result<Self> {
        absolute(app)?;
        Ok(Self)
    }
}
#[cfg(not(target_os = "linux"))]
impl GameRepository for LocalRepository {
    fn create(
        &mut self,
        _: SaveSnapshot,
        _: &PreparedWriteEvidence,
        _: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure> {
        Err(unsupported(StorageOperation::Create))
    }
    fn replace(
        &mut self,
        _: SaveTarget,
        _: SaveSnapshot,
        _: &PreparedWriteEvidence,
        _: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure> {
        Err(unsupported(StorageOperation::Replace))
    }
    fn reconcile(
        &mut self,
        _: PendingWrite,
        _: &PreparedWriteEvidence,
        _: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure> {
        Err(unsupported(StorageOperation::Reconcile))
    }
    fn load(
        &mut self,
        _: &SaveId,
        _: SaveCopy,
        _: &CancellationToken,
    ) -> Result<StoredGame, StorageFailure> {
        Err(unsupported(StorageOperation::Load))
    }
    fn list(
        &mut self,
        _: SavePage,
        _: &CancellationToken,
    ) -> Result<SavePageResult, StorageFailure> {
        Err(unsupported(StorageOperation::List))
    }
}
#[cfg(not(target_os = "linux"))]
fn unsupported(op: StorageOperation) -> StorageFailure {
    failure(
        op,
        StorageStage::Admission,
        StorageFailureKind::Unsupported,
        "local save storage is supported on Linux only",
    )
}
