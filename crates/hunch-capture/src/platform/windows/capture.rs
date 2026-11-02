use wasapi::{
    AudioCaptureClient, AudioClient, DeviceEnumerator, Direction, Handle, SampleType, StreamMode,
    WasapiError, WaveFormat, deinitialize, initialize_mta,
};
use windows::Win32::Media::Audio::AUDCLNT_E_DEVICE_INVALIDATED;

use super::process::ProcessWatch;
use super::target::Target;
use super::{WasapiResult, backend_error};
use crate::{CaptureError, EndReason, PendingStream, StreamFormat, StreamSink};

/// Process loopback reports no mix format, so this is requested and Windows converts to it.
const PROCESS_LOOPBACK_FORMAT: StreamFormat = StreamFormat {
    sample_rate: 48_000,
    channels: 2,
};
const FLOAT_BITS: usize = 32;
const BYTES_PER_SAMPLE: usize = FLOAT_BITS / 8;
/// The shared-mode buffer: room for several engine periods if the thread is late.
const BUFFER_DURATION_HNS: i64 = 2_000_000;
/// How long to wait for a buffer event before checking for a stop or a closed app. Loopback
/// signals nothing while the source is silent, so timeouts are normal.
const EVENT_WAIT_MILLISECONDS: u32 = 100;
/// Process loopback includes the target's child processes, where browsers play audio.
const INCLUDE_PROCESS_TREE: bool = true;

/// Captures one target on the calling thread until the stream is dropped or the source ends.
pub struct Capture {
    pub target: Target,
}

impl Capture {
    pub fn run(self, pending: PendingStream) {
        if let Err(error) = initialize_mta().ok() {
            pending.failed(CaptureError::Backend(format!(
                "cannot initialize COM: {error}"
            )));
            return;
        }
        match OpenCapture::open(&self.target) {
            Ok(capture) => {
                let sink = pending.started(capture.format);
                if let Some(reason) = capture.pump(&sink) {
                    sink.end(reason);
                }
            }
            Err(error) => pending.failed(error),
        }
        deinitialize();
    }
}

struct OpenCapture {
    client: AudioClient,
    capture: AudioCaptureClient,
    event: Handle,
    format: StreamFormat,
    /// Set for apps: process loopback keeps running silently after the app exits.
    app: Option<ProcessWatch>,
}

impl OpenCapture {
    fn open(target: &Target) -> Result<Self, CaptureError> {
        match target {
            Target::Output { endpoint_id } => Self::open_device(DeviceTarget {
                target,
                endpoint_id,
                direction: Direction::Render,
            }),
            Target::Input { endpoint_id } => Self::open_device(DeviceTarget {
                target,
                endpoint_id,
                direction: Direction::Capture,
            }),
            Target::App { process_id } => Self::open_app(AppTarget {
                target,
                process_id: *process_id,
            }),
        }
    }

    fn open_device(device_target: DeviceTarget<'_>) -> Result<Self, CaptureError> {
        let not_found = || CaptureError::SourceNotFound(device_target.target.to_source_id());
        let enumerator = DeviceEnumerator::new().map_err(backend_error)?;
        let device = enumerator
            .get_device(device_target.endpoint_id)
            .map_err(|_| not_found())?;
        if device.get_direction() != device_target.direction {
            return Err(not_found());
        }
        let mut client = device.get_iaudioclient().map_err(backend_error)?;
        let mix_format = client.get_mixformat().map_err(backend_error)?;
        let format = StreamFormat {
            sample_rate: mix_format.get_samplespersec(),
            channels: mix_format.get_nchannels(),
        };
        Self::start(StartRequest {
            client: &mut client,
            format,
        })
        .map_err(backend_error)?;
        Self::finish(StartedClient {
            client,
            format,
            app: None,
        })
    }

