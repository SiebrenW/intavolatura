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
9. [x] **Ranking**: weighted score; inspect with `cargo run --example show -- "D minor" A2 "#"`. Tuning the weights on real bass lines continues. Tune against pinned choices in saved files with `cargo run --release --example tune -- <files>`.
10. [x] **File format**: save/load the app's own text format.
11. [x] **App: bass staff**: Slint window, bass staff rendering, keyboard and mouse editing, key dropdown. Check layout offscreen with `cargo run -p intavolatura --example screenshot -- <file.intav> <out.png> [selected] [scale]`.
12. [x] **App: voicing staff and tablature**: grand staff, Italian and French tablature, column alignment, scrolling.
13. [x] **App: polish**: settings (tablature style, hints, fret span, diapason retuning, number of candidates to cycle), About dialog with Slint attribution, startup time and memory checks.

## Roadmap, phase 2

Four features, in this order: rhythm comes first because playback needs exact timing and PDF needs bar lines to break systems at; PDF reuses the display list; instruments touch everything, so they come last, onto a settled base. Each feature starts with a short design session on its open decisions (recommendations given), then builds in tested, committed steps.

### A. More rhythm

Goal: real continuo parts fit without warnings.

Steps:
14. [x] **Ties across bar lines**: a note that runs over a bar line is drawn as tied notes on both sides (as export already writes it), replacing the warning colour. Display only; the model keeps one note.
15. [x] **Upbeat**: a bass line can start with an incomplete bar. Bar lines, export (`implicit` first measure) and import (instead of the current warning) follow.
16. [x] **Time signature changes**: a time signature can start at any note; drawn there on both staves, kept in files, exported and imported (instead of being ignored).
17. [x] **Uneven suspension splits**: a figuring part can carry its own duration, e.g. a dotted quarter then an eighth for 7–6.
18. [ ] **Triplets** (optional): three notes in the time of two, for proportio/tripla passages.

Decided:
- Upbeat: set next to the time signature as a duration; files store `upbeat: 4`.
- Meter change: `T` on a note opens the time signature picker "from this note on"; drawn before that note; files store it on the note (`| time 3/2`). A change after an incomplete bar marks that bar in the warning colour.
- Uneven splits: a colon and a duration after a part, `7:4.-6:8`; parts without a length share the rest equally; lengths that do not add up are refused like an invalid figuring.
- Triplets: later.

### B. Playback

Goal: hear the bass line and its realization as the theorbo would sound, to judge voicings and fingerings by ear.

Steps:
19. [x] **Audio engine** (core-independent module in the app): a plucked-string synthesizer (Karplus–Strong), one voice per course, so a re-struck course stops its previous note and open diapasons ring long. Output through `cpal`; started lazily on first play, so startup stays fast.
20. [x] **Scheduling**: the bass line becomes timed course/fret events (bass on the beat, upper notes slightly arpeggiated), held suspension voices keep sounding, rests are silent. Tempo in quarter notes per minute.
21. [x] **Transport**: Space plays and stops from the selected column (or the start); a playhead highlights the sounding column and scrolls with it; Esc stops.
22. [x] **Settings**: tempo, arpeggiation spread, volume; play bass only / realization only / both.
22a. [x] **Tuning**: a reference pitch so you can play along: presets A = 415 (baroque, default), 440 (modern), 392 (French baroque), 430 (classical), 466 (Venetian/cornetto), or any value. Optionally a temperament (see decisions).

Decided: synthesized plucked strings, no sample files; tempo in the file (`tempo: 72`, default 72), shown in the Playback popup; pinned and chosen fingerings are what sounds, so you hear the course choices; the reference pitch is a setting (default A = 415) with presets 392, 415, 430, 440, 466 or any value from 380 to 480; equal temperament only, other temperaments are Later. "Both" (bass doubled over the chords) was dropped: the bass is already in the chord. The arpeggio spread (default 25 ms between courses, lowest first) and the volume are settings.

Risks: audio APIs differ per platform; `cpal` covers macOS, Windows and Linux (ALSA/PulseAudio). Audio runs on its own thread so editing stays responsive.

### C. More instruments

Goal: realize the same figured bass for other continuo instruments, each in its proper tablature.

