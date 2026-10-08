//! Writes a PDF of a `.intav` file: `cargo run -p intavolatura --example pdf -- in.intav out.pdf [tab|bass|all] [french]`.

use intavolatura::app::App;
use intavolatura::print::{self, Contents, PrintOptions};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let app = App::open(std::path::Path::new(&args[1])).expect("open");
    let contents = match args.get(3).map(String::as_str) {
        Some("tab") => Contents::Tablature,
        Some("all") => Contents::All,
        _ => Contents::WithBass,
    };
    let options = PrintOptions {
        contents,
        title: "Test".into(),
        ..PrintOptions::default()
    };
    let pages = print::layout(app.line(), &app.scene(), &options);
    let pdf = print::write(&pages, options.page_size, &options.title).expect("pdf");
    std::fs::write(&args[2], pdf).expect("write");
}
