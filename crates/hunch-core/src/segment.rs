//! Splits a stream into listening windows: sound opens a window, sustained silence closes it.
//!
//! The level is the RMS of fixed-length frames, with separate start and end thresholds
//! (hysteresis) so a level hovering near one threshold doesn't flap.

#[derive(Debug, Clone, PartialEq)]
pub struct SegmenterConfig {
    /// Length of one level frame.
    pub frame_seconds: f32,
    /// A window opens when the level stays above this for `start_seconds`.
    pub start_threshold_db: f32,
    pub start_seconds: f32,
    /// An open window closes when the level stays below this for `end_seconds`.
    pub end_threshold_db: f32,
    pub end_seconds: f32,
}

impl Default for SegmenterConfig {
    fn default() -> Self {
        Self {
            frame_seconds: 0.05,
            start_threshold_db: -45.0,
            start_seconds: 0.3,
            end_threshold_db: -55.0,
            end_seconds: 1.5,
        }
    }
}

/// Positions are sample indices since the segmenter was created or reset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentEvent {
    /// A window opened. `at_sample` is where the sound began, so the run of loud frames that
    /// confirmed it is part of the window.
    Started { at_sample: u64 },
    /// The open window closed. `at_sample` is where the silence began; the window ends there.
    Ended { at_sample: u64 },
}

/// Streaming window detector. Results don't depend on how the input is chunked.
pub struct Segmenter {
    frame_len: usize,
    start_power: f32,
    end_power: f32,
    start_frames: u32,
    end_frames: u32,
    frame_energy: f32,
    frame_filled: usize,
    position: u64,
    is_open: bool,
    /// Consecutive frames past the threshold that would flip the state: loud while closed,
    /// quiet while open.
    run_frames: u32,
    run_start: u64,
}

impl Segmenter {
    pub fn new(sample_rate: u32) -> Self {
        Self::with_config(sample_rate, SegmenterConfig::default())
    }

    pub fn with_config(sample_rate: u32, config: SegmenterConfig) -> Self {
        let frame_len = ((config.frame_seconds * sample_rate as f32).round() as usize).max(1);
        let frame_seconds = frame_len as f32 / sample_rate.max(1) as f32;
        let frames_for = |seconds: f32| ((seconds / frame_seconds).round() as u32).max(1);
        Self {
            frame_len,
            start_power: db_to_power(config.start_threshold_db),
            end_power: db_to_power(config.end_threshold_db),
            start_frames: frames_for(config.start_seconds),
            end_frames: frames_for(config.end_seconds),
            frame_energy: 0.0,
            frame_filled: 0,
            position: 0,
            is_open: false,
            run_frames: 0,
            run_start: 0,
        }
    }

    /// Mono samples in [-1, 1]. Calls `on_event` for each window start or end, in order.
    pub fn push(&mut self, samples: &[f32], mut on_event: impl FnMut(SegmentEvent)) {
        let mut rest = samples;
        while !rest.is_empty() {
            let take = rest.len().min(self.frame_len - self.frame_filled);
            let (head, tail) = rest.split_at(take);
            // Summed one sample at a time so the result doesn't depend on chunk boundaries.
            for sample in head {
                self.frame_energy += sample * sample;
            }
            self.frame_filled += take;
            self.position += take as u64;
            rest = tail;

            if self.frame_filled == self.frame_len {
                let power = self.frame_energy / self.frame_len as f32;
                self.frame_energy = 0.0;
                self.frame_filled = 0;
                if let Some(event) = self.on_frame(power) {
                    on_event(event);
                }
            }
        }
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Everything before this sample is settled: inside the window or outside it for good.
    /// Samples from here on may still turn out to start or end a window.
    pub fn undecided_from(&self) -> u64 {
        if self.run_frames > 0 {
            return self.run_start;
        }
        self.position - self.frame_filled as u64
    }

    pub fn reset(&mut self) {
        self.frame_energy = 0.0;
        self.frame_filled = 0;
        self.position = 0;
        self.is_open = false;
        self.run_frames = 0;
        self.run_start = 0;
    }

    fn on_frame(&mut self, power: f32) -> Option<SegmentEvent> {
        let frame_start = self.position - self.frame_len as u64;
        let (is_flipping, frames_needed) = if self.is_open {
            (power < self.end_power, self.end_frames)
        } else {
            (power > self.start_power, self.start_frames)
        };
        if !is_flipping {
            self.run_frames = 0;
            return None;
        }

        if self.run_frames == 0 {
            self.run_start = frame_start;
        }
        self.run_frames += 1;
        if self.run_frames < frames_needed {
            return None;
        }

        self.run_frames = 0;
        self.is_open = !self.is_open;
        let at_sample = self.run_start;
        Some(if self.is_open {
            SegmentEvent::Started { at_sample }
        } else {
            SegmentEvent::Ended { at_sample }
        })
    }
}

fn db_to_power(db: f32) -> f32 {
    10f32.powf(db / 10.0)
}

#[cfg(test)]
mod tests {
    use std::f32::consts::PI;

