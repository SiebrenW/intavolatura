//! Lays out the views as a display list of glyphs and bars in logical pixels.
//!
//! All positions are computed here so the Slint side only draws. Music
//! glyphs use the Bravura font (SMuFL code points): its line box is centred
//! on the baseline, so a glyph centred on `y` sits on that staff position.

use intavolatura_core::{
    Accidental, BassLine, Duration, Figure, FigureAccidental, Instrument, Key, NoteValue, Pitch,
    Realization, TimeSignature,
};

/// One staff space.
pub const SPACE: f32 = 10.0;
const HALF: f32 = SPACE / 2.0;
/// Bravura's em is four staff spaces.
const MUSIC_SIZE: f32 = 4.0 * SPACE;
const FIGURE_SIZE: f32 = 3.8 * SPACE;
const RHYTHM_SIZE: f32 = 3.0 * SPACE;
const LINE: f32 = 1.0;

pub const COLUMN_WIDTH: f32 = 64.0;
const KEY_SIGNATURE_X: f32 = 52.0;
const KEY_SIGNATURE_STEP: f32 = 11.5;
/// Room for the time signature after the key signature.
const TIME_SIGNATURE_WIDTH: f32 = 30.0;
/// Columns grow with the duration, but less than in proportion: a whole
/// note is twice as wide as a quarter, not four times.
const MIN_PART_WIDTH: f32 = 0.85 * COLUMN_WIDTH;
const STEM: f32 = 3.5 * SPACE;
const STEM_WIDTH: f32 = 1.2;
const BAR_LINE: f32 = 1.4;

/// Staff position (see `Pitch::staff_position`) of each staff's top line.
const BASS_TOP_POSITION: i16 = 26; // A3
const TREBLE_TOP_POSITION: i16 = 38; // F5
/// Middle C: the lowest note written on the treble staff of the voicing.
const MIDDLE_C_POSITION: i16 = 28;

/// Vertical layout of the bass staff row.
pub const BASS_STAFF_TOP: f32 = 70.0;
/// Top of the figures under the bass staff.
pub const FIGURES_TOP: f32 = BASS_STAFF_TOP + 4.0 * SPACE + 78.0;
const FIGURE_LINE: f32 = 26.0;
/// Where the bass staff row ends; clicks above it edit the bass line.
pub const BASS_ROW_BOTTOM: f32 = FIGURES_TOP + 4.0 * FIGURE_LINE;

/// Vertical layout of the voicing grand staff.
const TREBLE_STAFF_TOP: f32 = BASS_ROW_BOTTOM + SPACE;
const VOICING_BASS_TOP: f32 = TREBLE_STAFF_TOP + 11.0 * SPACE;

/// Vertical layout of the tablature: six lines for the fretted courses,
/// diapason signs below.
/// Room above the tablature for Italian diapason signs (two ledger levels).
/// and rhythm signs above those.
const TAB_TOP: f32 = VOICING_BASS_TOP + 9.0 * SPACE + 6.0 * TAB_LINE;
const TAB_LINE: f32 = 16.0;
/// Size of the historical tablature signs, a little under Bravura's four
/// staff spaces per em with a tablature line gap as the space.
const TAB_MUSIC_SIZE: f32 = 3.0 * TAB_LINE;
const TAB_GLYPH_WIDTH: f32 = 0.55 * TAB_LINE;
/// Bravura sets the signs on their baseline; this lowers them so they are
/// centred on the line instead.
const TAB_GLYPH_DROP: f32 = 0.36 * TAB_LINE;
const TAB_BOTTOM: f32 = TAB_TOP + 5.0 * TAB_LINE;
pub const CONTENT_BOTTOM: f32 = TAB_BOTTOM + 4.0 * TAB_LINE;

/// Something drawn as text, centred on `(x, y)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    pub x: f32,
    pub y: f32,
    pub text: String,
    /// Bravura instead of the interface font.
    pub music: bool,
    pub size: f32,
    pub faded: bool,
    /// Drawn in the warning colour, e.g. a note running over a bar line.
    pub warning: bool,
}

/// A filled rectangle: staff lines, ledger lines, tablature lines.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bar {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Painted in the background colour, to clear lines behind a glyph.
    pub knockout: bool,
}

/// A tie: a filled curve within the box at `(x, y)`, as SVG path commands
/// in that box's pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Tie {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub commands: String,
}

impl Tie {
    /// A tie curving down from `x1` to `x2`, hanging below `y`.
    fn below(x1: f32, x2: f32, y: f32) -> Tie {
        let (width, height) = (x2 - x1, 0.6 * SPACE);
        let w = width;
        let (h, inner) = (height, height - 1.6);
        Tie {
            x: x1,
            y,
            width,
            height,
            commands: format!(
                "M 0 0 Q {mid} {outer} {w} 0 Q {mid} {inner2} 0 0 Z",
                mid = w / 2.0,
                outer = 2.0 * h,
                inner2 = 2.0 * inner,
            ),
        }
    }
}

/// Everything to draw, plus the size of the scrollable content.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub glyphs: Vec<Glyph>,
    pub bars: Vec<Bar>,
    pub ties: Vec<Tie>,
    pub width: f32,
    pub height: f32,
}

/// Horizontal layout shared by every row. A column is one bass note; a
/// suspension's column holds one part per figuring, side by side.
#[derive(Clone, Debug, PartialEq)]
pub struct Columns {
    /// Left edge of each column, followed by the right edge of the last.
    edges: Vec<f32>,
}

impl Columns {
    /// Columns for bass notes of these durations with these numbers of
    /// figurings.
    pub fn new(key: Key, notes: &[(Duration, usize)]) -> Columns {
        let mut edges = vec![time_signature_x(key) + TIME_SIGNATURE_WIDTH];
        for &(duration, parts) in notes {
            let last = *edges.last().expect("never empty");
            let by_duration = COLUMN_WIDTH * (duration.ticks() as f32 / 8.0).sqrt();
            let width = by_duration.max(parts.max(1) as f32 * MIN_PART_WIDTH);
            edges.push(last + width);
        }
        Columns { edges }
    }

