//! The page-view editor widget (docs/specs/editor.md §3, §4). It draws the
//! pages the layout engine made, exactly as they print, plus the caret,
//! selection, find matches and agent marks, and turns keys, clicks and
//! input-method events into [`PageEvent`]s. It changes nothing itself: the
//! app turns events into commands.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use coldopen_core::{BlockId, Document, View};
use coldopen_layout::{Coverage, Draft, LineKind, Look, Page, PlacedLine};
use coldopen_screenplay::Pt;
use iced::advanced::Renderer as _;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{self, Renderer as _, Text};
use iced::advanced::widget::{self, Tree, tree};
use iced::advanced::{Clipboard, Shell};
use iced::alignment;
use iced::keyboard::{self, Modifiers};
use iced::mouse;
use iced::{Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size};

use crate::editor::{Completion, Spot};
use crate::fonts;
use crate::theme::Palette;

/// Logical pixels per point at 100% zoom: a page is drawn at its paper size
/// on a 96-pixels-per-inch display (editor.md §3).
const PX_PER_PT: f32 = 96.0 / 72.0;
/// Space around and between pages, in logical pixels.
pub const MARGIN: f32 = 28.0;
pub const GAP: f32 = 24.0;
/// The autocomplete list: the height of a row and the list's width.
pub const COMPLETION_ROW: f32 = 22.0;
const COMPLETION_WIDTH: f32 = 220.0;

/// A key pressed while the page area has focus.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyPress {
    pub key: keyboard::Key,
    pub modifiers: Modifiers,
    /// The text the key produces, if any (accents, AltGr and dead keys
    /// already applied).
    pub text: Option<String>,
}

/// What the page area tells the app.
#[derive(Clone, Debug, PartialEq)]
pub enum PageEvent {
    Key(KeyPress),
    /// Text committed by an input method.
    Text(String),
    /// Text an input method is composing, or `None` when it stops.
    Preedit(Option<String>),
    /// A press on a page: the page index, the point in Pt units from the
    /// paper's top-left corner, Shift, and the click count.
    Press {
        page: usize,
        x: Pt,
        y: Pt,
        shift: bool,
        clicks: u8,
    },
    Drag {
        page: usize,
        x: Pt,
        y: Pt,
    },
    /// A click on an item of the autocomplete list.
    Pick(usize),
    /// A click on the page outside the autocomplete list, which only closes
    /// it.
    Dismiss,
}

/// Character ranges to paint behind the text, in normal-form coordinates:
/// `((element, byte), (element, byte))`, start inclusive, end exclusive.
pub type NormRange = ((usize, u32), (usize, u32));

/// Everything the widget draws, borrowed from the app.
pub struct PageView<'a, Message> {
    pub view: &'a View,
    pub doc: &'a Document,
    pub draft: Option<&'a Draft>,
    pub palette: Palette,
    pub zoom: f32,
    pub focused: bool,
    pub caret: Option<Spot>,
    pub caret_on: bool,
    pub preedit: Option<&'a str>,
    pub completion: Option<&'a Completion>,
    pub selection: Option<NormRange>,
    pub matches: &'a [NormRange],
    /// Normal-form elements whose blocks an agent changed.
    pub marked: &'a BTreeSet<usize>,
    pub coverage: &'a dyn Coverage,
    pub on_event: Box<dyn Fn(PageEvent) -> Message + 'a>,
}

#[derive(Default)]
struct State {
    pressed: Option<(usize, Point)>,
    last_click: Option<(Instant, Point, u8)>,
    /// The modifier keys held now, for Shift+click.
    modifiers: Modifiers,
    /// Glyph coverage answers, per character and look.
    cache: RefCell<Vec<(char, Look, bool)>>,
}

