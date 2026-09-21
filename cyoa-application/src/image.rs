//! Optional image generation seam. Images are disabled in v1.

use thiserror::Error;

/// Only the disabled state is supported until image generation is implemented.
/// A future generated variant will need a validated image asset type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImageOutcome {
    Disabled,
}

#[derive(Debug, Error)]
#[error("image generation failed: {0}")]
pub struct ImageError(pub String);

/// Disabled generation is an explicit outcome, distinct from a failed request.
pub trait ImageBackend: Send {
    fn generate(&mut self, prompt: &str) -> Result<ImageOutcome, ImageError>;
}
