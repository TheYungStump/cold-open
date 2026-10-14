//! Reads back a PDF we wrote and checks it draws what the layout placed
//! (layout.md §11, §12).
//!
//! The reader follows the file's cross-reference table, then runs each
//! page's content stream the way a viewer does: every glyph advances by the
//! width its font declares (`/DW`, `/W`) times the font size, plus the
//! character spacing `Tc`. It understands only what our writer emits and
//! panics on anything else, so a change in the writer cannot slip past it.
//!
//! Positions are integers in millionths of a point, so 7.2 pt and 108 pt
//! compare exactly.

use std::collections::BTreeMap;

use coldopen_layout::{Coverage, Fonts, Layout};
use unicode_width::UnicodeWidthChar;

/// One millionth of a point per unit; a `Pt` (hundredths) is 10 000.
const MICRO: i64 = 1_000_000;

#[derive(Clone, Debug, PartialEq)]
enum Obj {
    Num(i64),
    Name(String),
    Str(Vec<u8>),
    Array(Vec<Obj>),
    Dict(BTreeMap<String, Obj>),
    Ref(u32),
    Op(String),
}

impl Obj {
    fn dict(&self) -> &BTreeMap<String, Obj> {
        match self {
            Obj::Dict(d) => d,
            o => panic!("not a dictionary: {o:?}"),
        }
    }

    fn array(&self) -> &[Obj] {
        match self {
            Obj::Array(a) => a,
            o => panic!("not an array: {o:?}"),
        }
    }

    fn num(&self) -> i64 {
        match self {
            Obj::Num(n) => *n,
            o => panic!("not a number: {o:?}"),
        }
    }

    fn int(&self) -> i64 {
        let n = self.num();
        assert_eq!(n % MICRO, 0, "not an integer: {n}");
        n / MICRO
    }

    fn reference(&self) -> u32 {
        match self {
            Obj::Ref(r) => *r,
            o => panic!("not a reference: {o:?}"),
        }
    }

    fn name(&self) -> &str {
        match self {
            Obj::Name(n) => n,
            o => panic!("not a name: {o:?}"),
        }
    }
}

/// PDF tokens, with numbers in millionths.
struct Lexer<'a> {
    b: &'a [u8],
    i: usize,
}

fn is_space(c: u8) -> bool {
    matches!(c, 0 | b'\t' | b'\n' | 0x0c | b'\r' | b' ')
}

