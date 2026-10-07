//! Connects the app state to the Slint window.

use std::cell::RefCell;
use std::rc::Rc;

use intavolatura_core::Key;
use slint::platform::Key as SlintKey;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::app::{App, Input};
use crate::view::{self, TablatureStyle};
use crate::{AppWindow, Bar, Glyph};

/// Fills the window from `app` and connects every callback to it.
pub fn wire(window: &AppWindow, app: App) {
    let keys: Vec<Key> = Key::all().collect();
    let key_names: Vec<SharedString> = keys.iter().map(|k| k.to_string().into()).collect();
    window.set_keys(ModelRc::new(VecModel::from(key_names)));
    window.set_column_width(view::COLUMN_WIDTH);

    let app = Rc::new(RefCell::new(app));
    refresh(window, &app.borrow(), &keys);

    let weak = window.as_weak();
    let state = app.clone();
    let all_keys = keys.clone();
    window.on_key(move |text, shift, command| {
        let window = weak.unwrap();
        let Some(input) = input(&text) else {
            return false;
        };
        let mut app = state.borrow_mut();
        let was_editing = app.editing;
        let handled = app.key(input, shift, command);
        if app.editing && !was_editing {
            start_editing(&window, &app);
        } else if was_editing && !app.editing {
            window.invoke_focus_main();
        }
        refresh(&window, &app, &all_keys);
        handled
    });

    let weak = window.as_weak();
    let state = app.clone();
    let all_keys = keys.clone();
    window.on_clicked(move |x, y| {
        let window = weak.unwrap();
        let mut app = state.borrow_mut();
        if app.click(x, y) {
            start_editing(&window, &app);
        } else {
            window.invoke_focus_main();
        }
        refresh(&window, &app, &all_keys);
    });

    let weak = window.as_weak();
    let state = app.clone();
    let all_keys = keys.clone();
    window.on_key_selected(move |index| {
        let window = weak.unwrap();
        let mut app = state.borrow_mut();
        if let Some(&key) = all_keys.get(index as usize) {
            app.set_key(key);
        }
        refresh(&window, &app, &all_keys);
    });

    let weak = window.as_weak();
    let state = app.clone();
    let all_keys = keys.clone();
    window.on_tablature_selected(move |index| {
        let window = weak.unwrap();
        let mut app = state.borrow_mut();
        app.tablature = if index == 1 {
            TablatureStyle::French
        } else {
            TablatureStyle::Italian
        };
        refresh(&window, &app, &all_keys);
    });

    let weak = window.as_weak();
    window.on_figuring_edited(move |text| {
        let window = weak.unwrap();
        window.set_figuring_error(text.parse::<intavolatura_core::Figuring>().is_err());
    });

    let weak = window.as_weak();
    let state = app.clone();
    let all_keys = keys;
    window.on_figuring_accepted(move |text| {
        let window = weak.unwrap();
        let mut app = state.borrow_mut();
        if app.accept_figuring(&text) {
            window.invoke_focus_main();
        } else {
            window.set_figuring_error(true);
        }
        refresh(&window, &app, &all_keys);
    });
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

    let key = app.line().key();
    window.set_tablature_index(i32::from(app.tablature == TablatureStyle::French));
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
