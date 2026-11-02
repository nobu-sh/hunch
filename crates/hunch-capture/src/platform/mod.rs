#[cfg(not(windows))]
mod unsupported;
#[cfg(windows)]
mod windows;

use crate::AudioBackend;

/// The capture backend for the platform this was built for.
pub fn default_backend() -> Box<dyn AudioBackend> {
    #[cfg(windows)]
    return Box::new(self::windows::WindowsBackend);
    #[cfg(not(windows))]
    return Box::new(unsupported::UnsupportedBackend);
}
