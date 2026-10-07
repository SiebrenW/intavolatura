//! Connects the app state to the Slint window.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use intavolatura_core::{Figuring, Key};
use rfd::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
use slint::platform::Key as SlintKey;
use slint::{CloseRequestResponse, ComponentHandle, ModelRc, SharedString, VecModel, Weak};

use crate::app::{App, Input};
use crate::preferences::Preferences;
use crate::view::{self, TablatureStyle};
use crate::{AppWindow, Bar, Glyph};

const FILE_TYPE: (&str, &[&str]) = ("Intavolatura bass line", &["intav"]);

/// The window, the app state and the key list, shared by every callback.
#[derive(Clone)]
struct Shared {
    window: Weak<AppWindow>,
    app: Rc<RefCell<App>>,
    keys: Rc<[Key]>,
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
    let key_names: Vec<SharedString> = keys.iter().map(|k| k.to_string().into()).collect();
    window.set_keys(ModelRc::new(VecModel::from(key_names)));
    window.set_column_width(view::COLUMN_WIDTH);

    let shared = Shared {
        window: window.as_weak(),
        app: Rc::new(RefCell::new(app)),
        keys,
    };
    refresh(window, &shared.app.borrow(), &shared.keys);

    let s = shared.clone();
    window.on_key(move |text, shift, command| {
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
            let preferences = Preferences {
                show_hints: window.get_show_hints(),
                retune_diapasons: window.get_retune_diapasons(),
                fret_span: window.get_fret_span().clamp(2, 6) as u8,
                candidates_to_cycle: window.get_candidates_to_cycle().clamp(1, 50) as usize,
                ..app.preferences()
            };
            apply_preferences(app, &preferences);
        })
    });

    let s = shared.clone();
    window.on_open_file(move || {
        open_file(&s);
    });

    let s = shared.clone();
    window.on_save_file(move || {
        save_file(&s, false);
    });

    let weak = window.as_weak();
    window.on_figuring_edited(move |text| {
        weak.unwrap()
            .set_figuring_error(text.parse::<Figuring>().is_err());
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
    let mut dialog = FileDialog::new().add_filter(FILE_TYPE.0, FILE_TYPE.1);
    if let Some(dir) = start {
        dialog = dialog.set_directory(dir);
    }
    let Some(path) = dialog.pick_file() else {
        return true;
    };
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
        })
        .collect();
    let bars: Vec<Bar> = scene
        .bars
        .into_iter()
        .map(|b| Bar {
            x: b.x,
            y: b.y,
            width: b.width,
            height: b.height,
            knockout: b.knockout,
        })
        .collect();
    window.set_glyphs(ModelRc::new(VecModel::from(glyphs)));
    window.set_bars(ModelRc::new(VecModel::from(bars)));
    window.set_content_width(scene.width);
    window.set_content_height(scene.height);
    window.set_window_title(app.title().into());

    let preferences = app.preferences();
    window.set_tablature_index(i32::from(preferences.tablature == TablatureStyle::French));
    window.set_show_hints(preferences.show_hints);
    window.set_retune_diapasons(preferences.retune_diapasons);
    window.set_fret_span(preferences.fret_span.into());
    window.set_candidates_to_cycle(preferences.candidates_to_cycle as i32);

    let key = app.line().key();
    window.set_key_index(keys.iter().position(|&k| k == key).unwrap_or(0) as i32);

    let columns = app.columns();
    window.set_has_selection(app.selected.is_some());
    window.set_editing(app.editing);
    if let Some(index) = app.selected {
        let center = columns.center(index);
        window.set_selection_x(center - view::COLUMN_WIDTH / 2.0);
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
