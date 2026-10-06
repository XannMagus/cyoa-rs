//! Disabled image adapter.

use cyoa_application::image::{ImageBackend, ImageError, ImageOutcome};
use cyoa_core::text::RenderedPrompt;

#[derive(Debug, Default)]
pub struct NullImageBackend;

impl ImageBackend for NullImageBackend {
    fn generate(&mut self, _prompt: &RenderedPrompt) -> Result<ImageOutcome, ImageError> {
        Ok(ImageOutcome::Disabled)
    }
}
