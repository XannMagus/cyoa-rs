//! Disabled image adapter.

use cyoa_application::image::{ImageBackend, ImageError, ImageOutcome};

#[derive(Debug, Default)]
pub struct NullImageBackend;

impl ImageBackend for NullImageBackend {
    fn generate(&mut self, _prompt: &str) -> Result<ImageOutcome, ImageError> {
        Ok(ImageOutcome::Disabled)
    }
}
