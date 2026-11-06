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

/// An app as the Windows backend lists it: a process-id id and the executable name.
fn running_app(name: &str, process_id: u32) -> Source {
    Source {
        id: SourceId::new(format!("app:{process_id}")),
        name: name.to_owned(),
        ..app("unused")
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

fn remembered_device(source_id: &str) -> RememberedSource {
    RememberedSource::Device {
        source_id: source_id.to_owned(),
    }
}

fn remembered_app(name: &str) -> RememberedSource {
    RememberedSource::App {
        name: name.to_owned(),
    }
}

fn start_from(sources: Vec<Source>, remembered: Option<&RememberedSource>) -> Option<String> {
    let listenables = listenables(Available {
        sources,
        includes_dev_source: false,
    });
    choose_start(StartChoice {
        listenables: &listenables,
        remembered,
        wanted_dev_source: None,
    })
    .map(|listenable| listenable.id().to_owned())
}

fn start(remembered: Option<&RememberedSource>) -> Option<String> {
    let listenables = sample_listenables();
    choose_start(StartChoice {
        listenables: &listenables,
        remembered,
        wanted_dev_source: None,
    })
    .map(|listenable| listenable.id().to_owned())
}

#[test]
fn starts_the_remembered_device_while_it_exists() {
    assert_eq!(
        start(Some(&remembered_device("speakers"))).as_deref(),
        Some("speakers")
    );
}

#[test]
fn falls_back_to_the_default_output_when_the_remembered_device_is_gone() {
    assert_eq!(
        start(Some(&remembered_device("unplugged"))).as_deref(),
        Some("headphones")
    );
}

#[test]
fn finds_a_restarted_app_by_name() {
    let sources = vec![device("headphones", true), running_app("chrome", 7_000)];

    let started = start_from(sources, Some(&remembered_app("chrome")));

    assert_eq!(started.as_deref(), Some("app:7000"));
}

#[test]
fn app_names_match_ignoring_case() {
    let sources = vec![device("headphones", true), running_app("Chrome", 7_000)];

    let started = start_from(sources, Some(&remembered_app("chrome")));

    assert_eq!(started.as_deref(), Some("app:7000"));
}

#[test]
fn the_first_listed_app_wins_when_several_share_the_name() {
    let sources = vec![
        device("headphones", true),
        running_app("chrome", 300),
        running_app("chrome", 100),
    ];

    let started = start_from(sources, Some(&remembered_app("chrome")));

    assert_eq!(started.as_deref(), Some("app:300"));
}

#[test]
fn falls_back_to_the_default_output_when_the_remembered_app_is_not_running() {
    let sources = vec![device("headphones", true), running_app("spotify", 7_000)];

    let started = start_from(sources, Some(&remembered_app("chrome")));

    assert_eq!(started.as_deref(), Some("headphones"));
}

#[test]
fn a_remembered_app_never_matches_a_device_of_the_same_name() {
    let speakers = Source {
        name: "chrome".to_owned(),
        ..device("speakers", false)
    };

    let started = start_from(
        vec![speakers, device("headphones", true)],
        Some(&remembered_app("chrome")),
    );

    assert_eq!(started.as_deref(), Some("headphones"));
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
        wanted_dev_source: None,
    });

    assert_eq!(started, None);
}

#[test]
fn the_dev_override_beats_the_remembered_source() {
    let listenables = sample_listenables();

    let started = choose_start(StartChoice {
        listenables: &listenables,
        remembered: Some(&remembered_app("chrome.exe")),
        wanted_dev_source: Some(DevSource::TestBeat),
    });

    assert_eq!(started, Some(Listenable::Dev(DevSource::TestBeat)));
}

#[test]
fn a_remembered_dev_source_falls_back_when_the_build_has_none() {
    let listenables = listenables(Available {
        sources: vec![device("headphones", true)],
        includes_dev_source: false,
    });

    let started = choose_start(StartChoice {
        listenables: &listenables,
        remembered: Some(&remembered_device(DevSource::TestBeat.id())),
        wanted_dev_source: Some(DevSource::TestBeat),
    });

    assert_eq!(started.as_ref().map(Listenable::id), Some("headphones"));
}

#[test]
fn only_apps_and_outputs_are_watched_for_silence() {
    assert!(Listenable::Capture(app("chrome")).watches_for_silence());
    assert!(Listenable::Capture(device("speakers", false)).watches_for_silence());
    assert!(!Listenable::Capture(mic("mic")).watches_for_silence());
    assert!(!Listenable::Dev(DevSource::TestBeat).watches_for_silence());
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
        serde_json::to_value(Listenable::Dev(DevSource::TestBeat).to_dto()).expect("serializes"),
        json!({
            "id": "dev:test-beat",
            "kind": "device",
            "name": "Test beat (dev)",
            "isDefault": false,
            "isOutput": false,
        })
    );
}

#[test]
fn the_dev_override_can_start_the_test_mix() {
    let listenables = sample_listenables();

    let started = choose_start(StartChoice {
        listenables: &listenables,
        remembered: None,
        wanted_dev_source: Some(DevSource::TestMix),
    });

    assert_eq!(started.as_ref().map(Listenable::id), Some("dev:test-mix"));
}

#[test]
fn dev_sources_are_named_by_their_variable_value() {
    assert_eq!(
        DevSource::from_variable("test-beat"),
        Some(DevSource::TestBeat)
    );
    assert_eq!(
        DevSource::from_variable("test-mix"),
        Some(DevSource::TestMix)
    );
    assert_eq!(DevSource::from_variable("dev:test-mix"), None);
}
