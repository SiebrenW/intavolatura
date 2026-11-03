//! Connects the app state to the Slint window.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use intavolatura_core::{Duration, Instrument, Key, Mode, NoteValue, Source, TimeSignature};
use rfd::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use slint::platform::Key as SlintKey;
use slint::{CloseRequestResponse, ComponentHandle, ModelRc, SharedString, VecModel, Weak};

use intavolatura_core::midi;
use intavolatura_core::musicxml::{self, PartInfo};

use crate::app::{App, Input};
use crate::audio::Player;
use crate::preferences::{Preferences, Theme};
use crate::print::{Contents, PageSize};
use crate::view::{self, TablatureStyle};
use crate::{AppWindow, Bar, Glyph, Tie};

const FILE_TYPE: (&str, &[&str]) = ("Intavolatura bass line", &["intav"]);

/// The window, the app state and the key list, shared by every callback.
#[derive(Clone)]
struct Shared {
    window: Weak<AppWindow>,
    app: Rc<RefCell<App>>,
    keys: Rc<[Key]>,
    import: Rc<RefCell<Option<PendingImport>>>,
    play: Rc<Play>,
}

/// Playback: the audio output, the tick it started from while it plays,
/// and the timer that moves the playhead.
struct Play {
    player: RefCell<Player>,
    from: Cell<Option<u32>>,
    timer: slint::Timer,
}

/// A score waiting for its part to be chosen: text, file, parts.
type PendingImport = (Score, PathBuf, Vec<PartInfo>);

/// The contents of a score file to import from.
enum Score {
    Xml(String),
    Midi(Vec<u8>),
}

impl Shared {
    /// Runs `f` on the app, then redraws the window.
    fn update<R>(&self, f: impl FnOnce(&AppWindow, &mut App) -> R) -> R {
        let window = self.window.unwrap();
        let mut app = self.app.borrow_mut();
        let result = f(&window, &mut app);
        refresh(&window, &app, &self.keys);
        result
    }
}

