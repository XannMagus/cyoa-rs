//! One bounded, isolated storage child per operation. No vendor/auth dependencies.
use super::{codec, repository::LocalRepository};
use crate::backends::process::{
    self, EnvPolicy, MaxStderrBytes, MaxStdoutBytes, ProcessBounds, ProcessSpec, RequestWorkspace,
    SupervisorError,
};
use cyoa_application::{
    cancellation::{CancellationSource, CancellationToken},
    persistence::*,
};
use cyoa_core::text::WorldTitle;
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Read, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

pub const INTERNAL_HELPER_ARG: &str = "--cyoa-storage-helper-v1";
pub const MAX_HELPER_BYTES: usize = 160 * 1024 * 1024;
#[derive(Clone)]
pub struct HelperConfig {
    program: PathBuf,
    data: PathBuf,
    bounds: ProcessBounds,
}
impl HelperConfig {
    pub fn new(program: PathBuf, data: PathBuf) -> io::Result<Self> {
        if !program.is_absolute() || !data.is_absolute() {
            return Err(io::Error::other(
                "helper executable and data directory must be absolute",
            ));
        }
        Ok(Self {
            program,
            data,
            bounds: ProcessBounds::new(
                Duration::from_secs(10),
                MaxStdoutBytes::new(MAX_HELPER_BYTES).unwrap(),
                MaxStderrBytes::new(65536).unwrap(),
            )
            .unwrap(),
        })
    }
    pub fn with_bounds(mut self, bounds: ProcessBounds) -> Self {
        self.bounds = bounds;
        self
    }
}
pub struct SupervisedRepository {
    config: HelperConfig,
    preparation: Box<dyn Preparation>,
}
trait Preparation: Send {
    fn now(&mut self) -> io::Result<SavedAt>;
    fn random(&mut self) -> io::Result<[u8; 16]>;
}
struct SystemPreparation;
impl Preparation for SystemPreparation {
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
impl SupervisedRepository {
    pub fn new(config: HelperConfig) -> Self {
        Self {
            config,
            preparation: Box::new(SystemPreparation),
        }
    }
}

macro_rules! mapped_enum{
    ($wire:ident,$app:ident,[$($v:ident),+])=>{
        #[derive(Serialize,Deserialize)]enum $wire{$($v),+}
        impl From<$app> for $wire{fn from(value:$app)->Self{match value{$($app::$v=>Self::$v),+}}}
        impl From<$wire> for $app{fn from(value:$wire)->Self{match value{$($wire::$v=>Self::$v),+}}}
    };
}
mapped_enum!(
    StageWire,
    StorageStage,
    [
        Admission, Read, Decode, Prepare, Lock, Write, Backup, Replace, Sync, Cleanup, Worker
    ]
);
mapped_enum!(
    OperationWire,
    StorageOperation,
    [Create, Replace, Reconcile, Load, List, Inspect]
);
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum KindWire {
    NotFound,
    Busy,
    Conflict,
    Corrupt { location: String },
    FutureVersion { found: u32, supported: u32 },
    Unsupported,
    TooLarge,
    Io,
    Cancelled,
    Timeout,
    WorkerFault,
}
impl From<StorageFailureKind> for KindWire {
    fn from(v: StorageFailureKind) -> Self {
        match v {
            StorageFailureKind::NotFound => Self::NotFound,
            StorageFailureKind::Busy => Self::Busy,
            StorageFailureKind::Conflict => Self::Conflict,
            StorageFailureKind::Corrupt { location } => Self::Corrupt { location },
            StorageFailureKind::FutureVersion { found, supported } => Self::FutureVersion {
                found: found.get(),
                supported: supported.get(),
            },
            StorageFailureKind::Unsupported => Self::Unsupported,
            StorageFailureKind::TooLarge => Self::TooLarge,
            StorageFailureKind::Io => Self::Io,
            StorageFailureKind::Cancelled => Self::Cancelled,
            StorageFailureKind::Timeout => Self::Timeout,
            StorageFailureKind::WorkerFault => Self::WorkerFault,
        }
    }
}
impl From<KindWire> for StorageFailureKind {
    fn from(v: KindWire) -> Self {
        match v {
            KindWire::NotFound => Self::NotFound,
            KindWire::Busy => Self::Busy,
            KindWire::Conflict => Self::Conflict,
            KindWire::Corrupt { location } => Self::Corrupt { location },
            // A zero version breaks the helper protocol: report it as the helper's fault.
            KindWire::FutureVersion { found, supported } => {
                match (
                    SaveFormatVersion::new(found),
                    SaveFormatVersion::new(supported),
                ) {
                    (Ok(found), Ok(supported)) => Self::FutureVersion { found, supported },
                    _ => Self::WorkerFault,
                }
            }
            KindWire::Unsupported => Self::Unsupported,
            KindWire::TooLarge => Self::TooLarge,
            KindWire::Io => Self::Io,
            KindWire::Cancelled => Self::Cancelled,
            KindWire::Timeout => Self::Timeout,
            KindWire::WorkerFault => Self::WorkerFault,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum VisibilityWire {
    Unchanged,
    Replaced { stamp: [u8; 32] },
    Unknown,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FailureWire {
    operation: OperationWire,
    stage: StageWire,
    kind: KindWire,
    message: String,
    visibility: VisibilityWire,
    cleanup_errors: Vec<String>,
}
impl From<StorageFailure> for FailureWire {
    fn from(v: StorageFailure) -> Self {
        Self {
            operation: v.operation.into(),
            stage: v.stage.into(),
            kind: v.kind.into(),
            message: v.message.into(),
            visibility: match v.visibility {
                WriteVisibility::Unchanged => VisibilityWire::Unchanged,
                WriteVisibility::Replaced { stamp } => VisibilityWire::Replaced {
                    stamp: *stamp.bytes(),
                },
                WriteVisibility::Unknown => VisibilityWire::Unknown,
            },
            cleanup_errors: v.cleanup_errors.into_vec(),
        }
    }
}
impl From<FailureWire> for StorageFailure {
    fn from(v: FailureWire) -> Self {
        Self {
            operation: v.operation.into(),
            stage: v.stage.into(),
            kind: v.kind.into(),
            message: v.message.into(),
            visibility: match v.visibility {
                VisibilityWire::Unchanged => WriteVisibility::Unchanged,
                VisibilityWire::Replaced { stamp } => WriteVisibility::Replaced {
                    stamp: ContentStamp::new(stamp),
                },
                VisibilityWire::Unknown => WriteVisibility::Unknown,
            },
            pending: None,
            cleanup_errors: v.cleanup_errors.into(),
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u32,
    data: PathBuf,
    intent: IntentWire,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum IntentWire {
    Read {
        id: String,
        backup: bool,
    },
    List {
        after: Option<String>,
        limit: u8,
    },
    Apply {
        id: String,
        previous: Option<[u8; 32]>,
        stamp: [u8; 32],
        document: String,
        operation: OperationWire,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    version: u32,
    result: Result<OutputWire, FailureWire>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum OutputWire {
    Document(String),
    Receipt {
        stamp: [u8; 32],
        revision: u64,
        seconds: i64,
        nanos: u32,
    },
    Page {
        entries: Vec<RowWire>,
        next: Option<String>,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RowWire {
    id: String,
    status: ListingWire,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum ListingWire {
    Inspected { primary: PrimaryWire, backup: bool },
    Busy,
    Unreadable,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum PrimaryWire {
    Valid {
        title: String,
        seconds: i64,
        nanos: u32,
        count: usize,
        demo: bool,
    },
    Missing,
    Corrupt,
    FutureVersion {
        version: u32,
    },
    Unreadable,
}
impl From<SaveListing> for RowWire {
    fn from(v: SaveListing) -> Self {
        Self {
            id: v.id.as_str().into(),
            status: match v.status {
                SaveListingStatus::Inspected { primary, backup } => ListingWire::Inspected {
                    backup: backup == BackupCopy::Present,
                    primary: match primary {
                        PrimaryCopy::Valid(summary) => PrimaryWire::Valid {
                            title: summary.title.as_str().into(),
                            seconds: summary.saved_at.unix_seconds(),
                            nanos: summary.saved_at.nanoseconds(),
                            count: summary.turns.get(),
                            demo: matches!(summary.source, StorySource::Demo { .. }),
                        },
                        PrimaryCopy::Missing => PrimaryWire::Missing,
                        PrimaryCopy::Corrupt => PrimaryWire::Corrupt,
                        PrimaryCopy::FutureVersion { version } => PrimaryWire::FutureVersion {
                            version: version.get(),
                        },
                        PrimaryCopy::Unreadable => PrimaryWire::Unreadable,
                    },
                },
                SaveListingStatus::Busy => ListingWire::Busy,
                SaveListingStatus::Unreadable => ListingWire::Unreadable,
            },
        }
    }
}
fn boundary(op: StorageOperation, message: impl Into<String>) -> StorageFailure {
    StorageFailure {
        operation: op,
        stage: StorageStage::Worker,
        kind: StorageFailureKind::WorkerFault,
        message: message.into().into(),
        visibility: WriteVisibility::Unchanged,
        pending: None,
        cleanup_errors: Box::default(),
    }
}
fn preparation_failure(op: StorageOperation, error: io::Error) -> StorageFailure {
    let mut failure = boundary(op, error.to_string());
    failure.stage = StorageStage::Prepare;
    failure.kind = StorageFailureKind::Io;
    failure
}
fn check(cancel: &CancellationToken, op: StorageOperation) -> Result<(), StorageFailure> {
    if cancel.is_cancelled() {
        Err(StorageFailure::cancelled(op))
    } else {
        Ok(())
    }
}
fn transport_kind(error: &SupervisorError) -> StorageFailureKind {
    match error {
        SupervisorError::Cancelled { .. } => StorageFailureKind::Cancelled,
        SupervisorError::Timeout { .. } => StorageFailureKind::Timeout,
        SupervisorError::Unsupported => StorageFailureKind::Unsupported,
        SupervisorError::OutputBoundExceeded { .. } => StorageFailureKind::TooLarge,
        SupervisorError::Cleanup {
            initial: Some(initial),
            ..
        } => transport_kind(initial),
        _ => StorageFailureKind::WorkerFault,
    }
}
fn launched(error: &SupervisorError) -> bool {
    match error {
        SupervisorError::Spawn(_) | SupervisorError::Unsupported => false,
        SupervisorError::Cleanup {
            initial: Some(initial),
            ..
        } => launched(initial),
        _ => true,
    }
}
fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> io::Result<T> {
    if bytes.len() > MAX_HELPER_BYTES {
        return Err(io::Error::other("helper document exceeds 160 MiB"));
    }
    let value = crate::json::strict_value(bytes).map_err(io::Error::other)?;
    serde_json::from_value(value).map_err(io::Error::other)
}
struct Limited {
    bytes: Vec<u8>,
}
impl Write for Limited {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.bytes.len().saturating_add(b.len()) > MAX_HELPER_BYTES {
            return Err(io::Error::other("helper document exceeds 160 MiB"));
        }
        self.bytes.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn encode<T: Serialize>(value: &T) -> io::Result<Vec<u8>> {
    let mut writer = Limited { bytes: vec![] };
    serde_json::to_writer(&mut writer, value).map_err(io::Error::other)?;
    writer.write_all(b"\n")?;
    Ok(writer.bytes)
}

/// Invoked before Clap or vendor resolution. Only this child blocks on disk.
pub fn run_internal(input: impl Read, mut output: impl Write) -> io::Result<()> {
    let mut bytes = vec![];
    input
        .take(MAX_HELPER_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    let request: Request = parse(&bytes)?;
    if request.version != 1 {
        return Err(io::Error::other("unsupported storage helper protocol"));
    }
    let reply = Reply {
        version: 1,
        result: execute(request).map_err(Into::into),
    };
    output.write_all(&encode(&reply)?)?;
    output.flush()
}
#[cfg(target_os = "linux")]
fn execute(request: Request) -> Result<OutputWire, StorageFailure> {
    let token = CancellationSource::default().token();
    let mut repo = LocalRepository::new(request.data)
        .map_err(|e| boundary(StorageOperation::Load, e.to_string()))?;
    match request.intent {
        IntentWire::Read { id, backup } => {
            let id =
                SaveId::new(id).map_err(|e| boundary(StorageOperation::Load, e.to_string()))?;
            let bytes = repo.read_document(
                &id,
                if backup {
                    SaveCopy::Backup
                } else {
                    SaveCopy::Primary
                },
                &token,
            )?;
            Ok(OutputWire::Document(String::from_utf8(bytes).map_err(
                |e| boundary(StorageOperation::Load, e.to_string()),
            )?))
        }
        IntentWire::List { after, limit } => {
            let after = after
                .map(SaveId::new)
                .transpose()
                .map_err(|e| boundary(StorageOperation::List, e.to_string()))?;
            let page = PageSize::new(limit)
                .map(|size| SavePage::new(after, size))
                .map_err(|e| boundary(StorageOperation::List, e.to_string()))?;
            let result = repo.list(page, &token)?;
            Ok(OutputWire::Page {
                entries: result.entries.into_iter().map(Into::into).collect(),
                next: result.next.map(|id| id.as_str().into()),
            })
        }
        IntentWire::Apply {
            id,
            previous,
            stamp,
            document,
            operation,
        } => {
            let operation = operation.into();
            if !matches!(
                operation,
                StorageOperation::Create | StorageOperation::Replace | StorageOperation::Reconcile
            ) {
                return Err(boundary(operation, "invalid write operation"));
            }
            let id = SaveId::new(id).map_err(|e| boundary(operation, e.to_string()))?;
            let pending = PendingWrite::new(
                id,
                previous.map(ContentStamp::new),
                ContentStamp::new(stamp),
                document.into_bytes(),
            )
            .map_err(|e| boundary(operation, e.to_string()))?;
            let receipt = repo.execute_pending(pending, &token, operation)?;
            Ok(OutputWire::Receipt {
                stamp: *receipt.stamp.bytes(),
                revision: receipt.metadata.revision.get(),
                seconds: receipt.metadata.saved_at.unix_seconds(),
                nanos: receipt.metadata.saved_at.nanoseconds(),
            })
        }
    }
}
#[cfg(not(target_os = "linux"))]
fn execute(_: Request) -> Result<OutputWire, StorageFailure> {
    let mut e = boundary(
        StorageOperation::Load,
        "storage helper supported on Linux only",
    );
    e.kind = StorageFailureKind::Unsupported;
    Err(e)
}

impl SupervisedRepository {
    fn call(
        &self,
        intent: IntentWire,
        op: StorageOperation,
        pending: Option<&PendingWrite>,
        cancel: &CancellationToken,
        started: Instant,
    ) -> Result<OutputWire, StorageFailure> {
        check(cancel, op)?;
        let request = encode(&Request {
            version: 1,
            data: self.config.data.clone(),
            intent,
        })
        .map_err(|e| boundary(op, e.to_string()))?;
        let remaining = self
            .config
            .bounds
            .deadline()
            .checked_sub(started.elapsed())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| {
                let mut e = boundary(op, "storage operation deadline expired before dispatch");
                e.kind = StorageFailureKind::Timeout;
                e
            })?;
        let bounds = ProcessBounds::new(
            remaining,
            MaxStdoutBytes::new(self.config.bounds.max_stdout_bytes()).unwrap(),
            MaxStderrBytes::new(self.config.bounds.max_stderr_bytes()).unwrap(),
        )
        .unwrap();
        let workspace = RequestWorkspace::new().map_err(|e| boundary(op, e.to_string()))?;
        let mut records = 0;
        let result = process::run(
            ProcessSpec {
                workspace,
                program: self.config.program.clone(),
                args: vec![INTERNAL_HELPER_ARG.into()],
                env: EnvPolicy::new(),
                stdin: request,
                bounds,
            },
            cancel,
            &mut |_| {
                records += 1;
                if records > 1 {
                    Err("storage helper returned multiple records".into())
                } else {
                    Ok(())
                }
            },
        );
        let mut apply_transport_error = |error: SupervisorError| {
            let unlaunched = !launched(&error);
            let mut failure = boundary(op, error.to_string());
            failure.kind = transport_kind(&error);
            if let SupervisorError::Cleanup { failures, .. } = &error {
                failure.cleanup_errors = failures.iter().map(ToString::to_string).collect();
            }
            if pending.is_some() && !unlaunched {
                failure.visibility = WriteVisibility::Unknown;
            }
            failure
        };
        let outcome = result.map_err(&mut apply_transport_error)?;
        let reply: Reply = parse(outcome.diagnostics.stdout()).map_err(|e| {
            let mut error = boundary(op, e.to_string());
            if pending.is_some() {
                error.visibility = WriteVisibility::Unknown;
            }
            error
        })?;
        if started.elapsed() >= self.config.bounds.deadline() {
            let mut error = boundary(
                op,
                "storage operation deadline expired while decoding reply",
            );
            error.kind = StorageFailureKind::Timeout;
            if pending.is_some() {
                error.visibility = WriteVisibility::Unknown;
            }
            return Err(error);
        }
        if reply.version != 1 {
            let mut error = boundary(op, "unsupported helper reply version");
            if pending.is_some() {
                error.visibility = WriteVisibility::Unknown;
            }
            return Err(error);
        }
        reply.result.map_err(|e| {
            let mut error: StorageFailure = e.into();
            let invalid_visibility = match (&error.visibility, pending) {
                (WriteVisibility::Replaced { stamp }, Some(attempt)) => {
                    *stamp != attempt.intended_stamp()
                }
                (WriteVisibility::Replaced { .. } | WriteVisibility::Unknown, None) => true,
                _ => false,
            };
            if error.operation != op || invalid_visibility {
                error = boundary(op, "helper error operation or visibility mismatch");
                if pending.is_some() {
                    error.visibility = WriteVisibility::Unknown;
                }
            }
            error
        })
    }
    fn prepare(
        &mut self,
        snapshot: &SaveSnapshot,
        id: SaveId,
        previous: Option<ContentStamp>,
        revision: SaveRevision,
        op: StorageOperation,
    ) -> Result<PendingWrite, StorageFailure> {
        let saved_at = self
            .preparation
            .now()
            .map_err(|e| preparation_failure(op, e))?;
        let bytes = codec::encode(
            snapshot,
            &SaveMetadata {
                id: id.clone(),
                revision,
                saved_at,
            },
        )
        .map_err(|e| {
            let mut error = super::repository::decode_failure(op, e);
            error.stage = StorageStage::Prepare;
            error
        })?;
        PendingWrite::new(id, previous, codec::stamp(&bytes), bytes)
            .map_err(|e| boundary(op, e.to_string()))
    }
    fn apply(
        &self,
        pending: PendingWrite,
        prepared: &PreparedWriteEvidence,
        op: StorageOperation,
        cancel: &CancellationToken,
        started: Instant,
    ) -> Result<SaveReceipt, StorageFailure> {
        // The caller has this evidence before any mutating child exists.
        prepared.publish(pending.clone());
        let result = (|| {
            let document = String::from_utf8(pending.bytes().to_vec())
                .map_err(|e| boundary(op, e.to_string()))?;
            let intended = codec::decode(pending.bytes(), pending.target(), SaveCopy::Primary)
                .map_err(|e| boundary(op, e.to_string()))?;
            if intended.stamp != pending.intended_stamp() {
                return Err(boundary(op, "prepared stamp differs from bytes"));
            }
            let output = self.call(
                IntentWire::Apply {
                    id: pending.target().as_str().into(),
                    previous: pending.previous_stamp().map(|s| *s.bytes()),
                    stamp: *pending.intended_stamp().bytes(),
                    document,
                    operation: op.into(),
                },
                op,
                Some(&pending),
                cancel,
                started,
            )?;
            match output {
                OutputWire::Receipt {
                    stamp,
                    revision,
                    seconds,
                    nanos,
                } if stamp == *intended.stamp.bytes()
                    && revision == intended.metadata.revision.get()
                    && seconds == intended.metadata.saved_at.unix_seconds()
                    && nanos == intended.metadata.saved_at.nanoseconds() =>
                {
                    Ok(SaveReceipt {
                        metadata: intended.metadata,
                        stamp: intended.stamp,
                    })
                }
                _ => {
                    let mut e = boundary(op, "helper receipt does not match prepared write");
                    e.visibility = WriteVisibility::Unknown;
                    Err(e)
                }
            }
        })();
        result.map_err(|mut e| {
            e.pending = Some(Box::new(pending));
            e
        })
    }
    fn read(
        &self,
        id: &SaveId,
        copy: SaveCopy,
        cancel: &CancellationToken,
        started: Instant,
    ) -> Result<StoredGame, StorageFailure> {
        let op = StorageOperation::Load;
        match self.call(
            IntentWire::Read {
                id: id.as_str().into(),
                backup: copy == SaveCopy::Backup,
            },
            op,
            None,
            cancel,
            started,
        )? {
            OutputWire::Document(document) => {
                let stored = codec::decode(document.as_bytes(), id, copy)
                    .map_err(|e| super::repository::decode_failure(op, e))?;
                check(cancel, op)?;
                if started.elapsed() >= self.config.bounds.deadline() {
                    let mut error =
                        boundary(op, "storage operation deadline expired while decoding save");
                    error.kind = StorageFailureKind::Timeout;
                    return Err(error);
                }
                Ok(stored)
            }
            _ => Err(boundary(op, "helper returned wrong output kind")),
        }
    }
}
impl GameRepository for SupervisedRepository {
    fn create(
        &mut self,
        snapshot: SaveSnapshot,
        prepared: &PreparedWriteEvidence,
        cancel: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure> {
        let op = StorageOperation::Create;
        let started = Instant::now();
        check(cancel, op)?;
        for _ in 0..8 {
            let random = self
                .preparation
                .random()
                .map_err(|e| preparation_failure(op, e))?;
            let suffix = random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            let title = snapshot.game().world().outline().title().as_str();
            let mut slug = String::new();
            for c in title.chars() {
                if c.is_ascii_alphanumeric() {
                    if slug.len() == 40 {
                        break;
                    }
                    slug.push(c.to_ascii_lowercase());
                } else if !slug.is_empty() && !slug.ends_with('-') && slug.len() < 40 {
                    slug.push('-');
                }
            }
            let slug = slug.trim_end_matches('-');
            let slug = if slug.is_empty() { "story" } else { slug };
            let id = SaveId::new(format!("{slug}-{suffix}")).expect("generated grammar");
            let pending = self.prepare(&snapshot, id, None, SaveRevision::new(1).unwrap(), op)?;
            match self.apply(pending, prepared, op, cancel, started) {
                Err(e)
                    if e.kind == StorageFailureKind::Conflict
                        && e.visibility == WriteVisibility::Unchanged =>
                {
                    continue;
                }
                result => return result,
            }
        }
        Err(boundary(op, "save ID collisions exhausted eight attempts"))
    }
    fn replace(
        &mut self,
        target: SaveTarget,
        snapshot: SaveSnapshot,
        prepared: &PreparedWriteEvidence,
        cancel: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure> {
        let op = StorageOperation::Replace;
        let started = Instant::now();
        check(cancel, op)?;
        let old = self
            .read(&target.id, SaveCopy::Primary, cancel, started)
            .map_err(|mut e| {
                e.operation = op;
                if e.kind == StorageFailureKind::NotFound {
                    e.kind = StorageFailureKind::Conflict;
                }
                e
            })?;
        if old.stamp != target.expected_stamp {
            let mut e = boundary(op, "save changed; create a save copy");
            e.kind = StorageFailureKind::Conflict;
            return Err(e);
        }
        let revision = old.metadata.revision.next().map_err(|e| {
            let mut error = boundary(op, e.to_string());
            error.kind = StorageFailureKind::Conflict;
            error.stage = StorageStage::Prepare;
            error
        })?;
        let pending = self.prepare(&snapshot, target.id, Some(old.stamp), revision, op)?;
        self.apply(pending, prepared, op, cancel, started)
    }
    fn reconcile(
        &mut self,
        pending: PendingWrite,
        prepared: &PreparedWriteEvidence,
        cancel: &CancellationToken,
    ) -> Result<SaveReceipt, StorageFailure> {
        check(cancel, StorageOperation::Reconcile)?;
        self.apply(
            pending,
            prepared,
            StorageOperation::Reconcile,
            cancel,
            Instant::now(),
        )
    }
    fn load(
        &mut self,
        id: &SaveId,
        copy: SaveCopy,
        cancel: &CancellationToken,
    ) -> Result<StoredGame, StorageFailure> {
        self.read(id, copy, cancel, Instant::now())
    }
    fn list(
        &mut self,
        page: SavePage,
        cancel: &CancellationToken,
    ) -> Result<SavePageResult, StorageFailure> {
        let op = StorageOperation::List;
        let after = page.after().cloned();
        let limit = page.size().get();
        let output = self.call(
            IntentWire::List {
                after: after.as_ref().map(|id| id.as_str().into()),
                limit,
            },
            op,
            None,
            cancel,
            Instant::now(),
        )?;
        let OutputWire::Page { entries, next } = output else {
            return Err(boundary(op, "helper returned wrong output kind"));
        };
        if entries.len() > usize::from(limit) {
            return Err(boundary(op, "helper exceeded page size"));
        }
        let mut rows = vec![];
        for row in entries {
            let id = SaveId::new(row.id).map_err(|e| boundary(op, e.to_string()))?;
            if after.as_ref().is_some_and(|after| id <= *after)
                || rows.last().is_some_and(|last: &SaveListing| id <= last.id)
            {
                return Err(boundary(op, "helper listing is unordered"));
            }
            let status = match row.status {
                ListingWire::Inspected { primary, backup } => SaveListingStatus::Inspected {
                    backup: if backup {
                        BackupCopy::Present
                    } else {
                        BackupCopy::Absent
                    },
                    primary: match primary {
                        PrimaryWire::Valid {
                            title,
                            seconds,
                            nanos,
                            count,
                            demo,
                        } => {
                            let at = SavedAt::new(seconds, nanos)
                                .map_err(|e| boundary(op, e.to_string()))?;
                            let title = WorldTitle::new(title)
                                .map_err(|_| boundary(op, "helper listing metadata invalid"))?;
                            let source = if demo {
                                StorySource::Demo {
                                    scenario: DemoScenarioId::HarbourV1,
                                }
                            } else {
                                StorySource::Live
                            };
                            if at.nanoseconds() != nanos || !source.admits(count) {
                                return Err(boundary(op, "helper listing metadata invalid"));
                            }
                            PrimaryCopy::Valid(SaveSummary {
                                title,
                                saved_at: at,
                                turns: SavedTurnCount::new(count),
                                source,
                            })
                        }
                        PrimaryWire::Missing => PrimaryCopy::Missing,
                        PrimaryWire::Corrupt => PrimaryCopy::Corrupt,
                        PrimaryWire::FutureVersion { version } => {
                            match SaveFormatVersion::new(version) {
                                Ok(version) if version > super::migrations::CURRENT => {
                                    PrimaryCopy::FutureVersion { version }
                                }
                                _ => return Err(boundary(op, "invalid future version")),
                            }
                        }
                        PrimaryWire::Unreadable => PrimaryCopy::Unreadable,
                    },
                },
                ListingWire::Busy => SaveListingStatus::Busy,
                ListingWire::Unreadable => SaveListingStatus::Unreadable,
            };
            rows.push(SaveListing { id, status });
        }
        let next = next
            .map(SaveId::new)
            .transpose()
            .map_err(|e| boundary(op, e.to_string()))?;
        if next.is_some()
            && (rows.len() != usize::from(limit) || rows.last().map(|row| &row.id) != next.as_ref())
        {
            return Err(boundary(op, "helper listing cursor invalid"));
        }
        Ok(SavePageResult {
            entries: rows,
            next,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn helper_versions_round_trip_and_a_zero_version_is_a_helper_fault() {
        let found = SaveFormatVersion::new(3).unwrap();
        let supported = super::super::migrations::CURRENT;
        let wire = KindWire::from(StorageFailureKind::FutureVersion { found, supported });
        assert_eq!(
            StorageFailureKind::from(wire),
            StorageFailureKind::FutureVersion { found, supported }
        );
        for (found, supported) in [(0, 1), (2, 0)] {
            assert_eq!(
                StorageFailureKind::from(KindWire::FutureVersion { found, supported }),
                StorageFailureKind::WorkerFault
            );
        }
    }
    #[test]
    fn private_protocol_rejects_duplicate_keys_unknown_fields_versions_and_wrong_shapes_before_disk()
     {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("data");
        let valid =
            serde_json::json!({"version":1,"data":path,"intent":{"List":{"after":null,"limit":1}}});
        for mutation in 0..5 {
            let mut request = valid.clone();
            match mutation {
                0 => request["version"] = 2.into(),
                1 => request["intent"]["List"]["limit"] = 0.into(),
                2 => request["intent"]["List"]["after"] = "../escape".into(),
                3 => request["extra"] = true.into(),
                _ => request["intent"] = true.into(),
            };
            let mut reply = vec![];
            let result = run_internal(serde_json::to_vec(&request).unwrap().as_slice(), &mut reply);
            if result.is_ok() {
                let parsed: Reply = parse(&reply).unwrap();
                assert!(parsed.result.is_err());
            }
            assert!(!path.exists());
        }
        assert!(
            parse::<Reply>(br#"{"version":1,"version":1,"result":{"Ok":{"Document":""}}}"#)
                .is_err()
        );
        assert!(
            parse::<Reply>(br#"{"version":1,"result":{"Ok":{"Document":""}},"extension":true}"#)
                .is_err()
        );
        let mut output = vec![];
        run_internal(serde_json::to_vec(&valid).unwrap().as_slice(), &mut output).unwrap();
        let reply: Reply = parse(&output).unwrap();
        assert!(
            matches!(reply.result,Ok(OutputWire::Page{entries,next:None})if entries.is_empty())
        );
        assert!(!path.exists());
    }
    struct BrokenPreparation {
        entropy: bool,
    }
    impl Preparation for BrokenPreparation {
        fn now(&mut self) -> io::Result<SavedAt> {
            Err(io::Error::other("clock unavailable"))
        }
        fn random(&mut self) -> io::Result<[u8; 16]> {
            if self.entropy {
                Err(io::Error::other("entropy unavailable"))
            } else {
                Ok([0; 16])
            }
        }
    }
    #[test]
    fn preparation_clock_or_entropy_failure_launches_no_helper_and_keeps_visibility_unchanged() {
        let id = SaveId::new("harbour-0123456789abcdef0123456789abcdef").unwrap();
        let snapshot = codec::decode(
            include_bytes!("../../tests/fixtures/saves/v1-full-story.json"),
            &id,
            SaveCopy::Primary,
        )
        .unwrap()
        .snapshot;
        for entropy in [true, false] {
            let root = tempfile::tempdir().unwrap();
            let evidence = PreparedWriteEvidence::default();
            let mut repo = SupervisedRepository::new(
                HelperConfig::new(root.path().join("absent-helper"), root.path().join("data"))
                    .unwrap(),
            );
            repo.preparation = Box::new(BrokenPreparation { entropy });
            let error = repo
                .create(
                    snapshot.clone(),
                    &evidence,
                    &CancellationSource::default().token(),
                )
                .unwrap_err();
            assert_eq!(error.kind, StorageFailureKind::Io);
            assert_eq!(error.stage, StorageStage::Prepare);
            assert_eq!(error.visibility, WriteVisibility::Unchanged);
            assert!(error.pending.is_none());
            assert!(evidence.pending().is_none());
            assert_eq!(
                &*error.message,
                if entropy {
                    "entropy unavailable"
                } else {
                    "clock unavailable"
                }
            );
            assert!(!root.path().join("data").exists());
        }
    }
}
