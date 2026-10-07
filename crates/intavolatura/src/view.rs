//! Lays out the views as a display list of glyphs and bars in logical pixels.
//!
//! All positions are computed here so the Slint side only draws. Music
//! glyphs use the Bravura font (SMuFL code points): its line box is centred
//! on the baseline, so a glyph centred on `y` sits on that staff position.

use intavolatura_core::{
    Accidental, BassLine, Figure, FigureAccidental, Fingering, Instrument, Key, Pitch, Realization,
};

/// One staff space.
pub const SPACE: f32 = 10.0;
const HALF: f32 = SPACE / 2.0;
/// Bravura's em is four staff spaces.
const MUSIC_SIZE: f32 = 4.0 * SPACE;
const FIGURE_SIZE: f32 = 3.8 * SPACE;
const LINE: f32 = 1.0;

pub const COLUMN_WIDTH: f32 = 64.0;
const KEY_SIGNATURE_X: f32 = 52.0;
const KEY_SIGNATURE_STEP: f32 = 11.5;

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
const TAB_TOP: f32 = VOICING_BASS_TOP + 10.0 * SPACE;
const TAB_LINE: f32 = 16.0;
const TAB_SIZE: f32 = 15.0;
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

/// Everything to draw, plus the size of the scrollable content.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub glyphs: Vec<Glyph>,
    pub bars: Vec<Bar>,
    pub width: f32,
    pub height: f32,
}

/// Horizontal layout shared by every row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Columns {
    /// Left edge of the first column.
    pub left: f32,
    pub count: usize,
}

impl Columns {
    pub fn new(key: Key, count: usize) -> Columns {
        let accidentals = key.signature().unsigned_abs() as f32;
        Columns {
            left: KEY_SIGNATURE_X + accidentals * KEY_SIGNATURE_STEP + 16.0,
            count,
        }
    }

    pub fn center(&self, index: usize) -> f32 {
        self.left + (index as f32 + 0.5) * COLUMN_WIDTH
    }

    /// Content width, leaving one empty column to click into for appending.
    pub fn width(&self) -> f32 {
        self.left + (self.count as f32 + 1.0) * COLUMN_WIDTH + SPACE
    }

