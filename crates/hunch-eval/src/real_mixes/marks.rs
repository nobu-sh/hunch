//! Hand-marked segments of a recorded mix, from the CSV next to its WAV.
//!
//! One row per segment, starting at `start_seconds` and lasting until the next row (the last one
//! until the end of the recording), with a header:
//!
//! ```csv
//! start_seconds,key,bpm,note
//! 0,A minor,124,
//! 3:05,C major,128,second track
//! 4:40,C major,128,drop
//! ```
//!
//! - `start_seconds` is seconds (`185.5`) or a timestamp (`3:05.5`, `1:03:05`), increasing.
//! - `key` is a label in the Beatport key format (`A minor`, `Bb major`, alternatives split by
//!   `|`). Empty, `x` or `unknown` leaves the key unmarked, so it isn't scored.
//! - `bpm` and `note` are optional. Empty `bpm` leaves the tempo unmarked.
//! - Lines starting with `#` are comments. Audio before the first row isn't scored.

use std::path::Path;

use csv::{ReaderBuilder, StringRecord, Trim};
use hunch_core::Key;

use crate::keydata::{self, Exclusion};

const HEADER: [&str; 4] = ["start_seconds", "key", "bpm", "note"];
/// The columns a row needs; `bpm` and `note` may be left off.
const REQUIRED_COLUMNS: usize = 2;
const SECONDS_PER_MINUTE: f32 = 60.0;
/// `h:mm:ss` has at most three parts.
const MAX_TIMESTAMP_PARTS: usize = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub start_seconds: f32,
    /// Keys that count as correct; empty when the key isn't marked.
    pub keys: Vec<Key>,
    pub bpm: Option<f32>,
    pub note: String,
}

pub fn read_marks(path: &Path) -> Result<Vec<Mark>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    parse_marks(&text).map_err(|error| format!("{}: {error}", path.display()))
}

pub fn parse_marks(text: &str) -> Result<Vec<Mark>, String> {
    let mut reader = ReaderBuilder::new()
        .trim(Trim::All)
        .comment(Some(b'#'))
        .flexible(true)
        .from_reader(text.as_bytes());
    let header = reader
        .headers()
        .map_err(|error| format!("cannot read the header: {error}"))?;
    if !header.iter().eq(HEADER) {
        return Err(format!(
            "the header must be `{}`, found `{}`",
            HEADER.join(","),
            header.iter().collect::<Vec<_>>().join(",")
        ));
    }

    let mut marks: Vec<Mark> = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|error| format!("cannot read a row: {error}"))?;
        let line = record.position().map_or(0, csv::Position::line);
        let mark = parse_row(&record).map_err(|error| format!("line {line}: {error}"))?;
        if let Some(previous) = marks.last() {
            if mark.start_seconds <= previous.start_seconds {
                return Err(format!(
                    "line {line}: starts at {} s, not after the previous row's {} s",
                    mark.start_seconds, previous.start_seconds
                ));
            }
        }
        marks.push(mark);
    }
    if marks.is_empty() {
        return Err("no rows".to_string());
    }
    Ok(marks)
}

fn parse_row(record: &StringRecord) -> Result<Mark, String> {
    if record.len() < REQUIRED_COLUMNS || record.len() > HEADER.len() {
        return Err(format!(
            "expected {REQUIRED_COLUMNS} to {} columns, found {}",
            HEADER.len(),
            record.len()
        ));
    }
    let field = |index: usize| record.get(index).unwrap_or_default();
    Ok(Mark {
        start_seconds: parse_time(field(0))?,
        keys: parse_key(field(1))?,
        bpm: parse_bpm(field(2))?,
        note: field(3).to_string(),
    })
}

/// Seconds, or `m:ss` / `h:mm:ss` with optional fractional seconds.
pub fn parse_time(text: &str) -> Result<f32, String> {
    let parts: Vec<&str> = text.split(':').collect();
    if parts.len() > MAX_TIMESTAMP_PARTS {
        return Err(format!("`{text}` is not a time"));
    }
    let mut seconds = 0.0;
    for part in &parts {
        let value: f32 = part
            .parse()
            .map_err(|_| format!("`{text}` is not a time"))?;
        seconds = seconds * SECONDS_PER_MINUTE + value;
    }
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(format!("`{text}` is not a time"));
    }
    Ok(seconds)
}

