//! Application state and its response to input, independent of Slint.

use std::path::{Path, PathBuf};

use intavolatura_core::file_format::{self, Document};
use intavolatura_core::musicxml;
use intavolatura_core::{
    BassLine, BassNote, Duration, Editor, Figurings, Instrument, Key, Letter, NoteValue, Pitch,
    Realization, Settings, TimeSignature, realize,
};

use crate::preferences::{Preferences, Theme};
use crate::view::{self, ColumnHit, Columns, TablatureStyle, ViewOptions};

/// Keys the app reacts to, decoupled from the toolkit's key codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Up,
    Down,
    Left,
    Right,
    Delete,
    Enter,
    Escape,
    Char(char),
}

/// Lowest and highest staff positions a click may place a bass note on.
const LOWEST_CLICK: i16 = 9; // E1
const HIGHEST_CLICK: i16 = 31; // F4

pub struct App {
    editor: Editor,
    instrument: Instrument,
    /// The instrument tuned for `tuned_for`.
    tuned: Instrument,
    tuned_for: Key,
    settings: Settings,
    show_hints: bool,
    pub theme: Theme,
    pub tablature: TablatureStyle,
    sign_size: u8,
    italian_on_line: bool,
    french_on_line: bool,
    /// The realization of each note, and the note and key it was made for.
    realizations: Vec<(BassNote, Key, Realization)>,
    pub selected: Option<usize>,
    /// The duration new notes get: the last one chosen.
    pub duration: Duration,
    pub editing: bool,
    pub path: Option<PathBuf>,
    /// The bass line as last opened or saved, to tell whether it changed.
    saved: BassLine,
    pub status: String,
}

impl App {
    pub fn new(document: Document, path: Option<PathBuf>) -> App {
        let settings = Settings::default();
        let key = document.line.key();
        let tuned = settings.tune(&document.instrument, key);
        let mut app = App {
            selected: (!document.line.notes().is_empty()).then_some(0),
            editor: Editor::new(document.line),
            instrument: document.instrument,
            tuned,
            tuned_for: key,
            settings,
            show_hints: true,
            theme: Theme::System,
            tablature: TablatureStyle::Italian,
            sign_size: Preferences::default().sign_size,
            italian_on_line: Preferences::default().italian_on_line,
            french_on_line: Preferences::default().french_on_line,
            realizations: Vec::new(),
            editing: false,
            saved: BassLine::new(key),
            duration: Duration::QUARTER,
            path,
            status: String::new(),
        };
        app.saved = app.line().clone();
        app.update_realizations();
        app
    }

    pub fn empty(key: Key) -> App {
        App::new(
            Document {
                line: BassLine::new(key),
                instrument: Instrument::theorbo_in_a(),
            },
            None,
        )
    }