    /// Number of parts in a column, judged from its width is not possible,
    /// so callers pass it to [`Columns::part_x`].
    fn part_width(&self, index: usize, parts: usize) -> f32 {
        self.width_of(index) / parts.max(1) as f32
    }

    pub fn count(&self) -> usize {
        self.edges.len() - 1
    }

    /// Left edge of a column, or where the next one would start.
    pub fn left(&self, index: usize) -> f32 {
        self.edges[index.min(self.count())]
    }

    /// Width of a column; past the last, the empty column for appending.
    pub fn width_of(&self, index: usize) -> f32 {
        if index < self.count() {
            self.edges[index + 1] - self.edges[index]
        } else {
            COLUMN_WIDTH
        }
    }

    pub fn center(&self, index: usize) -> f32 {
        self.left(index) + self.width_of(index) / 2.0
    }

    /// Centre of part `part` of `parts` within a column; the notehead sits
    /// a little left of centre to leave room for stems, dots and flags.
    pub fn part_x(&self, index: usize, part: usize, parts: usize) -> f32 {
        let width = self.part_width(index, parts);
        self.left(index) + part as f32 * width + width.min(COLUMN_WIDTH) * 0.45
    }

    /// Content width, leaving one empty column to click into for appending.
    pub fn width(&self) -> f32 {
        self.left(self.count()) + COLUMN_WIDTH + SPACE
    }

