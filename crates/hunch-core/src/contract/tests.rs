use serde_json::{Value, json};

use super::*;
use crate::{Key, Mode, PitchClass};

fn a_minor_estimate() -> Estimate {
    Estimate {
        key: Some(Key {
            tonic: PitchClass::new(9).expect("A is a pitch class"),
            mode: Mode::Minor,
        }),
        key_confidence: 0.5,
        bpm: Some(120.0),
        bpm_confidence: 0.75,
        bpm_alternate: Some(60.0),
        tuning_cents: Some(-2.5),
        seconds_analyzed: 8.0,
    }
}

fn to_json(event: SessionEvent) -> Value {
    serde_json::to_value(SessionEventDto::from(event)).expect("session events serialize")
}

#[test]
fn window_started_is_tagged_camel_case() {
    let event = SessionEvent::WindowStarted {
        window_id: 3,
        at_seconds: 1.5,
        reason: WindowStartReason::Sound,
    };
    assert_eq!(
        to_json(event),
        json!({ "type": "windowStarted", "windowId": 3, "atSeconds": 1.5, "reason": "sound" })
    );
}

#[test]
fn start_reasons_are_camel_case() {
    let reasons = [
        (WindowStartReason::Sound, "sound"),
        (WindowStartReason::Change, "change"),
    ];
    for (reason, expected) in reasons {
        let event = SessionEvent::WindowStarted {
            window_id: 1,
            at_seconds: 0.0,
            reason,
        };
        assert_eq!(to_json(event)["reason"], expected);
    }
}

#[test]
fn end_reasons_are_camel_case() {
    let reasons = [
        (WindowEndReason::Silence, "silence"),
        (WindowEndReason::Change, "change"),
        (WindowEndReason::Finished, "finished"),
    ];
    for (reason, expected) in reasons {
        let event = SessionEvent::WindowEnded {
            window_id: 1,
            estimate: None,
            duration_seconds: 5.0,
            too_short: false,
            reason,
        };
        assert_eq!(to_json(event)["reason"], expected);
    }
}

#[test]
fn estimate_has_every_field_in_camel_case() {
    let event = SessionEvent::Estimate {
        window_id: 1,
        estimate: a_minor_estimate(),
    };
    assert_eq!(
        to_json(event),
        json!({
            "type": "estimate",
            "windowId": 1,
            "estimate": {
                "key": "A minor",
                "keyConfidence": 0.5,
                "bpm": 120.0,
                "bpmConfidence": 0.75,
                "bpmAlternate": 60.0,
                "tuningCents": -2.5,
                "secondsAnalyzed": 8.0,
            },
        })
    );
}

#[test]
fn missing_values_are_null_not_absent() {
    let estimate = Estimate {
        key: None,
        bpm: None,
        bpm_alternate: None,
        tuning_cents: None,
        ..a_minor_estimate()
    };
    let json = serde_json::to_value(EstimateDto::from(estimate)).expect("estimates serialize");
    for field in ["key", "bpm", "bpmAlternate", "tuningCents"] {
        assert_eq!(json[field], Value::Null, "{field}");
    }
}

#[test]
fn window_ended_without_an_estimate_sends_null() {
    let event = SessionEvent::WindowEnded {
        window_id: 2,
        estimate: None,
        duration_seconds: 2.0,
        too_short: true,
        reason: WindowEndReason::Silence,
    };
    assert_eq!(
        to_json(event),
        json!({
            "type": "windowEnded",
            "windowId": 2,
            "estimate": null,
            "durationSeconds": 2.0,
            "tooShort": true,
            "reason": "silence",
        })
    );
}
