//! Intavolatura: enter a figured bass line and see it realized for theorbo.

use std::path::PathBuf;

use intavolatura::app::App;
use intavolatura::{AppWindow, ui};
use intavolatura_core::Key;
use intavolatura_core::file_format;
use slint::ComponentHandle;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = match std::env::args_os().nth(1).map(PathBuf::from) {
        Some(path) if path.exists() => {
            let text = std::fs::read_to_string(&path)?;
            let document =
                file_format::read(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            App::new(document, Some(path))
        }
        Some(path) => {
            let mut app = App::empty(default_key());
            app.path = Some(path);
            app
        }
        None => App::empty(default_key()),
    };

    let window = AppWindow::new()?;
    ui::wire(&window, app);
    window.run()?;
    Ok(())
}

fn default_key() -> Key {
    "D minor".parse().expect("valid key")
}
