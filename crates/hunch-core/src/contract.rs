//! The JSON shape of estimates and session events that every frontend consumes
//! (`packages/ui/src/estimate.ts` mirrors it).

use serde::Serialize;

use crate::{Estimate, SessionEvent};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EstimateDto {
    pub key: Option<String>,
    pub key_confidence: f32,
    pub bpm: Option<f32>,
    pub bpm_confidence: f32,
    pub bpm_alternate: Option<f32>,
    pub tuning_cents: Option<f32>,
    pub seconds_analyzed: f32,
}

impl From<Estimate> for EstimateDto {
    fn from(estimate: Estimate) -> Self {
        Self {
            key: estimate.key.map(|key| key.to_string()),
            key_confidence: estimate.key_confidence,
            bpm: estimate.bpm,
            bpm_confidence: estimate.bpm_confidence,
            bpm_alternate: estimate.bpm_alternate,
            tuning_cents: estimate.tuning_cents,
            seconds_analyzed: estimate.seconds_analyzed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SessionEventDto {
    WindowStarted {
        window_id: u32,
        at_seconds: f32,
    },
    Estimate {
        window_id: u32,
        estimate: EstimateDto,
    },
    WindowEnded {
        window_id: u32,
        estimate: Option<EstimateDto>,
        duration_seconds: f32,
        too_short: bool,
    },
}

impl From<SessionEvent> for SessionEventDto {
    fn from(event: SessionEvent) -> Self {
        match event {
            SessionEvent::WindowStarted {
                window_id,
                at_seconds,
            } => Self::WindowStarted {
                window_id,
                at_seconds,
            },
            SessionEvent::Estimate {
                window_id,
                estimate,
            } => Self::Estimate {
                window_id,
                estimate: estimate.into(),
            },
            SessionEvent::WindowEnded {
                window_id,
                estimate,
                duration_seconds,
                too_short,
            } => Self::WindowEnded {
                window_id,
                estimate: estimate.map(EstimateDto::from),
                duration_seconds,
                too_short,
            },
        }
    }
}

#[cfg(test)]
mod tests;