fn parse_key(text: &str) -> Result<Vec<Key>, String> {
    match keydata::parse_key_label(text) {
        Ok(keys) => Ok(keys),
        Err(Exclusion::NoLabel) => Ok(Vec::new()),
        Err(exclusion) => Err(format!("key `{text}`: {}", exclusion.name())),
    }
}

fn parse_bpm(text: &str) -> Result<Option<f32>, String> {
    if text.is_empty() {
        return Ok(None);
    }
    match text.parse::<f32>() {
        Ok(bpm) if bpm.is_finite() && bpm > 0.0 => Ok(Some(bpm)),
        _ => Err(format!("bpm `{text}` is not a positive number")),
    }
}

#[cfg(test)]
mod tests {
    use hunch_core::{Mode, PitchClass};

    use super::*;

    const A: u8 = 9;
    const C: u8 = 0;

    fn key(index: u8, mode: Mode) -> Key {
        let tonic = PitchClass::new(index).expect("valid pitch class");
        Key { tonic, mode }
    }

    #[test]
    fn parses_rows_with_optional_bpm_and_note() {
        let marks = parse_marks(
            "start_seconds,key,bpm,note\n\
             0,A minor,124\n\
             3:05,C major,,\"second track, no tempo\"\n\
             # a comment\n\
             1:00:00.5,x,128,drop\n",
        )
        .expect("parse");

        assert_eq!(
            marks,
            vec![
                Mark {
                    start_seconds: 0.0,
                    keys: vec![key(A, Mode::Minor)],
                    bpm: Some(124.0),
                    note: String::new(),
                },
                Mark {
                    start_seconds: 185.0,
                    keys: vec![key(C, Mode::Major)],
                    bpm: None,
                    note: "second track, no tempo".to_string(),
                },
                Mark {
                    start_seconds: 3600.5,
                    keys: Vec::new(),
                    bpm: Some(128.0),
                    note: "drop".to_string(),
                },
            ]
        );
    }

    #[test]
    fn keeps_key_alternatives() {
        let marks = parse_marks("start_seconds,key,bpm,note\n0,A minor | C major,,\n").unwrap();
        assert_eq!(
            marks[0].keys,
            vec![key(A, Mode::Minor), key(C, Mode::Major)]
        );
    }

    #[test]
    fn rejects_a_wrong_header() {
        let error = parse_marks("time,key\n0,A minor\n").unwrap_err();
        assert!(error.contains("header"), "{error}");
    }

    #[test]
    fn rejects_starts_that_do_not_increase() {
        let error =
            parse_marks("start_seconds,key,bpm,note\n10,A minor,,\n10,C major,,\n").unwrap_err();
        assert!(error.starts_with("line 3"), "{error}");
    }

    #[test]
    fn rejects_bad_fields_with_their_line() {
        let cases = [
            "start_seconds,key,bpm,note\nsoon,A minor,,\n",
            "start_seconds,key,bpm,note\n0,H minor,,\n",
            "start_seconds,key,bpm,note\n0,A minor,-5,\n",
            "start_seconds,key,bpm,note\n0\n",
        ];
        for text in cases {
            let error = parse_marks(text).unwrap_err();
            assert!(error.starts_with("line 2"), "{text:?}: {error}");
        }
    }

    #[test]
    fn rejects_a_file_without_rows() {
        assert!(parse_marks("start_seconds,key,bpm,note\n").is_err());
    }

    #[test]
    fn parses_seconds_and_timestamps() {
        assert_eq!(parse_time("93.5"), Ok(93.5));
        assert_eq!(parse_time("1:33.5"), Ok(93.5));
        assert_eq!(parse_time("1:02:03"), Ok(3723.0));
        assert!(parse_time("1:2:3:4").is_err());
        assert!(parse_time("-1").is_err());
    }
}
