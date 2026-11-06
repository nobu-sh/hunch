//! JS bindings for `hunch-core`. The exported shape is a contract with `extension/`.

use hunch_core::contract::{EstimateDto, SessionEventDto};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const ESTIMATE_TS: &str = r#"
export interface Estimate {
  key: string | null;
  keyConfidence: number;
  bpm: number | null;
  bpmConfidence: number;
  bpmAlternate: number | null;
  tuningCents: number | null;
  secondsAnalyzed: number;
}

export type SessionEvent =
  | { type: 'windowStarted'; windowId: number; atSeconds: number; reason: 'sound' | 'change' }
  | { type: 'estimate'; windowId: number; estimate: Estimate }
  | { type: 'windowEnded'; windowId: number; estimate: Estimate | null; durationSeconds: number; tooShort: boolean; reason: 'silence' | 'change' | 'finished' };
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "Estimate")]
    pub type JsEstimate;

    #[wasm_bindgen(typescript_type = "SessionEvent[]")]
    pub type JsSessionEvents;
}

/// serde-wasm-bindgen turns `None` into `undefined` by default; the contract says `null`.
fn to_js(value: &impl Serialize) -> Result<JsValue, serde_wasm_bindgen::Error> {
    let serializer = serde_wasm_bindgen::Serializer::new().serialize_missing_as_null(true);
    value.serialize(&serializer)
}

/// Crate version, so the extension can log which analyzer build it loaded.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[wasm_bindgen(js_name = Analyzer)]
pub struct WasmAnalyzer {
    inner: hunch_core::Analyzer,
}

#[wasm_bindgen(js_class = Analyzer)]
impl WasmAnalyzer {
    #[wasm_bindgen(constructor)]
    pub fn new(#[wasm_bindgen(js_name = sampleRate)] sample_rate: u32) -> Self {
        Self {
            inner: hunch_core::Analyzer::new(sample_rate),
        }
    }

    pub fn push(&mut self, samples: &[f32]) -> Result<Option<JsEstimate>, JsError> {
        let Some(estimate) = self.inner.push(samples) else {
            return Ok(None);
        };
        let value = to_js(&EstimateDto::from(estimate))?;
        Ok(Some(value.unchecked_into()))
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }
}

#[wasm_bindgen(js_name = Session)]
pub struct WasmSession {
    inner: hunch_core::Session,
}

#[wasm_bindgen(js_class = Session)]
impl WasmSession {
    #[wasm_bindgen(constructor)]
    pub fn new(#[wasm_bindgen(js_name = sampleRate)] sample_rate: u32) -> Self {
        Self {
            inner: hunch_core::Session::new(sample_rate),
        }
    }

    pub fn push(&mut self, samples: &[f32]) -> Result<JsSessionEvents, JsError> {
        let events: Vec<SessionEventDto> = self
            .inner
            .push(samples)
            .into_iter()
            .map(SessionEventDto::from)
            .collect();
        Ok(to_js(&events)?.unchecked_into())
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }
}
