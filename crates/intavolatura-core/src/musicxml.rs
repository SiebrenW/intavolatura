//! MusicXML export and import.
//!
//! Export writes a partwise MusicXML 4.0 score: the figured bass as one part
//! and, optionally, the realization as a second part on a grand staff.
//! Durations use 8 divisions per quarter, so ticks are written unchanged.
//!
//! Import reads the lowest sounding note of one part, or of all parts, as a
//! bass line with its figures, and reports everything it had to change to fit the app's model.

use std::collections::HashMap;
use std::fmt::Write;
use std::io::Read;
use std::path::Path;

use crate::bass_line::BassLine;
use crate::duration::TimeSignature;
use crate::duration::{Duration, NoteValue, WHOLE};
use crate::figuring::{FigureAccidental, Figuring};
use crate::file_format::Document;
use crate::key::Key;
use crate::key::Mode;
use crate::pitch::Pitch;
use crate::pitch::{Accidental, Letter};
use crate::realization::Realization;
use crate::reduction;
use crate::timing::{BarSpan, timing};

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
                let lengths = note
                    .figurings
                    .part_ticks(ticks)
                    .unwrap_or_else(|| vec![ticks / parts.len() as u32; parts.len()]);
                parts.iter().zip(lengths).collect()
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
    // Measures follow the same bars as the screen: an upbeat is measure 0.
    let mut bars = timing(line).bars;
    if bars.is_empty() {
        bars.push(BarSpan {
            start: 0,
            length: line.time().bar_ticks(),
            time: line.time(),
            upbeat: false,
            incomplete: false,
        });
    }
    let first = if bars[0].upbeat { 0 } else { 1 };

    let _ = writeln!(out, "  <part id=\"{id}\">");
    for (m, bar) in bars.iter().enumerate() {
        let (from, to) = (bar.start, bar.start + bar.length);
        let implicit = if bar.upbeat { " implicit=\"yes\"" } else { "" };
        let _ = writeln!(out, "    <measure number=\"{}\"{implicit}>", m + first);
        if m == 0 {
            write_attributes(out, line, grand);
        } else if bar.time != bars[m - 1].time {
            out.push_str("      <attributes>");
            write_time(out, bar.time);
            out.push_str("</attributes>\n");
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
                let pieces = Duration::pieces(stop - start);
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

    out.push_str("      <attributes>\n");
    let _ = writeln!(out, "        <divisions>{DIVISIONS}</divisions>");
    let _ = writeln!(
        out,
        "        <key><fifths>{}</fifths><mode>{mode}</mode></key>",
        key.signature()
    );
    out.push_str("        ");
    write_time(out, time);
    out.push('\n');
    if grand {
        out.push_str("        <staves>2</staves>\n");
        out.push_str("        <clef number=\"1\"><sign>G</sign><line>2</line></clef>\n");
        out.push_str("        <clef number=\"2\"><sign>F</sign><line>4</line></clef>\n");
    } else {
        out.push_str("        <clef><sign>F</sign><line>4</line></clef>\n");
    }
    out.push_str("      </attributes>\n");
}

fn write_time(out: &mut String, time: TimeSignature) {
    let symbol = match (time.beats, time.beat_value) {
        (4, 4) => " symbol=\"common\"",
        (2, 2) => " symbol=\"cut\"",
        _ => "",
    };
    let _ = write!(
        out,
        "<time{symbol}><beats>{}</beats><beat-type>{}</beat-type></time>",
        time.beats, time.beat_value
    );
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

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A part of a MusicXML score, as offered for import.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartInfo {
    pub id: String,
    pub name: String,
    pub has_figures: bool,
}

/// A bass line read from MusicXML, and what had to change to read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub document: Document,
    pub changes: Vec<String>,
}

/// The MusicXML text of a file: plain `.musicxml`/`.xml`, or the root
/// file of a compressed `.mxl`.
pub fn read_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if !bytes.starts_with(b"PK") {
        return String::from_utf8(bytes).map_err(|_| "the file is not UTF-8 text".to_string());
    }
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    fn read<R: std::io::Read + std::io::Seek>(
        archive: &mut zip::ZipArchive<R>,
        name: &str,
    ) -> Option<String> {
        let mut text = String::new();
        archive.by_name(name).ok()?.read_to_string(&mut text).ok()?;
        Some(text)
    }
    // META-INF/container.xml names the score; otherwise take the first XML file.
    let root = read(&mut archive, "META-INF/container.xml").and_then(|container| {
        let doc = roxmltree::Document::parse(&container).ok()?;
        let path = doc
            .descendants()
            .find(|n| n.has_tag_name("rootfile"))?
            .attribute("full-path")?
            .to_string();
        Some(path)
    });
    let name = match root {
        Some(name) => name,
        None => (0..archive.len())
            .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
            .find(|n| {
                !n.starts_with("META-INF") && (n.ends_with(".xml") || n.ends_with(".musicxml"))
            })
            .ok_or("no score found in the .mxl file")?,
    };
    read(&mut archive, &name).ok_or_else(|| format!("could not read {name} from the .mxl file"))
}

