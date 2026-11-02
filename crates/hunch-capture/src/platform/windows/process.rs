use std::collections::HashMap;
use std::mem;

use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_TIMEOUT};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};

const EXE_SUFFIX: &str = ".exe";
/// Guards the parent walk against cycles from reused process ids.
const MAX_ANCESTORS: usize = 64;

pub struct ProcessEntry {
    pub process_id: u32,
    pub parent_id: u32,
    pub exe_name: String,
}

/// The processes running at one moment, with their parents and executable names.
pub struct ProcessTable {
    processes: HashMap<u32, ProcessEntry>,
}

impl ProcessTable {
    pub fn snapshot() -> windows::core::Result<Self> {
        let snapshot = OwnedHandle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)? });
        let mut entries = Vec::new();
        let mut entry = PROCESSENTRY32W {
            dwSize: mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = unsafe { Process32FirstW(snapshot.0, &mut entry) };
        while found.is_ok() {
            entries.push(ProcessEntry {
                process_id: entry.th32ProcessID,
                parent_id: entry.th32ParentProcessID,
                exe_name: exe_name(&entry.szExeFile),
            });
            found = unsafe { Process32NextW(snapshot.0, &mut entry) };
        }
        Ok(Self::from_entries(entries))
    }

    pub fn from_entries(entries: impl IntoIterator<Item = ProcessEntry>) -> Self {
        let processes = entries
            .into_iter()
            .map(|entry| (entry.process_id, entry))
            .collect();
        Self { processes }
    }

    /// The executable name without `.exe`, such as `chrome`.
    pub fn app_name(&self, process_id: u32) -> Option<String> {
        let exe_name = &self.processes.get(&process_id)?.exe_name;
        let stem_len = exe_name.len().saturating_sub(EXE_SUFFIX.len());
        let has_exe_suffix = exe_name
            .get(stem_len..)
            .is_some_and(|suffix| suffix.eq_ignore_ascii_case(EXE_SUFFIX));
        let name = if has_exe_suffix {
            &exe_name[..stem_len]
        } else {
            exe_name
        };
        Some(name.to_owned())
    }

    /// The topmost ancestor running the same executable. Browsers play audio from a child
    /// process, and capturing the root's process tree includes it.
    pub fn app_root(&self, process_id: u32) -> u32 {
        let Some(start) = self.processes.get(&process_id) else {
            return process_id;
        };
        let mut root = process_id;
        for _ in 0..MAX_ANCESTORS {
            let parent_id = self.processes[&root].parent_id;
            let is_same_app = self
                .processes
                .get(&parent_id)
                .is_some_and(|parent| parent.exe_name.eq_ignore_ascii_case(&start.exe_name));
            if !is_same_app || parent_id == root {
                break;
            }
            root = parent_id;
        }
        root
    }
}

fn exe_name(raw: &[u16]) -> String {
    let len = raw.iter().position(|&unit| unit == 0).unwrap_or(raw.len());
    String::from_utf16_lossy(&raw[..len])
}

/// Watches whether a process is still running.
pub struct ProcessWatch(OwnedHandle);

impl ProcessWatch {
    /// `None` if the process doesn't exist or can't be opened.
    pub fn open(process_id: u32) -> Option<Self> {
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, process_id) }.ok()?;
        Some(Self(OwnedHandle(handle)))
    }

    pub fn is_running(&self) -> bool {
        // A process handle is signaled once the process exits.
        unsafe { WaitForSingleObject(self.0.0, 0) == WAIT_TIMEOUT }
    }
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPLORER: u32 = 10;
    const CHROME_MAIN: u32 = 20;
    const CHROME_AUDIO: u32 = 21;
    const CHROME_RENDERER: u32 = 22;

    fn entry(process_id: u32, parent_id: u32, exe_name: &str) -> ProcessEntry {
        ProcessEntry {
            process_id,
            parent_id,
            exe_name: exe_name.to_owned(),
        }
    }

    fn browser_tree() -> ProcessTable {
        ProcessTable::from_entries([
            entry(EXPLORER, 1, "explorer.exe"),
            entry(CHROME_MAIN, EXPLORER, "chrome.exe"),
            entry(CHROME_AUDIO, CHROME_MAIN, "chrome.exe"),
            entry(CHROME_RENDERER, CHROME_AUDIO, "Chrome.EXE"),
        ])
    }

    #[test]
    fn a_child_maps_to_the_topmost_process_of_the_same_app() {
        let table = browser_tree();

        assert_eq!(table.app_root(CHROME_AUDIO), CHROME_MAIN);
        assert_eq!(table.app_root(CHROME_RENDERER), CHROME_MAIN);
        assert_eq!(table.app_root(CHROME_MAIN), CHROME_MAIN);
    }

    #[test]
    fn unknown_processes_are_their_own_root() {
        assert_eq!(browser_tree().app_root(999), 999);
    }

    #[test]
    fn a_parent_cycle_stops() {
        let table = ProcessTable::from_entries([entry(1, 2, "a.exe"), entry(2, 1, "a.exe")]);

        assert!([1, 2].contains(&table.app_root(1)));
    }

    #[test]
    fn app_names_drop_the_exe_suffix() {
        let table = browser_tree();

        assert_eq!(table.app_name(CHROME_RENDERER).as_deref(), Some("Chrome"));
        assert_eq!(table.app_name(EXPLORER).as_deref(), Some("explorer"));
        assert_eq!(table.app_name(999), None);
    }
}