/// Fills the window from `app` and connects every callback to it.
pub fn wire(window: &AppWindow, app: App) {
    let keys: Rc<[Key]> = Key::all().collect();
    let key_names: Vec<SharedString> = keys.iter().map(|&k| key_name(k).into()).collect();
    window.set_keys(ModelRc::new(VecModel::from(key_names)));
    window.set_column_width(view::COLUMN_WIDTH);
    let names: Vec<SharedString> = Instrument::all().iter().map(|i| i.name().into()).collect();
    window.set_instruments(ModelRc::new(VecModel::from(names)));
    let times: Vec<SharedString> = TimeSignature::ALL
        .iter()
        .map(|t| match (t.beats, t.beat_value) {
            (4, 4) => "4/4 (C)".into(),
            (2, 2) => "2/2 (¢)".into(),
            _ => t.to_string().into(),
        })
        .collect();
    window.set_time_signatures(ModelRc::new(VecModel::from(times)));
    window.set_command_key(
        if cfg!(target_os = "macos") {
            "⌘"
        } else {
            "Ctrl"
        }
        .into(),
    );

    let shared = Shared {
        window: window.as_weak(),
        app: Rc::new(RefCell::new(app)),
        keys,
        import: Rc::new(RefCell::new(None)),
        play: Rc::new(Play {
            player: RefCell::new(Player::new()),
            from: Cell::new(None),
            timer: slint::Timer::default(),
        }),
    };
    refresh(window, &shared.app.borrow(), &shared.keys);

    let s = shared.clone();
    window.on_key(move |text, shift, command| {
        let editing = s.app.borrow().editing;
        if !editing && (text == "?" || text == SharedString::from(SlintKey::F1)) {
            s.window.unwrap().invoke_show_shortcuts();
            return true;
        }
        if !editing && !command && text == "t" && s.app.borrow().selected.is_some() {
            s.window.unwrap().invoke_choose_meter_change();
            return true;
        }
        if !editing && !command && text == "k" && s.app.borrow().selected.is_some() {
            s.window.unwrap().invoke_choose_key_change();
            return true;
        }
        if !editing && !command && text == " " {
            toggle_play(&s);
            return true;
        }
        if text == SharedString::from(SlintKey::Escape) && s.play.from.get().is_some() {
            stop_play(&s);
            return true;
        }
        let Some(input) = input(&text) else {
            return false;
        };
        if command && !s.app.borrow().editing {
            match input {
                Input::Char('o') => return open_file(&s),
                Input::Char('s') => return save_file(&s, shift),
                Input::Char('S') => return save_file(&s, true),
                _ => {}
            }
        }
        s.update(|window, app| {
            let was_editing = app.editing;
            let handled = app.key(input, shift, command);
            if app.editing && !was_editing {
                start_editing(window, app);
            } else if was_editing && !app.editing {
                window.invoke_focus_main();
            }
            handled
        })
    });

    let s = shared.clone();
    window.on_clicked(move |x, y| {
        s.update(|window, app| {
            if app.click(x, y) {
                start_editing(window, app);
            } else {
                window.invoke_focus_main();
            }
        })
    });

    let s = shared.clone();
    window.on_key_selected(move |index| {
        let key = s.keys.get(index as usize).copied();
        s.update(|_, app| {
            if let Some(key) = key {
                app.set_key(key);
            }
        })
    });

    let s = shared.clone();
    window.on_key_change_selected(move |index| {
        let key = usize::try_from(index)
            .ok()
            .and_then(|i| s.keys.get(i).copied());
        s.update(|_, app| app.set_key_change(key))
    });

    let s = shared.clone();
    window.on_time_selected(move |index| {
        s.update(|_, app| {
            if let Some(&time) = TimeSignature::ALL.get(index as usize) {
                app.set_time(time);
            }
        })
    });

    let s = shared.clone();
    window.on_meter_change_selected(move |index| {
        s.update(|_, app| {
            let time = usize::try_from(index - 1)
                .ok()
                .and_then(|i| TimeSignature::ALL.get(i).copied());
            app.set_time_change(time);
        })
    });

    let s = shared.clone();
    window.on_upbeat_selected(move |index| {
        s.update(|_, app| {
            let choices = upbeat_choices(app.line().time());
            let upbeat = usize::try_from(index - 1)
                .ok()
                .and_then(|i| choices.get(i).copied());
            app.set_upbeat(upbeat);
        })
    });

    let s = shared.clone();
    window.on_duration_selected(move |index| {
        s.update(|_, app| {
            if let Some(&value) = NoteValue::ALL.get(index as usize) {
                app.set_duration(Duration {
                    value,
                    ..app.current_duration()
                });
            }
        })
    });

    let s = shared.clone();
    window.on_dot_toggled(move || {
        s.update(|_, app| {
            let current = app.current_duration();
            app.set_duration(Duration {
                dotted: !current.dotted,
                triplet: false,
                ..current
            });
        })
    });

    let s = shared.clone();
    window.on_triplet_toggled(move || {
        s.update(|_, app| {
            let current = app.current_duration();
            app.set_duration(Duration {
                dotted: false,
                triplet: !current.triplet,
                ..current
            });
        })
    });

    let s = shared.clone();
    window.on_tablature_selected(move |index| {
        s.update(|_, app| {
            let mut preferences = app.preferences();
            preferences.tablature = if index == 1 {
                TablatureStyle::French
            } else {
                TablatureStyle::Italian
            };
            apply_preferences(app, &preferences);
        })
    });

    let s = shared.clone();
    window.on_preferences_changed(move || {
        s.update(|window, app| {
            let theme = usize::try_from(window.get_theme()).ok();
            let preferences = Preferences {
                theme: theme
                    .and_then(|i| Theme::ALL.get(i).copied())
                    .unwrap_or_default(),
                show_hints: window.get_show_hints(),
                retune_diapasons: window.get_retune_diapasons(),
                fret_span: window.get_fret_span().clamp(2, 6) as u8,
                candidates_to_cycle: window.get_candidates_to_cycle().clamp(1, 50) as usize,
                sign_size: window.get_sign_size().clamp(50, 150) as u8,
                italian_on_line: window.get_italian_on_line(),
                french_on_line: window.get_french_on_line(),
                print_contents: usize::try_from(window.get_print_contents())
                    .ok()
                    .and_then(|i| Contents::ALL.get(i).copied())
                    .unwrap_or_default(),
                page_size: usize::try_from(window.get_page_size())
                    .ok()
                    .and_then(|i| PageSize::ALL.get(i).copied())
                    .unwrap_or_default(),
                staff_mm: window.get_staff_mm().clamp(5, 12) as u8,
                play_source: if window.get_play_source() == 1 {
                    Source::Bass
                } else {
                    Source::Realization
                },
                a4: window.get_a4().clamp(380, 480) as u16,
                spread_ms: window.get_spread_ms().clamp(0, 100) as u8,
                volume: window.get_volume().clamp(0.0, 100.0).round() as u8,
                ..app.preferences()
            };
            apply_preferences(app, &preferences);
        })
    });

    let s = shared.clone();
    window.on_instrument_selected(move |index| {
        if let Some(instrument) = Instrument::all().into_iter().nth(index as usize) {
            s.update(|_, app| app.set_instrument(instrument));
        }
    });

    let s = shared.clone();
    window.on_play_toggled(move || toggle_play(&s));

    let s = shared.clone();
    window.on_tempo_edited(move |tempo| {
        s.update(|_, app| app.set_tempo(tempo.clamp(20, 300) as u16));
    });

    let s = shared.clone();
    window.on_open_file(move || {
        open_file(&s);
    });

    let s = shared.clone();
    window.on_export_file(move |with_realization| {
        export_file(&s, with_realization);
    });

    let s = shared.clone();
    window.on_export_pdf(move || export_pdf(&s));

    let s = shared.clone();
    window.on_import_part(move |index| import_part(&s, index as usize));

    let s = shared.clone();
    window.on_save_file(move || {
        save_file(&s, false);
    });

    let s = shared.clone();
    window.on_figuring_edited(move |text| {
        let fits = s.app.borrow().figuring_text_fits(&text);
        s.window.unwrap().set_figuring_error(!fits);
    });

    let s = shared.clone();
    window.on_figuring_accepted(move |text| {
        s.update(|window, app| {
            if app.accept_figuring(&text) {
                window.invoke_focus_main();
            } else {
                window.set_figuring_error(true);
            }
        })
    });

    let s = shared;
    window.window().on_close_requested(move || {
        if confirm_discard(&s.app.borrow()) {
            CloseRequestResponse::HideWindow
        } else {
            CloseRequestResponse::KeepWindowShown
        }
    });
}