impl<Message> PageView<'_, Message> {
    /// Logical pixels per Pt unit (hundredths of a point).
    fn k(&self) -> f32 {
        self.zoom * PX_PER_PT / 100.0
    }

    fn pages(&self) -> Vec<&Page> {
        self.view.layout().pages.iter().collect()
    }

    fn page_size(&self) -> Size {
        let f = self.view.layout().format();
        match self.draft {
            Some(d) => Size::new(
                f.page_width.0 as f32 * self.k(),
                (d.height.0 + f.margin_top.0 + f.margin_bottom.0) as f32 * self.k(),
            ),
            None => Size::new(
                f.page_width.0 as f32 * self.k(),
                f.page_height.0 as f32 * self.k(),
            ),
        }
    }

    fn page_count(&self) -> usize {
        match self.draft {
            Some(_) => 1,
            None => self.view.layout().pages.len().max(1),
        }
    }

    /// The top-left corner of page `p`, relative to the widget.
    fn page_origin(&self, width: f32, p: usize) -> Point {
        let s = self.page_size();
        Point::new(
            ((width - s.width) / 2.0).max(MARGIN),
            MARGIN + p as f32 * (s.height + GAP),
        )
    }

    /// The page under a point, and the point in Pt units on that page.
    fn locate(&self, bounds: Rectangle, at: Point) -> Option<(usize, Pt, Pt)> {
        let s = self.page_size();
        let rel = Point::new(at.x - bounds.x, at.y - bounds.y);
        let p = ((rel.y - MARGIN) / (s.height + GAP)).floor().max(0.0) as usize;
        let p = p.min(self.page_count().saturating_sub(1));
        let o = self.page_origin(bounds.width, p);
        let k = self.k();
        Some((
            p,
            Pt(((rel.x - o.x) / k) as i32),
            Pt(((rel.y - o.y) / k) as i32),
        ))
    }

    fn has_glyph(&self, state: &State, c: char, look: Look) -> bool {
        let mut cache = state.cache.borrow_mut();
        if let Some(&(_, _, v)) = cache.iter().find(|(ch, l, _)| *ch == c && *l == look) {
            return v;
        }
        let v = self.coverage.has_glyph(c, look);
        if cache.len() > 512 {
            cache.clear();
        }
        cache.push((c, look, v));
        v
    }

    /// Where the caret's line is, in widget coordinates, for scrolling.
    pub fn caret_rect(&self, width: f32) -> Option<Rectangle> {
        let spot = self.caret?;
        let o = self.page_origin(width, spot.page);
        let k = self.k();
        let lh = self.view.layout().format().line_height.0 as f32 * k;
        Some(Rectangle::new(
            Point::new(o.x + spot.x.0 as f32 * k, o.y + spot.top.0 as f32 * k),
            Size::new(2.0, lh),
        ))
    }

    /// The autocomplete list's box, in window coordinates: under the
    /// caret's line.
    fn completion_box(&self, bounds: Rectangle) -> Option<Rectangle> {
        let c = self.completion?;
        let r = self.caret_rect(bounds.width)?;
        Some(Rectangle::new(
            Point::new(bounds.x + r.x, bounds.y + r.y + r.height + 2.0),
            Size::new(
                COMPLETION_WIDTH,
                COMPLETION_ROW * c.items.len() as f32 + 6.0,
            ),
        ))
    }

    fn in_range(range: &NormRange, elem: usize, byte: u32) -> bool {
        (elem, byte) >= range.0 && (elem, byte) < range.1
    }
}

/// The total height for `pages` pages of height `page_h`.
pub fn content_height(pages: usize, page_h: f32) -> f32 {
    2.0 * MARGIN + pages as f32 * page_h + pages.saturating_sub(1) as f32 * GAP
}

