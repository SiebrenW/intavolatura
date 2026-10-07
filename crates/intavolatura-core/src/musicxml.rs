//! MusicXML export (and, later in this module, import).
//!
//! Export writes a partwise MusicXML 4.0 score: the figured bass as one part
//! and, optionally, the realization as a second part on a grand staff.
//! Durations use 8 divisions per quarter, so ticks are written unchanged.

use std::fmt::Write;

use crate::bass_line::BassLine;
use crate::duration::{Duration, NoteValue, WHOLE};
use crate::figuring::{FigureAccidental, Figuring};
use crate::key::Mode;
use crate::pitch::Pitch;
use crate::realization::Realization;

/// Ticks per quarter note: MusicXML's `divisions`.
const DIVISIONS: u32 = WHOLE / 4;
/// Notes at or above this staff position (middle C) go on the treble staff.
const MIDDLE_C: i16 = 28;

/// Something sounding (or silent) for a stretch of time in one staff.
struct Event<'a> {
    start: u32,
    ticks: u32,
    /// Empty for a rest. Each note: pitch, tied from before, tied onwards.
    notes: Vec<(Pitch, bool, bool)>,
    /// Figurings and how long each lasts, for the figured bass part.
    figurings: Vec<(&'a Figuring, u32)>,
    /// A rest that only fills time on a staff the chord does not use, so
    /// it is not printed.
    filler: bool,
}

/// Writes `line` as MusicXML. With `realizations` (one per note), a second
/// part holds the realization on a grand staff.
pub fn export(line: &BassLine, realizations: Option<&[Realization]>, title: &str) -> String {
    let mut out = String::new();
    out.push_str(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n",
        "<!DOCTYPE score-partwise PUBLIC \"-//Recordare//DTD MusicXML 4.0 Partwise//EN\" ",
        "\"http://www.musicxml.org/dtds/partwise.dtd\">\n",
        "<score-partwise version=\"4.0\">\n",
    ));
    let _ = writeln!(
        out,
        "  <work><work-title>{}</work-title></work>",
        escape(title)
    );
    out.push_str("  <identification><encoding><software>Intavolatura</software></encoding></identification>\n");
    out.push_str("  <part-list>\n");
    out.push_str("    <score-part id=\"P1\"><part-name>Basso continuo</part-name></score-part>\n");
    if realizations.is_some() {
        out.push_str("    <score-part id=\"P2\"><part-name>Realization</part-name></score-part>\n");
    }
    out.push_str("  </part-list>\n");

    let bass = bass_events(line);
    write_part(&mut out, line, "P1", &[bass], false);
    if let Some(realizations) = realizations {
        let (treble, lower) = realization_events(line, realizations);
        write_part(&mut out, line, "P2", &[treble, lower], true);
    }
    out.push_str("</score-partwise>\n");
    out
}

fn bass_events(line: &BassLine) -> Vec<Event<'_>> {
    line.notes()
        .iter()
        .zip(line.placements())
        .map(|(note, placement)| {
            let ticks = note.duration.ticks();
            let parts = note.figurings.parts();
            let figurings = if note.rest || note.figurings.is_empty() {
                Vec::new()
            } else {
                let each = ticks / parts.len() as u32;
                parts.iter().map(|f| (f, each)).collect()
            };
            Event {
                start: placement.start,
                ticks,
                notes: if note.rest {
                    Vec::new()
                } else {
                    vec![(note.pitch, false, false)]
                },
                figurings,
                filler: false,
            }
        })
        .collect()
}