/// A key with its signature, as shown on the key picker: `B♭ major · 2♭`.
fn key_label(key: Key) -> String {
    let signature = match key.signature() {
        0 => "no ♯/♭".to_string(),
        n if n > 0 => format!("{n}♯"),
        n => format!("{}♭", -n),
    };
    format!("{} · {signature}", key_name(key))
}

/// A key with proper accidentals: `B♭ major`.
fn key_name(key: Key) -> String {
    let tonic = key.tonic();
    let accidental = match tonic.accidental.semitones() {
        -1 => "♭",
        1 => "♯",
        _ => "",
    };
    let mode = match key.mode() {
        Mode::Major => "major",
        Mode::Minor => "minor",
    };
    format!("{}{accidental} {mode}", tonic.letter.as_char())
}

/// Upbeat lengths that fit in a bar of `time`, shortest first.
fn upbeat_choices(time: TimeSignature) -> Vec<Duration> {
    NoteValue::ALL
        .iter()
        .rev()
        .flat_map(|&v| [Duration::new(v), Duration::dotted(v)])
        .filter(|d| d.ticks() < time.bar_ticks() && d.ticks() >= 12)
        .collect()
}

/// An upbeat as a fraction of a whole note: 1/4, 3/8.
fn upbeat_label(duration: Duration) -> String {
    let (mut numerator, mut denominator) = (duration.ticks(), intavolatura_core::duration::WHOLE);
    while numerator % 2 == 0 && denominator % 2 == 0 || numerator % 3 == 0 && denominator % 3 == 0 {
        let factor = if numerator % 2 == 0 && denominator % 2 == 0 {
            2
        } else {
            3
        };
        numerator /= factor;
        denominator /= factor;
    }
    format!("Upbeat {numerator}/{denominator}")
}