fn parse(xml: &str) -> Result<roxmltree::Document<'_>, String> {
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let doc = roxmltree::Document::parse_with_options(xml, options).map_err(|e| e.to_string())?;
    match doc.root_element().tag_name().name() {
        "score-partwise" => Ok(doc),
        "score-timewise" => Err("timewise MusicXML is not supported".into()),
        _ => Err("not a MusicXML score".into()),
    }
}

fn child<'a, 'i>(node: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
    node.children().find(|n| n.has_tag_name(name))
}

fn child_text<'a>(node: roxmltree::Node<'a, '_>, name: &str) -> Option<&'a str> {
    child(node, name).and_then(|n| n.text()).map(str::trim)
}

/// The parts of a score, in order, for choosing one to import.
pub fn parts(xml: &str) -> Result<Vec<PartInfo>, String> {
    let doc = parse(xml)?;
    let root = doc.root_element();
    let names: Vec<(String, String)> = child(root, "part-list")
        .into_iter()
        .flat_map(|list| list.children().filter(|n| n.has_tag_name("score-part")))
        .map(|p| {
            let id = p.attribute("id").unwrap_or_default().to_string();
            let name = child_text(p, "part-name").unwrap_or_default().to_string();
            (id, name)
        })
        .collect();
    let mut found: Vec<PartInfo> = names
        .into_iter()
        .map(|(id, name)| {
            let part = root
                .children()
                .find(|n| n.has_tag_name("part") && n.attribute("id") == Some(id.as_str()));
            let has_figures =
                part.is_some_and(|p| p.descendants().any(|n| n.has_tag_name("figured-bass")));
            // Names can span lines, with a range under the voice.
            let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
            let name = if name.is_empty() { id.clone() } else { name };
            PartInfo {
                id,
                name,
                has_figures,
            }
        })
        .collect();
    if found.len() > 1 {
        found.push(PartInfo {
            id: ALL_PARTS.into(),
            name: "All parts, lowest note".into(),
            has_figures: false,
        });
    }
    Ok(found)
}

/// The id that asks [`import`] for the lowest sounding note of every part.
pub const ALL_PARTS: &str = "*";

/// A moment in a score: ticks into a measure.
#[derive(Clone, Copy)]
struct At {
    measure: usize,
    ticks: f64,
}

/// A pitched note as read, with ties already joined.
struct Sounding {
    start: At,
    end: At,
    pitch: Pitch,
}

/// One `<figured-bass>` as read.
struct Figure {
    at: At,
    text: String,
    ticks: Option<f64>,
}

/// What one part says, before parts are combined and durations fitted.
#[derive(Default)]
struct PartRead {
    /// Each measure's length in ticks, and its printed number.
    lengths: Vec<f64>,
    numbers: Vec<String>,
    implicit_first: bool,
    sounding: Vec<Sounding>,
    figures: Vec<Figure>,
    key: Option<Key>,
    time: Option<TimeSignature>,
    /// Later meters, from the measure that starts them.
    time_changes: Vec<(usize, TimeSignature)>,
    graces: usize,
}