    /// Opens a saved bass line.
    pub fn open(path: &Path) -> Result<App, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let document = file_format::read(&text).map_err(|e| e.to_string())?;
        Ok(App::new(document, Some(path.to_path_buf())))
    }

    /// Writes the bass line to `path`, which becomes the file being edited.
    pub fn save_to(&mut self, path: PathBuf) -> bool {
        let document = Document {
            line: self.line().clone(),
            instrument: self.instrument.clone(),
        };
        match std::fs::write(&path, file_format::write(&document)) {
            Ok(()) => {
                self.status = format!("Saved {}", path.display());
                self.saved = self.line().clone();
                self.path = Some(path);
                true
            }
            Err(e) => {
                self.status = format!("Could not save {}: {e}", path.display());
                false
            }
        }
    }

    /// Writes the bass line as MusicXML, with the realization part if asked.
    pub fn export_musicxml(&mut self, path: &Path, with_realization: bool) {
        let realizations: Vec<Realization> = self
            .realizations
            .iter()
            .map(|(_, _, r)| r.clone())
            .collect();
        let title = self
            .path
            .as_deref()
            .or(Some(path))
            .and_then(Path::file_stem)
            .map_or("Untitled".into(), |s| s.to_string_lossy().into_owned());
        let xml = musicxml::export(
            self.line(),
            with_realization.then_some(&realizations[..]),
            &title,
        );
        self.status = match std::fs::write(path, xml) {
            Ok(()) => format!("Exported {}", path.display()),
            Err(e) => format!("Could not export {}: {e}", path.display()),
        };
    }

    /// Whether the bass line differs from the file it was opened from or
    /// last saved to.
    pub fn is_modified(&self) -> bool {
        *self.line() != self.saved
    }

    /// The window title: file name, a dot when modified, the app name.
    pub fn title(&self) -> String {
        let name = self
            .path
            .as_deref()
            .and_then(Path::file_name)
            .map_or("Untitled".into(), |n| n.to_string_lossy());
        let modified = if self.is_modified() { " •" } else { "" };
        format!("{name}{modified} — Intavolatura")
    }

    pub fn preferences(&self) -> Preferences {
        Preferences {
            theme: self.theme,
            tablature: self.tablature,
            show_hints: self.show_hints,
            retune_diapasons: self.settings.retune_diapasons,
            fret_span: self.settings.hand.fret_span,
            candidates_to_cycle: self.settings.candidates_to_cycle,
            sign_size: self.sign_size,
            italian_on_line: self.italian_on_line,
            french_on_line: self.french_on_line,
        }
    }

    /// Applies new preferences, recomputing every realization if the
    /// fingering settings changed.
    pub fn set_preferences(&mut self, preferences: &Preferences) {
        self.theme = preferences.theme;
        self.sign_size = preferences.sign_size;
        self.italian_on_line = preferences.italian_on_line;
        self.french_on_line = preferences.french_on_line;
        self.tablature = preferences.tablature;
        self.show_hints = preferences.show_hints;
        let settings = preferences.settings();
        if settings != self.settings {
            self.settings = settings;
            self.tuned = self.settings.tune(&self.instrument, self.tuned_for);
            self.realizations.clear();
            self.update_realizations();
        }
    }

    pub fn line(&self) -> &BassLine {
        self.editor.line()
    }

    pub fn columns(&self) -> Columns {
        view::columns_for(self.line())
    }

    pub fn scene(&self) -> view::Scene {
        let realizations: Vec<Realization> = self
            .realizations
            .iter()
            .map(|(_, _, r)| r.clone())
            .collect();
        let options = ViewOptions {
            show_hints: self.show_hints,
            tablature: self.tablature,
            signs: self.preferences().signs(),
        };
        view::scene(self.line(), &realizations, &self.tuned, &options)
    }

    /// One line describing the selected note.
    pub fn describe_selection(&self) -> String {
        let Some(index) = self.selected else {
            return "Click the staff to add a bass note · ? shows all keys".to_string();
        };
        let note = &self.line().notes()[index];
        let (_, _, realization) = &self.realizations[index];
        let value = match note.duration.value {
            NoteValue::Whole => "whole",
            NoteValue::Half => "half",
            NoteValue::Quarter => "quarter",
            NoteValue::Eighth => "eighth",
            NoteValue::Sixteenth => "sixteenth",
        };
        let dotted = if note.duration.dotted { "dotted " } else { "" };
        if note.rest {
            return format!("{dotted}{value} rest · click the staff or press R for a note");
        }
        let figuring = note.figurings.to_string();
        let fingering = match realization.shown {
            None => "no playable fingering".to_string(),
            Some(shown) => {
                let position = realization
                    .candidates
                    .iter()
                    .position(|r| r.fingering == shown)
                    .map_or("pinned".to_string(), |i| {
                        format!("{}/{}", i + 1, realization.candidates.len())
                    });
                let pinned = if note.pin.is_some() { ", pinned" } else { "" };
                // Each part's notes; a suspension's parts joined by an arrow.
                let parts: Vec<String> = realization
                    .parts
                    .iter()
                    .map(|part| {
                        let notes: Vec<String> =
                            part.notes.iter().map(|(p, _)| p.to_string()).collect();
                        notes.join(" ")
                    })
                    .collect();
                format!("fingering {position}{pinned}: {}", parts.join(" → "))
            }
        };
        let figuring = if figuring.is_empty() {
            "5/3".to_string()
        } else {
            figuring
        };
        format!(
            "{} {dotted}{value}  {}  ·  {}",
            note.pitch, figuring, fingering
        )
    }

    /// Handles a key press; returns whether it was used.
    pub fn key(&mut self, input: Input, shift: bool, command: bool) -> bool {
        if self.editing {
            if input == Input::Escape {
                self.editing = false;
                return true;
            }
            return false;
        }
        if command {
            return match input {
                Input::Char('z') if shift => self.edit(|e| e.redo()),
                Input::Char('Z') => self.edit(|e| e.redo()),
                Input::Char('z') => self.edit(|e| e.undo()),
                Input::Char('y') => self.edit(|e| e.redo()),
                _ => false,
            };
        }

        let count = self.line().notes().len();
        match (input, self.selected) {
            (Input::Left, Some(i)) => self.selected = Some(i.saturating_sub(1)),
            (Input::Right, Some(i)) => self.selected = Some((i + 1).min(count - 1)),
            (Input::Left | Input::Right, None) if count > 0 => {
                self.selected = Some(if input == Input::Left { count - 1 } else { 0 })
            }
            (Input::Up | Input::Down, Some(i)) => {
                let up = input == Input::Up;
                if shift {
                    self.edit(|e| e.step_accidental(i, up));
                } else {
                    self.edit(|e| e.step_pitch(i, if up { 1 } else { -1 }));
                }
            }
            (Input::Char('n'), Some(i)) => {
                let pitch = self.line().notes()[i].pitch;
                self.edit(|e| e.set_pitch(i, pitch.with_accidental(Default::default())));
            }
            (Input::Delete, Some(i)) => {
                self.edit(|e| e.remove(i));
                let count = self.line().notes().len();
                self.selected = (count > 0).then(|| i.min(count - 1));
            }
            (Input::Enter, Some(i)) => self.editing = !self.line().notes()[i].rest,
            (Input::Char(c @ ('[' | ']')), Some(i)) => {
                let pin = self.realizations[i].2.cycle(c == ']');
                self.edit(|e| e.set_pin(i, pin));
            }
            (Input::Char('r'), Some(i)) => {
                self.edit(|e| e.toggle_rest(i));
            }
            (Input::Char(c @ '3'..='7'), _) => {
                let value = NoteValue::ALL[(b'7' - c as u8) as usize];
                self.set_duration(Duration::new(value));
            }
            (Input::Char('.'), _) => {
                let current = self.current_duration();
                self.set_duration(Duration {
                    dotted: !current.dotted,
                    ..current
                });
            }
            (Input::Escape, Some(_)) => self.selected = None,
            _ => return false,
        }
        true
    }

    /// Handles a click at content coordinates; returns whether to start
    /// editing the figuring of the selected column.
    pub fn click(&mut self, x: f32, y: f32) -> bool {
        self.editing = false;
        let hit = self.columns().hit(x);
        if y > view::BASS_ROW_BOTTOM {
            if let ColumnHit::Column(i) = hit {
                self.selected = Some(i);
            }
            return false;
        }
        if y > view::FIGURES_TOP - view::SPACE {
            if let ColumnHit::Column(i) = hit {
                self.selected = Some(i);
                // A rest has no figures to edit.
                self.editing = !self.line().notes()[i].rest;
                return self.editing;
            }
            return false;
        }

        let position = view::bass_staff_position(y).clamp(LOWEST_CLICK, HIGHEST_CLICK);
        let letter = Letter::from_index(position);
        let pitch =
            Pitch::from_staff_position(position, self.line().key().signature_accidental(letter));
        match hit {
            ColumnHit::Column(i) => {
                self.selected = Some(i);
                self.edit(|e| e.set_pitch(i, pitch));
            }
            ColumnHit::Insert(i) => {
                let duration = self.duration;
                self.edit(|e| e.insert(i, pitch, duration));
                self.selected = Some(i);
            }
        }
        false
    }

    /// The canonical text of the selected note's figuring, for editing.
    pub fn selected_figuring_text(&self) -> String {
        self.selected
            .map(|i| self.line().notes()[i].figurings.to_string())
            .unwrap_or_default()
    }

    /// Applies typed figuring text; on invalid text keeps editing and
    /// returns false.
    pub fn accept_figuring(&mut self, text: &str) -> bool {
        let (Some(i), Ok(figurings)) = (self.selected, text.parse::<Figurings>()) else {
            return false;
        };
        self.edit(|e| e.set_figurings(i, figurings));
        self.editing = false;
        true
    }

    /// The selected note's duration, or the one new notes get.
    pub fn current_duration(&self) -> Duration {
        self.selected
            .map_or(self.duration, |i| self.line().notes()[i].duration)
    }

    /// Sets the selected note's duration, and the one new notes get.
    pub fn set_duration(&mut self, duration: Duration) {
        self.duration = duration;
        if let Some(i) = self.selected {
            self.edit(|e| e.set_duration(i, duration));
        }
    }

    /// Starts a new time signature at the selected note, or removes it.
    pub fn set_time_change(&mut self, time: Option<TimeSignature>) {
        if let Some(i) = self.selected {
            self.edit(|e| e.set_time_change(i, time));
        }
    }

    /// Sets the upbeat: a short first bar of this length, or none.
    pub fn set_upbeat(&mut self, upbeat: Option<Duration>) {
        self.edit(|e| e.set_upbeat(upbeat));
    }

    pub fn set_time(&mut self, time: TimeSignature) {
        self.edit(|e| e.set_time(time));
    }

    pub fn set_key(&mut self, key: Key) {
        self.edit(|e| e.set_key(key));
    }

    fn edit(&mut self, change: impl FnOnce(&mut Editor) -> bool) -> bool {
        let changed = change(&mut self.editor);
        if changed {
            self.status.clear();
            self.update_realizations();
            let count = self.line().notes().len();
            self.selected = self.selected.filter(|&i| i < count);
        }
        changed
    }

    /// Recomputes only the realizations whose note or key changed.
    fn update_realizations(&mut self) {
        let key = self.line().key();
        if self.tuned_for != key {
            self.tuned = self.settings.tune(&self.instrument, key);
            self.tuned_for = key;
        }
        let notes = self.editor.line().notes();
        self.realizations.truncate(notes.len());
        for (i, note) in notes.iter().enumerate() {
            let fresh = self
                .realizations
                .get(i)
                .is_some_and(|(n, k, _)| n == note && *k == key);
            if !fresh {
                let r = realize(note, key, &self.tuned, &self.settings);
                let entry = (note.clone(), key, r);
                match self.realizations.get_mut(i) {
                    Some(slot) => *slot = entry,
                    None => self.realizations.push(entry),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::{BASS_STAFF_TOP, COLUMN_WIDTH};

    fn app() -> App {
        App::empty("D minor".parse().unwrap())
    }

    /// The y of a staff position on the bass staff.
    fn y(position: i16) -> f32 {
        BASS_STAFF_TOP + (26 - position) as f32 * view::SPACE / 2.0
    }

    #[test]
    fn clicking_appends_notes_spelled_in_the_key() {
        let mut app = app();
        let x = app.columns().center(0);
        app.click(x, y(20)); // B2, flat in D minor
        let x = app.columns().center(1);
        app.click(x, y(22)); // D3
        let pitches: Vec<String> = app
            .line()
            .notes()
            .iter()
            .map(|n| n.pitch.to_string())
            .collect();
        assert_eq!(pitches, ["Bb2", "D3"]);
        assert_eq!(app.selected, Some(1));
    }

    #[test]
    fn clicking_a_column_moves_its_note_and_between_columns_inserts() {
        let mut app = app();
        app.click(app.columns().center(0), y(22));
        app.click(app.columns().center(1), y(19));
        app.click(app.columns().center(0), y(21));
        app.click(app.columns().left(0) + COLUMN_WIDTH, y(18));
        let pitches: Vec<String> = app
            .line()
            .notes()
            .iter()
            .map(|n| n.pitch.to_string())
            .collect();
        assert_eq!(pitches, ["C3", "G2", "A2"]);
    }

    #[test]
    fn keys_move_select_and_undo() {
        let mut app = app();
        app.click(app.columns().center(0), y(22)); // D3
        assert!(app.key(Input::Up, false, false));
        assert_eq!(app.line().notes()[0].pitch.to_string(), "E3");
        app.key(Input::Up, true, false);
        assert_eq!(app.line().notes()[0].pitch.to_string(), "E#3");
        app.key(Input::Char('n'), false, false);
        assert_eq!(app.line().notes()[0].pitch.to_string(), "E3");
        app.key(Input::Char('z'), false, true);
        assert_eq!(app.line().notes()[0].pitch.to_string(), "E#3");
        app.key(Input::Char('z'), true, true);
        assert_eq!(app.line().notes()[0].pitch.to_string(), "E3");
        app.key(Input::Delete, false, false);
        assert!(app.line().notes().is_empty());
        assert_eq!(app.selected, None);
    }

    #[test]
    fn figurings_are_accepted_only_when_valid() {
        let mut app = app();
        app.click(app.columns().center(0), y(19)); // A2
        assert!(app.key(Input::Enter, false, false));
        assert!(app.editing);
        assert!(!app.accept_figuring("1"));
        assert!(app.editing);
        assert!(app.accept_figuring("#"));
        assert!(!app.editing);
        assert_eq!(app.line().notes()[0].figurings.to_string(), "#");
    }

    #[test]
    fn r_toggles_a_rest_and_rests_take_no_figures() {
        let mut app = app();
        app.click(app.columns().center(0), y(19));
        assert!(app.key(Input::Char('r'), false, false));
        assert!(app.line().notes()[0].rest);
        assert!(app.describe_selection().contains("rest"));
        app.key(Input::Enter, false, false);
        assert!(!app.editing);
        app.key(Input::Char('r'), false, false);
        assert!(!app.line().notes()[0].rest);
    }

    #[test]
    fn cycling_pins_and_wraps_back_to_unpinned() {
        let mut app = app();
        app.click(app.columns().center(0), y(19));
        app.key(Input::Char(']'), false, false);
        assert!(app.line().notes()[0].pin.is_some());
        app.key(Input::Char('['), false, false);
        assert!(app.line().notes()[0].pin.is_none());
    }

    #[test]
    fn saving_clears_the_modified_mark() {
        let dir = std::env::temp_dir().join(format!("intavolatura-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("line.intav");

        let mut app = app();
        assert!(!app.is_modified());
        assert!(app.title().starts_with("Untitled —"));
        app.click(app.columns().center(0), y(22));
        assert!(app.is_modified());
        assert!(app.title().starts_with("Untitled •"));

        assert!(app.save_to(path.clone()));
        assert!(!app.is_modified());
        assert!(app.title().starts_with("line.intav —"));

        let reopened = App::open(&path).unwrap();
        assert_eq!(reopened.line(), app.line());
        assert!(!reopened.is_modified());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn preferences_change_what_is_found() {
        let mut app = app();
        app.click(app.columns().center(0), y(19)); // A2
        let mut preferences = app.preferences();
        preferences.candidates_to_cycle = 2;
        app.set_preferences(&preferences);
        assert_eq!(app.realizations[0].2.candidates.len(), 2);
        assert_eq!(app.preferences(), preferences);
    }

    #[test]
    fn changing_key_recomputes_with_retuned_diapasons() {
        let mut app = App::empty("C major".parse().unwrap());
        app.click(app.columns().center(0), y(17)); // F2
        app.set_key("D major".parse().unwrap());
        // The click spelled F2; the key change keeps the written note.
        assert_eq!(app.line().notes()[0].pitch.to_string(), "F2");
        assert!(app.describe_selection().starts_with("F2"));
    }
}