    fn open_app(app_target: AppTarget<'_>) -> Result<Self, CaptureError> {
        let watch = ProcessWatch::open(app_target.process_id)
            .ok_or_else(|| CaptureError::SourceNotFound(app_target.target.to_source_id()))?;
        // Activation is asynchronous; this blocks until Windows completes it.
        let mut client = AudioClient::new_application_loopback_client(
            app_target.process_id,
            INCLUDE_PROCESS_TREE,
        )
        .map_err(|error| {
            CaptureError::Backend(format!(
                "process loopback needs Windows 10 2004 or later; activation failed: {error}"
            ))
        })?;
        Self::start(StartRequest {
            client: &mut client,
            format: PROCESS_LOOPBACK_FORMAT,
        })
        .map_err(backend_error)?;
        Self::finish(StartedClient {
            client,
            format: PROCESS_LOOPBACK_FORMAT,
            app: Some(watch),
        })
    }

    /// Initializes shared, event-driven capture of 32-bit float in `format`.
    fn start(request: StartRequest<'_>) -> WasapiResult<()> {
        let wave_format = WaveFormat::new(
            FLOAT_BITS,
            FLOAT_BITS,
            &SampleType::Float,
            request.format.sample_rate as usize,
            usize::from(request.format.channels),
            None,
        );
        let mode = StreamMode::EventsShared {
            autoconvert: true,
            buffer_duration_hns: BUFFER_DURATION_HNS,
        };
        request
            .client
            .initialize_client(&wave_format, &Direction::Capture, &mode)
    }

    fn finish(started: StartedClient) -> Result<Self, CaptureError> {
        let event = started
            .client
            .set_get_eventhandle()
            .map_err(backend_error)?;
        let capture = started
            .client
            .get_audiocaptureclient()
            .map_err(backend_error)?;
        started.client.start_stream().map_err(backend_error)?;
        Ok(Self {
            client: started.client,
            capture,
            event,
            format: started.format,
            app: started.app,
        })
    }

    /// Forwards audio until the stream is dropped (`None`) or the source ends.
    fn pump(&self, sink: &StreamSink) -> Option<EndReason> {
        while !sink.is_stopped() {
            let has_app_exited = self.app.as_ref().is_some_and(|app| !app.is_running());
            if has_app_exited {
                return Some(EndReason::SourceGone);
            }
            if let Err(error) = self.drain(sink) {
                return Some(end_reason(error));
            }
            // A timeout only means nothing played; the loop checks the source either way.
            let _ = self.event.wait_for_event(EVENT_WAIT_MILLISECONDS);
        }
        None
    }

    /// Sends every packet Windows has buffered.
    fn drain(&self, sink: &StreamSink) -> WasapiResult<()> {
        let channels = usize::from(self.format.channels);
        let bytes_per_frame = channels * BYTES_PER_SAMPLE;
        while let Some(frames) = self.capture.get_next_packet_size()? {
            if frames == 0 {
                break;
            }
            let mut bytes = vec![0u8; frames as usize * bytes_per_frame];
            let (frames_read, info) = self.capture.read_from_device(&mut bytes)?;
            let sample_count = frames_read as usize * channels;
            let samples = if info.flags.silent {
                vec![0.0; sample_count]
            } else {
                floats_from_bytes(&bytes[..sample_count * BYTES_PER_SAMPLE])
            };
            sink.send_frames(samples);
        }
        Ok(())
    }
}

impl Drop for OpenCapture {
    fn drop(&mut self) {
        // Stopping a client whose device is gone fails, and there's nothing left to do then.
        let _ = self.client.stop_stream();
    }
}

struct DeviceTarget<'a> {
    target: &'a Target,
    endpoint_id: &'a str,
    direction: Direction,
}

struct AppTarget<'a> {
    target: &'a Target,
    process_id: u32,
}

struct StartRequest<'a> {
    client: &'a mut AudioClient,
    format: StreamFormat,
}

struct StartedClient {
    client: AudioClient,
    format: StreamFormat,
    app: Option<ProcessWatch>,
}

fn floats_from_bytes(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(BYTES_PER_SAMPLE)
        .map(|sample| f32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]))
        .collect()
}

fn end_reason(error: WasapiError) -> EndReason {
    let is_device_gone = matches!(
        &error,
        WasapiError::Windows(windows_error) if windows_error.code() == AUDCLNT_E_DEVICE_INVALIDATED
    );
    if is_device_gone {
        EndReason::SourceGone
    } else {
        EndReason::Failed(error.to_string())
    }
}