fn ticks_of(duration: f64, divisions: f64) -> f64 {
    duration * DIVISIONS as f64 / divisions
}

fn read_part(part: roxmltree::Node<'_, '_>, changes: &mut Vec<String>) -> PartRead {
    let mut read = PartRead::default();
    let mut divisions = 1.0f64;
    let mut octave_shift = 0i8;
    let mut current_time = TimeSignature::COMMON;
    // Notes waiting for the end of a tie, by voice and pitch.
    let mut open: HashMap<(String, i16), usize> = HashMap::new();

    for (m, measure) in part
        .children()
        .filter(|n| n.has_tag_name("measure"))
        .enumerate()
    {
        let bar = measure.attribute("number").unwrap_or("?").to_string();
        if m == 0 {
            read.implicit_first = measure.attribute("implicit") == Some("yes");
        }
        let (mut pos, mut high, mut last_start) = (0.0f64, 0.0f64, 0.0f64);
        for node in measure.children().filter(|n| n.is_element()) {
            match node.tag_name().name() {
                "attributes" => {
                    if let Some(d) = child_text(node, "divisions").and_then(|t| t.parse().ok()) {
                        divisions = d;
                    }
                    if let Some(shift) = child(node, "transpose")
                        .and_then(|t| child_text(t, "octave-change"))
                        .and_then(|t| t.parse().ok())
                    {
                        octave_shift = shift;
                    }
                    if let Some(k) = child(node, "key") {
                        let found = read_key(k);
                        match (read.key, found) {
                            (None, Some(found)) => read.key = Some(found),
                            (None, None) => {
                                changes.push("The key could not be read; using C major.".into());
                                read.key = Some("C major".parse().expect("valid key"));
                            }
                            (Some(old), new) if new != Some(old) => changes.push(format!(
                                "Bar {bar} changes key; the app keeps one key, so it stays {old}."
                            )),
                            _ => {}
                        }
                    }
                    if let Some(t) = child(node, "time") {
                        let found = read_time(t);
                        match (read.time, found) {
                            (None, Some(found)) => {
                                read.time = Some(found);
                                current_time = found;
                            }
                            (None, None) => {
                                changes.push("The time signature could not be read; using 4/4.".into());
                                read.time = Some(TimeSignature::COMMON);
                            }
                            // A later meter starts with its measure.
                            (Some(_), Some(found)) if found != current_time => {
                                read.time_changes.push((m, found));
                                current_time = found;
                            }
                            (Some(_), None) => changes.push(format!(
                                "Bar {bar}: a time signature could not be read; the meter stays {current_time}."
                            )),
                            _ => {}
                        }
                    }
                }
                "backup" | "forward" => {
                    let d = child_text(node, "duration")
                        .and_then(|t| t.parse().ok())
                        .map_or(0.0, |d| ticks_of(d, divisions));
                    if node.has_tag_name("backup") {
                        pos = (pos - d).max(0.0);
                    } else {
                        pos += d;
                        high = high.max(pos);
                    }
                }
                "figured-bass" => {
                    let (text, duration) = read_figured_bass(node);
                    read.figures.push(Figure {
                        at: At {
                            measure: m,
                            ticks: pos,
                        },
                        text,
                        ticks: duration.map(|d| ticks_of(d, divisions)),
                    });
                }
                "note" => {
                    if child(node, "grace").is_some() || child(node, "cue").is_some() {
                        read.graces += 1;
                        continue;
                    }
                    let duration = child_text(node, "duration")
                        .and_then(|t| t.parse().ok())
                        .map_or(0.0, |d| ticks_of(d, divisions));
                    let chord = child(node, "chord").is_some();
                    let start = if chord { last_start } else { pos };
                    if !chord {
                        last_start = pos;
                        pos += duration;
                        high = high.max(pos);
                    }
                    let Some(pitch) = child(node, "pitch").and_then(read_pitch) else {
                        continue; // a rest, or unpitched
                    };
                    let pitch = Pitch::new(
                        pitch.name.letter,
                        pitch.name.accidental,
                        pitch.octave + octave_shift,
                    );
                    let voice = child_text(node, "voice").unwrap_or("1").to_string();
                    let mut ties: Vec<&str> = node
                        .children()
                        .filter(|n| n.has_tag_name("tie"))
                        .filter_map(|n| n.attribute("type"))
                        .collect();
                    if ties.is_empty() {
                        ties = node
                            .descendants()
                            .filter(|n| n.has_tag_name("tied"))
                            .filter_map(|n| n.attribute("type"))
                            .collect();
                    }
                    let key = (voice, pitch.semitones());
                    let end = At {
                        measure: m,
                        ticks: start + duration,
                    };
                    if ties.contains(&"stop")
                        && let Some(&i) = open.get(&key)
                    {
                        read.sounding[i].end = end;
                        if !ties.contains(&"start") {
                            open.remove(&key);
                        }
                        continue;
                    }
                    read.sounding.push(Sounding {
                        start: At {
                            measure: m,
                            ticks: start,
                        },
                        end,
                        pitch,
                    });
                    if ties.contains(&"start") {
                        open.insert(key, read.sounding.len() - 1);
                    } else {
                        open.remove(&key);
                    }
                }
                _ => {}
            }
        }
        read.lengths.push(high.max(pos));
        read.numbers.push(bar);
    }
    read
}

