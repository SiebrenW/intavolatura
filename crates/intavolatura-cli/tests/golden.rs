//! Each `golden/<name>.request.json` must produce `golden/<name>.response.json`.
//! After an intended change (e.g. new ranking weights), regenerate the
//! responses with `BLESS=1 cargo test -p intavolatura-cli` and review the diff.

use std::fs;
use std::path::Path;

#[test]
fn requests_give_their_golden_responses() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let bless = std::env::var_os("BLESS").is_some();
    let mut checked = 0;
    let mut failures = Vec::new();
    for entry in fs::read_dir(&dir).expect("golden directory") {
        let path = entry.expect("directory entry").path();
        let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".request.json"))
        else {
            continue;
        };
        let request = fs::read_to_string(&path).expect("request file");
        let actual = intavolatura_cli::respond(&request) + "\n";
        let expected_path = dir.join(format!("{name}.response.json"));
        if bless {
            fs::write(&expected_path, &actual).expect("write response");
        } else {
            let expected = fs::read_to_string(&expected_path).unwrap_or_default();
            if actual != expected {
                failures.push(format!("{name}:\n{actual}"));
            }
        }
        checked += 1;
    }
    assert!(checked > 0, "no golden requests in {}", dir.display());
    assert!(
        failures.is_empty(),
        "differs from golden:\n{}",
        failures.join("\n")
    );
}
