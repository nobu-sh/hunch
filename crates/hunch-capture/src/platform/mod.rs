mod unsupported;

use crate::AudioBackend;

/// The capture backend for the platform this was built for.
pub fn default_backend() -> Box<dyn AudioBackend> {
    Box::new(unsupported::UnsupportedBackend)
}