/// Reads part `id` of a score as a bass line on the theorbo: the lowest
/// sounding note of the part, whatever its voices and staves. [`ALL_PARTS`]
/// takes the lowest note of every part instead.
pub fn import(xml: &str, id: &str) -> Result<Import, String> {
    let doc = parse(xml)?;
    let chosen: Vec<_> = doc
        .root_element()
        .children()
        .filter(|n| n.has_tag_name("part"))
        .filter(|n| id == ALL_PARTS || n.attribute("id") == Some(id))
        .collect();
    if chosen.is_empty() {
        return Err(format!("no part {id}"));
    }
    let mut changes = Vec::new();
    let reads: Vec<PartRead> = chosen
        .iter()
        .enumerate()
        .map(|(i, part)| {
            let mut ignored = Vec::new();
            read_part(*part, if i == 0 { &mut changes } else { &mut ignored })
        })
        .collect();

    // Measures line up across parts; each is as long as its longest.
    let count = reads.iter().map(|r| r.lengths.len()).max().unwrap_or(0);
    let mut starts = vec![0.0f64];
    for m in 0..count {
        let length = reads
            .iter()
            .filter_map(|r| r.lengths.get(m))
            .fold(0.0, |a: f64, &b| a.max(b));
        starts.push(starts[m] + length);
    }
    let abs = |at: At| starts[at.measure] + at.ticks;
    let (grid, on_grid) = (reduction::grid, reduction::on_grid);

    let mut spans: Vec<reduction::Span> = Vec::new();
    let mut vanished = 0;
    for read in &reads {
        for s in &read.sounding {
            let (a, b) = (abs(s.start), abs(s.end));
            let (qa, qb) = (grid(a), grid(b));
            if qb <= qa {
                vanished += 1;
                continue;
            }
            spans.push(reduction::Span {
                start: qa,
                end: qb,
                pitch: s.pitch,
                exact: on_grid(a) && on_grid(b),
            });
        }
    }
    if spans.is_empty() {
        return Err("the part has no notes".into());
    }

    let first = &reads[0];
    let figures: Vec<reduction::Figure> = reads
        .iter()
        .flat_map(|r| &r.figures)
        .map(|f| reduction::Figure {
            at: grid(abs(f.at)),
            text: f.text.clone(),
            ticks: f.ticks,
        })
        .collect();
    Ok(reduction::assemble(
        spans,
        vanished,
        &figures,
        &reduction::Layout {
            starts: &starts,
            numbers: &first.numbers,
            time_changes: &first.time_changes,
            key: first.key,
            time: first.time,
            implicit_first: first.implicit_first,
            graces: first.graces,
        },
        changes,
    ))
}