/// The realization as two staves of events: treble and bass.
fn realization_events<'a>(
    line: &'a BassLine,
    realizations: &[Realization],
) -> (Vec<Event<'a>>, Vec<Event<'a>>) {
    let (mut treble, mut lower) = (Vec::new(), Vec::new());
    for ((note, placement), realization) in
        line.notes().iter().zip(line.placements()).zip(realizations)
    {
        let ticks = note.duration.ticks();
        if note.rest || realization.shown.is_none() {
            for staff in [&mut treble, &mut lower] {
                staff.push(Event {
                    start: placement.start,
                    ticks,
                    notes: Vec::new(),
                    figurings: Vec::new(),
                    filler: false,
                });
            }
            continue;
        }
        let each = ticks / realization.parts.len() as u32;
        for (k, part) in realization.parts.iter().enumerate() {
            for (staff, on_treble) in [(&mut treble, true), (&mut lower, false)] {
                let notes: Vec<(Pitch, bool, bool)> = part
                    .notes
                    .iter()
                    .filter(|(p, _)| (p.staff_position() >= MIDDLE_C) == on_treble)
                    .map(|&(p, struck)| (p, k > 0 && !struck, false))
                    .collect();
                // A note held into this part is tied from the previous one.
                if k > 0
                    && let Some(previous) = staff.last_mut()
                {
                    for held in notes.iter().filter(|n| n.1).map(|n| n.0) {
                        for n in previous.notes.iter_mut().filter(|n| n.0 == held) {
                            n.2 = true;
                        }
                    }
                }
                staff.push(Event {
                    start: placement.start + k as u32 * each,
                    ticks: each,
                    filler: notes.is_empty(),
                    notes,
                    figurings: Vec::new(),
                });
            }
        }
    }
    (treble, lower)
}

