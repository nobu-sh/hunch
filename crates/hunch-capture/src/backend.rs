use crate::{CaptureError, CaptureStream, Source, SourceId};

/// A platform's way to find and open audio sources. Nothing above this trait knows which OS
/// API is underneath.
pub trait AudioBackend: Send + Sync {
    /// Devices and the apps that currently have audio sessions, at the moment of the call.
    fn list_sources(&self) -> Result<Vec<Source>, CaptureError>;

    /// Starts capturing `id`. The stream reports [`crate::EndReason::SourceGone`] if the source
    /// goes away while it runs.
    fn open(&self, id: &SourceId) -> Result<CaptureStream, CaptureError>;

    /// Whether `list_sources` can include apps. A UI hides its app list when this is false.
    fn lists_apps(&self) -> bool;
}
