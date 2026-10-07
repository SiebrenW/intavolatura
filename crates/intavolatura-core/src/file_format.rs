//! The app's own text file format for saving a bass line.
//!
//! ```text
//! intavolatura 1
//! key: D minor
//! instrument: theorbo-a-14
//!
//! D3
//! C#3 | 6
//! A2 | # | pin 1:0 2:0 3:2 6:0
//! ```
//!
//! After the version line come `name: value` headers, a blank line, then
//! one bass note per line: the written pitch, optionally its figuring in
//! canonical form, optionally its pinned fingering as `course:fret` pairs
//! with course 1 first. Unplayed courses are left out.

use std::error::Error;
use std::fmt;

use crate::bass_line::{BassLine, BassNote};
use crate::fingering::Fingering;
use crate::instrument::{Instrument, MAX_COURSES};

const VERSION_LINE: &str = "intavolatura 1";

/// What a saved file holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    pub line: BassLine,
    /// The instrument as defined, before any retuning to the key.
    pub instrument: Instrument,
}

/// Writes a document in the file format.
pub fn write(document: &Document) -> String {
    let mut out = format!(
        "{VERSION_LINE}\nkey: {}\ninstrument: {}\n\n",
        document.line.key(),
        document.instrument.id()
    );
    for note in document.line.notes() {
        out.push_str(&note.pitch.to_string());
        if !note.figuring.is_empty() || note.pin.is_some() {
            out.push_str(" |");
            if !note.figuring.is_empty() {
                out.push(' ');
                out.push_str(&note.figuring.to_string());
            }
        }
        if let Some(pin) = note.pin {
            out.push_str(" | pin");
            for (course, fret) in pin.played() {
                out.push_str(&format!(" {}:{fret}", course + 1));
            }
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

    let (mut key, mut instrument) = (None, None);
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
        line: BassLine::with_notes(key, notes),
        instrument,
    })
}

fn read_note(line: &str, instrument: &Instrument) -> Result<BassNote, &'static str> {
    let mut parts = line.split('|').map(str::trim);
    let pitch = parts
        .next()
        .and_then(|p| p.parse().ok())
        .ok_or("invalid pitch")?;
    let mut note = BassNote::new(pitch);
    if let Some(figuring) = parts.next() {
        note.figuring = figuring.parse().map_err(|_| "invalid figuring")?;
    }
    if let Some(pin) = parts.next() {
        note.pin = Some(read_pin(pin, instrument)?);
    }
    if parts.next().is_some() {
        return Err("too many `|` separators");
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
instrument: theorbo-a-14

D3
C#3 | 6
A2 | # | pin 1:0 2:0 3:2 6:0
G2 | | pin 4:0 5:0 7:0
Bb2 | 6+ 4
";

    #[test]
    fn reads_a_document() {
        let doc = read(SAMPLE).unwrap();
        assert_eq!(doc.line.key().to_string(), "D minor");
        assert_eq!(doc.instrument.id(), "theorbo-a-14");
        let notes = doc.line.notes();
        assert_eq!(notes.len(), 5);
        assert_eq!(notes[1].figuring.to_string(), "6");
        let pin = notes[2].pin.unwrap();
        assert_eq!(
            pin.played().collect::<Vec<_>>(),
            [(0, 0), (1, 0), (2, 2), (5, 0)]
        );
        assert!(notes[3].figuring.is_empty() && notes[3].pin.is_some());
        assert_eq!(notes[4].figuring.to_string(), "6+ 4");
    }

    #[test]
    fn writing_then_reading_round_trips() {
        let doc = read(SAMPLE).unwrap();
        let written = write(&doc);
        assert_eq!(written, SAMPLE);
        assert_eq!(read(&written).unwrap(), doc);
    }

    #[test]
    fn tolerates_surrounding_whitespace_and_blank_note_lines() {
        let text =
            "\n  intavolatura 1\nkey: C major\ninstrument: theorbo-a-14\n\n  C3 |  6 5  \n\n";
        let doc = read(text).unwrap();
        assert_eq!(doc.line.notes()[0].figuring.to_string(), "6 5");
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
                "intavolatura 1\ntempo: 60\n".to_string(),
                2,
                "unknown header",
            ),
            (format!("{header}C3\nX9\n"), 6, "invalid pitch"),
            (format!("{header}C3 | 8\n"), 5, "invalid figuring"),
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
            (format!("{header}C3 | | 1:0\n"), 5, "expected `pin`"),
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
}