/// Applies and stores new preferences; a failure to store is shown, not fatal.
fn apply_preferences(app: &mut App, preferences: &Preferences) {
    app.set_preferences(preferences);
    if let Err(e) = preferences.save() {
        app.status = format!("Could not save settings: {e}");
    }
}

/// Asks whether unsaved changes may be thrown away; true if there are none.
fn confirm_discard(app: &App) -> bool {
    !app.is_modified()
        || MessageDialog::new()
            .set_level(MessageLevel::Warning)
            .set_title("Unsaved changes")
            .set_description("The bass line has unsaved changes. Discard them?")
            .set_buttons(MessageButtons::OkCancelCustom(
                "Discard".into(),
                "Cancel".into(),
            ))
            .show()
            == MessageDialogResult::Custom("Discard".into())
}

/// Lets the user pick a file and opens it, keeping the current preferences.
fn open_file(s: &Shared) -> bool {
    // No borrow is held while a dialog runs its own event loop.
    if !confirm_discard(&s.app.borrow()) {
        return true;
    }
    let start = s
        .app
        .borrow()
        .path
        .as_ref()
        .and_then(|p| p.parent().map(PathBuf::from));
    let mut dialog = FileDialog::new()
        .add_filter(
            "Bass lines, MusicXML and MIDI",
            &["intav", "musicxml", "xml", "mxl", "mid", "midi"],
        )
        .add_filter(FILE_TYPE.0, FILE_TYPE.1)
        .add_filter("MusicXML", &["musicxml", "xml", "mxl"])
        .add_filter("MIDI", &["mid", "midi"]);
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    let Some(path) = dialog.pick_file() else {
        return true;
    };
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    let is_midi = extension
        .as_deref()
        .is_some_and(|e| ["mid", "midi"].contains(&e));
    let is_score = is_midi
        || extension
            .as_deref()
            .is_some_and(|e| ["musicxml", "xml", "mxl"].contains(&e));
    if is_score {
        let read = if is_midi {
            std::fs::read(&path)
                .map_err(|e| e.to_string())
                .and_then(|bytes| Ok((midi::parts(&bytes)?, Score::Midi(bytes))))
        } else {
            musicxml::read_file(&path).and_then(|xml| Ok((musicxml::parts(&xml)?, Score::Xml(xml))))
        };
        match read {
            Ok((parts, _)) if parts.is_empty() => {
                s.update(|_, app| app.status = format!("{} has no parts", path.display()));
            }
            Ok((parts, score)) => {
                let window = s.window.unwrap();
                let names: Vec<SharedString> = parts
                    .iter()
                    .map(|p| {
                        let figured = if p.has_figures { " · figured" } else { "" };
                        format!("{}{figured}", p.name).into()
                    })
                    .collect();
                window.set_import_parts(ModelRc::new(VecModel::from(names)));
                let file = path
                    .file_name()
                    .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                *s.import.borrow_mut() = Some((score, path, parts));
                s.update(|_, app| app.status = format!("Choose a part of {file} to import"));
                // Shown after the file dialog's click has been dealt with,
                // or the window can close the popup again at once.
                let weak = s.window.clone();
                slint::Timer::single_shot(std::time::Duration::from_millis(150), move || {
                    if let Some(window) = weak.upgrade() {
                        window.invoke_choose_import_part();
                    }
                });
            }
            Err(e) => {
                s.update(|_, app| app.status = format!("Could not read {}: {e}", path.display()))
            }
        }
        return true;
    }
    s.update(|window, app| match App::open(&path) {
        Ok(mut opened) => {
            opened.set_preferences(&app.preferences());
            *app = opened;
            window.set_scroll_x(0.0);
        }
        Err(e) => app.status = format!("Could not open {}: {e}", path.display()),
    });
    true
}

