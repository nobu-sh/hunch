use std::process;
use std::sync::atomic::{AtomicU32, Ordering};

use super::*;

static NEXT_DIR: AtomicU32 = AtomicU32::new(0);

/// A fresh directory per test, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let name = format!(
            "hunch-remembered-{}-{}",
            process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        );
        Self(std::env::temp_dir().join(name))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn nothing_saved_loads_as_none() {
    let dir = TempDir::new();

    assert_eq!(RememberedSourceFile::in_dir(&dir.0).load(), None);
}

#[test]
fn a_saved_id_loads_back() {
    let dir = TempDir::new();
    let file = RememberedSourceFile::in_dir(&dir.0);

    file.save("app:42").expect("saves");

    assert_eq!(file.load().as_deref(), Some("app:42"));
}

#[test]
fn the_file_is_camel_case_json() {
    let dir = TempDir::new();
    RememberedSourceFile::in_dir(&dir.0)
        .save("app:42")
        .expect("saves");

    let text = fs::read_to_string(dir.0.join(FILE_NAME)).expect("file exists");

    assert_eq!(text, r#"{"sourceId":"app:42"}"#);
}

#[test]
fn a_malformed_file_loads_as_none() {
    let dir = TempDir::new();
    fs::create_dir_all(&dir.0).expect("creates dir");
    fs::write(dir.0.join(FILE_NAME), "not json").expect("writes");

    assert_eq!(RememberedSourceFile::in_dir(&dir.0).load(), None);
}