Steps:
23. [x] **Instrument definitions** (done: theorbo in A, Renaissance lute in G with 6, 7 or 8 courses; baroque guitar with bourdons; theorbo in G (a tone below the one in A), archlute in G (the lute's fretted courses and eight diapasons F2–F1; tuning to verify), 13-course baroque lute in D minor (F4 D4 A3 F3 D3 A2, diapasons G2–A1)) as data, with per-instrument rules: courses and tuning, fret counts, diapasons, which tablature styles apply, and realization rules that differ (e.g. on re-entrant guitar the lowest sounding note need not be the bass). Candidates: Renaissance lute in G (6–8 courses), archlute in G (14 courses), 13-course baroque lute (D minor tuning, French tablature), 5-course baroque guitar (re-entrant, with or without bourdons), theorbo in G.
24. [x] **Instrument picker** per bass line, bottom right of the window (files already store `instrument:`), diapason retuning per instrument. Changing instrument realizes every note again; pinned fingerings are kept and ignored where they do not fit. Not undoable.
25. [x] **Tablature drawing generalized** (done for five and six lines; diapason signs for the lutes follow the theorbo's; check them against sources): as many lines as fretted courses (5 for guitar, 6 for lute), each instrument's diapason signs, 7th-course conventions of the lute.
26. [ ] **Fingering rules per instrument** (done: the baroque guitar prefers chords on every course, since continuo is strummed, and ignores doubled thirds; to do: hand limits, weights and register per instrument): hand limits, ranking weights and register defaults tuned per instrument.

Open decisions:
- Which instruments first? (Recommended: Renaissance lute in G and baroque guitar, the two you named before; then archlute.)
- Baroque guitar continuo: fingered chords like the theorbo (recommended first), or also alfabeto (strummed chord letters), which is a separate notation?
- One instrument per bass line (recommended), or show two instruments side by side?

### D. PDF export of the tablature

Goal: a printable score to play from.

Steps:
27. [x] **System layout**: break the line into systems at bar lines to fit the page width, repeat clefs, key signature and tablature start on every system; page breaks between systems; title and page numbers.
28. [x] **PDF writer**: draw the existing display list (glyphs, lines, ties) as vector PDF with Bravura embedded (subset), using `pdf-writer` and `ttf-parser`/`subsetter`; no raster images, small files, sharp at any zoom.
29. [x] **Export dialog**: what to print (tablature only; tablature with the bass staff; all three rows), page size (A4/Letter), staff height, kept in the preferences. The title is the file name; a composer line waits until files carry metadata.

Decided: tablature with the figured bass staff above it by default; A4 by default; printing follows the screen settings for tablature style, sign size and placement. Systems break at bar lines (after any note if a bar is too long for a line), later systems repeat clefs and key signature but not the time signature, lines are stretched up to 35% to fill the width, and ties across a break are drawn as two half ties.

Risks: font embedding and glyph mapping need care; checked by opening output in a PDF viewer and comparing to the screen render.

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
  - [x] Supported figurings: empty/5/3/5-3, 6, 6/3, 6/4, 7, 7/5/3, 6/5, 4/3, 6/4/3, 4/2, 2, 6/4/2; for suspensions also 4 (5/4), 9 (9/5/3) and 8 (the octave)
  - [x] Suspensions: figurings in sequence joined by a dash (`4-3`, `7-6`, `9-8`, `6 4-5 3`, `4-#`), each a full figuring, shown side by side under one bass note
  - [x] One fingering per suspension: held voices stay on their course and fret; each resolving voice moves by step, on its own course if it can (else another, ranked lower)
  - [x] The resolution's tablature shows only the moved notes; on the voicing staff held notes are tied
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
  - [x] Every shortcut and the figuring syntax listed in a Keys overview (button, `?` or F1)
  - [x] Tablature in Italian or French style (in Settings)
  - [x] Long bass lines scroll horizontally; no system breaks
- [x] Rhythm
  - [x] Each bass note has a duration (whole to sixteenth, optionally dotted); keys 7 6 5 4 3 and `.`, or the duration bar; new notes take the last duration chosen
  - [x] One time signature per bass line (4/4 as C, 2/2 as ¢, 3/4, 3/2, 2/4, 6/4, 6/8, 3/8, 12/8), chosen in a picker; bar lines are placed automatically
  - [x] A note running over a bar line is drawn in the warning colour (ties across bar lines are not supported yet)
  - [x] A suspension's figurings split the note equally
  - [x] Rests: `R` turns the selected note into a rest of the same length (or back); a click on the staff makes it a note again. Shown on every staff, with a rhythm sign over an empty tablature column
  - [x] Note values with stems, flags and dots on both staves; lute rhythm signs above the tablature; columns widen with duration (a whole note twice a quarter)
- [x] Files
  - [x] Save/load in the app's own text format (key, instrument, bass notes, figurings, pins); format described in `crates/intavolatura-core/src/file_format.rs`. Open/Save buttons and Ctrl/Cmd+O, Ctrl/Cmd+S, Ctrl/Cmd+Shift+S with native dialogs; a path on the command line opens that file
  - [x] The window title marks unsaved changes; opening another file or closing asks before discarding them
  - [x] A pin that is not a valid fingering for its chord (e.g. after changing settings) is ignored, not an error
  - [x] MusicXML export (Export button): the figured bass part, and optionally the realization on a grand staff (held notes tied, filler rests hidden); notes over a bar line are split and tied; no tablature. Check with `cargo run --example to_musicxml -- <in.intav> <out.musicxml>`
  - [x] MusicXML import through Open (.musicxml, .xml, .mxl): choose the part, or "All parts, lowest note"; the bass is the lowest sounding note across all voices and staves, ties joined, octave transposition applied; lengths that fit no note value are rounded to the sixteenth grid without moving later bars, long notes and rests are split at bar lines and meter changes, meter changes are read; unsupported figures are left empty; a summary lists the changes. Triplets are rounded until step 18. Check with `cargo run --example from_musicxml -- <file> [part]`. Round trip through MuseScore 4 (export → .mxl → import) gives back the same bass line
  - [x] MIDI import through Open (.mid, .midi): choose a track or channel (drums left out) or all parts; bars from the time signature events, key and tempo from the file, lowest sounding note, rounded to the sixteenth grid; no upbeat detection (MIDI has none). Shares the reduction with MusicXML. Check with `cargo run --example from_midi -- <file> [part]`
  - [x] Playback: Space plays from the selected note (or the start) and stops, Esc stops; a playhead follows the sounding column; the Playback popup (timer icon) sets tempo (saved in the file), what to play, pitch, arpeggio spread and volume. Plucked-string synthesis (Karplus–Strong, one voice per course), output through `cpal` opened on first play (startup unchanged at about 140 ms). Check sound with `cargo run -p intavolatura --example play -- <in.intav>`
  - [x] PDF export (Export popup → Save PDF…): tablature, figured bass with tablature, or all rows; A4 or Letter; staff height in mm; vector output with a Bravura subset (about 10–15 KB for three systems), systems broken at bar lines, page numbers. Check with `cargo run -p intavolatura --example pdf -- <in.intav> <out.pdf> [tab|bass|all]`

## Later

- Rarer figure combinations (9/7, 9/4, 7/4, 6/4/2 with a 7, …) and continuation lines.
- Voice leading between chords.
- Temperaments for playback (1/4- and 1/6-comma meantone, Vallotti) to play along with keyboard continuo.
- Playback: a metronome click, looping a selection, a count-in.

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
| `4-3`, `6 4-5 3` | figurings in sequence (a suspension); each part as above |
| `7:4.-6:8`, `7:4.-6` | uneven suspension: a length after a colon; parts without one share the rest equally; lengths that do not fit the note are refused |

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

## Tablature: signs

All signs use the historical forms in Bravura's SMuFL lute tablature ranges (French letters U+EBC0–EBCC and course signs U+EBCD–EBD0, Italian numerals U+EBE0–EBE9; SIL OFL, already bundled). Settings: sign size (50–150%), and per style whether signs rest on the line, as in many sources (French default), or are centred on it with the line cleared behind (Italian default).

## Tablature: diapason signs (theorbo, courses 7–14)

- French: 7 = `a`, 8 = `/a`, 9 = `//a`, 10 = `///a`, 11–14 = `4`, `5`, `6`, `7`; below the staff.
- Italian: the staff is upside down (course 1 on the bottom line), so diapasons go above it: 7 = `0` on a ledger line above the staff; 8–14 = the course number on an extra ledger line above that.

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

Kept in `preferences.txt` in the platform's config directory (macOS `~/Library/Application Support/Intavolatura`, Windows `%APPDATA%\Intavolatura`, Linux `$XDG_CONFIG_HOME/intavolatura`): tablature sign size and placement, theme (system, light or dark; default follows the system), tablature style, leading-tone hints, diapason retuning, left-hand fret span (2–6), fingerings to cycle (1–50).

## Platforms

- Desktop only: macOS, Windows and Linux (personal use across own devices).
- Mobile is out of scope for now.
