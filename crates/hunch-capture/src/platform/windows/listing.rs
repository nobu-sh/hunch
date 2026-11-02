use std::collections::BTreeMap;

use wasapi::{Device, DeviceEnumerator, Direction, SessionState};

use super::process::ProcessTable;
use super::target::Target;
use super::{WasapiResult, backend_error};
use crate::{CaptureError, Source, SourceKind};

/// The session Windows uses for system sounds, which belongs to no process.
const SYSTEM_SOUNDS_PROCESS_ID: u32 = 0;

/// Lists sources through the endpoint enumerator. Needs COM on the calling thread.
pub struct Listing {
    enumerator: DeviceEnumerator,
}

impl Listing {
    pub fn new() -> Result<Self, CaptureError> {
        let enumerator = DeviceEnumerator::new().map_err(backend_error)?;
        Ok(Self { enumerator })
    }

    /// Active output devices, then active input devices, each with the default first.
    pub fn devices(&self) -> Result<Vec<Source>, CaptureError> {
        let mut sources = self.devices_for(Direction::Render)?;
        sources.extend(self.devices_for(Direction::Capture)?);
        Ok(sources)
    }

    /// Apps with an audio session on any output device, one entry per app, sorted by name.
    pub fn apps(&self) -> Result<Vec<Source>, CaptureError> {
        let processes = ProcessTable::snapshot().map_err(backend_error)?;
        let own_process_id = std::process::id();
        let mut apps = BTreeMap::new();
        for process_id in self.session_process_ids().map_err(backend_error)? {
            let is_ignored = process_id == SYSTEM_SOUNDS_PROCESS_ID || process_id == own_process_id;
            if is_ignored {
                continue;
            }
            let root_id = processes.app_root(process_id);
            // A process that exited since its session was listed has no name to show.
            let Some(name) = processes.app_name(root_id) else {
                continue;
            };
            apps.insert(root_id, name);
        }

        let mut sources: Vec<Source> = apps
            .into_iter()
            .map(|(process_id, name)| Source {
                id: Target::App { process_id }.to_source_id(),
                kind: SourceKind::App,
                name,
                is_default: false,
                is_output: false,
            })
            .collect();
        sources.sort_by_key(|source| source.name.to_lowercase());
        Ok(sources)
    }

    fn devices_for(&self, direction: Direction) -> Result<Vec<Source>, CaptureError> {
        let default_id = self
            .enumerator
            .get_default_device(&direction)
            .and_then(|device| device.get_id())
            .ok();
        let is_output = direction == Direction::Render;
        let mut sources = Vec::new();
        for device in self.active_devices(direction).map_err(backend_error)? {
            let endpoint_id = device.get_id().map_err(backend_error)?;
            let name = device.get_friendlyname().map_err(backend_error)?;
            let is_default = default_id.as_deref() == Some(endpoint_id.as_str());
            let target = if is_output {
                Target::Output { endpoint_id }
            } else {
                Target::Input { endpoint_id }
            };
            sources.push(Source {
                id: target.to_source_id(),
                kind: SourceKind::Device,
                name,
                is_default,
                is_output,
            });
        }
        sources.sort_by_key(|source| !source.is_default);
        Ok(sources)
    }

    fn session_process_ids(&self) -> WasapiResult<Vec<u32>> {
        let mut process_ids = Vec::new();
        for device in self.active_devices(Direction::Render)? {
            let sessions = device
                .get_iaudiosessionmanager()?
                .get_audiosessionenumerator()?;
            for index in 0..sessions.get_count()? {
                let session = sessions.get_session(index)?;
                if session.get_state()? == SessionState::Expired {
                    continue;
                }
                process_ids.push(session.get_process_id()?);
            }
        }
        Ok(process_ids)
    }

    fn active_devices(&self, direction: Direction) -> WasapiResult<Vec<Device>> {
        let collection = self.enumerator.get_device_collection(&direction)?;
        let count = collection.get_nbr_devices()?;
        (0..count)
            .map(|index| collection.get_device_at_index(index))
            .collect()
    }
}