fn read_key(node: roxmltree::Node<'_, '_>) -> Option<Key> {
    let fifths: i8 = child_text(node, "fifths")?.parse().ok()?;
    let mode = match child_text(node, "mode") {
        Some("minor") => Mode::Minor,
        _ => Mode::Major,
    };
    Key::all().find(|k| k.signature() == fifths && k.mode() == mode)
}

fn read_time(node: roxmltree::Node<'_, '_>) -> Option<TimeSignature> {
    let beats: u8 = child_text(node, "beats")?.parse().ok()?;
    let value: u8 = child_text(node, "beat-type")?.parse().ok()?;
    format!("{beats}/{value}").parse().ok()
}

fn read_pitch(node: roxmltree::Node<'_, '_>) -> Option<Pitch> {
    let letter = Letter::from_char(child_text(node, "step")?.chars().next()?)?;
    let alter: f64 = child_text(node, "alter")
        .and_then(|a| a.parse().ok())
        .unwrap_or(0.0);
    let accidental = Accidental::new(alter.round().clamp(-2.0, 2.0) as i8)?;
    let octave: i8 = child_text(node, "octave")?.parse().ok()?;
    Some(Pitch::new(letter, accidental, octave))
}

/// One `<figured-bass>` as figuring text in the app's syntax.
/// One `<figured-bass>`: its figures in the app's syntax, and how long they
/// last in divisions, if given.
fn read_figured_bass(node: roxmltree::Node<'_, '_>) -> (String, Option<f64>) {
    let duration = child_text(node, "duration").and_then(|d| d.parse().ok());
    (read_figures(node), duration)
}

