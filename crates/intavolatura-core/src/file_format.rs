//! The app's own text file format for saving a bass line.
//!
//! ```text
//! intavolatura 1
//! key: D minor
//! time: 3/2
//! tempo: 72
//! instrument: theorbo-a-14
//!
//! D3 2
//! C#3 4 | 6
//! A2 2. | # | pin 1:0 2:0 3:2 6:0
//! ```
//!
//! After the version line come `name: value` headers (`time` defaults to
//! 4/4; `tempo` is quarter notes a minute and optional), a blank line, then one bass note per line: the written pitch and
//! its duration (`4` for a quarter, `2.` for a dotted half; a quarter if
//! left out), optionally its figurings in canonical form, optionally its
//! pinned fingering as `course:fret` pairs with course 1 first. Unplayed
//! courses are left out. A rest is `r` and its duration.

use std::error::Error;
use std::fmt;

use crate::bass_line::{BassLine, BassNote};
use crate::duration::{Duration, TimeSignature};
use crate::fingering::Fingering;
use crate::instrument::{Instrument, MAX_COURSES};

/// The tempos a file may ask for, in quarter notes a minute.
pub const TEMPOS: std::ops::RangeInclusive<u16> = 20..=300;

const VERSION_LINE: &str = "intavolatura 1";

/// What a saved file holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    pub line: BassLine,
    /// The instrument as defined, before any retuning to the key.
    pub instrument: Instrument,
    /// Quarter notes a minute for playback, if the file sets one.
    pub tempo: Option<u16>,
}

/// Writes a document in the file format.
pub fn write(document: &Document) -> String {
    let mut out = format!(
        "{VERSION_LINE}\nkey: {}\ntime: {}\n",
        document.line.key(),
        document.line.time()
    );
    if let Some(upbeat) = document.line.upbeat() {
        out.push_str(&format!("upbeat: {upbeat}\n"));
    }
    if let Some(tempo) = document.tempo {
        out.push_str(&format!("tempo: {tempo}\n"));
    }
    out.push_str(&format!("instrument: {}\n\n", document.instrument.id()));
    for note in document.line.notes() {
        if note.rest {
            out.push_str(&format!("r {}", note.duration));
        } else {
            out.push_str(&format!("{} {}", note.pitch, note.duration));
        }
        let extras = note.pin.is_some() || note.time_change.is_some() || note.key_change.is_some();
        if !note.figurings.is_empty() || extras {
            out.push_str(" |");
            if !note.figurings.is_empty() {
                out.push(' ');
                out.push_str(&note.figurings.to_string());
            }
        }
        if let Some(pin) = note.pin {
            out.push_str(" | pin");
            for (course, fret) in pin.played() {
                out.push_str(&format!(" {}:{fret}", course + 1));
            }
        }
        if let Some(time) = note.time_change {
            out.push_str(&format!(" | time {time}"));
        }
        if let Some(key) = note.key_change {
            out.push_str(&format!(" | key {key}"));
        }
        out.push('\n');
    }
    out
}

/// Reads a document from the file format.
pub fn read(text: &str) -> Result<Document, ReadError> {
    let mut lines = text.lines().enumerate().map(|(i, l)| (i + 1, l.trim()));
    let error = |line: usize, message: &str| ReadError {
        line,
        message: message.to_string(),
    };

    match lines.by_ref().find(|(_, l)| !l.is_empty()) {
        Some((_, VERSION_LINE)) => {}
        Some((n, _)) => return Err(error(n, "not an intavolatura 1 file")),
        None => return Err(error(1, "empty file")),
    }

    let (mut key, mut instrument, mut time) = (None, None, TimeSignature::COMMON);
    let (mut upbeat, mut tempo) = (None, None);
    for (n, l) in lines.by_ref() {
        if l.is_empty() {
            break;
        }
        let (name, value) = l
            .split_once(':')
            .ok_or_else(|| error(n, "expected `name: value`"))?;
        let value = value.trim();
        match name.trim() {
            "key" => key = Some(value.parse().map_err(|_| error(n, "invalid key"))?),
            "upbeat" => {
                upbeat = Some(value.parse().map_err(|_| error(n, "invalid upbeat"))?);
            }
            "tempo" => {
                tempo = Some(
                    value
                        .parse::<u16>()
                        .ok()
                        .filter(|t| TEMPOS.contains(t))
                        .ok_or_else(|| error(n, "invalid tempo"))?,
                );
            }
            "time" => {
                time = value
                    .parse()
                    .map_err(|_| error(n, "invalid time signature"))?
            }
            "instrument" => {
                instrument =
                    Some(Instrument::by_id(value).ok_or_else(|| error(n, "unknown instrument"))?);
            }
            _ => return Err(error(n, "unknown header")),
        }
    }
    let key = key.ok_or_else(|| error(1, "missing key"))?;
    let instrument = instrument.ok_or_else(|| error(1, "missing instrument"))?;

    let mut notes = Vec::new();
    for (n, l) in lines.filter(|(_, l)| !l.is_empty()) {
        notes.push(read_note(l, &instrument).map_err(|m| error(n, m))?);
    }
    Ok(Document {
        line: BassLine::with_notes(key, time, notes).with_upbeat(upbeat),
        instrument,
        tempo,
    })
}

