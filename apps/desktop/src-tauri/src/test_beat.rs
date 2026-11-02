//! The dev source: the same beat as `extension/dev/test-beat.ts`
//! (120 BPM, A natural minor: Am, Dm, Em, Am with kick, snare and hats), synthesized here and
//! played in real time.

use std::f32::consts::{PI, TAU};
use std::thread;
use std::time::{Duration, Instant};

use crate::feed::{AudioFeed, FeedEvent};

/// The session's native rate, so the feed needs no resampling.
const SAMPLE_RATE: u32 = 22_050;
const CHUNK_SECONDS: f32 = 0.1;

const BPM: f32 = 120.0;
const BEAT_SECONDS: f32 = 60.0 / BPM;
const BAR_SECONDS: f32 = BEAT_SECONDS * 4.0;
const LOOP_BARS: usize = 16;
const BEATS_PER_BAR: usize = 4;
/// MIDI notes per bar: the bass, then the chord.
const CHORDS: [[u8; 4]; 4] = [
    [45, 57, 60, 64], // Am: A2 bass, A3 C4 E4
    [50, 62, 65, 69], // Dm
    [52, 64, 67, 71], // Em
    [45, 57, 60, 64], // Am
];
const MASTER_GAIN: f32 = 0.5;

const BASS_GAIN: f32 = 0.25;
const CHORD_GAIN: f32 = 0.08;
const ATTACK_SECONDS: f32 = 0.02;
const RELEASE_LEAD_SECONDS: f32 = 0.1;
const RELEASE_TIME_CONSTANT: f32 = 0.05;

const KICK_SECONDS: f32 = 0.3;
const KICK_SWEEP_SECONDS: f32 = 0.15;
const KICK_START_HZ: f32 = 120.0;
const KICK_END_HZ: f32 = 40.0;

const NOISE_SECONDS: f32 = 0.2;
/// Where Web Audio's exponential ramps end, since they can't reach zero.
const SILENT_GAIN: f32 = 0.001;
/// Web Audio's default biquad Q, which it gives in dB for highpass filters.
const HIGHPASS_Q_DB: f32 = 1.0;
const SNARE: Hit = Hit {
    start: 0.0,
    length: 0.15,
    gain: 0.5,
    highpass_hz: 1500.0,
};
const HAT: Hit = Hit {
    start: 0.0,
    length: 0.04,
    gain: 0.15,
    highpass_hz: 7000.0,
};
const OFFBEAT_HAT_GAIN: f32 = 0.1;

/// Loops the rendered beat, handing out each chunk once it would have finished playing.
pub struct TestBeatFeed {
    samples: Vec<f32>,
    position: usize,
    samples_sent: u64,
    started: Instant,
}

impl TestBeatFeed {
    pub fn new() -> Self {
        Self {
            samples: render(SAMPLE_RATE),
            position: 0,
            samples_sent: 0,
            started: Instant::now(),
        }
    }
}