fn read_figures(node: roxmltree::Node<'_, '_>) -> String {
    let sign = |text: &str| match text {
        "sharp" => Some("#"),
        "flat" => Some("b"),
        "natural" => Some("n"),
        _ => None,
    };
    let figures: Vec<String> = node
        .children()
        .filter(|n| n.has_tag_name("figure"))
        .map(|figure| {
            let prefix = child_text(figure, "prefix").and_then(sign).unwrap_or("");
            let number = child_text(figure, "figure-number").unwrap_or("");
            let suffix = match child_text(figure, "suffix") {
                Some("slash" | "back-slash" | "plus" | "vertical") if !number.is_empty() => "+",
                Some(s) => sign(s).unwrap_or(""),
                None => "",
            };
            format!("{prefix}{number}{suffix}")
        })
        .filter(|f| !f.is_empty())
        .collect();
    figures.join(" ")
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
        let ticks = |n: u32| {
            Duration::pieces(n)
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(ticks(20), ["2", "8"]);
        assert_eq!(ticks(48), ["1."]);
        assert_eq!(ticks(12), ["4."]);
    }

    #[test]
    fn export_then_import_round_trips_the_bass_line() {
        let line = document(LINE);
        let xml = export(&line, Some(&realizations(&line)), "t");
        let parts = parts(&xml).unwrap();
        // The two parts, and the choice of every part's lowest note.
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].name, "Basso continuo");
        assert!(parts[0].has_figures && !parts[1].has_figures);
        assert_eq!(parts[2].id, ALL_PARTS);

        let imported = import(&xml, "P1").unwrap();
        assert!(imported.changes.is_empty(), "{:?}", imported.changes);
        let back = &imported.document.line;
        assert_eq!(back.key(), line.key());
        assert_eq!(back.time(), line.time());
        let summary = |l: &BassLine| -> Vec<String> {
            l.notes()
                .iter()
                .map(|n| {
                    if n.rest {
                        format!("r {}", n.duration)
                    } else {
                        format!("{} {} {}", n.pitch, n.duration, n.figurings)
                    }
                })
                .collect()
        };
        // Notes split over bar lines come back joined.
        assert_eq!(summary(back), summary(&line));
    }

    #[test]
    fn import_reports_what_it_changes() {
        let xml = r#"<?xml version="1.0"?>
<!DOCTYPE score-partwise PUBLIC "-//Recordare//DTD MusicXML 4.0 Partwise//EN" "http://www.musicxml.org/dtds/partwise.dtd">
<score-partwise version="4.0">
  <part-list><score-part id="P1"><part-name>Organo</part-name></score-part></part-list>
  <part id="P1">
    <measure number="0" implicit="yes">
      <attributes><divisions>2</divisions><key><fifths>1</fifths></key><time><beats>3</beats><beat-type>4</beat-type></time></attributes>
      <note><pitch><step>D</step><octave>3</octave></pitch><duration>2</duration><voice>1</voice></note>
    </measure>
    <measure number="1">
      <figured-bass><figure><figure-number>9</figure-number></figure><figure><figure-number>7</figure-number></figure></figured-bass>
      <note><pitch><step>G</step><octave>2</octave></pitch><duration>3</duration><voice>1</voice></note>
      <note><chord/><pitch><step>D</step><octave>2</octave></pitch><duration>3</duration><voice>1</voice></note>
      <note><pitch><step>F</step><alter>1</alter><octave>2</octave></pitch><duration>1</duration><voice>1</voice></note>
      <note><grace/><pitch><step>E</step><octave>2</octave></pitch><voice>1</voice></note>
      <backup><duration>4</duration></backup>
      <note><pitch><step>B</step><octave>3</octave></pitch><duration>4</duration><voice>2</voice></note>
      <figured-bass><figure><prefix>sharp</prefix></figure></figured-bass>
      <note><pitch><step>A</step><octave>2</octave></pitch><duration>2</duration><tie type="start"/><voice>1</voice></note>
    </measure>
    <measure number="2">
      <attributes><key><fifths>2</fifths></key></attributes>
      <note><pitch><step>A</step><octave>2</octave></pitch><duration>1</duration><tie type="stop"/><voice>1</voice></note>
      <note><rest/><duration>5</duration><voice>1</voice></note>
    </measure>
  </part>
</score-partwise>"#;
        assert_eq!(parts(xml).unwrap()[0].name, "Organo");
        let imported = import(xml, "P1").unwrap();
        let line = &imported.document.line;
        assert_eq!(line.key().to_string(), "G major");
        assert_eq!(line.time().to_string(), "3/4");
        let notes: Vec<String> = line
            .notes()
            .iter()
            .map(|n| {
                if n.rest {
                    format!("r {}", n.duration)
                } else {
                    format!("{} {} {}", n.pitch, n.duration, n.figurings)
                }
            })
            .collect();
        // D3 quarter; D2 (the chord's lowest) dotted quarter, its 9 7 dropped;
        // F#2 eighth; A2 tied quarter + eighth = dotted quarter with #;
        // the 2.5-quarter rest fills the bar: a half and an eighth.
        assert_eq!(
            notes,
            ["D3 4 ", "D2 4. ", "F#2 8 ", "A2 4. #", "r 2", "r 8"]
        );
        // The implicit first measure (a quarter) is now the upbeat.
        assert_eq!(line.upbeat().map(|d| d.to_string()), Some("4".to_string()));
        let text = imported.changes.join("\n");
        for expected in [
            "changes key",
            "notes above the bass",
            "grace",
            "not supported",
        ] {
            assert!(text.contains(expected), "missing {expected:?} in:\n{text}");
        }
    }

    /// A two-staff keyboard part: the bass is in the lower staff's second
    /// voice, under a melody that holds on while the bass moves.
    const KEYBOARD: &str = r#"<?xml version="1.0"?>
