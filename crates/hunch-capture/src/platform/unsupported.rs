use crate::{AudioBackend, CaptureError, CaptureStream, Source, SourceId};

const REASON: &str = "no capture backend for this platform yet";

/// Stands in on platforms without a backend: lists nothing and opens nothing.
pub struct UnsupportedBackend;

impl AudioBackend for UnsupportedBackend {
    fn list_sources(&self) -> Result<Vec<Source>, CaptureError> {
        Ok(Vec::new())
    }

    fn open(&self, _id: &SourceId) -> Result<CaptureStream, CaptureError> {
        Err(CaptureError::Unsupported(REASON.to_owned()))
    }

    fn lists_apps(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_nothing_and_refuses_to_open() {
        let backend = UnsupportedBackend;

        assert_eq!(backend.list_sources(), Ok(Vec::new()));
        let opened = backend.open(&SourceId::new("anything"));
        assert!(matches!(opened, Err(CaptureError::Unsupported(_))));
    }
}