/// Imports the chosen part of the waiting MusicXML score as a new, unsaved
/// bass line, and shows what had to change.
fn import_part(s: &Shared, index: usize) {
    let Some((score, path, parts)) = s.import.borrow_mut().take() else {
        return;
    };
    let Some(part) = parts.get(index) else {
        return;
    };
    s.update(|window, app| {
        match match &score {
            Score::Xml(xml) => musicxml::import(xml, &part.id),
            Score::Midi(bytes) => midi::import(bytes, &part.id),
        } {
            Ok(imported) => {
                let mut opened = App::new(imported.document, None);
                opened.set_preferences(&app.preferences());
                let file = path
                    .file_name()
                    .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                opened.status = match imported.changes.len() {
                    0 => format!("Imported {} from {file}", part.name),
                    n => format!("Imported {} from {file} with {n} changes", part.name),
                };
                *app = opened;
                window.set_scroll_x(0.0);
                if !imported.changes.is_empty() {
                    let changes: Vec<SharedString> = imported
                        .changes
                        .into_iter()
                        .map(SharedString::from)
                        .collect();
                    window.set_import_changes(ModelRc::new(VecModel::from(changes)));
                    window.invoke_show_import_report();
                }
            }
            Err(e) => app.status = format!("Could not import {}: {e}", part.name),
        }
    });
}

/// Saves to the current file, asking for one if there is none or `ask`.
fn save_file(s: &Shared, ask: bool) -> bool {
    let current = s.app.borrow().path.clone();
    let path = match current {
        Some(path) if !ask => Some(path),
        current => {
            let name = current
                .as_deref()
                .and_then(|p| p.file_name())
                .map_or("Untitled.intav".into(), |n| {
                    n.to_string_lossy().into_owned()
                });
            let mut dialog = FileDialog::new()
                .add_filter(FILE_TYPE.0, FILE_TYPE.1)
                .set_file_name(name);
            if let Some(dir) = current.as_deref().and_then(|p| p.parent()) {
                dialog = dialog.set_directory(dir);
            }
            dialog.save_file()
        }
    };
    if let Some(mut path) = path {
        if path.extension().is_none() {
            path.set_extension("intav");
        }
        s.update(|_, app| app.save_to(path));
    }
    true
}

/// Asks where to write MusicXML, then exports.
fn export_file(s: &Shared, with_realization: bool) {
    let current = s.app.borrow().path.clone();
    let name = current
        .as_deref()
        .and_then(|p| p.file_stem())
        .map_or("Untitled".into(), |n| n.to_string_lossy().into_owned());
    let mut dialog = FileDialog::new()
        .add_filter("MusicXML", &["musicxml"])
        .set_file_name(format!("{name}.musicxml"));
    if let Some(dir) = current.as_deref().and_then(|p| p.parent()) {
        dialog = dialog.set_directory(dir);
    }
    if let Some(mut path) = dialog.save_file() {
        if path.extension().is_none() {
            path.set_extension("musicxml");
        }
        s.update(|_, app| app.export_musicxml(&path, with_realization));
    }
}

/// Plays from the selected note, or stops if it is playing.
fn toggle_play(s: &Shared) {
    if s.play.from.get().is_some() {
        stop_play(s);
        return;
    }
    let window = s.window.unwrap();
    let playback = s.app.borrow().playback();
    if playback.strikes.is_empty() {
        s.update(|_, app| app.status = "Nothing to play".to_string());
        return;
    }
    let started = s
        .play
        .player
        .borrow_mut()
        .play(&playback.strikes, playback.end, playback.volume);
    if let Err(e) = started {
        s.update(|_, app| app.status = format!("Cannot play: {e}"));
        return;
    }
    s.play.from.set(Some(playback.from));
    window.set_playing(true);
    let tick = s.clone();
    s.play.timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(30),
        move || follow_playhead(&tick),
    );
}

/// Stops playing and clears the playhead.
fn stop_play(s: &Shared) {
    s.play.player.borrow_mut().stop();
    s.play.timer.stop();
    s.play.from.set(None);
    let window = s.window.unwrap();
    window.set_playing(false);
    window.set_has_playhead(false);
}

/// Moves the playhead to the sounding column, scrolling it into view.
fn follow_playhead(s: &Shared) {
    let Some(from) = s.play.from.get() else {
        return;
    };
    let position = s.play.player.borrow().position();
    let Some(position) = position else {
        stop_play(s);
        return;
    };
    let window = s.window.unwrap();
    let app = s.app.borrow();
    if let Some(index) = app.note_at(from, position) {
        let columns = app.columns();
        window.set_playhead_x(columns.left(index));
        window.set_playhead_width(columns.width_of(index));
        window.set_has_playhead(true);
        scroll_into_view(&window, columns.center(index));
    }
}

