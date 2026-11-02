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

fn app(name: &str) -> RememberedSource {
    RememberedSource::App {
        name: name.to_owned(),
    }
}

fn device(source_id: &str) -> RememberedSource {
    RememberedSource::Device {
        source_id: source_id.to_owned(),
    }
}

fn saved_text(remembered: &RememberedSource) -> String {
    let dir = TempDir::new();
    RememberedSourceFile::in_dir(&dir.0)
        .save(remembered)
        .expect("saves");
    fs::read_to_string(dir.0.join(FILE_NAME)).expect("file exists")
}

fn load_text(text: &str) -> Option<RememberedSource> {
    let dir = TempDir::new();
    fs::create_dir_all(&dir.0).expect("creates dir");
    fs::write(dir.0.join(FILE_NAME), text).expect("writes");
    RememberedSourceFile::in_dir(&dir.0).load()
}

#[test]
fn a_saved_source_loads_back() {
    for remembered in [app("chrome"), device("{0.0.0.00000000}.{speakers}")] {
        let dir = TempDir::new();
        let file = RememberedSourceFile::in_dir(&dir.0);

        file.save(&remembered).expect("saves");

        assert_eq!(file.load(), Some(remembered));
    }
}

#[test]
fn the_file_is_camel_case_json_tagged_by_kind() {
    assert_eq!(
        saved_text(&app("chrome")),
        r#"{"kind":"app","name":"chrome"}"#
    );
    assert_eq!(
        saved_text(&device("speakers")),
        r#"{"kind":"device","sourceId":"speakers"}"#
    );
}

#[test]
fn an_id_only_file_from_before_loads_as_an_id() {
    assert_eq!(
        load_text(r#"{"sourceId":"app:42"}"#),
        Some(device("app:42"))
    );
}

#[test]
fn apps_are_remembered_by_name_and_everything_else_by_id() {
    let chrome = Listenable::Capture(hunch_capture::Source {
        id: hunch_capture::SourceId::new("app:42"),
        kind: SourceKind::App,
        name: "chrome".to_owned(),
        is_default: false,
        is_output: false,
    });

    assert_eq!(RememberedSource::of(&chrome), app("chrome"));
    assert_eq!(
        RememberedSource::of(&Listenable::TestBeat),
        device(crate::sources::DEV_SOURCE_ID)
    );
}

#[test]
fn a_malformed_file_loads_as_none() {
    assert_eq!(load_text("not json"), None);
    assert_eq!(load_text(r#"{"kind":"app"}"#), None);
}