/// Writes one part, measure by measure, with one event list per staff.
fn write_part(out: &mut String, line: &BassLine, id: &str, staves: &[Vec<Event<'_>>], grand: bool) {
    let bar = line.time().bar_ticks();
    let end = staves
        .iter()
        .flat_map(|s| s.last())
        .map(|e| e.start + e.ticks)
        .max()
        .unwrap_or(0);
    let measures = end.div_ceil(bar).max(1);

    let _ = writeln!(out, "  <part id=\"{id}\">");
    for m in 0..measures {
        let (from, to) = (m * bar, (m + 1) * bar);
        let _ = writeln!(out, "    <measure number=\"{}\">", m + 1);
        if m == 0 {
            write_attributes(out, line, grand);
        }
        for (s, staff) in staves.iter().enumerate() {
            let mut written = 0;
            for event in staff {
                let (start, stop) = (event.start.max(from), (event.start + event.ticks).min(to));
                if start >= stop {
                    continue;
                }
                if start == event.start {
                    write_figured_bass(out, event);
                }
                let pieces = pieces(stop - start);
                for (j, piece) in pieces.iter().enumerate() {
                    let first = start == event.start && j == 0;
                    let last = stop == event.start + event.ticks && j == pieces.len() - 1;
                    let staff_number = grand.then_some(s + 1);
                    write_notes(out, event, *piece, first, last, staff_number);
                    written += piece.ticks();
                }
            }
            if s + 1 < staves.len() && written > 0 {
                let _ = writeln!(out, "      <backup><duration>{written}</duration></backup>");
            }
        }
        out.push_str("    </measure>\n");
    }
    out.push_str("  </part>\n");
}

fn write_attributes(out: &mut String, line: &BassLine, grand: bool) {
    let key = line.key();
    let time = line.time();
    let mode = match key.mode() {
        Mode::Major => "major",
        Mode::Minor => "minor",
    };
    let symbol = match (time.beats, time.beat_value) {
        (4, 4) => " symbol=\"common\"",
        (2, 2) => " symbol=\"cut\"",
        _ => "",
    };
    out.push_str("      <attributes>\n");
    let _ = writeln!(out, "        <divisions>{DIVISIONS}</divisions>");
    let _ = writeln!(
        out,
        "        <key><fifths>{}</fifths><mode>{mode}</mode></key>",
        key.signature()
    );
    let _ = writeln!(
        out,
        "        <time{symbol}><beats>{}</beats><beat-type>{}</beat-type></time>",
        time.beats, time.beat_value
    );
    if grand {
        out.push_str("        <staves>2</staves>\n");
        out.push_str("        <clef number=\"1\"><sign>G</sign><line>2</line></clef>\n");
        out.push_str("        <clef number=\"2\"><sign>F</sign><line>4</line></clef>\n");
    } else {
        out.push_str("        <clef><sign>F</sign><line>4</line></clef>\n");
    }
    out.push_str("      </attributes>\n");
}

fn write_figured_bass(out: &mut String, event: &Event<'_>) {
    for (figuring, ticks) in &event.figurings {
        out.push_str("      <figured-bass>\n");
        for figure in figuring.figures() {
            out.push_str("        <figure>");
            let prefix = match figure.accidental {
                Some(FigureAccidental::Sharp) => Some("sharp"),
                Some(FigureAccidental::Flat) => Some("flat"),
                Some(FigureAccidental::Natural) => Some("natural"),
                _ => None,
            };
            if let Some(prefix) = prefix {
                let _ = write!(out, "<prefix>{prefix}</prefix>");
            }
            if figure.number_shown {
                let _ = write!(out, "<figure-number>{}</figure-number>", figure.interval);
            }
            if figure.accidental == Some(FigureAccidental::Raised) {
                out.push_str("<suffix>slash</suffix>");
            }
            out.push_str("</figure>\n");
        }
        let _ = writeln!(out, "        <duration>{ticks}</duration>");
        out.push_str("      </figured-bass>\n");
    }
}

/// Writes a chord (or rest) of one piece of an event.
fn write_notes(
    out: &mut String,
    event: &Event<'_>,
    piece: Duration,
    first: bool,
    last: bool,
    staff: Option<usize>,
) {
    let voice = staff.map_or(1, |s| if s == 1 { 1 } else { 5 });
    let kind = match piece.value {
        NoteValue::Whole => "whole",
        NoteValue::Half => "half",
        NoteValue::Quarter => "quarter",
        NoteValue::Eighth => "eighth",
        NoteValue::Sixteenth => "16th",
    };
    let tail = |out: &mut String| {
        let _ = write!(out, "<voice>{voice}</voice><type>{kind}</type>");
        if piece.dotted {
            out.push_str("<dot/>");
        }
        if let Some(staff) = staff {
            let _ = write!(out, "<staff>{staff}</staff>");
        }
    };
    if event.notes.is_empty() {
        let hidden = if event.filler {
            " print-object=\"no\""
        } else {
            ""
        };
        let _ = write!(
            out,
            "      <note{hidden}><rest/><duration>{}</duration>",
            piece.ticks()
        );
        tail(out);
        out.push_str("</note>\n");
        return;
    }
    for (i, &(pitch, tied_in, tied_out)) in event.notes.iter().enumerate() {
        let stop = !first || tied_in;
        let start = !last || tied_out;
        out.push_str("      <note>");
        if i > 0 {
            out.push_str("<chord/>");
        }
        let _ = write!(out, "<pitch><step>{}</step>", pitch.name.letter.as_char());
        let alter = pitch.name.accidental.semitones();
        if alter != 0 {
            let _ = write!(out, "<alter>{alter}</alter>");
        }
        let _ = write!(out, "<octave>{}</octave></pitch>", pitch.octave);
        let _ = write!(out, "<duration>{}</duration>", piece.ticks());
        if stop {
            out.push_str("<tie type=\"stop\"/>");
        }
        if start {
            out.push_str("<tie type=\"start\"/>");
        }
        tail(out);
        if stop || start {
            out.push_str("<notations>");
            if stop {
                out.push_str("<tied type=\"stop\"/>");
            }
            if start {
                out.push_str("<tied type=\"start\"/>");
            }
            out.push_str("</notations>");
        }
        out.push_str("</note>\n");
    }
}

/// Splits a length into note values, longest first, so that any stretch
/// can be written: 20 ticks become a half and an eighth.
fn pieces(mut ticks: u32) -> Vec<Duration> {
    let mut out = Vec::new();
    while ticks > 0 {
        let piece = NoteValue::ALL
            .into_iter()
            .flat_map(|v| [Duration::dotted(v), Duration::new(v)])
            .find(|d| d.ticks() <= ticks)
            .unwrap_or(Duration::new(NoteValue::Sixteenth));
        out.push(piece);
        ticks = ticks.saturating_sub(piece.ticks());
    }
    out
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_format;
    use crate::instrument::Instrument;
    use crate::realization::{Settings, realize};

    fn document(text: &str) -> BassLine {
        file_format::read(text).unwrap().line
    }

    fn realizations(line: &BassLine) -> Vec<Realization> {
        let settings = Settings::default();
        let instrument = settings.tune(&Instrument::theorbo_in_a(), line.key());
        line.notes()
            .iter()
            .map(|n| realize(n, line.key(), &instrument, &settings))
            .collect()
    }

    const LINE: &str = "intavolatura 1\nkey: D minor\ntime: 3/2\ninstrument: theorbo-a-14\n\n\
        D3 2\nC#3 4 | #6\nr 4\nA2 1 | 4-#\nD2 1.\n";

    #[test]
    fn writes_the_figured_bass_part() {
        let xml = export(&document(LINE), None, "Test & <more>");
        assert!(xml.contains("<work-title>Test &amp; &lt;more&gt;</work-title>"));
        assert!(xml.contains("<fifths>-1</fifths><mode>minor</mode>"));
        assert!(xml.contains("<time><beats>3</beats><beat-type>2</beat-type></time>"));
        assert!(xml.contains("<divisions>8</divisions>"));
        assert!(!xml.contains("P2"));
        // C#3 with a sharp 6.
        assert!(xml.contains("<step>C</step><alter>1</alter><octave>3</octave>"));
        assert!(
            xml.contains("<figure><prefix>sharp</prefix><figure-number>6</figure-number></figure>")
        );
        // The suspension: two figured-bass elements of a half note each,
        // the second a lone sharp.
        assert!(xml.contains(
            "<figure-number>4</figure-number></figure>\n        <duration>16</duration>"
        ));
        assert!(
            xml.contains(
                "<figure><prefix>sharp</prefix></figure>\n        <duration>16</duration>"
            )
        );
        assert!(xml.contains("<note><rest/><duration>8</duration>"));
    }

    #[test]
    fn notes_over_a_bar_line_are_split_and_tied() {
        let xml = export(&document(LINE), None, "t");
        // Bar 1: half, quarter, quarter rest, then the whole note's first
        // half; bar 2 starts with its tied second half.
        let bars: Vec<&str> = xml.split("<measure number=").collect();
        assert_eq!(bars.len(), 1 + 3);
        assert!(bars[1].contains(
            "<step>A</step><octave>2</octave></pitch><duration>16</duration><tie type=\"start\"/>"
        ));
        assert!(bars[2].starts_with("\"2\">"));
        assert!(bars[2].contains(
            "<step>A</step><octave>2</octave></pitch><duration>16</duration><tie type=\"stop\"/>"
        ));
        // The dotted whole D2 (48 ticks) starts after beat 2 of bar 2: 32
        // ticks there, tied to 16 in bar 3.
        assert!(bars[2].contains(
            "<step>D</step><octave>2</octave></pitch><duration>32</duration><tie type=\"start\"/>"
        ));
        assert!(bars[3].contains(
            "<step>D</step><octave>2</octave></pitch><duration>16</duration><tie type=\"stop\"/>"
        ));
    }

    #[test]
    fn writes_the_realization_on_a_grand_staff_with_ties_for_held_notes() {
        let line = document(LINE);
        let r = realizations(&line);
        let xml = export(&line, Some(&r), "t");
        assert!(xml.contains("<score-part id=\"P2\">"));
        assert!(xml.contains("<staves>2</staves>"));
        assert!(xml.contains("<backup>"));
        assert!(xml.contains("<chord/>"));
        // Treble filler rests are hidden; the bass line's own rest shows.
        assert!(xml.contains("<note print-object=\"no\"><rest/>"));
        assert!(xml.matches("<note><rest/>").count() >= 3);
        assert!(xml.contains("<staff>2</staff>"));
        // The 4-# suspension: held notes are tied across its two parts.
        let p2 = &xml[xml.find("<part id=\"P2\">").unwrap()..];
        assert!(p2.contains("<tied type=\"start\"/>"));
        assert!(p2.contains("<tied type=\"stop\"/>"));
    }

    #[test]
    fn output_is_well_formed_xml() {
        let line = document(LINE);
        let r = realizations(&line);
        for xml in [export(&line, None, "a"), export(&line, Some(&r), "b")] {
            let body = &xml[xml.find("<score-partwise").unwrap()..];
            let mut stack = Vec::new();
            let mut rest = body;
            while let Some(open) = rest.find('<') {
                let close = rest[open..].find('>').unwrap() + open;
                let tag = &rest[open + 1..close];
                rest = &rest[close + 1..];
                if tag.ends_with('/') {
                    continue;
                }
                let name = tag.split_whitespace().next().unwrap();
                if let Some(name) = name.strip_prefix('/') {
                    assert_eq!(stack.pop(), Some(name.to_string()));
                } else {
                    stack.push(name.to_string());
                }
            }
            assert!(stack.is_empty(), "{stack:?}");
        }
    }

    #[test]
    fn lengths_split_into_note_values() {
        let ticks = |n: u32| pieces(n).iter().map(|d| d.to_string()).collect::<Vec<_>>();
        assert_eq!(ticks(20), ["2", "8"]);
        assert_eq!(ticks(48), ["1."]);
        assert_eq!(ticks(12), ["4."]);
    }
}
