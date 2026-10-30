//! The report view's rows as one widget (editor.md §20; phase 6a): as tall
//! as every row together, inside the view's scrolling area, it draws only
//! the lines in sight, one text call each, and the selected row's box. A
//! click names the row under it.
//!
//! Before, each row was a button holding a text, built again for every
//! scroll step: CI's CPU renderer took 50 to 81 ms a frame to scroll it,
//! against 11 to 18 ms for the pages, which the page view draws this way.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{self, Renderer as _, Text};
use iced::advanced::widget::{self, Tree};
use iced::advanced::{Clipboard, Renderer as _, Shell};
use iced::{
    Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size, alignment, mouse,
};

/// The report's lines, one per row, with the selected one boxed.
pub struct ReportLines<'a, Message> {
    lines: &'a [String],
    selected: usize,
    row_h: f32,
    font: Font,
    size: f32,
    /// Space before each line's text.
    pad_x: f32,
    color: Color,
    selected_fill: Color,
    selected_edge: Color,
    on_press: fn(usize) -> Message,
}

impl<'a, Message> ReportLines<'a, Message> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        lines: &'a [String],
        selected: usize,
        row_h: f32,
        font: Font,
        size: f32,
        color: Color,
        (selected_fill, selected_edge): (Color, Color),
        on_press: fn(usize) -> Message,
    ) -> Self {
        ReportLines {
            lines,
            selected,
            row_h,
            font,
            size,
            pad_x: 6.0,
            color,
            selected_fill,
            selected_edge,
            on_press,
        }
    }

    /// The row at `y` within the widget, if there is one.
    fn row_at(&self, bounds: Rectangle, y: f32) -> Option<usize> {
        let i = ((y - bounds.y) / self.row_h).floor();
        (i >= 0.0 && (i as usize) < self.lines.len()).then_some(i as usize)
    }
}

impl<Message> widget::Widget<Message, iced::Theme, iced::Renderer> for ReportLines<'_, Message> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let width = limits.max().width;
        let width = if width.is_finite() { width } else { 0.0 };
        layout::Node::new(Size::new(width, self.lines.len() as f32 * self.row_h))
    }

    fn update(
        &mut self,
        _tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        if let Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event
            && let Some(at) = cursor.position_over(layout.bounds())
            && let Some(i) = self.row_at(layout.bounds(), at.y)
        {
            shell.publish((self.on_press)(i));
            shell.capture_event();
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
        match cursor.position_over(layout.bounds()) {
            Some(at) if self.row_at(layout.bounds(), at.y).is_some() => mouse::Interaction::Pointer,
            _ => mouse::Interaction::default(),
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &iced::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let Some(clip) = viewport.intersection(&bounds) else {
            return;
        };
        let n = self.lines.len();
        let first = (((clip.y - bounds.y) / self.row_h).floor().max(0.0) as usize).min(n);
        let last = (((clip.y + clip.height - bounds.y) / self.row_h)
            .ceil()
            .max(0.0) as usize)
            .min(n);
        for i in first..last {
            let top = bounds.y + i as f32 * self.row_h;
            let row = Rectangle::new(
                Point::new(bounds.x, top),
                Size::new(bounds.width, self.row_h),
            );
            if i == self.selected {
                renderer.fill_quad(
                    Quad {
                        bounds: row,
                        border: Border {
                            color: self.selected_edge,
                            width: 1.0,
                            radius: 3.0.into(),
                        },
                        ..Quad::default()
                    },
                    self.selected_fill,
                );
            }
            let line = &self.lines[i];
            if line.is_empty() {
                continue;
            }
            let at = Point::new(bounds.x + self.pad_x, top);
            let width = (bounds.width - self.pad_x).max(0.0);
            renderer.fill_text(
                Text {
                    content: line.clone(),
                    bounds: Size::new(width, self.row_h),
                    size: Pixels(self.size),
                    line_height: text::LineHeight::Absolute(Pixels(self.row_h)),
                    font: self.font,
                    align_x: text::Alignment::Left,
                    align_y: alignment::Vertical::Top,
                    shaping: text::Shaping::Basic,
                    wrapping: text::Wrapping::None,
                },
                at,
                self.color,
                Rectangle::new(at, Size::new(width, self.row_h))
                    .intersection(&clip)
                    .unwrap_or(row),
            );
        }
    }
}

impl<'a, Message: 'a> From<ReportLines<'a, Message>> for Element<'a, Message> {
    fn from(r: ReportLines<'a, Message>) -> Self {
        Element::new(r)
    }
}
