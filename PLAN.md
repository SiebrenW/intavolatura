# Figured bass viewer

Enter a figured bass line; the app generates playable fingerings for each chord and shows them on a voicing staff and in tablature. Vocabulary is defined in [CONTEXT.md](./CONTEXT.md); key decisions are in [docs/adr/](./docs/adr/).

Pitches are written in scientific pitch notation everywhere (C4 = middle C, B3 is the note below it), in docs, code, tests and the file format.

## Roadmap

Each step is buildable and tested before the next starts.

1. [x] **Workspace**: Cargo workspace with `intavolatura-core` (lib) and `intavolatura` (app stub; Slint is added in step 11).
2. [x] **Pitch**: note letters, accidentals, octaves (scientific pitch notation), staff steps, semitone values.
3. [x] **Key**: major/minor keys, key signatures, scale degrees, diatonic stepping of a bass note.
4. [x] **Figuring**: parser for the input syntax, normalised figuring, display form.
5. [x] **Chord**: intervals; bass note + figuring + key → chord; required notes, dissonances, root, leading-tone hint.
6. [x] **Bass line**: columns, editing commands, pins, undo/redo.
7. [x] **Instrument**: courses, tuning, fret counts; the 14-course theorbo in A as data.
8. [x] **Fingering search**: all valid, playable candidate fingerings for a chord; benchmark (`cargo run --release --example search_bench`).
9. [x] **Ranking**: weighted score; inspect with `cargo run --example show -- "D minor" A2 "#"`. Tuning the weights on real bass lines continues.
10. [x] **File format**: save/load the app's own text format.
11. [x] **App: bass staff**: Slint window, bass staff rendering, keyboard and mouse editing, key dropdown. Check layout offscreen with `cargo run -p intavolatura --example screenshot -- <file.intav> <out.png> [selected] [scale]`.
12. [x] **App: voicing staff and tablature**: grand staff, Italian and French tablature, column alignment, scrolling.
13. [x] **App: polish**: settings (tablature style, hints, fret span, diapason retuning, number of candidates to cycle), About dialog with Slint attribution, startup time and memory checks.

## Features

- [x] Bass line input (bass staff)
  - [x] One key per bass line, chosen in a dropdown (e.g. C major, D minor); key signature shown on the bass staff and voicing staff
  - [x] Click after the last column to append a bass note; click between columns to insert; click a column to select it and move its bass note to the clicked pitch
  - [x] ↑/↓ move the bass note by staff step (diatonic, picks up the key signature)
  - [x] Shift+↑/↓ step the accidental (♭ / ♮ / ♯); `n` sets a natural
  - [x] ←/→ move between columns
  - [x] Delete/Backspace removes the selected bass note and its figuring
  - [x] Enter edits the column's figuring box; Esc returns to the column
  - [x] Undo/redo (Ctrl/Cmd+Z, Ctrl/Cmd+Shift+Z) for every change: notes, figurings, pins, key
- [x] Figuring input
  - [x] Supported figurings: empty/5/3/5-3, 6, 6/3, 6/4, 7, 7/5/3, 6/5, 4/3, 6/4/3, 4/2, 2, 6/4/2
  - [x] Accidentals on any figure (♯ ♭ ♮), raised figures (6+, slashed 6); a lone accidental applies to the 3rd
  - [x] Figures are literal: always within the key unless an accidental says otherwise
  - [x] Displayed with proper symbols, highest figure on top
  - [x] Leading-tone hint: faded (♯) in minor where a raised 3rd is likely but unmarked; can be disabled in settings
- [x] Fingering generation
  - [x] Generate all candidate fingerings for each chord on the current instrument
  - [x] Show the top-ranked candidate; `[` / `]` cycle through the top N and pin the choice (N is a setting, default 10)
  - [x] A pin is dropped when the bass note or figuring changes, and all pins when the key changes (undo restores them)
  - [x] Cycling back to the best candidate unpins, so the note follows the ranking again
- [x] Views
  - [x] Three rows aligned in columns: bass staff, voicing staff (grand staff), tablature
  - [x] Selected column highlighted across all rows
  - [x] Tablature in Italian or French style (setting)
  - [x] Long bass lines scroll horizontally; no system breaks
- [ ] Files
  - [x] Save/load in the app's own text format (key, instrument, bass notes, figurings, pins); format described in `crates/intavolatura-core/src/file_format.rs`. Open/Save buttons and Ctrl/Cmd+O, Ctrl/Cmd+S, Ctrl/Cmd+Shift+S with native dialogs; a path on the command line opens that file
  - [x] The window title marks unsaved changes; opening another file or closing asks before discarding them
  - [x] A pin that is not a valid fingering for its chord (e.g. after changing settings) is ignored, not an error
  - [ ] Later: MusicXML import/export

