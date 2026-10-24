//! JS bindings for `hunch-core`. The exported shape is a contract with `extension/`.

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
  secondsAnalyzed: number;
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "Estimate")]
    pub type JsEstimate;
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EstimateDto {
    key: Option<String>,
    key_confidence: f32,
    bpm: Option<f32>,
    bpm_confidence: f32,
    bpm_alternate: Option<f32>,
    seconds_analyzed: f32,
}

impl From<hunch_core::Estimate> for EstimateDto {
    fn from(estimate: hunch_core::Estimate) -> Self {
        Self {
            key: estimate.key.map(|key| key.to_string()),
            key_confidence: estimate.key_confidence,
            bpm: estimate.bpm,
            bpm_confidence: estimate.bpm_confidence,
            bpm_alternate: estimate.bpm_alternate,
            seconds_analyzed: estimate.seconds_analyzed,
        }
    }
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
        // serde-wasm-bindgen turns `None` into `undefined` by default; the contract says `null`.
        let serializer = serde_wasm_bindgen::Serializer::new().serialize_missing_as_null(true);
        let value = EstimateDto::from(estimate).serialize(&serializer)?;
        Ok(Some(value.unchecked_into()))
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }
}
