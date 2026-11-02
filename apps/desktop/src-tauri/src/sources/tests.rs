use hunch_capture::SourceId;
use serde_json::json;

use super::*;

fn device(id: &str, is_default: bool) -> Source {
    Source {
        id: SourceId::new(id),
        kind: SourceKind::Device,
        name: format!("Device {id}"),
        is_default,
        is_output: true,
    }
}

fn app(id: &str) -> Source {
    Source {
        id: SourceId::new(id),
        kind: SourceKind::App,
        name: format!("{id}.exe"),
        is_default: false,
        is_output: false,
    }
}

fn mic(id: &str) -> Source {
    Source {
        is_output: false,
        ..device(id, true)
    }
}

fn sample_listenables() -> Vec<Listenable> {
    listenables(Available {
        sources: vec![
            mic("mic"),
            device("speakers", false),
            device("headphones", true),
            app("chrome"),
        ],
        includes_dev_source: true,
    })
}

fn start(remembered: Option<&str>) -> Option<String> {
    let listenables = sample_listenables();
    choose_start(StartChoice {
        listenables: &listenables,
        remembered,
        wants_dev_source: false,
    })
    .map(|listenable| listenable.id().to_owned())
}

#[test]
fn starts_the_remembered_source_while_it_exists() {
    assert_eq!(start(Some("chrome")).as_deref(), Some("chrome"));
}

#[test]
fn falls_back_to_the_default_output_when_the_remembered_source_is_gone() {
    assert_eq!(start(Some("closed-app")).as_deref(), Some("headphones"));
}

#[test]
fn starts_the_default_output_when_nothing_was_remembered() {
    assert_eq!(start(None).as_deref(), Some("headphones"));
}

#[test]
fn the_default_input_is_not_a_fallback() {
    let listenables = listenables(Available {
        sources: vec![mic("mic"), device("speakers", false)],
        includes_dev_source: false,
    });

    let started = choose_start(StartChoice {
        listenables: &listenables,
        remembered: None,
        wants_dev_source: false,
    });

    assert_eq!(started, None);
}

#[test]
fn the_dev_override_beats_the_remembered_source() {
    let listenables = sample_listenables();

    let started = choose_start(StartChoice {
        listenables: &listenables,
        remembered: Some("chrome"),
        wants_dev_source: true,
    });

    assert_eq!(started, Some(Listenable::TestBeat));
}

#[test]
fn a_remembered_dev_source_falls_back_when_the_build_has_none() {
    let listenables = listenables(Available {
        sources: vec![device("headphones", true)],
        includes_dev_source: false,
    });

    let started = choose_start(StartChoice {
        listenables: &listenables,
        remembered: Some(DEV_SOURCE_ID),
        wants_dev_source: true,
    });

    assert_eq!(started.as_ref().map(Listenable::id), Some("headphones"));
}

#[test]
fn only_apps_and_outputs_are_watched_for_silence() {
    assert!(Listenable::Capture(app("chrome")).watches_for_silence());
    assert!(Listenable::Capture(device("speakers", false)).watches_for_silence());
    assert!(!Listenable::Capture(mic("mic")).watches_for_silence());
    assert!(!Listenable::TestBeat.watches_for_silence());
}

#[test]
fn a_source_list_is_camel_case() {
    let list = SourceListDto {
        sources: vec![Listenable::Capture(app("chrome")).to_dto()],
        lists_apps: true,
    };

    assert_eq!(
        serde_json::to_value(list).expect("serializes"),
        json!({
            "sources": [{
                "id": "chrome",
                "kind": "app",
                "name": "chrome.exe",
                "isDefault": false,
                "isOutput": false,
            }],
            "listsApps": true,
        })
    );
}

#[test]
fn a_device_has_kind_device() {
    let dto = Listenable::Capture(device("headphones", true)).to_dto();

    assert_eq!(
        serde_json::to_value(dto).expect("serializes"),
        json!({
            "id": "headphones",
            "kind": "device",
            "name": "Device headphones",
            "isDefault": true,
            "isOutput": true,
        })
    );
}

#[test]
fn the_dev_source_matches_the_contract() {
    assert_eq!(
        serde_json::to_value(Listenable::TestBeat.to_dto()).expect("serializes"),
        json!({
            "id": "dev:test-beat",
            "kind": "device",
            "name": "Test beat (dev)",
            "isDefault": false,
            "isOutput": false,
        })
    );
}