impl<Message> widget::Widget<Message, iced::Theme, iced::Renderer> for PageView<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let s = self.page_size();
        let width = limits.max().width.max(s.width + 2.0 * MARGIN);
        let width = if width.is_finite() {
            width
        } else {
            s.width + 2.0 * MARGIN
        };
        layout::Node::new(Size::new(
            width,
            content_height(self.page_count(), s.height),
        ))
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(button)) if self.completion.is_some() => {
                let Some(at) = cursor.position_over(bounds) else {
                    return;
                };
                match self.completion_box(bounds) {
                    Some(b) if b.contains(at) => {
                        let i = ((at.y - b.y - 3.0) / COMPLETION_ROW).floor();
                        if *button == mouse::Button::Left
                            && i >= 0.0
                            && (i as usize) < self.completion.map_or(0, |c| c.items.len())
                        {
                            shell.publish((self.on_event)(PageEvent::Pick(i as usize)));
                        }
                    }
                    _ => shell.publish((self.on_event)(PageEvent::Dismiss)),
                }
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(at) = cursor.position_over(bounds) else {
                    return;
                };
                let Some((page, x, y)) = self.locate(bounds, at) else {
                    return;
                };
                let now = Instant::now();
                let clicks = match state.last_click {
                    Some((t, p, n))
                        if now.duration_since(t) < Duration::from_millis(450)
                            && (p.x - at.x).abs() < 5.0
                            && (p.y - at.y).abs() < 5.0 =>
                    {
                        (n % 3) + 1
                    }
                    _ => 1,
                };
                state.last_click = Some((now, at, clicks));
                state.pressed = Some((page, at));
                let shift = state.modifiers.shift();
                shell.publish((self.on_event)(PageEvent::Press {
                    page,
                    x,
                    y,
                    shift,
                    clicks,
                }));
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.pressed.is_some()
                    && let Some(at) = cursor.position()
                    && let Some((page, x, y)) = self.locate(bounds, at)
                {
                    shell.publish((self.on_event)(PageEvent::Drag { page, x, y }));
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.pressed = None;
            }
            Event::Keyboard(keyboard::Event::ModifiersChanged(m)) => {
                state.modifiers = *m;
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers,
                text,
                ..
            }) if self.focused => {
                shell.publish((self.on_event)(PageEvent::Key(KeyPress {
                    key: key.clone(),
                    modifiers: *modifiers,
                    text: text.as_ref().map(|t| t.to_string()),
                })));
                shell.capture_event();
            }
            Event::InputMethod(ime) if self.focused => match ime {
                iced::advanced::input_method::Event::Preedit(content, _) => {
                    let p = (!content.is_empty()).then(|| content.clone());
                    shell.publish((self.on_event)(PageEvent::Preedit(p)));
                    shell.capture_event();
                }
                iced::advanced::input_method::Event::Commit(content) => {
                    shell.publish((self.on_event)(PageEvent::Preedit(None)));
                    shell.publish((self.on_event)(PageEvent::Text(content.clone())));
                    shell.capture_event();
                }
                _ => {}
            },
            Event::Window(iced::window::Event::RedrawRequested(_)) if self.focused => {
                if let Some(r) = self.caret_rect(bounds.width) {
                    let cursor =
                        Rectangle::new(Point::new(bounds.x + r.x, bounds.y + r.y), r.size());
                    shell.request_input_method(&iced::advanced::InputMethod::Enabled {
                        cursor,
                        purpose: iced::advanced::input_method::Purpose::Normal,
                        preedit: self.preedit.map(|p| iced::advanced::input_method::Preedit {
                            content: p,
                            selection: None,
                            text_size: None,
                        }),
                    });
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let over_list = cursor
            .position()
            .zip(self.completion_box(layout.bounds()))
            .is_some_and(|(at, b)| b.contains(at));
        if over_list {
            mouse::Interaction::Pointer
        } else if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &iced::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let pal = self.palette;
        let k = self.k();
        let format = self.view.layout().format();
        let lh = format.line_height.0 as f32 * k;
        let pitch = format.pitch.0 as f32 * k;
        let font_px = format.font_size.0 as f32 * k;
        let size = self.page_size();
        let clip = viewport.intersection(&bounds).unwrap_or(*viewport);

        renderer.fill_quad(
            Quad {
                bounds,
                ..Quad::default()
            },
            pal.desk,
        );

        // Tracked changes' authors, by slot (editor.md §17.1).
        let slots = self.doc.change_slots();
        let pages: Vec<&Page> = self.pages();
        for p in 0..self.page_count() {
            let o = self.page_origin(bounds.width, p);
            let rect = Rectangle::new(Point::new(bounds.x + o.x, bounds.y + o.y), size);
            if rect.intersection(&clip).is_none() {
                continue;
            }
            let edge = if pal.high_contrast { 2.0 } else { 1.0 };
            renderer.fill_quad(
                Quad {
                    bounds: rect,
                    border: Border {
                        color: pal.page_edge,
                        width: edge,
                        radius: 0.0.into(),
                    },
                    ..Quad::default()
                },
                pal.page,
            );
            let lines: Vec<&PlacedLine> = match self.draft {
                Some(d) => d.lines.iter().collect(),
                None => pages
                    .get(p)
                    .map(|pg| pg.lines.iter().collect())
                    .unwrap_or_default(),
            };
            let at = |x: Pt, y_top: f32| Point::new(rect.x + x.0 as f32 * k, rect.y + y_top);
            let mut labelled: Option<usize> = None;
            for l in &lines {
                let top = (l.y.0 - format.baseline.0) as f32 * k;
                if rect.y + top > clip.y + clip.height || rect.y + top + lh < clip.y {
                    continue;
                }
                let elem = l.source.map(|(e, _, _)| e);
                // Agent marks in the left margin.
                if let Some(e) = elem
                    && self.marked.contains(&e)
                    && matches!(l.kind, LineKind::Content(_))
                {
                    renderer.fill_quad(
                        Quad {
                            bounds: Rectangle::new(
                                Point::new(
                                    rect.x + 0.55 * 72.0 * PX_PER_PT * self.zoom,
                                    rect.y + top,
                                ),
                                Size::new(3.0, lh),
                            ),
                            ..Quad::default()
                        },
                        pal.agent,
                    );
                }
                // Tracked changes in the left margin: a bar on every line
                // holding a mark, and a label on the first line of a block
                // with a structural mark (editor.md §17.1).
                if matches!(l.kind, LineKind::Content(_)) {
                    let inch = 72.0 * PX_PER_PT * self.zoom;
                    if let Some(c) = l.clusters.iter().find_map(|c| c.look.change) {
                        renderer.fill_quad(
                            Quad {
                                bounds: Rectangle::new(
                                    Point::new(rect.x + 0.45 * inch, rect.y + top),
                                    Size::new(3.0, lh),
                                ),
                                ..Quad::default()
                            },
                            change_color(&pal, &slots, c),
                        );
                    }
                    if let Some(e) = elem
                        && labelled != Some(e)
                    {
                        labelled = Some(e);
                        if let Some((label, c)) = self.block_label(e) {
                            let w = 0.95 * inch;
                            let at = Point::new(rect.x + 0.45 * inch - w - 4.0, rect.y + top);
                            let mut t = page_text(&label, font_px * 0.62, lh, fonts::UI, w);
                            t.align_x = text::Alignment::Right;
                            renderer.fill_text(
                                t,
                                Point::new(at.x + w, at.y),
                                change_color(&pal, &slots, c),
                                text_clip(at, w, lh),
                            );
                        }
                    }
                }
                // Selection and find matches, behind the text.
                if let Some(e) = elem {
                    let mut cell = 0i32;
                    for c in &l.clusters {
                        let w = i32::from(c.cells);
                        let selected = self.selection.is_some_and(|r| Self::in_range(&r, e, c.src));
                        let found = self.matches.iter().any(|r| Self::in_range(r, e, c.src));
                        if selected || found {
                            let x = at(Pt(l.x.0 + cell * format.pitch.0), top);
                            renderer.fill_quad(
                                Quad {
                                    bounds: Rectangle::new(x, Size::new(w as f32 * pitch, lh)),
                                    ..Quad::default()
                                },
                                if selected { pal.selection } else { pal.find },
                            );
                        }
                        cell += w;
                    }
                }
                let color = match l.kind {
                    LineKind::PageNumber | LineKind::SceneNumber => pal.ink_quiet,
                    _ => pal.ink,
                };
                self.draw_line(
                    renderer,
                    state,
                    l,
                    at(l.x, top),
                    (pitch, lh, font_px),
                    color,
                    &slots,
                );
            }
        }

        // The caret, and input-method text being composed.
        if let Some(spot) = self.caret {
            let o = self.page_origin(bounds.width, spot.page);
            let x = bounds.x + o.x + spot.x.0 as f32 * k;
            let y = bounds.y + o.y + spot.top.0 as f32 * k;
            if let Some(pre) = self.preedit {
                let w = pre.chars().count() as f32 * pitch;
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle::new(Point::new(x, y), Size::new(w, lh)),
                        ..Quad::default()
                    },
                    pal.page,
                );
                renderer.fill_text(
                    page_text(pre, font_px, lh, fonts::PAGE, w + pitch),
                    Point::new(x, y),
                    pal.ink,
                    text_clip(Point::new(x, y), w + pitch, lh),
                );
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle::new(Point::new(x, y + lh - 2.0), Size::new(w, 1.0)),
                        ..Quad::default()
                    },
                    pal.caret,
                );
            } else if self.focused && self.caret_on {
                let w = if pal.high_contrast { 3.0 } else { 2.0 };
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle::new(Point::new(x - w / 2.0, y), Size::new(w, lh)),
                        ..Quad::default()
                    },
                    pal.caret,
                );
            }
            if let (Some(c), Some(b)) = (self.completion, self.completion_box(bounds)) {
                // Its own layer, so the page text under it does not show
                // through: a layer draws its quads before its text.
                renderer.with_layer(clip, |r| {
                    self.draw_completion(r, c, b.position());
                });
            }
        }
    }
}