    /// What a click at `x` means for the bass line.
    pub fn hit(&self, x: f32) -> ColumnHit {
        let margin = 0.2 * COLUMN_WIDTH;
        if x < self.left(0) {
            return ColumnHit::Insert(0);
        }
        let Some(index) = (0..self.count()).find(|&i| x < self.edges[i + 1]) else {
            return ColumnHit::Insert(self.count());
        };
        // Near either edge of a column, a click inserts at that boundary.
        if x - self.edges[index] < margin {
            ColumnHit::Insert(index)
        } else if self.edges[index + 1] - x < margin {
            ColumnHit::Insert(index + 1)
        } else {
            ColumnHit::Column(index)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnHit {
    Column(usize),
    /// Insert before this index; the count appends.
    Insert(usize),
}

/// Where the time signature starts, after the key signature.
fn time_signature_x(key: Key) -> f32 {
    KEY_SIGNATURE_X + key.signature().unsigned_abs() as f32 * KEY_SIGNATURE_STEP + 8.0
}

/// The staff position under a click at `y` on the bass staff.
pub fn bass_staff_position(y: f32) -> i16 {
    BASS_TOP_POSITION - ((y - BASS_STAFF_TOP) / HALF).round() as i16
}

/// How tablature is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TablatureStyle {
    /// Numbers, course 1 on the bottom line.
    Italian,
    /// Letters, course 1 on the top line.
    French,
}

/// What the views need besides the bass line itself.
pub struct ViewOptions {
    pub show_hints: bool,
    pub tablature: TablatureStyle,
}

/// Builds the scene for a bass line and the realization of each note on
/// `instrument`, tuned as the realizations were made.
pub fn scene(
    line: &BassLine,
    realizations: &[Realization],
    instrument: &Instrument,
    options: &ViewOptions,
) -> Scene {
    let key = line.key();
    let columns = columns_for(line);
    let mut scene = Scene {
        width: columns.width(),
        height: CONTENT_BOTTOM,
        ..Scene::default()
    };
    let width = scene.width;

    let staff = Staff {
        top: BASS_STAFF_TOP,
        top_position: BASS_TOP_POSITION,
    };
    let treble = Staff {
        top: TREBLE_STAFF_TOP,
        top_position: TREBLE_TOP_POSITION,
    };
    let bass = Staff {
        top: VOICING_BASS_TOP,
        top_position: BASS_TOP_POSITION,
    };

    // Staves, clefs, key and time signatures.
    staff.draw_lines(&mut scene, width);
    scene.glyph(26.0, staff.y(24), "\u{E062}", true, MUSIC_SIZE); // F clef on F3
    staff.draw_key_signature(&mut scene, key, BASS_TOP_POSITION);
    staff.draw_time_signature(&mut scene, time_signature_x(key), line.time());
    treble.draw_lines(&mut scene, width);
    bass.draw_lines(&mut scene, width);
    let system = (TREBLE_STAFF_TOP, VOICING_BASS_TOP + 4.0 * SPACE);
    scene.vertical(0.75, system.0, system.1, 1.5);
    scene.glyph(26.0, treble.y(32), "\u{E050}", true, MUSIC_SIZE); // G clef on G4
    scene.glyph(26.0, bass.y(24), "\u{E062}", true, MUSIC_SIZE);
    treble.draw_key_signature(&mut scene, key, TREBLE_TOP_POSITION);
    bass.draw_key_signature(&mut scene, key, BASS_TOP_POSITION);
    treble.draw_time_signature(&mut scene, time_signature_x(key), line.time());
    bass.draw_time_signature(&mut scene, time_signature_x(key), line.time());
    for line in 0..6 {
        scene.bar(
            0.0,
            TAB_TOP + line as f32 * TAB_LINE - LINE / 2.0,
            width,
            LINE,
        );
    }

    let placements = line.placements();
    for (index, note) in line.notes().iter().enumerate() {
        let parts = note.figurings.parts().len();
        let part_duration = note.duration.split(parts).unwrap_or(note.duration);
        let placement = placements[index];

        let x = columns.part_x(index, 0, parts);
        if note.rest {
            // Silence in every row: rests on the staves, and in the
            // tablature a rhythm sign over an empty column, as in the sources.
            for staff in [&staff, &treble, &bass] {
                staff.draw_rest(&mut scene, x, note.duration);
            }
            draw_rhythm_sign(&mut scene, x, note.duration, options.tablature);
            if placement.crosses_bar {
                let last = scene.glyphs.len();
                for glyph in &mut scene.glyphs[last.saturating_sub(4)..] {
                    glyph.warning = true;
                }
            }
            if placement.ends_bar {
                let x = columns.left(index + 1) - 2.0;
                scene.vertical(x, staff.top, staff.top + 4.0 * SPACE, BAR_LINE);
                scene.vertical(x, system.0, system.1, BAR_LINE);
                scene.vertical(x, TAB_TOP, TAB_BOTTOM, BAR_LINE);
            }
            continue;
        }

        // The bass sounds once, for the whole note, under the first figuring.
        let first_glyph = scene.glyphs.len();
        staff.draw_note(&mut scene, x, note.pitch, key, note.duration, 0.0, Some(0));
        staff.draw_stem(
            &mut scene,
            x,
            &[note.pitch.staff_position()],
            note.duration.value,
        );
        if placement.crosses_bar {
            for glyph in &mut scene.glyphs[first_glyph..] {
                glyph.warning = true;
            }
        }

        for (part, figuring) in note.figurings.parts().iter().enumerate() {
            let hint = realizations
                .get(index)
                .and_then(|r| r.parts.get(part))
                .and_then(|p| p.chord.leading_tone_hint())
                .filter(|_| options.show_hints);
            let x = columns.part_x(index, part, parts);
            draw_figuring(&mut scene, x, figuring.figures(), hint.is_some());
        }

        if placement.ends_bar {
            let x = columns.left(index + 1) - 2.0;
            scene.vertical(x, staff.top, staff.top + 4.0 * SPACE, BAR_LINE);
            scene.vertical(x, system.0, system.1, BAR_LINE);
            scene.vertical(x, TAB_TOP, TAB_BOTTOM, BAR_LINE);
        }

        let Some(realization) = realizations.get(index) else {
            continue;
        };
        if realization.shown.is_none() {
            scene.glyphs.push(Glyph {
                x: columns.center(index),
                y: TAB_TOP + 2.5 * TAB_LINE,
                text: "no fingering".to_string(),
                music: false,
                size: 11.0,
                faded: true,
                warning: false,
            });
            continue;
        }
        for (part, sounding) in realization.parts.iter().enumerate() {
            let x = columns.part_x(index, part, parts);
            draw_voicing(
                &mut scene,
                x,
                &sounding.notes,
                key,
                part_duration,
                &treble,
                &bass,
            );
            if part > 0 {
                let previous = columns.part_x(index, part - 1, parts);
                for (pitch, _) in sounding.notes.iter().filter(|(_, struck)| !struck) {
                    let staff = if pitch.staff_position() >= MIDDLE_C_POSITION {
                        &treble
                    } else {
                        &bass
                    };
                    let y = staff.y(pitch.staff_position()) + 0.6 * SPACE;
                    let start = previous + 0.8 * SPACE + dot_room(part_duration);
                    scene.ties.push(Tie::below(start, x - 0.8 * SPACE, y));
                }
            }
            draw_rhythm_sign(&mut scene, x, part_duration, options.tablature);
            draw_fingering(
                &mut scene,
                x,
                &sounding.struck,
                instrument,
                options.tablature,
            );
        }
    }
    scene
}

/// The columns for a bass line, sized by each note's duration and parts.
pub fn columns_for(line: &BassLine) -> Columns {
    let notes: Vec<(Duration, usize)> = line
        .notes()
        .iter()
        .map(|n| (n.duration, n.figurings.parts().len()))
        .collect();
    Columns::new(line.key(), &notes)
}

/// Extra room a dot takes after a notehead.
fn dot_room(duration: Duration) -> f32 {
    if duration.dotted { 0.8 * SPACE } else { 0.0 }
}

/// A rhythm sign above the tablature, as in lute and theorbo sources: above
/// the diapason ledger lines in Italian tablature, just above the staff in
/// French.
fn draw_rhythm_sign(scene: &mut Scene, x: f32, duration: Duration, style: TablatureStyle) {
    let glyph = match duration.value {
        NoteValue::Whole => "\u{EBA7}",
        NoteValue::Half => "\u{EBA8}",
        NoteValue::Quarter => "\u{EBA9}",
        NoteValue::Eighth => "\u{EBAA}",
        NoteValue::Sixteenth => "\u{EBAB}",
    };
    let y = match style {
        TablatureStyle::Italian => TAB_TOP - 3.0 * TAB_LINE,
        TablatureStyle::French => TAB_TOP - 1.1 * TAB_LINE,
    };
    scene.glyph(x, y, glyph, true, RHYTHM_SIZE);
    if duration.dotted {
        scene.glyph(
            x + 0.9 * SPACE,
            y - 0.6 * SPACE,
            "\u{E1E7}",
            true,
            RHYTHM_SIZE,
        );
    }
}

/// Draws a voicing on the grand staff: middle C and up on the treble staff.
/// Unisons are drawn once; the upper note of a second moves right. Each
/// staff's notes share one stem.
fn draw_voicing(
    scene: &mut Scene,
    x: f32,
    voicing: &[(Pitch, bool)],
    key: Key,
    duration: Duration,
    treble: &Staff,
    bass: &Staff,
) {
    // A held note is tied from the part before, so it needs no accidental.
    let mut notes = voicing.to_vec();
    notes.sort_by_key(|(p, _)| (p.staff_position(), p.semitones()));
    notes.dedup_by_key(|(p, _)| *p);
    for (staff, on_treble) in [(treble, true), (bass, false)] {
        let on_staff: Vec<(Pitch, bool)> = notes
            .iter()
            .copied()
            .filter(|(p, _)| (p.staff_position() >= MIDDLE_C_POSITION) == on_treble)
            .collect();
        let mut previous: Option<(i16, bool)> = None;
        let mut accidentals = 0;
        for &(pitch, struck) in &on_staff {
            let position = pitch.staff_position();
            let shifted = previous.is_some_and(|(p, s)| position - p == 1 && !s);
            let shift = if shifted { 1.18 * SPACE } else { 0.0 };
            let column = struck.then_some(accidentals % 2);
            if staff.draw_note(scene, x, pitch, key, duration, shift, column) {
                accidentals += 1;
            }
            previous = Some((position, shifted));
        }
        let positions: Vec<i16> = on_staff.iter().map(|(p, _)| p.staff_position()).collect();
        staff.draw_stem(scene, x, &positions, duration.value);
    }
}

/// Draws struck courses and frets in tablature: fretted courses on the six
/// lines, diapasons as signs below.
fn draw_fingering(
    scene: &mut Scene,
    x: f32,
    struck: &[(usize, u8)],
    instrument: &Instrument,
    style: TablatureStyle,
) {
    let courses = instrument.courses();
    let fretted = courses.iter().take_while(|c| !c.is_diapason()).count();
    let mut diapasons = 0;
    for &(course, fret) in struck {
        if !courses[course].is_diapason() {
            let line = match style {
                TablatureStyle::Italian => fretted - 1 - course,
                TablatureStyle::French => course,
            };
            let text = match style {
                TablatureStyle::Italian => italian_number(fret as usize),
                TablatureStyle::French => french_letter(fret).to_string(),
            };
            scene.tab_sign(x, TAB_TOP + line as f32 * TAB_LINE, &text, true);
            continue;
        }

        let ordinal = course - fretted; // 0 for course 7
        match style {
            TablatureStyle::French => {
                // Courses 7–10 have their own signs (a, /a, //a, ///a);
                // courses 11–14 are numbered 4–7.
                let text = match ordinal {
                    0..=3 => char::from_u32(0xEBCD + ordinal as u32)
                        .expect("SMuFL code point")
                        .to_string(),
                    n => italian_number(n),
                };
                let y = TAB_BOTTOM + (1 + diapasons) as f32 * TAB_LINE;
                scene.tab_sign(x, y, &text, false);
                diapasons += 1;
            }
            TablatureStyle::Italian => {
                // Italian tablature is upside down, so the diapasons sit above
                // the staff: course 7 is 0 on the first ledger line, the others
                // their course number on a ledger line above that.
                let (level, text) = if ordinal == 0 {
                    (1, italian_number(0))
                } else {
                    diapasons += 1;
                    (1 + diapasons, italian_number(course + 1))
                };
                let y = TAB_TOP - level as f32 * TAB_LINE;
                scene.bar(x - 1.4 * SPACE, y - LINE / 2.0, 2.8 * SPACE, LINE);
                scene.tab_sign(x, y, &text, true);
            }
        }
    }
}

/// French tablature letter for a fret, in Bravura's historical forms: a for
/// open, then b, c … n (there is no j).
fn french_letter(fret: u8) -> char {
    match fret {
        0..=12 => char::from_u32(0xEBC0 + fret as u32).expect("SMuFL code point"),
        _ => '?',
    }
}

/// A number in Bravura's historical Italian tablature digits.
fn italian_number(n: usize) -> String {
    n.to_string()
        .chars()
        .map(|d| char::from_u32(0xEBE0 + d.to_digit(10).unwrap_or(0)).expect("SMuFL code point"))
        .collect()
}

/// A five-line staff, positioned by its top line.
struct Staff {
    top: f32,
    top_position: i16,
}

impl Staff {
    fn y(&self, position: i16) -> f32 {
        self.top + (self.top_position - position) as f32 * HALF
    }

    fn bottom_position(&self) -> i16 {
        self.top_position - 8
    }

    fn draw_lines(&self, scene: &mut Scene, width: f32) {
        for line in 0..5 {
            scene.bar(
                0.0,
                self.top + line as f32 * SPACE - LINE / 2.0,
                width,
                LINE,
            );
        }
    }

    fn draw_key_signature(&self, scene: &mut Scene, key: Key, top_position: i16) {
        // Positions on the bass staff; the treble staff is two octaves higher.
        const SHARPS: [i16; 7] = [24, 21, 25, 22, 19, 23, 20];
        const FLATS: [i16; 7] = [20, 23, 19, 22, 18, 21, 17];
        let shift = top_position - BASS_TOP_POSITION;
        let count = key.signature().unsigned_abs() as usize;
        let (positions, glyph) = if key.signature() > 0 {
            (&SHARPS, "\u{E262}")
        } else {
            (&FLATS, "\u{E260}")
        };
        for (i, &position) in positions.iter().take(count).enumerate() {
            let x = KEY_SIGNATURE_X + i as f32 * KEY_SIGNATURE_STEP;
            scene.glyph(x, self.y(position + shift), glyph, true, MUSIC_SIZE);
        }
    }

    /// Draws a whole note with ledger lines and, where the key signature
    /// does not already give it, its accidental. `shift` moves the notehead
    /// sideways, e.g. for the upper note of a second.
    /// Draws a notehead for `duration` with ledger lines, a dot if dotted,
    /// and, where the key signature does not already give it, an accidental.
    /// `shift` moves the notehead sideways, e.g. for the upper note of a
    /// second. Returns whether an accidental was drawn; `accidental_column`
    /// moves it further left to keep stacked accidentals apart, and `None`
    /// draws none, as for a tied note.
    #[allow(clippy::too_many_arguments)]
    fn draw_note(
        &self,
        scene: &mut Scene,
        x: f32,
        pitch: Pitch,
        key: Key,
        duration: Duration,
        shift: f32,
        accidental_column: Option<usize>,
    ) -> bool {
        let position = pitch.staff_position();
        let y = self.y(position);
        let head_x = x + shift;
        let head = match duration.value {
            NoteValue::Whole => "\u{E0A2}",
            NoteValue::Half => "\u{E0A3}",
            _ => "\u{E0A4}",
        };
        scene.glyph(head_x, y, head, true, MUSIC_SIZE);
        if duration.dotted {
            // A dot sits in a space: a note on a line puts it in the space above.
            let on_line = (position - self.top_position) % 2 == 0;
            let dot_y = if on_line { y - HALF } else { y };
            scene.glyph(head_x + 1.3 * SPACE, dot_y, "\u{E1E7}", true, MUSIC_SIZE);
        }

        let ledgers = (position..=self.bottom_position() - 2)
            .chain(self.top_position + 2..=position)
            .filter(|p| (p - self.top_position) % 2 == 0);
        for ledger in ledgers {
            scene.bar(
                head_x - 1.2 * SPACE,
                self.y(ledger) - LINE / 2.0,
                2.4 * SPACE,
                LINE,
            );
        }

        let accidental = pitch.name.accidental;
        let column =
            accidental_column.filter(|_| accidental != key.signature_accidental(pitch.name.letter));
        if let Some(column) = column {
            let ax = x - 1.6 * SPACE - column as f32 * 1.1 * SPACE;
            scene.glyph(ax, y, accidental_glyph(accidental), true, MUSIC_SIZE);
        }
        column.is_some()
    }

    /// Draws one stem for noteheads at these positions, with flags for
    /// eighths and sixteenths: up when the notes lie below the middle line,
    /// down otherwise. Whole notes have none.
    fn draw_stem(&self, scene: &mut Scene, x: f32, positions: &[i16], value: NoteValue) {
        let (Some(&low), Some(&high)) = (positions.iter().min(), positions.iter().max()) else {
            return;
        };
        if value == NoteValue::Whole {
            return;
        }
        let middle = self.top_position - 4;
        let up = (low + high) < 2 * middle;
        let half_head = 0.59 * SPACE;
        let (stem_x, from, to) = if up {
            (
                x + half_head - STEM_WIDTH / 2.0,
                self.y(low),
                self.y(high) - STEM,
            )
        } else {
            (
                x - half_head + STEM_WIDTH / 2.0,
                self.y(high),
                self.y(low) + STEM,
            )
        };
        scene.vertical(stem_x, from, to, STEM_WIDTH);
        let flag = match (value.flags(), up) {
            (1, true) => "\u{E240}",
            (1, false) => "\u{E241}",
            (2, true) => "\u{E242}",
            (2, false) => "\u{E243}",
            _ => return,
        };
        // Flags hang from the stem end, extending to its right.
        scene.glyph(stem_x + 0.55 * SPACE, to, flag, true, MUSIC_SIZE);
    }

    /// Draws a rest: the whole rest hangs from the fourth line, the half
    /// rest sits on the middle line, shorter rests centre on it.
    fn draw_rest(&self, scene: &mut Scene, x: f32, duration: Duration) {
        let (glyph, y) = match duration.value {
            NoteValue::Whole => ("\u{E4E3}", self.top + SPACE),
            NoteValue::Half => ("\u{E4E4}", self.top + 2.0 * SPACE),
            NoteValue::Quarter => ("\u{E4E5}", self.top + 2.0 * SPACE),
            NoteValue::Eighth => ("\u{E4E6}", self.top + 2.0 * SPACE),
            NoteValue::Sixteenth => ("\u{E4E7}", self.top + 2.0 * SPACE),
        };
        scene.glyph(x, y, glyph, true, MUSIC_SIZE);
        if duration.dotted {
            scene.glyph(
                x + 1.3 * SPACE,
                self.top + 1.5 * SPACE,
                "\u{E1E7}",
                true,
                MUSIC_SIZE,
            );
        }
    }

    /// Draws the time signature: C for 4/4, ¢ for 2/2, else stacked digits.
    fn draw_time_signature(&self, scene: &mut Scene, x: f32, time: TimeSignature) {
        let x = x + TIME_SIGNATURE_WIDTH / 2.0 - 4.0;
        let middle = self.top + 2.0 * SPACE;
        match (time.beats, time.beat_value) {
            (4, 4) => scene.glyph(x, middle, "\u{E08A}", true, MUSIC_SIZE),
            (2, 2) => scene.glyph(x, middle, "\u{E08B}", true, MUSIC_SIZE),
            (beats, value) => {
                let digits = |n: u8| -> String {
                    n.to_string()
                        .chars()
                        .map(|d| {
                            char::from_u32(0xE080 + d.to_digit(10).unwrap_or(0)).unwrap_or(' ')
                        })
                        .collect()
                };
                scene.glyph(x, self.top + SPACE, &digits(beats), true, MUSIC_SIZE);
                scene.glyph(x, self.top + 3.0 * SPACE, &digits(value), true, MUSIC_SIZE);
            }
        }
    }
}

fn accidental_glyph(accidental: Accidental) -> &'static str {
    match accidental.semitones() {
        -2 => "\u{E264}",
        -1 => "\u{E260}",
        0 => "\u{E261}",
        1 => "\u{E262}",
        _ => "\u{E263}",
    }
}

/// Draws figures below the bass staff, highest first, then a faded (♯)
/// leading-tone hint if there is one.
fn draw_figuring(scene: &mut Scene, x: f32, figures: &[Figure], hint: bool) {
    let mut y = FIGURES_TOP;
    for figure in figures {
        scene.glyph(x, y, &figure_text(figure), true, FIGURE_SIZE);
        y += FIGURE_LINE;
    }
    if hint {
        let text = "\u{EA6A}\u{EA66}\u{EA6B}";
        scene.glyphs.push(Glyph {
            x,
            y,
            text: text.to_string(),
            music: true,
            size: FIGURE_SIZE,
            faded: true,
            warning: false,
        });
    }
}

/// A figure in Bravura's figured-bass glyphs: accidental before the number,
/// or a raised number (slashed or with a stroke) where Bravura has one.
fn figure_text(figure: &Figure) -> String {
    let digit = |n: u8| match n {
        2 => '\u{EA52}',
        3 => '\u{EA54}',
        4 => '\u{EA55}',
        5 => '\u{EA57}',
        6 => '\u{EA5B}',
        7 => '\u{EA5D}',
        8 => '\u{EA60}',
        _ => '\u{EA61}',
    };
    let raised = |n: u8| match n {
        2 => Some('\u{EA53}'),
        4 => Some('\u{EA56}'),
        5 => Some('\u{EA58}'),
        6 => Some('\u{EA5C}'),
        7 => Some('\u{EA5E}'),
        9 => Some('\u{EA62}'),
        _ => None,
    };
    let accidental = |a: FigureAccidental| match a {
        FigureAccidental::Sharp => '\u{EA66}',
        FigureAccidental::Flat => '\u{EA64}',
        FigureAccidental::Natural => '\u{EA65}',
        FigureAccidental::Raised => '\u{EA6C}',
    };

    let mut text = String::new();
    match (figure.accidental, figure.number_shown) {
        (Some(a), false) => text.push(accidental(a)),
        (Some(FigureAccidental::Raised), true) => match raised(figure.interval) {
            Some(c) => text.push(c),
            None => {
                text.push(digit(figure.interval));
                text.push('\u{EA6C}');
            }
        },
        (Some(a), true) => {
            text.push(accidental(a));
            text.push(digit(figure.interval));
        }
        (None, _) => text.push(digit(figure.interval)),
    }
    text
}

impl Scene {
    fn bar(&mut self, x: f32, y: f32, width: f32, height: f32) {
        self.bars.push(Bar {
            x,
            y,
            width,
            height,
            knockout: false,
        });
    }

