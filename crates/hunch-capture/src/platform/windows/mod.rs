//! WASAPI backend. No `wasapi` or `windows` type leaves this module.

mod capture;
mod listing;
mod process;
mod target;

use std::thread;

use wasapi::{WasapiError, deinitialize, initialize_mta};

use crate::{AudioBackend, CaptureError, CaptureStream, Source, SourceId};
use capture::Capture;
use listing::Listing;
use target::Target;

type WasapiResult<T> = Result<T, WasapiError>;

/// Devices through WASAPI (loopback on outputs, normal capture on inputs) and apps through
/// process loopback.
pub struct WindowsBackend;

impl AudioBackend for WindowsBackend {
    fn list_sources(&self) -> Result<Vec<Source>, CaptureError> {
        with_com(|| {
            let listing = Listing::new()?;
            let mut sources = listing.devices()?;
            sources.extend(listing.apps()?);
            Ok(sources)
        })
    }

    fn open(&self, id: &SourceId) -> Result<CaptureStream, CaptureError> {
        let target = Target::parse(id).ok_or_else(|| CaptureError::SourceNotFound(id.clone()))?;
        CaptureStream::spawn(move |pending| Capture { target }.run(pending))
    }

    fn lists_apps(&self) -> bool {
        true
    }
}

/// Runs `work` on a short-lived MTA thread, so the caller's thread keeps whatever COM
/// apartment it has (a UI thread is usually STA).
fn with_com<T: Send>(
    work: impl FnOnce() -> Result<T, CaptureError> + Send,
) -> Result<T, CaptureError> {
    thread::scope(|scope| {
        scope
            .spawn(|| {
                initialize_mta().ok().map_err(|error| {
                    CaptureError::Backend(format!("cannot initialize COM: {error}"))
                })?;
                let result = work();
                deinitialize();
                result
            })
            .join()
            .unwrap_or_else(|_| Err(CaptureError::Backend("source listing panicked".to_owned())))
    })
}

fn backend_error(error: impl std::fmt::Display) -> CaptureError {
    CaptureError::Backend(error.to_string())
}