/// The clip for one text call: its own box. iced's CPU renderer clips a
/// text only when its clip rectangle reaches past the layer it is drawn
/// in, so passing the visible area would let a line straddling the top or
/// bottom of the pages draw over the bars around them.
fn text_clip(at: Point, width: f32, height: f32) -> Rectangle {
    Rectangle::new(at, Size::new(width, height))
}

fn page_text(content: &str, font_px: f32, lh: f32, font: Font, width: f32) -> Text<String, Font> {
    Text {
        content: content.to_string(),
        bounds: Size::new(width, lh),
        size: Pixels(font_px),
        line_height: text::LineHeight::Absolute(Pixels(lh)),
        font,
        align_x: text::Alignment::Left,
        align_y: alignment::Vertical::Top,
        shaping: text::Shaping::Advanced,
        wrapping: text::Wrapping::None,
    }
}

impl<Message> PageView<'_, Message> {
    /// Draws a placed line cluster by cluster on the grid: runs of
    /// one-cell clusters in one style go in one text call; wide clusters
    /// and missing glyphs are placed on their own cells.
    /// The label a laid-out element's block shows in the margin for its
    /// structural tracked change, with that change.
    fn block_label(&self, elem: usize) -> Option<(String, u64)> {
        let bi = *self.view.map.get(elem)?;
        let m = &self.doc.blocks.get(bi)?.marks;
        if let Some(c) = m.deleted {
            return Some(("Deleted".into(), c.0));
        }
        if let Some(c) = m.inserted {
            return Some(("New".into(), c.0));
        }
        if let Some(r) = m.retyped.first() {
            return Some((format!("Was {}", kind_title(r.from)), r.change.0));
        }
        if let Some(c) = m.split {
            return Some(("Split".into(), c.0));
        }
        if let Some(c) = m.joined {
            return Some(("Joined".into(), c.0));
        }
        if let Some(mv) = m.moved.first() {
            return Some(("Moved".into(), mv.change.0));
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_line(
        &self,
        renderer: &mut iced::Renderer,
        state: &State,
        line: &PlacedLine,
        origin: Point,
        (pitch, lh, font_px): (f32, f32, f32),
        color: Color,
        slots: &BTreeMap<u64, u8>,
    ) {
        let pal = self.palette;
        let ink = |look: Look| match look.change {
            Some(c) => change_color(&pal, slots, c),
            None => color,
        };
        let mut run = String::new();
        let mut run_cell = 0i32;
        let mut run_look = Look::default();
        let mut cell = 0i32;
        let flush = |renderer: &mut iced::Renderer, run: &mut String, run_cell: i32, look: Look| {
            if run.is_empty() {
                return;
            }
            let color = ink(look);
            let w = run.chars().count() as f32 * pitch;
            let at = Point::new(origin.x + run_cell as f32 * pitch, origin.y);
            renderer.fill_text(
                page_text(
                    run,
                    font_px,
                    lh,
                    fonts::page(look.bold, look.italic),
                    w + pitch,
                ),
                at,
                color,
                text_clip(at, w + pitch, lh),
            );
            let thick = (font_px / 16.0).max(1.0);
            if look.underline {
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle::new(
                            Point::new(at.x, at.y + lh * 0.86),
                            Size::new(run_cell_width(run, pitch), thick),
                        ),
                        ..Quad::default()
                    },
                    color,
                );
            }
            if look.strike {
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle::new(
                            Point::new(at.x, at.y + lh * 0.52),
                            Size::new(run_cell_width(run, pitch), thick),
                        ),
                        ..Quad::default()
                    },
                    color,
                );
            }
            run.clear();
        };
        for (s, start, c) in line.cluster_iter() {
            let first = s.chars().next().unwrap_or(' ');
            let missing = !self.has_glyph(state, first, c.look);
            let simple = c.cells == 1 && s.chars().count() == 1 && !missing;
            if simple && (run.is_empty() || c.look == run_look) {
                if run.is_empty() {
                    run_cell = start as i32;
                    run_look = c.look;
                }
                run.push_str(s);
                cell = start as i32 + 1;
                continue;
            }
            flush(renderer, &mut run, run_cell, run_look);
            if simple {
                run_cell = start as i32;
                run_look = c.look;
                run.push_str(s);
                cell = start as i32 + 1;
                continue;
            }
            let at = Point::new(origin.x + start as f32 * pitch, origin.y);
            let w = f32::from(c.cells) * pitch;
            if missing {
                // The same box the PDF prints for a missing glyph.
                for k in 0..c.cells {
                    renderer.fill_quad(
                        Quad {
                            bounds: Rectangle::new(
                                Point::new(
                                    at.x + f32::from(k) * pitch + pitch * 0.15,
                                    at.y + lh * 0.2,
                                ),
                                Size::new(pitch * 0.7, lh * 0.6),
                            ),
                            border: Border {
                                color: ink(c.look),
                                width: 1.0,
                                radius: 0.0.into(),
                            },
                            ..Quad::default()
                        },
                        Color::TRANSPARENT,
                    );
                }
            } else {
                renderer.fill_text(
                    page_text(
                        s,
                        font_px,
                        lh,
                        fonts::page(c.look.bold, c.look.italic),
                        w + pitch,
                    ),
                    at,
                    ink(c.look),
                    text_clip(at, w + pitch, lh),
                );
            }
            cell = start as i32 + i32::from(c.cells);
        }
        flush(renderer, &mut run, run_cell, run_look);
        let _ = cell;
    }

    fn draw_completion(&self, renderer: &mut iced::Renderer, c: &Completion, at: Point) {
        let pal = self.palette;
        let row = COMPLETION_ROW;
        let w = COMPLETION_WIDTH;
        let h = row * c.items.len() as f32 + 6.0;
        renderer.fill_quad(
            Quad {
                bounds: Rectangle::new(at, Size::new(w, h)),
                border: Border {
                    color: pal.border,
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..Quad::default()
            },
            pal.panel_alt,
        );
        for (i, item) in c.items.iter().enumerate() {
            let y = at.y + 3.0 + i as f32 * row;
            if i == c.selected {
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle::new(Point::new(at.x + 3.0, y), Size::new(w - 6.0, row)),
                        border: Border {
                            radius: 4.0.into(),
                            ..Border::default()
                        },
                        ..Quad::default()
                    },
                    pal.accent_soft,
                );
            }
            renderer.fill_text(
                Text {
                    content: item.clone(),
                    bounds: Size::new(w - 16.0, row),
                    size: Pixels(13.0),
                    line_height: text::LineHeight::Absolute(Pixels(row)),
                    font: fonts::UI,
                    align_x: text::Alignment::Left,
                    align_y: alignment::Vertical::Center,
                    shaping: text::Shaping::Advanced,
                    wrapping: text::Wrapping::None,
                },
                Point::new(at.x + 10.0, y + row / 2.0),
                if i == c.selected {
                    pal.accent_text
                } else {
                    pal.text
                },
                text_clip(Point::new(at.x + 10.0, y), w - 16.0, row),
            );
        }
    }
}