fn read_note(line: &str, instrument: &Instrument) -> Result<BassNote, &'static str> {
    let mut parts = line.split('|').map(str::trim);
    let mut head = parts.next().unwrap_or_default().split_whitespace();
    let first = head.next().ok_or("invalid pitch")?;
    let mut note = if first == "r" {
        BassNote::rest(Duration::QUARTER)
    } else {
        BassNote::new(first.parse().map_err(|_| "invalid pitch")?)
    };
    if let Some(duration) = head.next() {
        note.duration = duration.parse().map_err(|_| "invalid duration")?;
    }
    if head.next().is_some() {
        return Err("expected a pitch and a duration");
    }
    if let Some(figuring) = parts.next()
        && !note.rest
    {
        note.figurings = figuring.parse().map_err(|_| "invalid figuring")?;
    }
    // Then optional sections, each named by its first word.
    for section in parts {
        if let Some(time) = section.strip_prefix("time") {
            note.time_change = Some(time.trim().parse().map_err(|_| "invalid time signature")?);
        } else if let Some(key) = section.strip_prefix("key") {
            note.key_change = Some(key.trim().parse().map_err(|_| "invalid key")?);
        } else if section.starts_with("pin") && !note.rest {
            note.pin = Some(read_pin(section, instrument)?);
        } else {
            return Err("expected `pin`, `time` or `key`");
        }
    }
    Ok(note)
}

fn read_pin(text: &str, instrument: &Instrument) -> Result<Fingering, &'static str> {
    let stops = text.strip_prefix("pin").ok_or("expected `pin`")?;
    let mut frets = [None; MAX_COURSES];
    for stop in stops.split_whitespace() {
        let (course, fret) = stop.split_once(':').ok_or("expected `course:fret`")?;
        let course: usize = course.parse().map_err(|_| "invalid course")?;
        let fret: u8 = fret.parse().map_err(|_| "invalid fret")?;
        let c = course
            .checked_sub(1)
            .and_then(|i| instrument.courses().get(i))
            .ok_or("no such course")?;
        if fret > c.frets {
            return Err("fret beyond the course");
        }
        if frets[course - 1].replace(fret).is_some() {
            return Err("course pinned twice");
        }
    }
    Ok(Fingering::from_frets(frets))
}

/// Why a file could not be read, with the 1-based line where it went wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl Error for ReadError {}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
intavolatura 1
key: D minor
time: 3/2
upbeat: 2
instrument: theorbo-a-14

