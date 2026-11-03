//! `intavolatura-cli <request.json>`: realizes the figured bass in the
//! request and prints the response as JSON on stdout. Used by the MuseScore
//! plugin; the request and response are described in `lib.rs`.

use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        println!(r#"{{"error": "usage: intavolatura-cli <request.json>"}}"#);
        return ExitCode::FAILURE;
    };
    match std::fs::read_to_string(&path) {
        Ok(request) => {
            println!("{}", intavolatura_cli::respond(&request));
            ExitCode::SUCCESS
        }
        Err(e) => {
            let error = serde_json::json!({ "error": format!("cannot read {path}: {e}") });
            println!("{error}");
            ExitCode::FAILURE
        }
    }
}