fn run_cell_width(run: &str, pitch: f32) -> f32 {
    run.chars().count() as f32 * pitch
}

impl<'a, Message: 'a> From<PageView<'a, Message>> for Element<'a, Message> {
    fn from(p: PageView<'a, Message>) -> Element<'a, Message> {
        Element::new(p)
    }
}

/// Block ids to normal-form element indices, for marks.
/// The screen color of a tracked change's author (editor.md §17.1).
pub fn change_color(pal: &Palette, slots: &BTreeMap<u64, u8>, change: u64) -> Color {
    let slot = slots.get(&change).copied().unwrap_or(0);
    pal.changes[usize::from(slot) % pal.changes.len()]
}

/// An element kind as the margin label says it.
pub fn kind_title(k: coldopen_screenplay::ElementKind) -> &'static str {
    use coldopen_screenplay::ElementKind::*;
    match k {
        SceneHeading => "Scene heading",
        Action => "Action",
        Character => "Character",
        Parenthetical => "Parenthetical",
        Dialogue => "Dialogue",
        Transition => "Transition",
        Lyric => "Lyric",
        Shot => "Shot",
        General => "General",
        PageBreak => "Page break",
        Section => "Section",
        Synopsis => "Synopsis",
        Note => "Note",
        Boneyard => "Boneyard",
    }
}

pub fn marked_elements(view: &View, doc: &Document, blocks: &BTreeSet<BlockId>) -> BTreeSet<usize> {
    let mut out = BTreeSet::new();
    for id in blocks {
        if let Some(bi) = doc.index_of(*id) {
            for (k, e) in view.sources.elements.iter().enumerate() {
                if e.source == bi || e.segments.iter().any(|s| s.source == bi) {
                    out.insert(k);
                }
            }
        }
    }
    out
}