    /// What a click at `x` means for the bass line.
    pub fn hit(&self, x: f32) -> ColumnHit {
        let offset = (x - self.left) / COLUMN_WIDTH;
        if offset < 0.0 {
            return ColumnHit::Insert(0);
        }
        let index = offset.floor() as usize;
        if index >= self.count {
            return ColumnHit::Insert(self.count);
        }
        // The outer fifth of a column on either side inserts at that boundary.
        match offset.fract() {
            f if f < 0.2 => ColumnHit::Insert(index),
            f if f > 0.8 => ColumnHit::Insert(index + 1),
            _ => ColumnHit::Column(index),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnHit {
    Column(usize),
    /// Insert before this index; the count appends.
    Insert(usize),
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
    let columns = Columns::new(key, line.notes().len());
    let mut scene = Scene {
        width: columns.width(),
        height: CONTENT_BOTTOM,
        ..Scene::default()
    };

    let staff = Staff {
        top: BASS_STAFF_TOP,
        top_position: BASS_TOP_POSITION,
    };
    let width = scene.width;
    staff.draw_lines(&mut scene, width);
    scene.glyph(26.0, staff.y(24), "\u{E062}", true, MUSIC_SIZE); // F clef on F3
    staff.draw_key_signature(&mut scene, key, BASS_TOP_POSITION);

    for (index, note) in line.notes().iter().enumerate() {
        let x = columns.center(index);
        staff.draw_note(&mut scene, x, note.pitch, key, 0.0, 0);
        let hint = realizations
            .get(index)
            .and_then(|r| r.chord.leading_tone_hint())
            .filter(|_| options.show_hints);
        draw_figuring(&mut scene, x, note.figuring.figures(), hint.is_some());
    }

    let treble = Staff {
        top: TREBLE_STAFF_TOP,
        top_position: TREBLE_TOP_POSITION,
    };
    let bass = Staff {
        top: VOICING_BASS_TOP,
        top_position: BASS_TOP_POSITION,
    };
    treble.draw_lines(&mut scene, width);
    bass.draw_lines(&mut scene, width);
    scene.bar(
        0.0,
        TREBLE_STAFF_TOP,
        1.5,
        VOICING_BASS_TOP + 4.0 * SPACE - TREBLE_STAFF_TOP,
    );
    scene.glyph(26.0, treble.y(32), "\u{E050}", true, MUSIC_SIZE); // G clef on G4
    scene.glyph(26.0, bass.y(24), "\u{E062}", true, MUSIC_SIZE);
    treble.draw_key_signature(&mut scene, key, TREBLE_TOP_POSITION);
    bass.draw_key_signature(&mut scene, key, BASS_TOP_POSITION);

    for line in 0..6 {
        scene.bar(
            0.0,
            TAB_TOP + line as f32 * TAB_LINE - LINE / 2.0,
            width,
            LINE,
        );
    }

    for (index, realization) in realizations.iter().enumerate() {
        let x = columns.center(index);
        draw_voicing(&mut scene, x, &realization.voicing, key, &treble, &bass);
        match realization.shown {
            Some(fingering) => {
                draw_fingering(&mut scene, x, &fingering, instrument, options.tablature)
            }
            None => scene.glyphs.push(Glyph {
                x,
                y: TAB_TOP + 2.5 * TAB_LINE,
                text: "no fingering".to_string(),
                music: false,
                size: 11.0,
                faded: true,
            }),
        }
    }
    scene
}

/// Draws a voicing on the grand staff: middle C and up on the treble staff.
/// Unisons are drawn once; the upper note of a second moves right.
fn draw_voicing(
    scene: &mut Scene,
    x: f32,
    voicing: &[Pitch],
    key: Key,
    treble: &Staff,
    bass: &Staff,
) {
    let mut notes = voicing.to_vec();
    notes.sort_by_key(|p| (p.staff_position(), p.semitones()));
    notes.dedup();
    for (staff, on_treble) in [(treble, true), (bass, false)] {
        let mut previous: Option<(i16, bool)> = None;
        let mut accidentals = 0;
        for &pitch in notes
            .iter()
            .filter(|p| (p.staff_position() >= MIDDLE_C_POSITION) == on_treble)
        {
            let position = pitch.staff_position();
            let shifted = previous.is_some_and(|(p, s)| position - p == 1 && !s);
            let shift = if shifted { 1.25 * SPACE } else { 0.0 };
            if staff.draw_note(scene, x, pitch, key, shift, accidentals % 2) {
                accidentals += 1;
            }
            previous = Some((position, shifted));
        }
    }
}

/// Draws a fingering in tablature: fretted courses on the six lines,
/// diapasons as signs below.
fn draw_fingering(
    scene: &mut Scene,
    x: f32,
    fingering: &Fingering,
    instrument: &Instrument,
    style: TablatureStyle,
) {
    let courses = instrument.courses();
    let fretted = courses.iter().take_while(|c| !c.is_diapason()).count();
    let mut diapasons = 0;
    for (course, fret) in fingering.played() {
        if !courses[course].is_diapason() {
            let line = match style {
                TablatureStyle::Italian => fretted - 1 - course,
                TablatureStyle::French => course,
            };
            let text = match style {
                TablatureStyle::Italian => fret.to_string(),
                TablatureStyle::French => french_letter(fret).to_string(),
            };
            scene.tab_sign(x, TAB_TOP + line as f32 * TAB_LINE, &text, true);
            continue;
        }

        let ordinal = course - fretted; // 0 for course 7
        match style {
            TablatureStyle::French => {
                let text = match ordinal {
                    0..=3 => format!("{}a", "/".repeat(ordinal)),
                    n => n.to_string(),
                };
                let y = TAB_BOTTOM + (1 + diapasons) as f32 * TAB_LINE;
                scene.tab_sign(x, y, &text, false);
                diapasons += 1;
            }
            TablatureStyle::Italian => {
                // Course 7 is 0 on the first ledger line, the others their
                // course number on a ledger line below.
                let (level, text) = if ordinal == 0 {
                    (1, "0".to_string())
                } else {
                    diapasons += 1;
                    (1 + diapasons, (course + 1).to_string())
                };
                let y = TAB_BOTTOM + level as f32 * TAB_LINE;
                scene.bar(x - 1.4 * SPACE, y - LINE / 2.0, 2.8 * SPACE, LINE);
                scene.tab_sign(x, y, &text, true);
            }
        }
    }
}

/// French tablature letter for a fret: a for open, then b, c … with no j.
fn french_letter(fret: u8) -> char {
    const LETTERS: &[u8] = b"abcdefghiklmnop";
    LETTERS.get(fret as usize).map_or('?', |&b| b as char)
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
    /// Returns whether an accidental was drawn; `accidental_column` moves it
    /// further left to keep stacked accidentals apart.
    fn draw_note(
        &self,
        scene: &mut Scene,
        x: f32,
        pitch: Pitch,
        key: Key,
        shift: f32,
        accidental_column: usize,
    ) -> bool {
        let position = pitch.staff_position();
        let y = self.y(position);
        let head_x = x + shift;
        scene.glyph(head_x, y, "\u{E0A2}", true, MUSIC_SIZE);

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
        let drawn = accidental != key.signature_accidental(pitch.name.letter);
        if drawn {
            let ax = x - 1.6 * SPACE - accidental_column as f32 * 1.1 * SPACE;
            scene.glyph(ax, y, accidental_glyph(accidental), true, MUSIC_SIZE);
        }
        drawn
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
        _ => '\u{EA5D}',
    };
    let raised = |n: u8| match n {
        2 => Some('\u{EA53}'),
        4 => Some('\u{EA56}'),
        5 => Some('\u{EA58}'),
        6 => Some('\u{EA5C}'),
        7 => Some('\u{EA5E}'),
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
    fn tab_sign(&mut self, x: f32, y: f32, text: &str, knockout: bool) {
        if knockout {
            let width = 0.55 * TAB_SIZE * text.chars().count() as f32 + 3.0;
            self.bars.push(Bar {
                x: x - width / 2.0,
                y: y - 0.45 * TAB_SIZE,
                width,
                height: 0.9 * TAB_SIZE,
                knockout: true,
            });
        }
        self.glyph(x, y, text, false, TAB_SIZE);
    }

    fn glyph(&mut self, x: f32, y: f32, text: &str, music: bool, size: f32) {
        self.glyphs.push(Glyph {
            x,
            y,
            text: text.to_string(),
            music,
            size,
            faded: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use intavolatura_core::{BassNote, Mode};

    fn key(s: &str) -> Key {
        s.parse().unwrap()
    }

    #[test]
    fn clicks_map_to_columns_and_insert_points() {
        let columns = Columns::new(key("C major"), 2);
        assert_eq!(columns.hit(0.0), ColumnHit::Insert(0));
        assert_eq!(columns.hit(columns.center(0)), ColumnHit::Column(0));
        assert_eq!(columns.hit(columns.center(1)), ColumnHit::Column(1));
        assert_eq!(
            columns.hit(columns.left + COLUMN_WIDTH),
            ColumnHit::Insert(1)
        );
        assert_eq!(columns.hit(columns.center(2)), ColumnHit::Insert(2));
    }

    #[test]
    fn the_key_signature_pushes_the_columns_right() {
        let c = Columns::new(key("C major"), 0);
        let e = Columns::new(key("E major"), 0);
        assert_eq!(e.left - c.left, 4.0 * KEY_SIGNATURE_STEP);
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
                0.0,
                0,
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
            staff.draw_note(&mut scene, 0.0, pitch.parse().unwrap(), key(k), 0.0, 0);
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
        draw_fingering(&mut scene, 0.0, &fingering(stops), &instrument, style);
        scene.glyphs.into_iter().map(|g| (g.text, g.y)).collect()
    }

    #[test]
    fn italian_tablature_puts_course_one_on_the_bottom_line() {
        let signs = tab_signs(&[(0, 0), (2, 2), (5, 3)], TablatureStyle::Italian);
        assert_eq!(
            signs,
            [
                ("0".to_string(), TAB_TOP + 5.0 * TAB_LINE),
                ("2".to_string(), TAB_TOP + 3.0 * TAB_LINE),
                ("3".to_string(), TAB_TOP),
            ]
        );
    }

    #[test]
    fn french_tablature_uses_letters_with_course_one_on_top() {
        let signs = tab_signs(&[(0, 0), (2, 2), (5, 9)], TablatureStyle::French);
        assert_eq!(
            signs,
            [
                ("a".to_string(), TAB_TOP),
                ("c".to_string(), TAB_TOP + 2.0 * TAB_LINE),
                ("k".to_string(), TAB_TOP + 5.0 * TAB_LINE),
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
        assert_eq!(french, ["a", "/a", "///a", "4", "7"]);

        let italian = tab_signs(&[(6, 0), (9, 0)], TablatureStyle::Italian);
        assert_eq!(
            italian,
            [
                ("0".to_string(), TAB_BOTTOM + TAB_LINE),
                ("10".to_string(), TAB_BOTTOM + 2.0 * TAB_LINE),
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
        let voicing: Vec<Pitch> = pitches.iter().map(|p| p.parse().unwrap()).collect();
        let mut scene = Scene::default();
        draw_voicing(&mut scene, 0.0, &voicing, key(k), &treble, &bass);
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
        assert_eq!(xs, [0.0, 1.25 * SPACE, 0.0]);
    }

    #[test]
    fn hints_are_drawn_faded_and_can_be_hidden() {
        let k = key("D minor");
        let mut note = BassNote::new("A2".parse().unwrap());
        note.figuring = "".parse().unwrap();
        let line = BassLine::with_notes(k, vec![note.clone()]);
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