## Later

- Suspensions (4–3, 7–6, 9–8): several figurings in sequence over one bass note; needs rhythm within a note.
- Rarer figures (9, 9/7, 5/4, ♯4/2, …) and continuation lines.
- Rhythm, bar lines, voice leading between chords.
- More instruments: baroque guitar, renaissance lute in G.

## Figuring input syntax

| Typed | Meaning |
|---|---|
| `6` | 6 |
| `6 5`, `6/5`, `65` | 6/5 (adjacent digits are separate figures) |
| `#6`, `6#` | ♯6 (accidental binds to the touching digit) |
| `b7`, `n3` | ♭7, ♮3 |
| `6+`, `6\` | raised 6 |
| `#` | lone ♯: raised 3rd |
| `6 #` | 6 plus raised 3rd (space separates) |
| *(empty)* | 5/3 |

Order does not matter. Invalid input (letters, `0`, `8`, …) is highlighted and the previous figuring is kept.

## Fingering rules

### Valid (chord content)

- 3 to 5 notes, bass included.
- The bass is the lowest sounding note; no other note sounds below it.
- Bass at the written octave, or one octave lower; one octave higher only if the written note is below the instrument's range.
- Must contain every interval of the chord shape, except the 5th above the bass in 5/3 and 7 chords, which may be omitted unless written.
- Dissonances: the chord's 7th (the 7 in a 7, the 5 in a 6/5, the 3 in a 4/3, the bass in a 4/2) and the 4 in a 6/4.
- Never doubled: the leading tone and dissonances. Other notes may be doubled freely.

### Playable (theorbo)

- At most one note per course.
- Diapasons open only.
- Fretted courses up to their fret count (9; 11 on the top course).
- All stopped notes within a fret span of 4 (user setting, default 4).
- At most 4 left-hand fingers; a barré counts as one.
- Skipped courses are allowed.

### Ranking (weighted score, weights roughly in this order)

1. Bass at the written octave
2. Low stretch (fret distance and course distance, worse in low positions)
3. Complete chord (5th present)
4. No doubled 3rd
5. Low position
6. Open courses
7. Compact upper voices in the middle register
8. Four notes over three or five

Weights to be tuned against real bass lines.

## Instrument: 14-course theorbo in A

- Fretted courses 1–6: A3 E3 B3 G3 D3 A2 (courses 1 and 2 dropped an octave: re-entrant).
- Diapasons 7–14: G2 F2 E2 D2 C2 B1 A1 G1 (diatonic).
- Setting "retune diapasons to the key" (default on): diapasons take the key signature's accidentals, e.g. F#2 and C#2 in D major.
- Instruments are data (courses, tuning, fret count per course) so others can be added.

## Tablature: diapason signs (theorbo, courses 7–14)

- French: 7 = `a`, 8 = `/a`, 9 = `//a`, 10 = `///a`, 11–14 = `4`, `5`, `6`, `7`; below the staff.
- Italian: 7 = `0` on a ledger line below the staff; 8–14 = the course number on an extra ledger line below that.

## Technology

- Rust, Cargo workspace:
  - `intavolatura-core`: pure Rust, no GUI dependency. Bass line, figuring parser, chord, instrument, fingering search and ranking, file format, undo.
  - `intavolatura`: Slint GUI; maps core data to views and key presses to core commands.
- Slint for the GUI, under the royalty-free licence; attribution via an About dialog (`AboutSlint`).

## Performance

Optimisation is a first-class goal, not an afterthought.

- Snappy everywhere: editing, regenerating fingerings and redrawing must feel instant.
- Fast startup.
- Modest resource usage (CPU, memory, disk) out of respect for the user's machine.
- The fingering search is the likely hot path; benchmark it in the core crate.
- Measure startup with `INTAVOLATURA_STARTUP_CHECK=1 target/release/intavolatura <file>`: prints the time to the first frame and quits. Wrap in `/usr/bin/time -l` for peak memory.
- Measured 2026-10-07 on macOS (Apple silicon), release build: first frame ~140 ms warm (~180-260 ms cold), ~100 MB resident with the default OpenGL renderer (mostly the GPU stack; the software renderer used more at Retina scale), 12 MB stripped binary. Fingering search ~33 µs per chord.

## Settings

Kept in `preferences.txt` in the platform's config directory (macOS `~/Library/Application Support/Intavolatura`, Windows `%APPDATA%\Intavolatura`, Linux `$XDG_CONFIG_HOME/intavolatura`): theme (system, light or dark; default follows the system), tablature style, leading-tone hints, diapason retuning, left-hand fret span (2–6), fingerings to cycle (1–50).

## Platforms

- Desktop only: macOS, Windows and Linux (personal use across own devices).
- Mobile is out of scope for now.
