//! Private encoded save preparation behind the application's opaque handle.
use cyoa_application::persistence::*;

pub fn prepare_write(
    target: SaveId,
    previous: Option<ContentStamp>,
    intended: ContentStamp,
    bytes: Vec<u8>,
) -> Result<PendingWrite, InvalidPendingWrite> {
    PreparedDocument::new(target, previous, intended, bytes).map(PendingWrite::new)
}
/// Infrastructure-only access to the creating adapter's private encoded document.
/// A foreign adapter's handle has no document and is rejected by stamp/decode
/// validation before any storage mutation.
pub trait PreparedWriteBytes {
    fn bytes(&self) -> &[u8];
}
impl PreparedWriteBytes for PendingWrite {
    fn bytes(&self) -> &[u8] {
        self.evidence::<PreparedDocument>()
            .map_or(&[], |p| p.bytes())
    }
}
/// Opaque prepared-write evidence; application never parses or edits these bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PreparedDocument {
    target: SaveId,
    previous_stamp: Option<ContentStamp>,
    intended_stamp: ContentStamp,
    // Shared, not copied: a prepared write is retained by the caller's evidence
    // sink, a retry and failures at once, and can be up to MAX_SAVE_BYTES.
    bytes: std::sync::Arc<[u8]>,
}
impl SaveAttempt for PreparedDocument {
    fn target(&self) -> &SaveId {
        &self.target
    }
    fn previous_stamp(&self) -> Option<ContentStamp> {
        self.previous_stamp
    }
    fn intended_stamp(&self) -> ContentStamp {
        self.intended_stamp
    }
}
impl PreparedDocument {
    pub fn new(
        target: SaveId,
        previous_stamp: Option<ContentStamp>,
        intended_stamp: ContentStamp,
        bytes: Vec<u8>,
    ) -> Result<Self, InvalidPendingWrite> {
        if bytes.is_empty() || bytes.len() > MAX_SAVE_BYTES {
            return Err(InvalidPendingWrite);
        }
        Ok(Self {
            target,
            previous_stamp,
            intended_stamp,
            bytes: bytes.into(),
        })
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
