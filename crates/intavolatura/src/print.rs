//! PDF export: lays the score out in systems on pages, then writes the
//! pages as vector graphics with a subset of Bravura embedded.

use std::collections::BTreeMap;
use std::io::Write;

use flate2::{Compression, write::ZlibEncoder};
use intavolatura_core::{BassLine, timing};
use pdf_writer::types::{CidFontType, FontFlags, SystemInfo};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, Str, TextStr};
use subsetter::GlyphRemapper;
use ttf_parser::{Face, RawFace, Tag};

use crate::view::{
    self, BASS_ROW_BOTTOM, BASS_STAFF_TOP, Bar, Glyph, SPACE, Scene, TAB_BOTTOM, TAB_LINE, TAB_TOP,
    TREBLE_STAFF_TOP, Tie, VOICING_BASS_TOP, columns_for, time_signature_x,
};

const BRAVURA: &[u8] = include_bytes!("../fonts/Bravura.otf");

/// Page margin in points (about 15 mm).
const MARGIN: f32 = 42.0;

/// Which rows of the score are printed. The tablature is always there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Contents {
    Tablature,
    /// The figured bass staff with its figures above the tablature.
    #[default]
    WithBass,
    /// Figured bass, realization on the grand staff and tablature.
    All,
}

impl Contents {
    pub const ALL: [Contents; 3] = [Contents::Tablature, Contents::WithBass, Contents::All];
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageSize {
    #[default]
    A4,
    Letter,
}

impl PageSize {
    pub const ALL: [PageSize; 2] = [PageSize::A4, PageSize::Letter];

    /// Width and height in points.
    pub fn points(self) -> (f32, f32) {
        match self {
            PageSize::A4 => (595.28, 841.89),
            PageSize::Letter => (612.0, 792.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PrintOptions {
    pub contents: Contents,
    pub page_size: PageSize,
    /// One staff space in points; 5 gives staves 7 mm high.
    pub staff_size: f32,
    pub title: String,
}

impl Default for PrintOptions {
    fn default() -> PrintOptions {
        PrintOptions {
            contents: Contents::default(),
            page_size: PageSize::default(),
            staff_size: 5.0,
            title: String::new(),
        }
    }
}

/// A line of text in the interface font, left aligned at `(x, y)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub size: f32,
}

/// One page, in points from the top left.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Page {
    pub scene: Scene,
    pub labels: Vec<Label>,
}

/// The three rows of the display list, which print separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Bass,
    Voicing,
    Tablature,
}

impl Row {
    fn of(y: f32) -> Row {
        if y < BASS_ROW_BOTTOM {
            Row::Bass
        } else if y < VOICING_BASS_TOP + 8.5 * SPACE {
            Row::Voicing
        } else {
            Row::Tablature
        }
    }

