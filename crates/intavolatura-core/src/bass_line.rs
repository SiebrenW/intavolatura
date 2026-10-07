//! The bass line being edited, its editing commands and undo/redo.

use crate::figuring::Figuring;
use crate::fingering::Fingering;
use crate::key::Key;
use crate::pitch::{Accidental, Pitch};

/// One bass note with its figuring and, optionally, a pinned fingering.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BassNote {
    /// The bass as written, including its octave.
    pub pitch: Pitch,
    pub figuring: Figuring,
    pub pin: Option<Fingering>,
}

impl BassNote {
    /// A bass note with an empty figuring and no pin.
    pub fn new(pitch: Pitch) -> BassNote {
        BassNote {
            pitch,
            figuring: Figuring::empty(),
            pin: None,
        }
    }
}

/// An ordered sequence of bass notes in a single key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BassLine {
    key: Key,
    notes: Vec<BassNote>,
}

impl BassLine {
    pub fn new(key: Key) -> BassLine {
        BassLine {
            key,
            notes: Vec::new(),
        }
    }

    pub fn key(&self) -> Key {
        self.key
    }

    pub fn notes(&self) -> &[BassNote] {
        &self.notes
    }
}

/// A reversible change to a bass line; applying one returns its inverse.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Edit {
    Insert(usize, BassNote),
    Remove(usize),
    Replace(usize, BassNote),
    /// A new key with the pin of every note, in order.
    SetKey(Key, Vec<Option<Fingering>>),
}

impl Edit {
    fn apply(self, line: &mut BassLine) -> Edit {
        match self {
            Edit::Insert(index, note) => {
                line.notes.insert(index, note);
                Edit::Remove(index)
            }
            Edit::Remove(index) => Edit::Insert(index, line.notes.remove(index)),
            Edit::Replace(index, note) => {
                Edit::Replace(index, std::mem::replace(&mut line.notes[index], note))
            }
            Edit::SetKey(key, pins) => {
                let old_pins = line
                    .notes
                    .iter_mut()
                    .zip(pins)
                    .map(|(note, pin)| std::mem::replace(&mut note.pin, pin))
                    .collect();
                Edit::SetKey(std::mem::replace(&mut line.key, key), old_pins)
            }
        }
    }
}

/// A bass line with its editing commands and undo history.
///
/// Every command returns whether it changed anything; commands that change
/// nothing leave the history alone. Indices out of range panic.
#[derive(Clone, Debug)]
pub struct Editor {
    line: BassLine,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
}

