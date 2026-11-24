//! Immutable opaque attempt handles. Only the creating adapter knows the payload.
use cyoa_core::persistence::{ContentStamp, SaveAttempt, SaveId};
use std::{any::Any, fmt, sync::Arc};

trait ErasedAttempt: SaveAttempt {
    fn payload(&self) -> &dyn Any;
    fn same(&self, other: &dyn ErasedAttempt) -> bool;
}
struct Evidence<A>(A);
impl<A: SaveAttempt> SaveAttempt for Evidence<A> {
    fn target(&self) -> &SaveId {
        self.0.target()
    }
    fn previous_stamp(&self) -> Option<ContentStamp> {
        self.0.previous_stamp()
    }
    fn intended_stamp(&self) -> ContentStamp {
        self.0.intended_stamp()
    }
}
impl<A: SaveAttempt + Eq + 'static> ErasedAttempt for Evidence<A> {
    fn payload(&self) -> &dyn Any {
        &self.0
    }
    fn same(&self, other: &dyn ErasedAttempt) -> bool {
        other.payload().downcast_ref::<A>() == Some(&self.0)
    }
}

/// A clone retains the exact same private preparation across worker failure.
///
/// ```compile_fail
/// fn inspect(attempt: cyoa_application::attempt::PendingWrite) {
///     let encoded = attempt.bytes(); // no encoded-byte accessor in application
/// }
/// ```
#[derive(Clone)]
pub struct PendingWrite(Arc<dyn ErasedAttempt>);
impl PendingWrite {
    pub fn new<A: SaveAttempt + Eq + 'static>(evidence: A) -> Self {
        Self(Arc::new(Evidence(evidence)))
    }
    pub fn target(&self) -> &SaveId {
        self.0.target()
    }
    pub fn previous_stamp(&self) -> Option<ContentStamp> {
        self.0.previous_stamp()
    }
    pub fn intended_stamp(&self) -> ContentStamp {
        self.0.intended_stamp()
    }
    /// Trusted adapters recover their own evidence type. The real encoded
    /// document type is private to infrastructure, so callers cannot name it.
    pub fn evidence<A: 'static>(&self) -> Option<&A> {
        self.0.payload().downcast_ref()
    }
}
impl SaveAttempt for PendingWrite {
    fn target(&self) -> &SaveId {
        self.target()
    }
    fn previous_stamp(&self) -> Option<ContentStamp> {
        self.previous_stamp()
    }
    fn intended_stamp(&self) -> ContentStamp {
        self.intended_stamp()
    }
}
impl fmt::Debug for PendingWrite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PendingWrite")
            .field("target", self.target())
            .field("previous_stamp", &self.previous_stamp())
            .field("intended_stamp", &self.intended_stamp())
            .finish_non_exhaustive()
    }
}
impl PartialEq for PendingWrite {
    fn eq(&self, other: &Self) -> bool {
        self.0.same(other.0.as_ref())
    }
}
impl Eq for PendingWrite {}