    /// A tablature sign centred on `y`, optionally clearing the line behind it.
    /// A tablature sign in Bravura's historical forms, centred on `y`,
    /// optionally clearing the line behind it.
    fn tab_sign(&mut self, x: f32, y: f32, text: &str, knockout: bool) {
        if knockout {
            let width = TAB_GLYPH_WIDTH * text.chars().count() as f32 + 3.0;
            self.bars.push(Bar {
                x: x - width / 2.0,
                y: y - 0.45 * TAB_LINE,
                width,
                height: 0.9 * TAB_LINE,
                knockout: true,
            });
        }
        self.glyph(x, y + TAB_GLYPH_DROP, text, true, TAB_MUSIC_SIZE);
    }

    fn glyph(&mut self, x: f32, y: f32, text: &str, music: bool, size: f32) {
        self.glyphs.push(Glyph {
            x,
            y,
            text: text.to_string(),
            music,
            size,
            faded: false,
            warning: false,
        });
    }

    /// A vertical line, e.g. a stem or a bar line.
    fn vertical(&mut self, x: f32, from: f32, to: f32, width: f32) {
        self.bar(x - width / 2.0, from.min(to), width, (to - from).abs());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use intavolatura_core::{BassNote, Fingering, Mode};

    fn key(s: &str) -> Key {
        s.parse().unwrap()
    }

    #[test]
    fn clicks_map_to_columns_and_insert_points() {
        let q = Duration::QUARTER;
        let columns = Columns::new(key("C major"), &[(q, 1), (q, 1)]);
        assert_eq!(columns.hit(0.0), ColumnHit::Insert(0));
        assert_eq!(columns.hit(columns.center(0)), ColumnHit::Column(0));
        assert_eq!(columns.hit(columns.center(1)), ColumnHit::Column(1));
        assert_eq!(
            columns.hit(columns.left(0) + COLUMN_WIDTH),
            ColumnHit::Insert(1)
        );
        assert_eq!(columns.hit(columns.center(2)), ColumnHit::Insert(2));
    }

    #[test]
    fn a_suspension_column_is_wider_with_one_part_per_figuring() {
        let q = Duration::QUARTER;
        let columns = Columns::new(key("C major"), &[(q, 1), (q, 2), (q, 1)]);
        assert!((columns.width_of(1) - 2.0 * MIN_PART_WIDTH).abs() < 0.01);
        let (first, second) = (columns.part_x(1, 0, 2), columns.part_x(1, 1, 2));
        assert!(columns.left(1) < first && first < second && second < columns.left(2));
        assert_eq!(columns.hit(second), ColumnHit::Column(1));
        assert_eq!(columns.hit(columns.center(2)), ColumnHit::Column(2));
        assert_eq!(columns.hit(columns.left(2) + 1.0), ColumnHit::Insert(2));
    }

    #[test]
    fn longer_notes_get_wider_columns_but_less_than_in_proportion() {
        let d = |s: &str| s.parse::<Duration>().unwrap();
        let columns = Columns::new(key("C major"), &[(d("4"), 1), (d("1"), 1), (d("16"), 1)]);
        let close = |a: f32, b: f32| (a - b).abs() < 0.01;
        assert!(close(columns.width_of(0), COLUMN_WIDTH));
        assert!(close(columns.width_of(1), 2.0 * COLUMN_WIDTH));
        assert!(close(columns.width_of(2), MIN_PART_WIDTH));
    }

    #[test]
    fn notes_get_stems_flags_and_dots_by_value() {
        let staff = Staff {
            top: BASS_STAFF_TOP,
            top_position: BASS_TOP_POSITION,
        };
        let drawn = |pitch: &str, duration: &str| {
            let mut scene = Scene::default();
            let pitch: Pitch = pitch.parse().unwrap();
            let duration: Duration = duration.parse().unwrap();
            staff.draw_note(
                &mut scene,
                0.0,
                pitch,
                key("C major"),
                duration,
                0.0,
                Some(0),
            );
            staff.draw_stem(&mut scene, 0.0, &[pitch.staff_position()], duration.value);
            scene
        };
        let texts = |scene: &Scene| {
            scene
                .glyphs
                .iter()
                .map(|g| g.text.clone())
                .collect::<Vec<_>>()
        };
        // Whole: open head, no stem.
        let whole = drawn("C3", "1");
        assert_eq!(texts(&whole), ["\u{E0A2}"]);
        assert!(whole.bars.is_empty());
        // Dotted half on a space: open head, dot, stem.
        let half = drawn("C3", "2.");
        assert_eq!(texts(&half), ["\u{E0A3}", "\u{E1E7}"]);
        assert_eq!(half.bars.len(), 1);
        // Eighth low on the staff: stem up with an up flag.
        let eighth = drawn("A2", "8");
        assert_eq!(texts(&eighth), ["\u{E0A4}", "\u{E240}"]);
        // Sixteenth high on the staff: stem down with a down flag.
        let sixteenth = drawn("G3", "16");
        assert_eq!(texts(&sixteenth), ["\u{E0A4}", "\u{E243}"]);
        let stem = sixteenth.bars[0];
        assert!(stem.y >= staff.y(25) - 1.0 && stem.height > 3.0 * SPACE);
    }

    #[test]
    fn bar_lines_follow_full_bars() {
        let k = key("C major");
        let notes: Vec<BassNote> = ["2", "2", "2.", "4", "2.", "2"]
            .iter()
            .map(|d| BassNote {
                duration: d.parse().unwrap(),
                ..BassNote::new("C3".parse().unwrap())
            })
            .collect();
        let line = BassLine::with_notes(k, TimeSignature::COMMON, notes);
        let instrument = intavolatura_core::Instrument::theorbo_in_a();
        let options = ViewOptions {
            show_hints: false,
            tablature: TablatureStyle::Italian,
        };
        let s = scene(&line, &[], &instrument, &options);
        // Bar lines are the tall thin bars at column edges: three rows each.
        let columns = columns_for(&line);
        let at = |x: f32| {
            s.bars
                .iter()
                .filter(|b| (b.x + b.width / 2.0 - x).abs() < 0.01)
                .count()
        };
        assert_eq!(at(columns.left(2) - 2.0), 3);
        assert_eq!(at(columns.left(4) - 2.0), 3);
        assert_eq!(at(columns.left(3) - 2.0), 0);
        // The dotted half runs over the second bar line: drawn as a warning.
        assert!(s.glyphs.iter().any(|g| g.warning));
    }

    #[test]
    fn the_key_signature_pushes_the_columns_right() {
        let c = Columns::new(key("C major"), &[]);
        let e = Columns::new(key("E major"), &[]);
        assert_eq!(e.left(0) - c.left(0), 4.0 * KEY_SIGNATURE_STEP);
    }

    #[test]
    fn clicks_map_to_bass_staff_positions() {
        let staff = Staff {
            top: BASS_STAFF_TOP,
            top_position: BASS_TOP_POSITION,
        };
        for position in [11, 18, 24, 26, 30] {
            assert_eq!(bass_staff_position(staff.y(position)), position);
            assert_eq!(bass_staff_position(staff.y(position) + 2.0), position);
        }
    }

    #[test]
    fn ledger_lines_only_outside_the_staff() {
        let staff = Staff {
            top: BASS_STAFF_TOP,
            top_position: BASS_TOP_POSITION,
        };
        let ledgers = |pitch: &str| {
            let mut scene = Scene::default();
            staff.draw_note(
                &mut scene,
                0.0,
                pitch.parse().unwrap(),
                key("C major"),
                Duration::new(NoteValue::Whole),
                0.0,
                Some(0),
            );
            scene.bars.len()
        };
        assert_eq!(ledgers("D3"), 0);
        assert_eq!(ledgers("G2"), 0);
        assert_eq!(ledgers("F2"), 0);
        assert_eq!(ledgers("E2"), 1);
        assert_eq!(ledgers("C2"), 2);
        assert_eq!(ledgers("G1"), 3);
        assert_eq!(ledgers("B3"), 0);
        assert_eq!(ledgers("C4"), 1);
    }

    #[test]
    fn accidentals_are_drawn_only_against_the_key() {
        let staff = Staff {
            top: BASS_STAFF_TOP,
            top_position: BASS_TOP_POSITION,
        };
        let glyphs = |pitch: &str, k: &str| {
            let mut scene = Scene::default();
            staff.draw_note(
                &mut scene,
                0.0,
                pitch.parse().unwrap(),
                key(k),
                Duration::new(NoteValue::Whole),
                0.0,
                Some(0),
            );
            scene.glyphs.len()
        };
        assert_eq!(glyphs("F#3", "D major"), 1);
        assert_eq!(glyphs("F3", "D major"), 2);
        assert_eq!(glyphs("F#3", "C major"), 2);
        assert_eq!(glyphs("Bb2", "D minor"), 1);
    }

    #[test]
    fn figures_use_raised_glyphs_and_lone_accidentals() {
        let figures = |s: &str| {
            let figuring: intavolatura_core::Figuring = s.parse().unwrap();
            figuring
                .figures()
                .iter()
                .map(figure_text)
                .collect::<Vec<_>>()
        };
        assert_eq!(figures("6+"), ["\u{EA5C}"]);
        assert_eq!(figures("#6 4"), ["\u{EA66}\u{EA5B}", "\u{EA55}"]);
        assert_eq!(figures("6 #"), ["\u{EA5B}", "\u{EA66}"]);
        assert_eq!(figures("3+"), ["\u{EA54}\u{EA6C}"]);
        assert_eq!(figures("8"), ["\u{EA60}"]);
        assert_eq!(figures("9"), ["\u{EA61}"]);
    }

    fn fingering(stops: &[(usize, u8)]) -> Fingering {
        let mut frets = [None; intavolatura_core::instrument::MAX_COURSES];
        for &(course, fret) in stops {
            frets[course] = Some(fret);
        }
        Fingering::from_frets(frets)
    }

    fn tab_signs(stops: &[(usize, u8)], style: TablatureStyle) -> Vec<(String, f32)> {
        let mut scene = Scene::default();
        let instrument = intavolatura_core::Instrument::theorbo_in_a();
        let struck: Vec<(usize, u8)> = fingering(stops).played().collect();
        draw_fingering(&mut scene, 0.0, &struck, &instrument, style);
        scene
            .glyphs
            .into_iter()
            .map(|g| (g.text, g.y - TAB_GLYPH_DROP))
            .collect()
    }

    #[test]
    fn italian_tablature_puts_course_one_on_the_bottom_line() {
        let signs = tab_signs(&[(0, 0), (2, 2), (5, 3)], TablatureStyle::Italian);
        assert_eq!(
            signs,
            [
                (italian_number(0), TAB_TOP + 5.0 * TAB_LINE),
                (italian_number(2), TAB_TOP + 3.0 * TAB_LINE),
                (italian_number(3), TAB_TOP),
            ]
        );
    }

    #[test]
    fn french_tablature_uses_letters_with_course_one_on_top() {
        let signs = tab_signs(&[(0, 0), (2, 2), (5, 9)], TablatureStyle::French);
        assert_eq!(
            signs,
            [
                (french_letter(0).to_string(), TAB_TOP),
                (french_letter(2).to_string(), TAB_TOP + 2.0 * TAB_LINE),
                (french_letter(9).to_string(), TAB_TOP + 5.0 * TAB_LINE),
            ]
        );
    }

    #[test]
    fn diapason_signs_follow_each_style() {
        let french: Vec<String> = [6, 7, 9, 10, 13]
            .iter()
            .flat_map(|&c| tab_signs(&[(c, 0)], TablatureStyle::French))
            .map(|(t, _)| t)
            .collect();
        let sign = |c: u32| char::from_u32(c).unwrap().to_string();
        assert_eq!(
            french,
            [
                sign(0xEBCD),
                sign(0xEBCE),
                sign(0xEBD0),
                italian_number(4),
                italian_number(7)
            ]
        );
        // k is the tenth letter, as there is no j.
        assert_eq!(french_letter(9), '\u{EBC9}');

        let italian = tab_signs(&[(6, 0), (9, 0)], TablatureStyle::Italian);
        assert_eq!(
            italian,
            [
                (italian_number(0), TAB_TOP - TAB_LINE),
                (italian_number(10), TAB_TOP - 2.0 * TAB_LINE),
            ]
        );
    }

    fn grand_staff() -> (Staff, Staff) {
        let treble = Staff {
            top: TREBLE_STAFF_TOP,
            top_position: TREBLE_TOP_POSITION,
        };
        let bass = Staff {
            top: VOICING_BASS_TOP,
            top_position: BASS_TOP_POSITION,
        };
        (treble, bass)
    }

    fn noteheads(pitches: &[&str], k: &str) -> Vec<Glyph> {
        let (treble, bass) = grand_staff();
        let voicing: Vec<(Pitch, bool)> =
            pitches.iter().map(|p| (p.parse().unwrap(), true)).collect();
        let mut scene = Scene::default();
        let whole = Duration::new(NoteValue::Whole);
        draw_voicing(&mut scene, 0.0, &voicing, key(k), whole, &treble, &bass);
        scene.glyphs
    }

    #[test]
    fn voicings_split_at_middle_c_and_draw_unisons_once() {
        let glyphs = noteheads(&["A2", "E3", "A3", "A3", "C#4"], "D minor");
        let heads: Vec<f32> = glyphs
            .iter()
            .filter(|g| g.text == "\u{E0A2}")
            .map(|g| g.y)
            .collect();
        assert_eq!(heads.len(), 4);
        assert_eq!(
            heads
                .iter()
                .filter(|&&y| y < VOICING_BASS_TOP - SPACE)
                .count(),
            1
        );
        // C# is not in D minor's signature, so it gets a sharp.
        assert!(glyphs.iter().any(|g| g.text == "\u{E262}"));
    }

    #[test]
    fn the_upper_note_of_a_second_moves_right() {
        let glyphs = noteheads(&["F2", "G2", "B2"], "C major");
        let xs: Vec<f32> = glyphs
            .iter()
            .filter(|g| g.text == "\u{E0A2}")
            .map(|g| g.x)
            .collect();
        assert_eq!(xs, [0.0, 1.18 * SPACE, 0.0]);
    }

    #[test]
    fn hints_are_drawn_faded_and_can_be_hidden() {
        let k = key("D minor");
        let mut note = BassNote::new("A2".parse().unwrap());
        note.figurings = "".parse().unwrap();
        let line = BassLine::with_notes(k, TimeSignature::COMMON, vec![note.clone()]);
        let instrument = intavolatura_core::Instrument::theorbo_in_a();
        let settings = intavolatura_core::Settings::default();
        let realization = intavolatura_core::realize(&note, k, &instrument, &settings);
        assert_eq!(k.mode(), Mode::Minor);

        let faded = |show_hints| {
            let options = ViewOptions {
                show_hints,
                tablature: TablatureStyle::Italian,
            };
            let s = scene(
                &line,
                std::slice::from_ref(&realization),
                &instrument,
                &options,
            );
            s.glyphs.iter().filter(|g| g.faded).count()
        };
        assert_eq!(faded(true), 1);
        assert_eq!(faded(false), 0);
    }
}