    /// The least vertical extent of the row, around its staff.
    fn minimum(self) -> (f32, f32) {
        match self {
            Row::Bass => (BASS_STAFF_TOP - 3.0 * SPACE, BASS_STAFF_TOP + 6.0 * SPACE),
            Row::Voicing => (
                TREBLE_STAFF_TOP - 3.0 * SPACE,
                VOICING_BASS_TOP + 7.0 * SPACE,
            ),
            Row::Tablature => (TAB_TOP - 2.2 * TAB_LINE, TAB_BOTTOM + 0.8 * TAB_LINE),
        }
    }
}

/// What one system draws in one row, in scene pixels.
#[derive(Default)]
struct Piece {
    glyphs: Vec<Glyph>,
    bars: Vec<Bar>,
    ties: Vec<Tie>,
}

impl Piece {
    /// The vertical extent of the content, at least `minimum`.
    fn extent(&self, minimum: (f32, f32)) -> (f32, f32) {
        let mut top = minimum.0;
        let mut bottom = minimum.1;
        for g in &self.glyphs {
            top = top.min(g.y - 0.5 * g.size);
            bottom = bottom.max(g.y + 0.5 * g.size);
        }
        for b in &self.bars {
            top = top.min(b.y);
            bottom = bottom.max(b.y + b.height);
        }
        for t in &self.ties {
            bottom = bottom.max(t.y + t.height);
        }
        (top, bottom)
    }
}

/// A system's place on the line and in the width of a page.
struct Span {
    /// Where its notes start and end in the full scene.
    from: f32,
    to: f32,
    /// Where they start on the page, and the stretch that fills the line.
    left: f32,
    stretch: f32,
}

impl Span {
    fn map(&self, x: f32) -> f32 {
        self.left + (x - self.from) * self.stretch
    }
}

/// Lays the scene of `line` out on pages.
pub fn layout(line: &BassLine, scene: &Scene, options: &PrintOptions) -> Vec<Page> {
    let (page_width, page_height) = options.page_size.points();
    let pt = options.staff_size / SPACE;
    let width = (page_width - 2.0 * MARGIN) / pt;
    let height = (page_height - 2.0 * MARGIN) / pt;

    let columns = columns_for(line);
    let first_body = columns.left(0);
    let later_body = time_signature_x(line.key()) + 6.0;
    let end = columns.left(columns.count());

    // Where a system may end: after any slice, preferably at a bar line.
    let mut breaks: Vec<(f32, bool)> = Vec::new();
    for (index, note) in timing(line).notes.iter().enumerate() {
        if note.slices.is_empty() {
            breaks.push((columns.left(index + 1), false));
        }
        for (k, slice) in note.slices.iter().enumerate() {
            breaks.push((columns.slice_edges(index, k).1, slice.bar_line_after));
        }
    }

    let mut spans: Vec<Span> = Vec::new();
    let mut from = first_body;
    loop {
        let left = if spans.is_empty() {
            first_body
        } else {
            later_body
        };
        let room = width - left;
        if end - from <= room + 0.5 {
            spans.push(Span {
                from,
                to: f32::INFINITY,
                left,
                stretch: 1.0,
            });
            break;
        }
        let mut fits = breaks
            .iter()
            .filter(|(x, _)| *x > from + 1.0 && *x - from <= room);
        let to = fits
            .clone()
            .rfind(|(_, bar)| *bar)
            .or(fits.next_back())
            .or(breaks.iter().find(|(x, _)| *x > from + 1.0))
            .map_or(end, |(x, _)| *x);
        spans.push(Span {
            from,
            to,
            left,
            stretch: (room / (to - from)).clamp(1.0, 1.35),
        });
        from = to;
    }

    let rows: &[Row] = match options.contents {
        Contents::Tablature => &[Row::Tablature],
        Contents::WithBass => &[Row::Bass, Row::Tablature],
        Contents::All => &[Row::Bass, Row::Voicing, Row::Tablature],
    };
    let row_gap = 2.0 * SPACE;
    let system_gap = 5.0 * SPACE;
    let title_room = if options.title.is_empty() {
        0.0
    } else {
        4.0 * SPACE
    };

    let mut pages = vec![Page::default()];
    let mut pen = title_room;
    for (number, span) in spans.iter().enumerate() {
        let mut pieces: Vec<(Row, Piece, (f32, f32))> = rows
            .iter()
            .map(|&row| (row, Piece::default(), (0.0, 0.0)))
            .collect();
        distribute(scene, &columns, span, number == 0, width, &mut pieces);
        for (row, piece, extent) in &mut pieces {
            *extent = piece.extent(row.minimum());
        }
        let total: f32 = pieces.iter().map(|(_, _, e)| e.1 - e.0).sum::<f32>()
            + row_gap * (pieces.len() - 1) as f32;
        if pen > 0.0 && pen + total > height && !pages.last().is_some_and(is_blank) {
            pages.push(Page::default());
            pen = 0.0;
        }
        let page = pages.last_mut().expect("a page");
        for (_, piece, extent) in &pieces {
            emit(page, piece, pen - extent.0, pt);
            pen += extent.1 - extent.0 + row_gap;
        }
        pen += system_gap - row_gap;
    }

    if !options.title.is_empty() {
        pages[0].labels.push(Label {
            x: MARGIN,
            y: 14.0,
            text: options.title.clone(),
            size: 16.0,
        });
    }
    let count = pages.len();
    for (number, page) in pages.iter_mut().enumerate() {
        page.scene.width = page_width;
        page.scene.height = page_height;
        for glyph in &mut page.scene.glyphs {
            glyph.x += MARGIN;
            glyph.y += MARGIN;
        }
        for bar in &mut page.scene.bars {
            bar.x += MARGIN;
            bar.y += MARGIN;
        }
        for tie in &mut page.scene.ties {
            tie.x += MARGIN;
            tie.y += MARGIN;
        }
        for label in &mut page.labels {
            label.y += MARGIN;
        }
        if count > 1 {
            page.labels.push(Label {
                x: page_width / 2.0 - 2.0,
                y: page_height - MARGIN / 2.0,
                text: (number + 1).to_string(),
                size: 9.0,
            });
        }
    }
    pages
}

fn is_blank(page: &Page) -> bool {
    page.scene.glyphs.is_empty() && page.scene.bars.is_empty()
}

/// Sorts the scene's items into the system's rows: the header (clefs, key
/// signature) on every system, the time signature on the first, and the
/// notes between the span's ends.
fn distribute(
    scene: &Scene,
    columns: &view::Columns,
    span: &Span,
    first: bool,
    width: f32,
    pieces: &mut [(Row, Piece, (f32, f32))],
) {
    let header_end = columns.left(0);
    let time_x = header_end - 29.0;
    let mut put = |y: f32, f: &mut dyn FnMut(&mut Piece)| {
        if let Some((_, piece, _)) = pieces.iter_mut().find(|(row, _, _)| *row == Row::of(y)) {
            f(piece);
        }
    };

    for glyph in scene.glyphs.iter().filter(|g| g.music) {
        let x = if glyph.x < header_end {
            if !first && glyph.x >= time_x {
                continue;
            }
            glyph.x
        } else if glyph.x >= span.from && glyph.x < span.to {
            span.map(glyph.x)
        } else {
            continue;
        };
        put(glyph.y, &mut |p| {
            p.glyphs.push(Glyph { x, ..glyph.clone() })
        });
    }

    for bar in &scene.bars {
        let full_line = bar.x == 0.0 && bar.width == scene.width;
        let centre = bar.x + bar.width / 2.0;
        let placed = if full_line {
            Bar { width, ..*bar }
        } else if centre < header_end {
            *bar
        } else if centre >= span.from && centre < span.to {
            Bar {
                x: span.map(centre) - bar.width / 2.0,
                ..*bar
            }
        } else {
            continue;
        };
        put(bar.y, &mut |p| p.bars.push(placed));
    }

    for tie in &scene.ties {
        let (start, stop) = (tie.x.max(span.from), (tie.x + tie.width).min(span.to - 2.0));
        if stop - start < 6.0 || tie.x >= span.to || tie.x + tie.width <= span.from {
            continue;
        }
        let placed = Tie::below(span.map(start), span.map(stop), tie.y);
        put(tie.y, &mut |p| p.ties.push(placed.clone()));
    }
}

/// Adds a piece to the page, `dy` pixels down, converting to points.
fn emit(page: &mut Page, piece: &Piece, dy: f32, pt: f32) {
    for glyph in &piece.glyphs {
        page.scene.glyphs.push(Glyph {
            x: glyph.x * pt,
            y: (glyph.y + dy) * pt,
            size: glyph.size * pt,
            ..glyph.clone()
        });
    }
    for bar in &piece.bars {
        page.scene.bars.push(Bar {
            x: bar.x * pt,
            y: (bar.y + dy) * pt,
            width: bar.width * pt,
            height: bar.height * pt,
            knockout: bar.knockout,
        });
    }
    for tie in &piece.ties {
        page.scene.ties.push(Tie {
            x: tie.x * pt,
            y: (tie.y + dy) * pt,
            width: tie.width * pt,
            height: tie.height * pt,
            commands: scale_path(&tie.commands, pt),
        });
    }
}

/// Scales the numbers of SVG path commands.
fn scale_path(commands: &str, factor: f32) -> String {
    commands
        .split_whitespace()
        .map(|token| match token.parse::<f32>() {
            Ok(n) => (n * factor).to_string(),
            Err(_) => token.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Writes the pages as a PDF.
pub fn write(pages: &[Page], page_size: PageSize, title: &str) -> Result<Vec<u8>, String> {
    let (page_width, page_height) = page_size.points();
    let face = Face::parse(BRAVURA, 0).map_err(|e| e.to_string())?;
    let units = f32::from(face.units_per_em());

    // The glyphs used, renumbered for the subset.
    let mut remapper = GlyphRemapper::new();
    remapper.remap(0);
    let mut widths: BTreeMap<u16, f32> = BTreeMap::new();
    let mut ids = |text: &str| -> Vec<u16> {
        text.chars()
            .filter_map(|c| face.glyph_index(c))
            .map(|gid| {
                let new = remapper.remap(gid.0);
                let advance = face.glyph_hor_advance(gid).unwrap_or(0);
                widths.insert(new, 1000.0 * f32::from(advance) / units);
                new
            })
            .collect()
    };
    let texts: Vec<Vec<Vec<u16>>> = pages
        .iter()
        .map(|p| p.scene.glyphs.iter().map(|g| ids(&g.text)).collect())
        .collect();
    let subset = subsetter::subset(BRAVURA, 0, &remapper).map_err(|e| format!("{e:?}"))?;
    let cff = RawFace::parse(&subset, 0)
        .ok()
        .and_then(|face| face.table(Tag::from_bytes(b"CFF ")))
        .ok_or("the font subset has no outlines")?;

    let catalog = Ref::new(1);
    let tree = Ref::new(2);
    let info = Ref::new(3);
    let type0 = Ref::new(4);
    let cid = Ref::new(5);
    let descriptor = Ref::new(6);
    let file = Ref::new(7);
    let helvetica = Ref::new(8);
    let page_ref = |n: usize| Ref::new(9 + 2 * n as i32);
    let content_ref = |n: usize| Ref::new(10 + 2 * n as i32);

    let mut pdf = Pdf::new();
    pdf.catalog(catalog).pages(tree);
    pdf.document_info(info)
        .title(TextStr(title))
        .creator(TextStr("Intavolatura"));
    pdf.pages(tree)
        .kids((0..pages.len()).map(page_ref))
        .count(pages.len() as i32);

    let name = Name(b"ABCDEF+Bravura");
    pdf.type0_font(type0)
        .base_font(name)
        .encoding_predefined(Name(b"Identity-H"))
        .descendant_font(cid);
    {
        let mut font = pdf.cid_font(cid);
        font.subtype(CidFontType::Type0)
            .base_font(name)
            .system_info(SystemInfo {
                registry: Str(b"Adobe"),
                ordering: Str(b"Identity"),
                supplement: 0,
            })
            .font_descriptor(descriptor)
            .default_width(0.0);
        let mut array = font.widths();
        for (&id, &width) in &widths {
            array.consecutive(id, [width]);
        }
    }
    let bbox = face.global_bounding_box();
    let per_mille = |v: i16| 1000.0 * f32::from(v) / units;
    pdf.font_descriptor(descriptor)
        .name(name)
        .flags(FontFlags::SYMBOLIC)
        .bbox(Rect::new(
            per_mille(bbox.x_min),
            per_mille(bbox.y_min),
            per_mille(bbox.x_max),
            per_mille(bbox.y_max),
        ))
        .italic_angle(0.0)
        .ascent(per_mille(face.ascender()))
        .descent(per_mille(face.descender()))
        .cap_height(700.0)
        .stem_v(80.0)
        .font_file3(file);
    {
        let data = deflate(cff);
        let mut stream = pdf.stream(file, &data);
        stream.filter(Filter::FlateDecode);
        stream.pair(Name(b"Subtype"), Name(b"CIDFontType0C"));
    }
    pdf.type1_font(helvetica)
        .base_font(Name(b"Helvetica"))
        .encoding_predefined(Name(b"WinAnsiEncoding"));

    for (n, page) in pages.iter().enumerate() {
        let content = draw(page, &texts[n], &face, units, page_height);
        let mut p = pdf.page(page_ref(n));
        p.parent(tree)
            .media_box(Rect::new(0.0, 0.0, page_width, page_height))
            .contents(content_ref(n));
        let mut resources = p.resources();
        let mut fonts = resources.fonts();
        fonts.pair(Name(b"F1"), type0);
        fonts.pair(Name(b"F2"), helvetica);
        fonts.finish();
        resources.finish();
        p.finish();
        let data = deflate(&content);
        pdf.stream(content_ref(n), &data)
            .filter(Filter::FlateDecode);
    }
    Ok(pdf.finish())
}

fn deflate(data: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    // Writing to memory cannot fail.
    let _ = encoder.write_all(data);
    encoder.finish().unwrap_or_default()
}

/// A page's content stream: bars, ties, glyphs, then text.
fn draw(page: &Page, texts: &[Vec<u16>], face: &Face, units: f32, page_height: f32) -> Vec<u8> {
    let mut c = Content::new();
    for bar in &page.scene.bars {
        c.set_fill_gray(if bar.knockout { 1.0 } else { 0.0 });
        c.rect(
            bar.x,
            page_height - bar.y - bar.height,
            bar.width,
            bar.height,
        );
        c.fill_nonzero();
    }
    c.set_fill_gray(0.0);
    for tie in &page.scene.ties {
        path(&mut c, tie, page_height);
    }
    let mut gray = 0.0;
    for (glyph, ids) in page.scene.glyphs.iter().zip(texts) {
        let wanted = if glyph.faded { 0.55 } else { 0.0 };
        if wanted != gray {
            c.set_fill_gray(wanted);
            gray = wanted;
        }
        let advance: f32 = glyph
            .text
            .chars()
            .filter_map(|ch| face.glyph_index(ch))
            .map(|gid| f32::from(face.glyph_hor_advance(gid).unwrap_or(0)))
            .sum();
        let left = glyph.x - advance / units * glyph.size / 2.0;
        let bytes: Vec<u8> = ids.iter().flat_map(|id| id.to_be_bytes()).collect();
        c.begin_text();
        c.set_font(Name(b"F1"), glyph.size);
        c.set_text_matrix([1.0, 0.0, 0.0, 1.0, left, page_height - glyph.y]);
        c.show(Str(&bytes));
        c.end_text();
    }
    c.set_fill_gray(0.0);
    for label in &page.labels {
        let bytes: Vec<u8> = label
            .text
            .chars()
            .map(|ch| u8::try_from(u32::from(ch)).unwrap_or(b'?'))
            .collect();
        c.begin_text();
        c.set_font(Name(b"F2"), label.size);
        c.set_text_matrix([1.0, 0.0, 0.0, 1.0, label.x, page_height - label.y]);
        c.show(Str(&bytes));
        c.end_text();
    }
    c.finish().to_vec()
}

/// Fills a tie, converting its quadratic curves to cubic ones.
fn path(c: &mut Content, tie: &Tie, page_height: f32) {
    let numbers: Vec<f32> = tie
        .commands
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    // `M x y Q cx cy x y Q cx cy x y Z`, as view::Tie::below writes it.
    let [x0, y0, c1x, c1y, x1, y1, c2x, c2y, x2, y2] = numbers[..] else {
        return;
    };
    let at = |x: f32, y: f32| (tie.x + x, page_height - (tie.y + y));
    let cubic = |c: &mut Content, from: (f32, f32), control: (f32, f32), to: (f32, f32)| {
        let a = (
            from.0 + 2.0 / 3.0 * (control.0 - from.0),
            from.1 + 2.0 / 3.0 * (control.1 - from.1),
        );
        let b = (
            to.0 + 2.0 / 3.0 * (control.0 - to.0),
            to.1 + 2.0 / 3.0 * (control.1 - to.1),
        );
        c.cubic_to(a.0, a.1, b.0, b.1, to.0, to.1);
    };
    let (p0, p1, p2) = (at(x0, y0), at(x1, y1), at(x2, y2));
    c.move_to(p0.0, p0.1);
    cubic(c, p0, at(c1x, c1y), p1);
    cubic(c, p1, at(c2x, c2y), p2);
    c.close_path();
    c.fill_nonzero();
}

#[cfg(test)]
mod tests {
    use super::*;
    use intavolatura_core::{BassNote, Pitch};

    fn line_of(count: usize) -> BassLine {
        let notes = (0..count)
            .map(|i| {
                let pitch: Pitch = ["C3", "G2", "A2", "F2"][i % 4].parse().unwrap();
                BassNote::new(pitch)
            })
            .collect();
        BassLine::with_notes("C major".parse().unwrap(), Default::default(), notes)
    }

    fn pages_for(count: usize, options: &PrintOptions) -> Vec<Page> {
        let line = line_of(count);
        let scene = view::scene(
            &line,
            &[],
            &intavolatura_core::Instrument::theorbo_in_a(),
            &view::ViewOptions {
                show_hints: true,
                tablature: view::TablatureStyle::French,
                signs: view::Signs::default(),
            },
        );
        layout(&line, &scene, options)
    }

    #[test]
    fn a_short_line_is_one_system_on_one_page() {
        let pages = pages_for(4, &PrintOptions::default());
        assert_eq!(pages.len(), 1);
    }

    #[test]
    fn a_long_line_breaks_into_systems_and_pages() {
        let pages = pages_for(200, &PrintOptions::default());
        assert!(pages.len() > 1);
        let (w, h) = PageSize::A4.points();
        for page in &pages {
            for glyph in &page.scene.glyphs {
                assert!(glyph.x > 0.0 && glyph.x < w, "x {}", glyph.x);
                assert!(glyph.y > 0.0 && glyph.y < h, "y {}", glyph.y);
            }
        }
    }

    #[test]
    fn the_pdf_is_written() {
        let options = PrintOptions {
            title: "Test".into(),
            ..PrintOptions::default()
        };
        let pages = pages_for(30, &options);
        let pdf = write(&pages, options.page_size, "Test").unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(pdf.len() < 200_000, "{} bytes", pdf.len());
    }
}