/// Asks where to write the PDF, then exports.
fn export_pdf(s: &Shared) {
    let current = s.app.borrow().path.clone();
    let name = current
        .as_deref()
        .and_then(|p| p.file_stem())
        .map_or("Untitled".into(), |n| n.to_string_lossy().into_owned());
    let mut dialog = FileDialog::new()
        .add_filter("PDF", &["pdf"])
        .set_file_name(format!("{name}.pdf"));
    if let Some(dir) = current.as_deref().and_then(|p| p.parent()) {
        dialog = dialog.set_directory(dir);
    }
    if let Some(mut path) = dialog.save_file() {
        if path.extension().is_none() {
            path.set_extension("pdf");
        }
        s.update(|_, app| app.export_pdf(&path));
    }
}

/// Translates Slint's key text into the app's inputs.
fn input(text: &str) -> Option<Input> {
    let is = |key: SlintKey| text == SharedString::from(key).as_str();
    Some(if is(SlintKey::UpArrow) {
        Input::Up
    } else if is(SlintKey::DownArrow) {
        Input::Down
    } else if is(SlintKey::LeftArrow) {
        Input::Left
    } else if is(SlintKey::RightArrow) {
        Input::Right
    } else if is(SlintKey::Delete) || is(SlintKey::Backspace) {
        Input::Delete
    } else if is(SlintKey::Return) {
        Input::Enter
    } else if is(SlintKey::Escape) {
        Input::Escape
    } else {
        let mut chars = text.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => Input::Char(c),
            _ => return None,
        }
    })
}

fn start_editing(window: &AppWindow, app: &App) {
    window.set_figuring_text(app.selected_figuring_text().into());
    window.set_figuring_error(false);
    window.invoke_focus_figuring();
}

