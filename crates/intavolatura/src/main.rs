//! Intavolatura: enter a figured bass line and see it realized for theorbo.

use std::path::PathBuf;

use intavolatura::app::App;
use intavolatura::preferences::Preferences;
use intavolatura::{AppWindow, ui};
use intavolatura_core::Key;
use slint::ComponentHandle;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let started = std::time::Instant::now();
    let mut app = match std::env::args_os().nth(1).map(PathBuf::from) {
        Some(path) if path.exists() => {
            App::open(&path).map_err(|e| format!("{}: {e}", path.display()))?
        }
        Some(path) => {
            let mut app = App::empty(default_key());
            app.path = Some(path);
            app
        }
        None => App::empty(default_key()),
    };
    app.set_preferences(&Preferences::load());

    let window = AppWindow::new()?;
    ui::wire(&window, app);

    // For measuring startup: report when the first frame is drawn (or, with
    // renderers that cannot report frames, when the event loop first runs)
    // and quit.
    if std::env::var_os("INTAVOLATURA_STARTUP_CHECK").is_some() {
        let finish = move || {
            eprintln!("first frame after {:.1?}", started.elapsed());
            let _ = slint::quit_event_loop();
        };
        let mut done = false;
        let notified = window.window().set_rendering_notifier(move |state, _| {
            if !done && matches!(state, slint::RenderingState::AfterRendering) {
                done = true;
                finish();
            }
        });
        if notified.is_err() {
            slint::Timer::single_shot(std::time::Duration::ZERO, finish);
        }
    }

    window.run()?;
    Ok(())
}

fn default_key() -> Key {
    "D minor".parse().expect("valid key")
}
