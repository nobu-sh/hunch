use std::fmt;

use crate::SourceId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureError {
    /// This platform has no capture backend, or the OS is too old for the requested source.
    Unsupported(String),
    /// No source has this id, for example an app that has since closed.
    SourceNotFound(SourceId),
    /// The OS audio API failed.
    Backend(String),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureError::Unsupported(reason) => write!(f, "audio capture unsupported: {reason}"),
            CaptureError::SourceNotFound(id) => write!(f, "no audio source with id {id}"),
            CaptureError::Backend(message) => write!(f, "audio capture failed: {message}"),
        }
    }
}

impl std::error::Error for CaptureError {}