/// Pushes the app state into the window.
pub fn refresh(window: &AppWindow, app: &App, keys: &[Key]) {
    let scene = app.scene();
    let glyphs: Vec<Glyph> = scene
        .glyphs
        .into_iter()
        .map(|g| Glyph {
            x: g.x,
            y: g.y,
            text: g.text.into(),
            music: g.music,
            size: g.size,
            faded: g.faded,
            warning: g.warning,
        })
        .collect();
    let bars: Vec<Bar> = scene
        .bars
        .into_iter()
        .flat_map(|b| {
            // Renderers cannot draw rectangles thousands of pixels wide, as
            // the staff lines of a long line are.
            const PIECE: f32 = 2048.0;
            let pieces = (b.width / PIECE).ceil().max(1.0) as usize;
            (0..pieces).map(move |i| {
                let from = i as f32 * PIECE;
                Bar {
                    x: b.x + from,
                    y: b.y,
                    width: (b.width - from).min(PIECE),
                    height: b.height,
                    knockout: b.knockout,
                }
            })
        })
        .collect();
    let ties: Vec<Tie> = scene
        .ties
        .into_iter()
        .map(|t| Tie {
            x: t.x,
            y: t.y,
            width: t.width,
            height: t.height,
            commands: t.commands.into(),
        })
        .collect();
    window.set_ties(ModelRc::new(VecModel::from(ties)));
    window.set_glyphs(ModelRc::new(VecModel::from(glyphs)));
    window.set_bars(ModelRc::new(VecModel::from(bars)));
    window.set_content_width(scene.width);
    window.set_content_height(scene.height);
    window.set_window_title(app.title().into());

    let preferences = app.preferences();
    let theme = Theme::ALL
        .iter()
        .position(|&t| t == preferences.theme)
        .unwrap_or(0);
    window.set_theme(theme as i32);
    window.set_tablature_index(i32::from(preferences.tablature == TablatureStyle::French));
    window.set_show_hints(preferences.show_hints);
    window.set_retune_diapasons(preferences.retune_diapasons);
    window.set_fret_span(preferences.fret_span.into());
    window.set_candidates_to_cycle(preferences.candidates_to_cycle as i32);
    window.set_sign_size(preferences.sign_size.into());
    window.set_italian_on_line(preferences.italian_on_line);
    window.set_french_on_line(preferences.french_on_line);
    window.set_print_contents(
        Contents::ALL
            .iter()
            .position(|&c| c == preferences.print_contents)
            .unwrap_or(0) as i32,
    );
    window.set_page_size(
        PageSize::ALL
            .iter()
            .position(|&p| p == preferences.page_size)
            .unwrap_or(0) as i32,
    );
    window.set_staff_mm(preferences.staff_mm.into());
    window.set_play_source(i32::from(preferences.play_source == Source::Bass));
    window.set_a4(preferences.a4.into());
    window.set_spread_ms(preferences.spread_ms.into());
    window.set_volume(f32::from(preferences.volume));

    let time = app.line().time();
    window.set_time_index(
        TimeSignature::ALL
            .iter()
            .position(|&t| t == time)
            .unwrap_or(0) as i32,
    );
    let choices = upbeat_choices(time);
    let mut labels: Vec<SharedString> = vec!["No upbeat".into()];
    labels.extend(choices.iter().map(|&d| SharedString::from(upbeat_label(d))));
    window.set_upbeats(ModelRc::new(VecModel::from(labels)));
    let upbeat = app.line().upbeat();
    let upbeat_index = upbeat
        .and_then(|u| choices.iter().position(|&d| d == u))
        .map_or(0, |i| i + 1);
    window.set_upbeat_index(upbeat_index as i32);

    let duration = app.current_duration();
    let value = NoteValue::ALL
        .iter()
        .position(|&v| v == duration.value)
        .unwrap_or(2);
    window.set_duration_value(value as i32);
    window.set_duration_dotted(duration.dotted);
    window.set_duration_triplet(duration.triplet);

    let key = app.line().key();
    window.set_key_change_index(
        keys.iter()
            .position(|&k| k == app.current_key())
            .unwrap_or(0) as i32,
    );
    window.set_key_index(keys.iter().position(|&k| k == key).unwrap_or(0) as i32);
    window.set_key_label(key_label(key).into());

    let columns = app.columns();
    window.set_has_selection(app.selected.is_some());
    window.set_editing(app.editing);
    if let Some(index) = app.selected {
        let center = columns.center(index);
        window.set_selection_x(columns.left(index));
        window.set_selection_width(columns.width_of(index));
        window.set_edit_x(center);
        window.set_edit_y(view::FIGURES_TOP - view::SPACE);
        scroll_into_view(window, center);
    }

    let status = if app.status.is_empty() {
        app.describe_selection()
    } else {
        app.status.clone()
    };
    window.set_status(status.into());
    window.set_tempo(app.tempo.into());
    let instrument = app.instrument().id();
    window.set_instrument_index(
        Instrument::all()
            .iter()
            .position(|i| i.id() == instrument)
            .unwrap_or(0) as i32,
    );
}

/// Scrolls horizontally so the column centred at `center` is visible.
fn scroll_into_view(window: &AppWindow, center: f32) {
    let margin = view::COLUMN_WIDTH;
    let visible_left = -window.get_scroll_x();
    let width = window.get_view_width();
    if center - margin < visible_left {
        window.set_scroll_x(-(center - margin).max(0.0));
    } else if center + margin > visible_left + width {
        window.set_scroll_x(-(center + margin - width));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upbeats_offered_fit_in_a_bar() {
        let labels = |t: &str| {
            upbeat_choices(t.parse().unwrap())
                .into_iter()
                .map(upbeat_label)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            labels("3/4"),
            [
                "Upbeat 1/8",
                "Upbeat 3/16",
                "Upbeat 1/4",
                "Upbeat 3/8",
                "Upbeat 1/2"
            ]
        );
        assert!(labels("4/4").contains(&"Upbeat 3/4".to_string()));
        assert!(!labels("4/4").contains(&"Upbeat 1/1".to_string()));
    }

    #[test]
    fn key_labels_show_the_signature() {
        let label = |s: &str| key_label(s.parse().unwrap());
        assert_eq!(label("C major"), "C major · no ♯/♭");
        assert_eq!(label("E major"), "E major · 4♯");
        assert_eq!(label("D minor"), "D minor · 1♭");
        assert_eq!(label("Bb major"), "B♭ major · 2♭");
        assert_eq!(label("C# minor"), "C♯ minor · 4♯");
        assert_eq!(label("Cb major"), "C♭ major · 7♭");
    }
}