D3 2
C#3 4 | 6
A2 2. | # | pin 1:0 2:0 3:2 6:0
G2 4 | | pin 4:0 5:0 7:0
Bb2 8 | 6+ 4-5 3 | time 3/4
r 2 | | time 3/2
";

    #[test]
    fn reads_a_document() {
        let doc = read(SAMPLE).unwrap();
        assert_eq!(doc.line.key().to_string(), "D minor");
        assert_eq!(doc.instrument.id(), "theorbo-a-14");
        let notes = doc.line.notes();
        assert_eq!(notes.len(), 6);
        assert!(notes[5].rest && notes[5].duration.to_string() == "2");
        assert_eq!(
            notes[4].time_change.map(|t| t.to_string()),
            Some("3/4".into())
        );
        assert_eq!(
            notes[5].time_change.map(|t| t.to_string()),
            Some("3/2".into())
        );
        assert_eq!(notes[1].figurings.to_string(), "6");
        let pin = notes[2].pin.unwrap();
        assert_eq!(
            pin.played().collect::<Vec<_>>(),
            [(0, 0), (1, 0), (2, 2), (5, 0)]
        );
        assert!(notes[3].figurings.is_empty() && notes[3].pin.is_some());
        assert_eq!(notes[4].figurings.to_string(), "6+ 4-5 3");
        assert_eq!(doc.line.time().to_string(), "3/2");
        assert_eq!(
            doc.line.upbeat().map(|d| d.to_string()),
            Some("2".to_string())
        );
        assert_eq!(notes[2].duration.to_string(), "2.");
    }

    #[test]
    fn key_changes_round_trip() {
        let text = "intavolatura 1\nkey: D minor\ntime: 4/4\ninstrument: theorbo-a-14\n\nD3 4\nC3 4 | | time 3/4 | key C major\nG2 4 | | key A minor\n";
        let doc = read(text).unwrap();
        let keys: Vec<String> = doc.line.keys().iter().map(|k| k.to_string()).collect();
        assert_eq!(keys, ["D minor", "C major", "A minor"]);
        assert_eq!(write(&doc), text);
    }

    #[test]
    fn triplets_round_trip() {
        let text = "intavolatura 1\nkey: C major\ntime: 4/4\ninstrument: theorbo-a-14\n\nC3 8t\nD3 8t\nE3 8t\n";
        let doc = read(text).unwrap();
        assert!(doc.line.notes().iter().all(|n| n.duration.triplet));
        assert_eq!(write(&doc), text);
    }

    #[test]
    fn writing_then_reading_round_trips() {
        let doc = read(SAMPLE).unwrap();
        let written = write(&doc);
        assert_eq!(written, SAMPLE);
        assert_eq!(read(&written).unwrap(), doc);
    }

    #[test]
    fn older_files_without_durations_read_as_quarters_in_four_four() {
        let doc =
            read("intavolatura 1\nkey: C major\ninstrument: theorbo-a-14\n\nC3\nD3 | 6\n").unwrap();
        assert_eq!(doc.line.time(), TimeSignature::COMMON);
        assert!(
            doc.line
                .notes()
                .iter()
                .all(|n| n.duration.to_string() == "4")
        );
    }

    #[test]
    fn tolerates_surrounding_whitespace_and_blank_note_lines() {
        let text =
            "\n  intavolatura 1\nkey: C major\ninstrument: theorbo-a-14\n\n  C3 |  6 5  \n\n";
        let doc = read(text).unwrap();
        assert_eq!(doc.line.notes()[0].figurings.to_string(), "6 5");
    }

    #[test]
    fn reports_the_line_of_an_error() {
        let header = "intavolatura 1\nkey: C major\ninstrument: theorbo-a-14\n\n";
        let cases = [
            ("hello\n".to_string(), 1, "not an intavolatura 1 file"),
            (
                "intavolatura 1\nkey: H major\n".to_string(),
                2,
                "invalid key",
            ),
            (
                "intavolatura 1\nkey: C major\ninstrument: lute\n".to_string(),
                3,
                "unknown instrument",
            ),
            (
                "intavolatura 1\ncolour: red\n".to_string(),
                2,
                "unknown header",
            ),
            (format!("{header}C3\nX9\n"), 6, "invalid pitch"),
            (format!("{header}C3 | 1\n"), 5, "invalid figuring"),
            (format!("{header}C3 3\n"), 5, "invalid duration"),
            (
                format!("{header}C3 4 4\n"),
                5,
                "expected a pitch and a duration",
            ),
            (
                "intavolatura 1\ntime: 3/3\n".to_string(),
                2,
                "invalid time signature",
            ),
            (format!("{header}C3 | | pin 15:0\n"), 5, "no such course"),
            (
                format!("{header}C3 | | pin 7:2\n"),
                5,
                "fret beyond the course",
            ),
            (
                format!("{header}C3 | | pin 1:0 1:2\n"),
                5,
                "course pinned twice",
            ),
            (
                format!("{header}C3 | | 1:0\n"),
                5,
                "expected `pin`, `time` or `key`",
            ),
        ];
        for (text, line, message) in cases {
            let error = read(&text).unwrap_err();
            assert_eq!(
                (error.line, error.message.as_str()),
                (line, message),
                "{text:?}"
            );
        }
    }

    #[test]
    fn missing_headers_are_errors() {
        assert_eq!(
            read("intavolatura 1\nkey: C major\n").unwrap_err().message,
            "missing instrument"
        );
        assert_eq!(
            read("intavolatura 1\n\nC3\n").unwrap_err().message,
            "missing key"
        );
    }

    #[test]
    fn the_tempo_is_optional_and_checked() {
        let header = "intavolatura 1\nkey: C major\n";
        let with = |t: &str| format!("{header}tempo: {t}\ninstrument: theorbo-a-14\n\nC3\n");
        let doc = read(&with("96")).unwrap();
        assert_eq!(doc.tempo, Some(96));
        assert_eq!(read(&write(&doc)).unwrap().tempo, Some(96));
        assert_eq!(
            read("intavolatura 1\nkey: C major\ninstrument: theorbo-a-14\n\nC3\n")
                .unwrap()
                .tempo,
            None
        );
        assert!(!write(&read(&with("96")).unwrap()).is_empty());
        assert_eq!(read(&with("5")).unwrap_err().message, "invalid tempo");
        assert_eq!(read(&with("fast")).unwrap_err().message, "invalid tempo");
    }
}