    use super::*;

    const SAMPLE_RATE: u32 = 22_050;
    /// One 50 ms frame: boundaries snap to the frame grid.
    const FRAME_LEN: u64 = SAMPLE_RATE as u64 / 20 + 1;

    /// A sine whose RMS level is `db` dBFS.
    fn tone(seconds: f32, db: f32) -> Vec<f32> {
        let amplitude = 10f32.powf(db / 20.0) * 2f32.sqrt();
        let length = (seconds * SAMPLE_RATE as f32) as usize;
        (0..length)
            .map(|i| amplitude * (2.0 * PI * 440.0 * i as f32 / SAMPLE_RATE as f32).sin())
            .collect()
    }

    fn events_for(signal: &[f32], chunk_size: usize) -> Vec<SegmentEvent> {
        let mut segmenter = Segmenter::new(SAMPLE_RATE);
        let mut events = Vec::new();
        for chunk in signal.chunks(chunk_size) {
            segmenter.push(chunk, |event| events.push(event));
        }
        events
    }

    #[test]
    fn window_includes_the_confirming_sound_and_excludes_the_trailing_silence() {
        let signal = [tone(1.0, -80.0), tone(2.0, -20.0), tone(2.0, -80.0)].concat();
        let events = events_for(&signal, signal.len());

        let sound_start = SAMPLE_RATE as u64;
        let sound_end = 3 * SAMPLE_RATE as u64;
        let [
            SegmentEvent::Started { at_sample: start },
            SegmentEvent::Ended { at_sample: end },
        ] = events[..]
        else {
            panic!("expected one start and one end, got {events:?}");
        };
        assert!(start.abs_diff(sound_start) < FRAME_LEN, "start {start}");
        assert!(end.abs_diff(sound_end) < FRAME_LEN, "end {end}");
    }

    #[test]
    fn level_between_the_thresholds_neither_opens_nor_closes() {
        let between = tone(5.0, -50.0);
        assert!(events_for(&between, between.len()).is_empty());

        let signal = [tone(1.0, -20.0), tone(5.0, -50.0)].concat();
        let events = events_for(&signal, signal.len());
        assert!(matches!(events[..], [SegmentEvent::Started { .. }]));
    }

    #[test]
    fn short_sound_does_not_open_a_window() {
        let signal = [tone(0.2, -20.0), tone(1.0, -80.0)].concat();
        assert!(events_for(&signal, signal.len()).is_empty());
    }

    #[test]
    fn chunking_does_not_change_events() {
        let signal = [tone(1.0, -20.0), tone(2.0, -80.0), tone(1.0, -20.0)].concat();
        let whole = events_for(&signal, signal.len());
        assert_eq!(whole.len(), 3);
        for chunk_size in [1, 7, 1_102, 2_205] {
            assert_eq!(
                events_for(&signal, chunk_size),
                whole,
                "chunk size {chunk_size}"
            );
        }
    }
}
