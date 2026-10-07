//! Lays out the views as a display list of glyphs and bars in logical pixels.
//!
//! All positions are computed here so the Slint side only draws. Music
//! glyphs use the Bravura font (SMuFL code points): its line box is centred
//! on the baseline, so a glyph centred on `y` sits on that staff position.

use intavolatura_core::{Accidental, BassLine, Figure, FigureAccidental, Key, Pitch, Realization};

/// One staff space.
pub const SPACE: f32 = 10.0;
const HALF: f32 = SPACE / 2.0;
/// Bravura's em is four staff spaces.
const MUSIC_SIZE: f32 = 4.0 * SPACE;
const FIGURE_SIZE: f32 = 3.8 * SPACE;
const LINE: f32 = 1.0;

pub const COLUMN_WIDTH: f32 = 64.0;
const KEY_SIGNATURE_X: f32 = 52.0;
const KEY_SIGNATURE_STEP: f32 = 10.0;

/// Staff position (see `Pitch::staff_position`) of each staff's top line.
const BASS_TOP_POSITION: i16 = 26; // A3

/// Vertical layout of the bass staff row.
pub const BASS_STAFF_TOP: f32 = 70.0;
/// Top of the figures under the bass staff.
pub const FIGURES_TOP: f32 = BASS_STAFF_TOP + 4.0 * SPACE + 78.0;
const FIGURE_LINE: f32 = 26.0;
/// Where the bass staff row ends; clicks above it edit the bass line.
pub const BASS_ROW_BOTTOM: f32 = FIGURES_TOP + 4.0 * FIGURE_LINE;

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

/// What the views need besides the bass line itself.
pub struct ViewOptions {
    pub show_hints: bool,
}

/// Builds the scene for a bass line and the realization of each note.
pub fn scene(line: &BassLine, realizations: &[Realization], options: &ViewOptions) -> Scene {
    let key = line.key();
    let columns = Columns::new(key, line.notes().len());
    let mut scene = Scene {
        width: columns.width(),
        height: BASS_ROW_BOTTOM + SPACE,
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
        staff.draw_note(&mut scene, x, note.pitch, key, 0.0);
        let hint = realizations
            .get(index)
            .and_then(|r| r.chord.leading_tone_hint())
            .filter(|_| options.show_hints);
        draw_figuring(&mut scene, x, note.figuring.figures(), hint.is_some());
    }
    scene
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
            scene.bars.push(Bar {
                x: 0.0,
                y: self.top + line as f32 * SPACE - LINE / 2.0,
                width,
                height: LINE,
            });
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
    fn draw_note(&self, scene: &mut Scene, x: f32, pitch: Pitch, key: Key, shift: f32) {
        let position = pitch.staff_position();
        let y = self.y(position);
        let head_x = x + shift;
        scene.glyph(head_x, y, "\u{E0A2}", true, MUSIC_SIZE);

        let ledgers = (position..=self.bottom_position() - 2)
            .chain(self.top_position + 2..=position)
            .filter(|p| (p - self.top_position) % 2 == 0);
        for ledger in ledgers {
            scene.bars.push(Bar {
                x: head_x - 1.2 * SPACE,
                y: self.y(ledger) - LINE / 2.0,
                width: 2.4 * SPACE,
                height: LINE,
            });
        }

        let accidental = pitch.name.accidental;
        if accidental != key.signature_accidental(pitch.name.letter) {
            scene.glyph(
                x - 1.6 * SPACE,
                y,
                accidental_glyph(accidental),
                true,
                MUSIC_SIZE,
            );
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
            staff.draw_note(&mut scene, 0.0, pitch.parse().unwrap(), key("C major"), 0.0);
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
            staff.draw_note(&mut scene, 0.0, pitch.parse().unwrap(), key(k), 0.0);
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
            let options = ViewOptions { show_hints };
            let s = scene(&line, std::slice::from_ref(&realization), &options);
            s.glyphs.iter().filter(|g| g.faded).count()
        };
        assert_eq!(faded(true), 1);
        assert_eq!(faded(false), 0);
    }
}
