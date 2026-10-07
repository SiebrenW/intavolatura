//! Renders the window offscreen to a PNG, for checking the layout without
//! a display: `cargo run --example screenshot -- <file.intav> <out.png> [selected] [scale] [french] [dark]`

use std::rc::Rc;

use intavolatura::app::App;
use intavolatura::{AppWindow, ui};
use intavolatura_core::file_format;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, PhysicalSize, PlatformError, Rgb8Pixel};

struct Offscreen(Rc<MinimalSoftwareWindow>);

impl Platform for Offscreen {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.0.clone())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output, rest @ ..] = &args[..] else {
        return Err(
            "usage: screenshot <file.intav> <out.png> [selected] [scale] [french] [dark]".into(),
        );
    };
    let selected: Option<usize> = rest.first().and_then(|s| s.parse().ok());
    let scale: f32 = rest.get(1).and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let options = &rest[rest.len().min(2)..];
    let french = options.iter().any(|s| s == "french");
    let dark = options.iter().any(|s| s == "dark");

    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window.clone())))
        .map_err(|e| format!("{e:?}"))?;

    let document = file_format::read(&std::fs::read_to_string(input)?)?;
    let mut app = App::new(document, None);
    app.selected = selected.filter(|&i| i < app.line().notes().len());
    let mut preferences = app.preferences();
    if french {
        preferences.tablature = intavolatura::view::TablatureStyle::French;
    }
    if dark {
        preferences.theme = intavolatura::preferences::Theme::Dark;
    }
    app.set_preferences(&preferences);
    let ui = AppWindow::new()?;
    ui::wire(&ui, app);

    let (width, height) = ((1100.0 * scale) as u32, (720.0 * scale) as u32);
    window.dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged {
        scale_factor: scale,
    });
    window.set_size(PhysicalSize::new(width, height));
    ui.show()?;
    slint::platform::update_timers_and_animations();

    let mut pixels = vec![Rgb8Pixel::new(0, 0, 0); (width * height) as usize];
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, width as usize);
    });

    let file = std::fs::File::create(output)?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let bytes: Vec<u8> = pixels.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    encoder.write_header()?.write_image_data(&bytes)?;
    println!("wrote {output} ({width}x{height})");
    Ok(())
}