fn is_delim(c: u8) -> bool {
    matches!(
        c,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

/// Parses a decimal such as `-370.60547` into millionths, rounding half
/// away from zero past the sixth decimal.
fn decimal(s: &str) -> i64 {
    let (neg, digits) = match s.strip_prefix('-') {
        Some(d) => (true, d),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let (int, frac) = digits.split_once('.').unwrap_or((digits, ""));
    assert!(
        int.bytes().chain(frac.bytes()).all(|c| c.is_ascii_digit()) && !digits.is_empty(),
        "bad number {s:?}"
    );
    let mut v: i64 = if int.is_empty() {
        0
    } else {
        int.parse::<i64>().unwrap() * MICRO
    };
    let mut scale = MICRO / 10;
    for (k, c) in frac.bytes().enumerate() {
        let d = i64::from(c - b'0');
        if k < 6 {
            v += d * scale;
            scale /= 10;
        } else {
            if k == 6 && d >= 5 {
                v += 1;
            }
            break;
        }
    }
    if neg { -v } else { v }
}

impl<'a> Lexer<'a> {
    fn new(b: &'a [u8]) -> Lexer<'a> {
        Lexer { b, i: 0 }
    }

    fn skip_space(&mut self) {
        while self.i < self.b.len() && is_space(self.b[self.i]) {
            self.i += 1;
        }
    }

    /// The next object or operator, or `None` at the end.
    fn next(&mut self) -> Option<Obj> {
        self.skip_space();
        let c = *self.b.get(self.i)?;
        match c {
            b'/' => {
                self.i += 1;
                let s = self.i;
                while self.i < self.b.len()
                    && !is_space(self.b[self.i])
                    && !is_delim(self.b[self.i])
                {
                    self.i += 1;
                }
                Some(Obj::Name(
                    String::from_utf8(self.b[s..self.i].to_vec()).unwrap(),
                ))
            }
            b'(' => Some(Obj::Str(self.literal())),
            b'<' if self.b.get(self.i + 1) == Some(&b'<') => {
                self.i += 2;
                let mut d = BTreeMap::new();
                loop {
                    let key = match self.next() {
                        Some(Obj::Name(n)) => n,
                        Some(Obj::Op(o)) if o == ">>" => return Some(Obj::Dict(d)),
                        o => panic!("dictionary key {o:?}"),
                    };
                    let value = self.object();
                    d.insert(key, value);
                }
            }
            b'<' => {
                self.i += 1;
                let s = self.i;
                while self.b[self.i] != b'>' {
                    self.i += 1;
                }
                let hex: Vec<u8> = self.b[s..self.i]
                    .iter()
                    .copied()
                    .filter(|c| !is_space(*c))
                    .collect();
                self.i += 1;
                Some(Obj::Str(hex_bytes(&hex)))
            }
            b'[' => {
                self.i += 1;
                let mut a = Vec::new();
                loop {
                    match self.object() {
                        Obj::Op(o) if o == "]" => return Some(Obj::Array(a)),
                        o => a.push(o),
                    }
                }
            }
            b'>' if self.b.get(self.i + 1) == Some(&b'>') => {
                self.i += 2;
                Some(Obj::Op(">>".into()))
            }
            b']' => {
                self.i += 1;
                Some(Obj::Op("]".into()))
            }
            _ => {
                let s = self.i;
                while self.i < self.b.len()
                    && !is_space(self.b[self.i])
                    && !is_delim(self.b[self.i])
                {
                    self.i += 1;
                }
                assert!(self.i > s, "unexpected byte {c:#04x} at {s}");
                let word = std::str::from_utf8(&self.b[s..self.i]).unwrap();
                if word
                    .bytes()
                    .all(|c| c.is_ascii_digit() || matches!(c, b'.' | b'-' | b'+'))
                {
                    Some(Obj::Num(decimal(word)))
                } else {
                    Some(Obj::Op(word.to_string()))
                }
            }
        }
    }

    /// An object, folding `n 0 R` into a reference.
    fn object(&mut self) -> Obj {
        let o = self.next().expect("an object");
        if let Obj::Num(n) = o
            && n % MICRO == 0
        {
            let save = self.i;
            if let Some(Obj::Num(0)) = self.next()
                && let Some(Obj::Op(r)) = self.next()
                && r == "R"
            {
                return Obj::Ref(u32::try_from(n / MICRO).unwrap());
            }
            self.i = save;
        }
        o
    }

    fn literal(&mut self) -> Vec<u8> {
        self.i += 1;
        let mut out = Vec::new();
        let mut depth = 0;
        loop {
            let c = self.b[self.i];
            self.i += 1;
            match c {
                b'(' => {
                    depth += 1;
                    out.push(c);
                }
                b')' if depth == 0 => return out,
                b')' => {
                    depth -= 1;
                    out.push(c);
                }
                b'\\' => {
                    let e = self.b[self.i];
                    self.i += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'0'..=b'7' => {
                            let mut v = u32::from(e - b'0');
                            for _ in 0..2 {
                                match self.b[self.i] {
                                    d @ b'0'..=b'7' => {
                                        v = v * 8 + u32::from(d - b'0');
                                        self.i += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push((v & 0xff) as u8);
                        }
                        b'\n' => {}
                        other => out.push(other),
                    }
                }
                _ => out.push(c),
            }
        }
    }
}

fn hex_bytes(hex: &[u8]) -> Vec<u8> {
    let digit = |c: u8| match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => panic!("bad hex digit {c:#04x}"),
    };
    hex.chunks(2)
        .map(|p| digit(p[0]) * 16 + p.get(1).map_or(0, |c| digit(*c)))
        .collect()
}

/// An indirect object: its value and, for a stream, its decoded data.
struct Indirect {
    value: Obj,
    stream: Option<Vec<u8>>,
}

/// A parsed PDF: its objects by number and its trailer.
pub struct Pdf {
    objects: BTreeMap<u32, Indirect>,
    trailer: BTreeMap<String, Obj>,
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

impl Pdf {
    /// Parses a PDF through its cross-reference table, checking that every
    /// entry points at the object it names.
    pub fn parse(bytes: &[u8]) -> Pdf {
        assert!(bytes.starts_with(b"%PDF-1."), "not a PDF");
        let sx = bytes
            .windows(9)
            .rposition(|w| w == b"startxref")
            .expect("startxref");
        let mut lx = Lexer::new(&bytes[sx + 9..]);
        let xref = usize::try_from(lx.next().unwrap().int()).unwrap();
        assert!(bytes[xref..].starts_with(b"xref"), "startxref is wrong");
        let mut lx = Lexer::new(&bytes[xref + 4..]);
        let first = lx.next().unwrap().int();
        let count = usize::try_from(lx.next().unwrap().int()).unwrap();
        assert_eq!(first, 0);
        lx.skip_space();
        let table = xref + 4 + lx.i;
        let mut objects = BTreeMap::new();
        for n in 1..count {
            let e = &bytes[table + 20 * n..table + 20 * n + 20];
            assert_eq!(&e[17..20], b"n\r\n", "xref entry {n}");
            let at: usize = std::str::from_utf8(&e[..10]).unwrap().parse().unwrap();
            let head = format!("{n} 0 obj\n");
            assert!(
                bytes[at..].starts_with(head.as_bytes()),
                "xref entry {n} points at the wrong place"
            );
            let mut lx = Lexer::new(&bytes[at + head.len()..]);
            let value = lx.object();
            let stream = match lx.next() {
                Some(Obj::Op(k)) if k == "endobj" => None,
                Some(Obj::Op(k)) if k == "stream" => {
                    let s = at + head.len() + lx.i + 1;
                    let len = usize::try_from(value.dict()["Length"].int()).unwrap();
                    let raw = &bytes[s..s + len];
                    assert!(bytes[s + len..].starts_with(b"\nendstream\nendobj"));
                    Some(match value.dict().get("Filter") {
                        None => raw.to_vec(),
                        Some(f) => {
                            assert_eq!(f.name(), "FlateDecode");
                            miniz_oxide::inflate::decompress_to_vec_zlib(raw).unwrap()
                        }
                    })
                }
                o => panic!("object {n} ends with {o:?}"),
            };
            objects.insert(u32::try_from(n).unwrap(), Indirect { value, stream });
        }
        let t = find(bytes, b"trailer", table + 20 * count).expect("trailer");
        let trailer = Lexer::new(&bytes[t + 7..]).object().dict().clone();
        assert_eq!(
            trailer["Size"].int(),
            i64::try_from(count).unwrap(),
            "trailer /Size"
        );
        Pdf { objects, trailer }
    }

    fn get(&self, r: u32) -> &Obj {
        &self.objects[&r].value
    }

    fn stream(&self, r: u32) -> &[u8] {
        self.objects[&r].stream.as_deref().expect("a stream")
    }

    /// The page dictionaries, in order.
    fn pages(&self) -> Vec<&BTreeMap<String, Obj>> {
        let root = self.get(self.trailer["Root"].reference()).dict();
        let tree = self.get(root["Pages"].reference()).dict();
        let kids: Vec<&BTreeMap<String, Obj>> = tree["Kids"]
            .array()
            .iter()
            .map(|k| self.get(k.reference()).dict())
            .collect();
        assert_eq!(tree["Count"].int(), i64::try_from(kids.len()).unwrap());
        for k in &kids {
            assert_eq!(k["Type"].name(), "Page");
        }
        kids
    }

    /// The number of pages.
    pub fn page_count(&self) -> usize {
        self.pages().len()
    }

    /// What a page resource font needs for positioning and text.
    fn font(&self, r: u32) -> Font {
        let t0 = self.get(r).dict();
        assert_eq!(t0["Subtype"].name(), "Type0");
        assert_eq!(t0["Encoding"].name(), "Identity-H");
        let cid = self
            .get(t0["DescendantFonts"].array()[0].reference())
            .dict();
        let dw = cid.get("DW").map_or(1000, Obj::int);
        let mut widths = BTreeMap::new();
        if let Some(w) = cid.get("W") {
            let w = w.array();
            let mut k = 0;
            while k < w.len() {
                let start = w[k].int();
                if let Obj::Array(ws) = &w[k + 1] {
                    for (j, v) in ws.iter().enumerate() {
                        widths.insert(start + j as i64, v.int());
                    }
                    k += 2;
                } else {
                    for c in start..=w[k + 1].int() {
                        widths.insert(c, w[k + 2].int());
                    }
                    k += 3;
                }
            }
        }
        let cmap = String::from_utf8(self.stream(t0["ToUnicode"].reference()).to_vec()).unwrap();
        assert!(!cmap.contains("beginbfrange"), "bfrange is not read");
        let mut unicode = BTreeMap::new();
        for chunk in cmap.split("beginbfchar").skip(1) {
            let pairs = chunk.split("endbfchar").next().unwrap();
            let hexes: Vec<&str> = pairs
                .split(['<', '>'])
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            for p in hexes.chunks(2) {
                let code = i64::from_str_radix(p[0], 16).unwrap();
                let units: Vec<u16> = hex_bytes(p[1].as_bytes())
                    .chunks(2)
                    .map(|b| u16::from_be_bytes([b[0], b[1]]))
                    .collect();
                unicode.insert(code, String::from_utf16(&units).unwrap());
            }
        }
        Font {
            dw,
            widths,
            unicode,
        }
    }

    /// Every glyph each page draws, in drawing order.
    pub fn glyphs(&self) -> Vec<Vec<Glyph>> {
        self.pages()
            .iter()
            .map(|page| {
                let res = page["Resources"].dict();
                let fonts: BTreeMap<String, Font> = res
                    .get("Font")
                    .map(|f| {
                        f.dict()
                            .iter()
                            .map(|(k, v)| (k.clone(), self.font(v.reference())))
                            .collect()
                    })
                    .unwrap_or_default();
                run_content(self.stream(page["Contents"].reference()), &fonts)
            })
            .collect()
    }
}

struct Font {
    /// Default width, in 1/1000 em.
    dw: i64,
    widths: BTreeMap<i64, i64>,
    unicode: BTreeMap<i64, String>,
}

/// A glyph as a viewer places it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Glyph {
    /// The page resource name of its font, such as `F0`.
    pub font: String,
    /// Its text, from the font's ToUnicode map.
    pub text: String,
    /// Origin from the page's bottom-left corner, in millionths of a point.
    pub x: i64,
    pub y: i64,
    /// Its declared width, in 1/1000 em.
    pub width: i64,
    /// False for rotated or skewed text, such as the watermark.
    pub upright: bool,
}

#[derive(Clone)]
struct TextState {
    font: Option<String>,
    size: i64,
    tc: i64,
}

fn run_content(content: &[u8], fonts: &BTreeMap<String, Font>) -> Vec<Glyph> {
    let mut out = Vec::new();
    let mut lx = Lexer::new(content);
    let mut args: Vec<Obj> = Vec::new();
    let mut ts = TextState {
        font: None,
        size: 0,
        tc: 0,
    };
    let mut saved: Vec<TextState> = Vec::new();
    // Text matrix [a b c d e f], in millionths.
    let mut tm = [MICRO, 0, 0, MICRO, 0, 0];
    while let Some(o) = lx.next() {
        let Obj::Op(op) = o else {
            args.push(o);
            continue;
        };
        match op.as_str() {
            "q" => saved.push(ts.clone()),
            "Q" => ts = saved.pop().expect("Q without q"),
            "BT" => tm = [MICRO, 0, 0, MICRO, 0, 0],
            "Tf" => {
                ts.font = Some(args[0].name().to_string());
                ts.size = args[1].num();
            }
            "Tc" => ts.tc = args[0].num(),
            "Tm" => {
                for (k, a) in args.iter().enumerate() {
                    tm[k] = a.num();
                }
            }
            "Tj" | "TJ" => {
                let font = &fonts[ts.font.as_deref().expect("Tf before text")];
                let items: Vec<Obj> = if op == "Tj" {
                    vec![args[0].clone()]
                } else {
                    args[0].array().to_vec()
                };
                for item in items {
                    let advance = match item {
                        Obj::Str(codes) => {
                            assert_eq!(codes.len() % 2, 0, "two bytes per glyph");
                            for c in codes.chunks(2) {
                                let code = i64::from(u16::from_be_bytes([c[0], c[1]]));
                                let width = font.widths.get(&code).copied().unwrap_or(font.dw);
                                out.push(Glyph {
                                    font: ts.font.clone().unwrap(),
                                    text: font.unicode.get(&code).cloned().unwrap_or_default(),
                                    x: tm[4],
                                    y: tm[5],
                                    width,
                                    upright: tm[0] > 0
                                        && tm[0] == tm[3]
                                        && tm[1] == 0
                                        && tm[2] == 0,
                                });
                                let tx = width * ts.size / 1000 + ts.tc;
                                tm[4] += tx * tm[0] / MICRO;
                                tm[5] += tx * tm[1] / MICRO;
                            }
                            continue;
                        }
                        // A TJ adjustment moves back by n/1000 em.
                        Obj::Num(n) => -n * ts.size / (1000 * MICRO),
                        o => panic!("bad TJ item {o:?}"),
                    };
                    tm[4] += advance * tm[0] / MICRO;
                    tm[5] += advance * tm[1] / MICRO;
                }
            }
            "ET" | "gs" | "g" | "re" | "f" => {}
            other => panic!("operator {other} is not read"),
        }
        args.clear();
    }
    assert!(args.is_empty(), "operands left at the end");
    out
}

/// A glyph the layout says the PDF must draw.
#[derive(Clone, Debug)]
struct Want {
    text: char,
    face: usize,
    /// Required origin, or `None` for a mark drawn after its base, whose
    /// zero width puts it wherever its base ended.
    x: Option<i64>,
    y: i64,
    line: String,
}

/// True when the face's glyph for `c` has an advance.
fn has_advance(face: usize, c: char) -> bool {
    let f = ttf_parser::Face::parse(coldopen_fonts::FACES[face].data, 0).unwrap();
    f.glyph_index(c)
        .and_then(|g| f.glyph_hor_advance(g))
        .is_some_and(|a| a != 0)
}

/// The glyphs each page of `layout` must draw, from §11 and §12: a cluster
/// whose first character, or a later one with a width, has no glyph prints
/// one box (U+FFFD) per cell; otherwise each character prints at its cell,
/// a zero-width mark rides on its base, a missing mark is left out, and a
/// zero-width character whose glyph has an advance (the soft hyphen) draws
/// nothing.
fn wanted(layout: &Layout, fonts: &Fonts) -> Vec<Vec<Want>> {
    let f = layout.format();
    let pitch = i64::from(f.pitch.0) * 10_000;
    let mut pages = Vec::new();
    for page in layout.title.iter().chain(&layout.pages) {
        let mut want = Vec::new();
        for line in &page.lines {
            let y = i64::from(f.page_height.0 - line.y.0) * 10_000;
            for (text, cell, cl) in line.cluster_iter() {
                let x0 = i64::from(line.x.0) * 10_000 + i64::from(cell) * pitch;
                let face = usize::from(cl.look.bold) + 2 * usize::from(cl.look.italic);
                let mut push = |text: char, x: Option<i64>| {
                    want.push(Want {
                        text,
                        face,
                        x,
                        y,
                        line: line.text.clone(),
                    });
                };
                let width = |c: char| UnicodeWidthChar::width(c).unwrap_or(0);
                let has = |c: char| fonts.has_glyph(c, cl.look);
                let chars: Vec<char> = text.chars().collect();
                let boxed = chars
                    .iter()
                    .enumerate()
                    .any(|(i, &c)| !has(c) && (i == 0 || width(c) > 0));
                if boxed {
                    for k in 0..i64::from(cl.cells) {
                        push('\u{FFFD}', Some(x0 + k * pitch));
                    }
                    continue;
                }
                let (mut at, mut next) = (0i64, 0i64);
                for (i, &c) in chars.iter().enumerate() {
                    let w = width(c) as i64;
                    if i == 0 {
                        next = w;
                    } else if w == 0 {
                        if !has(c) {
                            continue;
                        }
                    } else {
                        at = next;
                        next += w;
                    }
                    if w == 0 && has_advance(face, c) {
                        continue;
                    }
                    let x = (i == 0 || w > 0).then_some(x0 + at * pitch);
                    push(c, x);
                }
            }
        }
        pages.push(want);
    }
    pages
}

/// Checks that the PDF draws exactly the glyphs the layout placed, each
/// one in its face at its grid position, and nothing else apart from the
/// watermark. A layout with no pages must give one blank page.
pub fn assert_draws_layout(bytes: &[u8], layout: &Layout, fonts: &Fonts) {
    let pdf = Pdf::parse(bytes);
    let want = wanted(layout, fonts);
    let got = pdf.glyphs();
    assert_eq!(got.len(), want.len().max(1), "page count");
    let none = Vec::new();
    for (p, page) in got.iter().enumerate() {
        let want = want.get(p).unwrap_or(&none);
        let got: Vec<&Glyph> = page.iter().filter(|g| g.upright).collect();
        for (k, w) in want.iter().enumerate() {
            let Some(g) = got.get(k) else {
                panic!(
                    "page {}: glyph {k} {:?} of line {:?} is not drawn",
                    p + 1,
                    w.text,
                    w.line
                );
            };
            let at = |x: i64| format!("{:.2} pt", x as f64 / MICRO as f64);
            assert!(
                g.text == w.text.to_string()
                    && g.font == format!("F{}", w.face)
                    && g.y == w.y
                    && w.x.is_none_or(|x| x == g.x),
                "page {}, glyph {k} of line {:?}:\n  want {:?} in F{} at x {}, y {}\n  got  {:?} in {} at x {}, y {}",
                p + 1,
                w.line,
                w.text,
                w.face,
                w.x.map_or("(after its base)".into(), at),
                at(w.y),
                g.text,
                g.font,
                at(g.x),
                at(g.y)
            );
        }
        assert_eq!(
            got.len(),
            want.len(),
            "page {}: the PDF draws {} glyphs, the layout places {}",
            p + 1,
            got.len(),
            want.len()
        );
    }
}

/// Glyphs drawn in rotated text, which only the watermark uses.
pub fn watermark_glyphs(bytes: &[u8]) -> Vec<Vec<Glyph>> {
    Pdf::parse(bytes)
        .glyphs()
        .into_iter()
        .map(|p| p.into_iter().filter(|g| !g.upright).collect())
        .collect()
}