<score-partwise version="4.0">
  <part-list><score-part id="P1"><part-name>Piano</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1">
      <attributes><divisions>1</divisions><key><fifths>0</fifths></key><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
      <note><pitch><step>C</step><octave>5</octave></pitch><duration>4</duration><voice>1</voice><staff>1</staff></note>
      <backup><duration>4</duration></backup>
      <note><pitch><step>C</step><octave>3</octave></pitch><duration>1</duration><voice>5</voice><staff>2</staff></note>
      <note><rest/><duration>1</duration><voice>5</voice><staff>2</staff></note>
      <note><pitch><step>G</step><octave>2</octave></pitch><duration>2</duration><voice>5</voice><staff>2</staff></note>
    </measure>
    <measure number="2">
      <note><rest measure="yes"/><duration>12</duration><voice>1</voice></note>
    </measure>
  </part>
</score-partwise>"#;

    fn summary(line: &BassLine) -> Vec<String> {
        line.notes()
            .iter()
            .map(|n| {
                if n.rest {
                    format!("r {}", n.duration)
                } else {
                    format!("{} {}", n.pitch, n.duration)
                }
            })
            .collect()
    }

    #[test]
    fn the_bass_is_the_lowest_sounding_note_in_any_voice_or_staff() {
        let imported = import(KEYBOARD, "P1").unwrap();
        // The held melody note never becomes a bass note: where the bass
        // rests, so does the line.
        assert_eq!(summary(&imported.document.line), ["C3 4", "r 4", "G2 2"]);
    }

    #[test]
    fn long_rests_and_notes_keep_the_bars_in_place() {
        // Three bars of rest, then a bar of 3/2 with a note.
        let xml = r#"<?xml version="1.0"?>
<score-partwise version="4.0">
  <part-list><score-part id="P1"><part-name>B</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1"><attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
      <note><rest/><duration>4</duration></note></measure>
    <measure number="2"><note><rest/><duration>4</duration></note></measure>
    <measure number="3"><note><rest/><duration>4</duration></note></measure>
    <measure number="4"><attributes><time><beats>3</beats><beat-type>2</beat-type></time></attributes>
      <note><pitch><step>E</step><octave>3</octave></pitch><duration>6</duration></note></measure>
  </part>
</score-partwise>"#;
        let imported = import(xml, "P1").unwrap();
        let line = &imported.document.line;
        // A whole-bar rest is a whole note...
        assert_eq!(summary(line), ["r 1", "r 1", "r 1", "E3 1."]);
        // ...and the meter change lands on the first note of its bar.
        assert_eq!(
            line.notes()[3].time_change.map(|t| t.to_string()),
            Some("3/2".into())
        );
    }

    #[test]
    fn triplets_keep_their_bar_and_are_reported() {
        let xml = r#"<?xml version="1.0"?>
<score-partwise version="4.0">
  <part-list><score-part id="P1"><part-name>B</part-name></score-part></part-list>
  <part id="P1">
    <measure number="1"><attributes><divisions>3</divisions><time><beats>2</beats><beat-type>4</beat-type></time></attributes>
      <note><pitch><step>C</step><octave>3</octave></pitch><duration>1</duration></note>
      <note><pitch><step>D</step><octave>3</octave></pitch><duration>1</duration></note>
      <note><pitch><step>E</step><octave>3</octave></pitch><duration>1</duration></note>
      <note><pitch><step>F</step><octave>3</octave></pitch><duration>3</duration></note></measure>
    <measure number="2">
      <note><pitch><step>G</step><octave>3</octave></pitch><duration>6</duration></note></measure>
  </part>
</score-partwise>"#;
        let imported = import(xml, "P1").unwrap();
        let ticks: u32 = imported
            .document
            .line
            .notes()
            .iter()
            .map(|n| n.duration.ticks())
            .sum();
        // Two 2/4 bars, however the triplet was rounded.
        assert_eq!(ticks, 32);
        assert!(
            imported
                .changes
                .iter()
                .any(|c| c.contains("no matching note value"))
        );
    }

    #[test]
    fn all_parts_gives_the_lowest_note_of_the_score() {
        let xml = r#"<?xml version="1.0"?>
<score-partwise version="4.0">
  <part-list><score-part id="P1"><part-name>Top</part-name></score-part><score-part id="P2"><part-name>Low</part-name></score-part></part-list>
  <part id="P1"><measure number="1"><attributes><divisions>1</divisions><time><beats>2</beats><beat-type>4</beat-type></time></attributes>
    <note><pitch><step>C</step><octave>5</octave></pitch><duration>2</duration></note></measure></part>
  <part id="P2"><measure number="1">
    <note><pitch><step>A</step><octave>2</octave></pitch><duration>1</duration></note>
    <note><pitch><step>B</step><octave>4</octave></pitch><duration>1</duration></note></measure></part>
</score-partwise>"#;
        let imported = import(xml, ALL_PARTS).unwrap();
        assert_eq!(summary(&imported.document.line), ["A2 4", "B4 4"]);
    }

    #[test]
    fn upbeats_export_as_measure_zero_and_come_back() {
        let text = "intavolatura 1\nkey: C major\ntime: 3/4\nupbeat: 4\ninstrument: theorbo-a-14\n\nG2 4\nC3 2.\nG2 2\n";
        let line = document(text);
        let xml = export(&line, None, "t");
        assert!(xml.contains("<measure number=\"0\" implicit=\"yes\">"));
        assert!(xml.contains("<measure number=\"1\">"));
        let back = import(&xml, "P1").unwrap();
        assert!(back.changes.is_empty(), "{:?}", back.changes);
        assert_eq!(back.document.line.upbeat(), line.upbeat());
        assert_eq!(back.document.line.notes().len(), 3);
    }

    #[test]
    fn meter_changes_export_and_come_back() {
        let text = "intavolatura 1\nkey: C major\ntime: 4/4\ninstrument: theorbo-a-14\n\n\
            C3 1\nD3 2. | | time 3/2\nE3 2. | | time 3/2\nF3 1 | | time 4/4\n";
        let mut line = document(text);
        // The second 3/2 is redundant in the file; it is kept as written.
        let xml = export(&line, None, "t");
        assert_eq!(
            xml.matches("<beats>3</beats><beat-type>2</beat-type>")
                .count(),
            1
        );
        assert_eq!(xml.matches("<time symbol=\"common\">").count(), 2);
        let back = import(&xml, "P1").unwrap();
        assert!(back.changes.is_empty(), "{:?}", back.changes);
        let changes: Vec<Option<String>> = back
            .document
            .line
            .notes()
            .iter()
            .map(|n| n.time_change.map(|t| t.to_string()))
            .collect();
        assert_eq!(
            changes,
            [None, Some("3/2".into()), None, Some("4/4".into())]
        );
        line = back.document.line;
        assert_eq!(line.time(), TimeSignature::COMMON);
    }

    #[test]
    fn uneven_suspensions_export_and_come_back() {
        let text = "intavolatura 1\nkey: C major\ntime: 3/4\ninstrument: theorbo-a-14\n\nC3 2 | 7:4.-6:8\nG2 4 | 4-3\n";
        let line = document(text);
        let xml = export(&line, None, "t");
        assert!(xml.contains("<duration>12</duration>\n      </figured-bass>"));
        let back = import(&xml, "P1").unwrap();
        assert!(back.changes.is_empty(), "{:?}", back.changes);
        let figurings: Vec<String> = back
            .document
            .line
            .notes()
            .iter()
            .map(|n| n.figurings.to_string())
            .collect();
        assert_eq!(figurings, ["7:4.-6:8", "4-3"]);
    }

    #[test]
    fn reads_compressed_mxl() {
        let line = document(LINE);
        let xml = export(&line, None, "t");
        let path = std::env::temp_dir().join(format!("intavolatura-{}.mxl", std::process::id()));
        {
            let file = std::fs::File::create(&path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("META-INF/container.xml", options).unwrap();
            std::io::Write::write_all(&mut zip, br#"<container><rootfiles><rootfile full-path="score.xml"/></rootfiles></container>"#).unwrap();
            zip.start_file("score.xml", options).unwrap();
            std::io::Write::write_all(&mut zip, xml.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        let read = read_file(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(read, xml);
    }
}
