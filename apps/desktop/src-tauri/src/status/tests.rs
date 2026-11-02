use hunch_capture::{Source, SourceId, SourceKind};
use serde_json::{Value, json};

use super::*;

fn chrome() -> Listenable {
    Listenable::Capture(Source {
        id: SourceId::new("app:42"),
        kind: SourceKind::App,
        name: "chrome.exe".to_owned(),
        is_default: false,
        is_output: false,
    })
}

fn to_json(status: SourceStatusDto) -> Value {
    serde_json::to_value(status).expect("serializes")
}

#[test]
fn listening_has_a_null_message() {
    assert_eq!(
        to_json(SourceStatusDto::listening(&chrome())),
        json!({ "sourceId": "app:42", "state": "listening", "message": null })
    );
}

#[test]
fn ended_says_the_app_closed() {
    assert_eq!(
        to_json(SourceStatusDto::ended(&chrome())),
        json!({
            "sourceId": "app:42",
            "state": "ended",
            "message": "chrome.exe closed, so there's nothing left to hear.",
        })
    );
}

#[test]
fn silent_has_its_state_and_a_message() {
    let json = to_json(SourceStatusDto::silent(&chrome()));

    assert_eq!(json["state"], "silent");
    assert!(
        json["message"]
            .as_str()
            .is_some_and(|message| message.contains("DRM"))
    );
}

#[test]
fn failed_without_a_source_sends_a_null_id() {
    let status = SourceStatusDto::failed(Failure {
        source: None,
        message: "no output device",
    });

    assert_eq!(
        to_json(status),
        json!({ "sourceId": null, "state": "failed", "message": "no output device" })
    );
}
