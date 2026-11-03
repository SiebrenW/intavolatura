# Intavolatura

A figured bass viewer: the user enters figured bass, and the app realizes the implied chords in staff notation and tablature.

## Language

Pitches are named in scientific pitch notation: C4 is middle C, octave numbers change between B and C (B3 is directly below C4).

**Bass note**:
The written note the user enters; the lowest sounding note of the chord, from which all figures are counted. It always has an explicit spelling (letter plus accidental), and may sound an octave below where it is written.
_Avoid_: Root (for the entered note)

**Key**:
The major or minor key of a bass line, shown as its key signature; figures are read as intervals within it unless an accidental says otherwise. A bass line has a first key and may start a new one at any note (a **key change**, drawn with naturals for what no longer applies); the instrument stays tuned to the first key. Short modulations may still appear only as accidentals.

**Duration**:
How long a bass note sounds, as a note value (whole, half, quarter, eighth, sixteenth), optionally dotted or a **triplet** (two thirds of its length, three in the time of two; a bracket with a 3 is drawn over each three consecutive triplet notes).
_Avoid_: Length, rhythm (for a single note)

**Rest**:
An entry in the bass line with only a duration: silence, with no figures and no chord.
_Avoid_: Pause, empty note

**Time signature**:
The single meter of a bass line, such as 4/4, 3/4, 3/2 or 6/8; it decides where bar lines fall.
_Avoid_: Meter (in code and docs), measure signature

**Upbeat**:
A short first bar before the first full bar, set with the time signature as a duration (e.g. one quarter in 3/4).
_Avoid_: Anacrusis, pickup

**Bar**:
A span of the bass line that fills one time signature; bar lines are placed automatically between bars.
_Avoid_: Measure

**Figure**:
A single number, optionally with an accidental, that names an interval above the bass note. A lone accidental is a figure for the 3rd. A ♯, ♭ or raised figure moves the key's note a semitone up or down; a ♮ means the plain letter.

**Figuring**:
The set of figures sounding together over a bass note; an empty figuring means 5/3. A bass note carries one figuring, or several in sequence for a suspension.
_Avoid_: Figures (for the set), chord symbol

**Suspension**:
A bass note with several figurings in sequence, where a dissonant voice resolves by step while the other voices hold (4–3, 7–6, 9–8). One fingering covers the whole suspension: only the resolving voices move.
_Avoid_: Retardation, changing figures

**Leading-tone hint**:
A faded, bracketed accidental such as (♯) under a bass note in a minor key, where the figuring leaves the 3rd unmarked but a raised 3rd is likely. A helper only: it never changes the chord, which always follows the key unless a figure says otherwise.
_Avoid_: Warning, auto-sharp

**Bass line**:
An ordered sequence of bass notes and rests, each bass note carrying its own figuring; the unit the user edits.

**Chord**:
The set of note names, without octaves, implied by a bass note and its figuring in the key.

**Voicing**:
The concrete sounding notes, with octaves, of a fingering. The voicing staff shows the voicing of the fingering the tablature shows.
_Avoid_: Realization (for the notes themselves)

**Fingering**:
A course and fret (or open diapason) for every note of a chord on a given instrument. It determines the voicing; several fingerings can share one voicing.
_Avoid_: Shape, grip, position

**Chart shape**:
A fingering of the baroque guitar that is a common guitar chord (A–G, minor, 7, maj7, m7; open and barré shapes as printed) on its five upper strings, optionally without its lowest courses. Ranked ahead of other candidates, and complete even with courses muted. Roots the chart lacks (F♯, B♭) get no help from it.
_Avoid_: Alfabeto (a different, historical chart)

**Candidate fingering**:
A fingering of a chord that is playable on the current instrument and contains the notes the figuring requires; candidates are ranked by how good the app judges them to be.

**Pinned fingering**:
A candidate fingering the user has chosen for a bass note, overriding the top-ranked candidate. Dropped when the bass note or figuring changes, since it belonged to a different chord.
_Avoid_: Override, custom voicing, pinned voicing

**Root**:
The note a chord is built on when stacked in thirds; derived from the bass note and figures, never entered directly. Differs from the bass note when the chord is inverted.

### Views

**Bass staff**:
The input staff showing the bass line as written, with its figurings.

**Voicing staff**:
A grand staff showing each chord's voicing as it sounds; its bass may differ by an octave from the bass staff.
_Avoid_: Chord staff, realization staff

**Column**:
One bass note together with everything shown for it in every view; the unit of selection and navigation.

**System**:
One line of the score on a printed page: the staves of every printed row from a bar line to a bar line, with the clefs and key signature repeated at its start.

**Reference pitch**:
The frequency of A4 that playback sounds at, e.g. 415 Hz for baroque pitch; a setting, not part of a bass line.

**Tempo**:
Quarter notes a minute for playback; part of the bass line's file.

### Instruments

**Instrument**:
A plucked instrument the tablature is written for, defined by its courses, their tuning, and how many frets each fretted course has (courses may differ, e.g. extra frets on the soundboard for the top course). First target: the 14-course theorbo in A.

**Course**:
A single string or a pair of strings played as one unit; the thing a tablature line or diapason sign refers to.
_Avoid_: String (when a pair is meant)

**Fretted course**:
A course that runs over the fingerboard and can be stopped at a fret.

**Diapason**:
A bass course that lies off the fingerboard and only sounds its open pitch. Diapasons are tuned diatonically, normally retuned to the key.
_Avoid_: Bass string, open bass

**Barré**:
One finger stopping several courses at the same fret; counts as a single finger of the left hand.

**Tablature style**:
The way course-and-fret positions are drawn: **Italian** (numbers, top course on the bottom line) or **French** (letters, top course on the top line). A display choice; the positions themselves are the same.
_Avoid_: Tab format, notation (for this choice)

**Re-entrant tuning**:
A tuning in which a course nearer the treble side sounds lower than its neighbour, as with the theorbo's top two courses being dropped an octave.