impl Editor {
    pub fn new(line: BassLine) -> Editor {
        Editor {
            line,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub fn line(&self) -> &BassLine {
        &self.line
    }

    /// Inserts a new bass note at `index`; `index == len` appends.
    pub fn insert(&mut self, index: usize, pitch: Pitch) -> bool {
        self.perform(Edit::Insert(index, BassNote::new(pitch)))
    }

    pub fn remove(&mut self, index: usize) -> bool {
        self.perform(Edit::Remove(index))
    }

    /// Moves the bass note to `pitch`, dropping its pin.
    pub fn set_pitch(&mut self, index: usize, pitch: Pitch) -> bool {
        self.change_chord(index, |note| note.pitch = pitch)
    }

    /// Moves the bass note by staff steps, taking the key's accidentals.
    pub fn step_pitch(&mut self, index: usize, steps: i16) -> bool {
        let pitch = self.line.key.step(self.line.notes[index].pitch, steps);
        self.set_pitch(index, pitch)
    }

    /// Raises (`up`) or lowers the bass note's accidental by a semitone,
    /// within flat to sharp.
    pub fn step_accidental(&mut self, index: usize, up: bool) -> bool {
        let pitch = self.line.notes[index].pitch;
        let current = pitch.name.accidental;
        let next = if up {
            current.raised()
        } else {
            current.lowered()
        };
        match next {
            Some(a) if (Accidental::FLAT..=Accidental::SHARP).contains(&a) => {
                self.set_pitch(index, pitch.with_accidental(a))
            }
            _ => false,
        }
    }

    /// Sets the figuring, dropping the pin if it changed.
    pub fn set_figuring(&mut self, index: usize, figuring: Figuring) -> bool {
        self.change_chord(index, |note| note.figuring = figuring)
    }

    /// Pins a fingering to the bass note, or unpins it with `None`.
    pub fn set_pin(&mut self, index: usize, pin: Option<Fingering>) -> bool {
        let mut note = self.line.notes[index].clone();
        note.pin = pin;
        self.replace(index, note)
    }

    /// Changes the key, dropping every pin since every chord may change.
    pub fn set_key(&mut self, key: Key) -> bool {
        if key == self.line.key {
            return false;
        }
        self.perform(Edit::SetKey(key, vec![None; self.line.notes.len()]))
    }

    pub fn undo(&mut self) -> bool {
        Self::step_history(&mut self.line, &mut self.undo, &mut self.redo)
    }

    pub fn redo(&mut self) -> bool {
        Self::step_history(&mut self.line, &mut self.redo, &mut self.undo)
    }

    /// Changes something that defines the chord, dropping the pin.
    fn change_chord(&mut self, index: usize, change: impl FnOnce(&mut BassNote)) -> bool {
        let mut note = self.line.notes[index].clone();
        change(&mut note);
        if note == self.line.notes[index] {
            return false;
        }
        note.pin = None;
        self.replace(index, note)
    }

    fn replace(&mut self, index: usize, note: BassNote) -> bool {
        if note == self.line.notes[index] {
            return false;
        }
        self.perform(Edit::Replace(index, note))
    }

    fn perform(&mut self, edit: Edit) -> bool {
        self.undo.push(edit.apply(&mut self.line));
        self.redo.clear();
        true
    }

    fn step_history(line: &mut BassLine, from: &mut Vec<Edit>, to: &mut Vec<Edit>) -> bool {
        match from.pop() {
            Some(edit) => {
                to.push(edit.apply(line));
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instrument::MAX_COURSES;
    use crate::key::Mode;

    fn key(tonic: &str, mode: Mode) -> Key {
        Key::new(tonic.parse().unwrap(), mode).unwrap()
    }

    fn editor_with(pitches: &[&str]) -> Editor {
        let mut editor = Editor::new(BassLine::new(key("D", Mode::Major)));
        for (i, p) in pitches.iter().enumerate() {
            editor.insert(i, p.parse().unwrap());
        }
        editor
    }

    fn pitches(editor: &Editor) -> Vec<String> {
        editor
            .line()
            .notes()
            .iter()
            .map(|n| n.pitch.to_string())
            .collect()
    }

    fn some_pin() -> Option<Fingering> {
        let mut frets = [None; MAX_COURSES];
        frets[0] = Some(0);
        Some(Fingering::from_frets(frets))
    }

    #[test]
    fn insert_appends_and_inserts_between() {
        let mut editor = editor_with(&["D3", "A2"]);
        editor.insert(1, "F#2".parse().unwrap());
        editor.insert(3, "D2".parse().unwrap());
        assert_eq!(pitches(&editor), ["D3", "F#2", "A2", "D2"]);
    }

    #[test]
    fn stepping_follows_the_key_and_accidentals_stay_within_one() {
        let mut editor = editor_with(&["D3"]);
        editor.step_pitch(0, 2);
        assert_eq!(pitches(&editor), ["F#3"]);
        assert!(editor.step_accidental(0, false));
        assert!(editor.step_accidental(0, false));
        assert_eq!(pitches(&editor), ["Fb3"]);
        assert!(!editor.step_accidental(0, false));
        assert_eq!(pitches(&editor), ["Fb3"]);
    }

    #[test]
    fn changing_the_chord_drops_the_pin_and_undo_restores_it() {
        let mut editor = editor_with(&["D3"]);
        editor.set_pin(0, some_pin());
        editor.set_figuring(0, "6".parse().unwrap());
        assert_eq!(editor.line().notes()[0].pin, None);
        editor.undo();
        assert_eq!(editor.line().notes()[0].pin, some_pin());
        assert!(editor.line().notes()[0].figuring.is_empty());

        editor.step_pitch(0, 1);
        assert_eq!(editor.line().notes()[0].pin, None);
        editor.undo();
        assert_eq!(editor.line().notes()[0].pin, some_pin());
    }

    #[test]
    fn changing_the_key_drops_all_pins_and_undo_restores_them() {
        let mut editor = editor_with(&["D3", "A2"]);
        editor.set_pin(1, some_pin());
        assert!(editor.set_key(key("D", Mode::Minor)));
        assert!(editor.line().notes().iter().all(|n| n.pin.is_none()));
        editor.undo();
        assert_eq!(editor.line().key(), key("D", Mode::Major));
        assert_eq!(editor.line().notes()[1].pin, some_pin());
    }

    #[test]
    fn unchanged_edits_are_not_recorded() {
        let mut editor = editor_with(&["D3"]);
        assert!(!editor.set_pitch(0, "D3".parse().unwrap()));
        assert!(!editor.set_figuring(0, Figuring::empty()));
        assert!(!editor.set_key(key("D", Mode::Major)));
        assert!(!editor.set_pin(0, None));
        // Only the insert is in the history.
        assert!(editor.undo());
        assert!(!editor.undo());
    }

    #[test]
    fn undo_and_redo_walk_the_history() {
        let mut editor = editor_with(&["D3", "A2", "B2"]);
        editor.remove(1);
        editor.set_figuring(1, "6".parse().unwrap());
        assert_eq!(pitches(&editor), ["D3", "B2"]);

        editor.undo();
        editor.undo();
        assert_eq!(pitches(&editor), ["D3", "A2", "B2"]);
        editor.redo();
        assert_eq!(pitches(&editor), ["D3", "B2"]);
        editor.redo();
        assert_eq!(editor.line().notes()[1].figuring.to_string(), "6");
        assert!(!editor.redo());
    }

    #[test]
    fn a_new_edit_clears_redo() {
        let mut editor = editor_with(&["D3"]);
        editor.undo();
        editor.insert(0, "E3".parse().unwrap());
        assert!(!editor.redo());
        assert_eq!(pitches(&editor), ["E3"]);
    }
}
