//! Private parent<->helper wire protocol: request, reply and failure DTOs and
//! their mapping to application types. Only the same executable speaks it.
use cyoa_application::persistence::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

macro_rules! mapped_enum{
    ($wire:ident,$app:ident,[$($v:ident),+])=>{
        #[derive(Serialize,Deserialize)]pub(super) enum $wire{$($v),+}
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
pub(super) enum KindWire {
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
pub(super) enum VisibilityWire {
    Unchanged,
    Replaced { stamp: [u8; 32] },
    Unknown,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FailureWire {
    pub(super) operation: OperationWire,
    pub(super) stage: StageWire,
    pub(super) kind: KindWire,
    pub(super) message: String,
    pub(super) visibility: VisibilityWire,
    pub(super) cleanup_errors: Vec<String>,
}
impl From<StorageFailure> for FailureWire {
    fn from(v: StorageFailure) -> Self {
        Self {
            operation: v.operation().into(),
            stage: v.stage().into(),
            kind: v.kind().into(),
            message: v.message().into(),
            visibility: match v.visibility() {
                WriteVisibility::Unchanged => VisibilityWire::Unchanged,
                WriteVisibility::Replaced { stamp } => VisibilityWire::Replaced {
                    stamp: *stamp.bytes(),
                },
                WriteVisibility::Unknown => VisibilityWire::Unknown,
            },
            cleanup_errors: v.cleanup_errors().to_vec(),
        }
    }
}
impl FailureWire {
    /// Rebuilds the helper's failure for the parent's own `attempt`. A claimed
    /// replacement must name exactly that attempt's stamp; anything else, or a
    /// replacement or unknown outcome without an attempt, is `None`.
    pub(super) fn into_failure(self, attempt: Option<&PendingWrite>) -> Option<StorageFailure> {
        let failure = StorageFailure::new(
            self.operation.into(),
            self.stage.into(),
            self.kind.into(),
            self.message,
        )
        .with_cleanup_errors(self.cleanup_errors);
        match (self.visibility, attempt) {
            (VisibilityWire::Unchanged, _) => Some(failure),
            (VisibilityWire::Replaced { stamp }, Some(attempt))
                if ContentStamp::new(stamp) == attempt.intended_stamp() =>
            {
                Some(failure.replaced(attempt.clone()))
            }
            (VisibilityWire::Unknown, Some(_)) => Some(failure.visibility_unknown()),
            _ => None,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub(super) version: u32,
    pub(super) data: PathBuf,
    pub(super) intent: IntentWire,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) enum IntentWire {
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
pub(super) struct Reply {
    pub(super) version: u32,
    pub(super) result: Result<OutputWire, FailureWire>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) enum OutputWire {
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
pub(super) struct RowWire {
    pub(super) id: String,
    pub(super) status: ListingWire,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) enum ListingWire {
    Inspected { primary: PrimaryWire, backup: bool },
    Busy,
    Unreadable,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) enum PrimaryWire {
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