impl AudioFeed for TestBeatFeed {
    fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }

    fn next(&mut self) -> FeedEvent {
        let chunk_len = (CHUNK_SECONDS * SAMPLE_RATE as f32) as usize;
        let chunk: Vec<f32> = self
            .samples
            .iter()
            .cycle()
            .skip(self.position)
            .take(chunk_len)
            .copied()
            .collect();
        self.position = (self.position + chunk_len) % self.samples.len();
        self.samples_sent += chunk_len as u64;

        let played_at = self.started
            + Duration::from_secs_f64(self.samples_sent as f64 / f64::from(SAMPLE_RATE));
        thread::sleep(played_at.saturating_duration_since(Instant::now()));
        FeedEvent::Audio(chunk)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Wave {
    Triangle,
    Sawtooth,
}

struct Tone {
    midi: u8,
    start: f32,
    length: f32,
    gain: f32,
    wave: Wave,
}

#[derive(Clone, Copy)]
struct Hit {
    start: f32,
    length: f32,
    gain: f32,
    highpass_hz: f32,
}

/// One loop of the beat, mono at `sample_rate`.
fn render(sample_rate: u32) -> Vec<f32> {
    let length = (LOOP_BARS as f32 * BAR_SECONDS * sample_rate as f32).ceil() as usize;
    let mut mix = Mix {
        samples: vec![0.0; length],
        sample_rate: sample_rate as f32,
        noise: noise((NOISE_SECONDS * sample_rate as f32) as usize),
    };

    for bar in 0..LOOP_BARS {
        let bar_start = bar as f32 * BAR_SECONDS;
        let [bass, chord @ ..] = CHORDS[bar % CHORDS.len()];
        mix.add_tone(Tone {
            midi: bass,
            start: bar_start,
            length: BAR_SECONDS,
            gain: BASS_GAIN,
            wave: Wave::Triangle,
        });
        for midi in chord {
            mix.add_tone(Tone {
                midi,
                start: bar_start,
                length: BAR_SECONDS,
                gain: CHORD_GAIN,
                wave: Wave::Sawtooth,
            });
        }
        for beat in 0..BEATS_PER_BAR {
            let beat_start = bar_start + beat as f32 * BEAT_SECONDS;
            let is_kick_beat = beat % 2 == 0;
            if is_kick_beat {
                mix.add_kick(beat_start);
            } else {
                mix.add_hit(Hit {
                    start: beat_start,
                    ..SNARE
                });
            }
            mix.add_hit(Hit {
                start: beat_start,
                ..HAT
            });
            mix.add_hit(Hit {
                start: beat_start + BEAT_SECONDS / 2.0,
                gain: OFFBEAT_HAT_GAIN,
                ..HAT
            });
        }
    }

    mix.samples
        .into_iter()
        .map(|sample| (sample * MASTER_GAIN).clamp(-1.0, 1.0))
        .collect()
}

struct Mix {
    samples: Vec<f32>,
    sample_rate: f32,
    noise: Vec<f32>,
}

impl Mix {
    /// Sample indices from `start` for `length` seconds, cut off at the end of the loop.
    fn span(&self, start: f32, length: f32) -> std::ops::Range<usize> {
        let first = (start * self.sample_rate) as usize;
        let end = ((start + length) * self.sample_rate) as usize;
        first.min(self.samples.len())..end.min(self.samples.len())
    }

    fn add_tone(&mut self, tone: Tone) {
        let hz = 440.0 * 2f32.powf((f32::from(tone.midi) - 69.0) / 12.0);
        let release_at = tone.length - RELEASE_LEAD_SECONDS;
        let span = self.span(tone.start, tone.length);
        let first = span.start;
        for index in span {
            let t = (index - first) as f32 / self.sample_rate;
            let envelope = if t < ATTACK_SECONDS {
                tone.gain * t / ATTACK_SECONDS
            } else if t < release_at {
                tone.gain
            } else {
                tone.gain * (-(t - release_at) / RELEASE_TIME_CONSTANT).exp()
            };
            let phase = (hz * t).fract();
            let wave = match tone.wave {
                Wave::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
                Wave::Sawtooth => 2.0 * phase - 1.0,
            };
            self.samples[index] += envelope * wave;
        }
    }

    /// A sine that sweeps down from 120 Hz, decaying over 0.3 s.
    fn add_kick(&mut self, start: f32) {
        let span = self.span(start, KICK_SECONDS);
        let first = span.start;
        let mut phase: f32 = 0.0;
        for index in span {
            let t = (index - first) as f32 / self.sample_rate;
            let sweep = (t / KICK_SWEEP_SECONDS).min(1.0);
            let hz = KICK_START_HZ * (KICK_END_HZ / KICK_START_HZ).powf(sweep);
            let envelope = SILENT_GAIN.powf(t / KICK_SECONDS);
            self.samples[index] += envelope * phase.sin();
            phase = (phase + TAU * hz / self.sample_rate) % TAU;
        }
    }

    /// Highpassed noise that decays over the hit's length: snares and hats.
    fn add_hit(&mut self, hit: Hit) {
        let mut filter = Highpass::new(HighpassParams {
            cutoff_hz: hit.highpass_hz,
            sample_rate: self.sample_rate,
        });
        let span = self.span(hit.start, hit.length);
        let first = span.start;
        for index in span {
            let offset = index - first;
            let Some(&noise) = self.noise.get(offset) else {
                return;
            };
            let t = offset as f32 / self.sample_rate;
            let envelope = hit.gain * (SILENT_GAIN / hit.gain).powf(t / hit.length);
            self.samples[index] += envelope * filter.process(noise);
        }
    }
}

/// Deterministic white noise in [-1, 1].
fn noise(len: usize) -> Vec<f32> {
    let mut state = 1u32;
    (0..len)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 8) as f32 / (1u32 << 23) as f32 - 1.0
        })
        .collect()
}

/// An RBJ cookbook highpass biquad, like Web Audio's `BiquadFilterNode`.
struct Highpass {
    b: [f32; 3],
    a: [f32; 2],
    inputs: [f32; 2],
    outputs: [f32; 2],
}

struct HighpassParams {
    cutoff_hz: f32,
    sample_rate: f32,
}

impl Highpass {
    fn new(
        HighpassParams {
            cutoff_hz,
            sample_rate,
        }: HighpassParams,
    ) -> Self {
        let q = 10f32.powf(HIGHPASS_Q_DB / 20.0);
        let w0 = 2.0 * PI * cutoff_hz / sample_rate;
        let alpha = w0.sin() / (2.0 * q);
        let cos = w0.cos();
        let a0 = 1.0 + alpha;
        Self {
            b: [
                (1.0 + cos) / 2.0 / a0,
                -(1.0 + cos) / a0,
                (1.0 + cos) / 2.0 / a0,
            ],
            a: [-2.0 * cos / a0, (1.0 - alpha) / a0],
            inputs: [0.0; 2],
            outputs: [0.0; 2],
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let output = self.b[0] * input + self.b[1] * self.inputs[0] + self.b[2] * self.inputs[1]
            - self.a[0] * self.outputs[0]
            - self.a[1] * self.outputs[1];
        self.inputs = [input, self.inputs[0]];
        self.outputs = [output, self.outputs[0]];
        output
    }
}

#[cfg(test)]
mod tests;
