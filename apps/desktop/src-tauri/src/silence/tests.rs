use super::*;

const SILENT: [f32; 4] = [0.0; 4];
const AUDIBLE: [f32; 4] = [0.0, 0.2, -0.3, 0.0];
const BEFORE_TIMEOUT: Duration = Duration::from_secs(14);
const AFTER_TIMEOUT: Duration = Duration::from_secs(16);

fn at(elapsed: Duration, samples: &[f32]) -> Observation<'_> {
    Observation { elapsed, samples }
}

#[test]
fn stays_quiet_before_the_timeout() {
    let mut detector = SilenceDetector::new(true);

    assert_eq!(detector.observe(at(BEFORE_TIMEOUT, &SILENT)), None);
}

#[test]
fn reports_silence_once_after_the_timeout() {
    let mut detector = SilenceDetector::new(true);

    assert_eq!(
        detector.observe(at(SILENCE_TIMEOUT, &SILENT)),
        Some(SilenceChange::BecameSilent)
    );
    assert_eq!(detector.observe(at(AFTER_TIMEOUT, &SILENT)), None);
}

#[test]
fn nothing_arriving_counts_as_silence() {
    let mut detector = SilenceDetector::new(true);

    assert_eq!(
        detector.observe(at(AFTER_TIMEOUT, &[])),
        Some(SilenceChange::BecameSilent)
    );
}

#[test]
fn noise_below_the_threshold_is_still_silence() {
    let mut detector = SilenceDetector::new(true);
    let dither = [DIGITAL_SILENCE_PEAK, -DIGITAL_SILENCE_PEAK];

    assert_eq!(
        detector.observe(at(AFTER_TIMEOUT, &dither)),
        Some(SilenceChange::BecameSilent)
    );
}

#[test]
fn audio_before_the_timeout_means_never_silent() {
    let mut detector = SilenceDetector::new(true);

    assert_eq!(detector.observe(at(BEFORE_TIMEOUT, &AUDIBLE)), None);
    assert_eq!(detector.observe(at(AFTER_TIMEOUT, &SILENT)), None);
}

#[test]
fn audio_after_silence_clears_it_once() {
    let mut detector = SilenceDetector::new(true);
    detector.observe(at(AFTER_TIMEOUT, &SILENT));

    assert_eq!(
        detector.observe(at(AFTER_TIMEOUT, &AUDIBLE)),
        Some(SilenceChange::HeardAudio)
    );
    assert_eq!(detector.observe(at(AFTER_TIMEOUT, &AUDIBLE)), None);
    assert_eq!(detector.observe(at(AFTER_TIMEOUT, &SILENT)), None);
}

#[test]
fn an_unwatched_source_is_never_silent() {
    let mut detector = SilenceDetector::new(false);

    assert_eq!(detector.observe(at(AFTER_TIMEOUT, &SILENT)), None);
}
