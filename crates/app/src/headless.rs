//! The app without a window. Real input events go through the real view
//! and widgets, the messages they publish go through `App::update`, and the
//! tasks `update` returns are run: their messages, widget operations (focus,
//! scrolling), clipboard and exit. Only the window itself is missing.
//!
//! The gate tests and the gate driver use it, so what they check is the app
//! a writer runs, not a model of it.

use std::collections::VecDeque;
use std::future::Future;
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll, Wake, Waker};
use std::time::Instant;

use iced::advanced::clipboard::{Clipboard, Kind};
use iced::advanced::graphics::text::font_system;
use iced::advanced::renderer::{self, Headless as _};
use iced::advanced::widget::operation::Outcome;
use iced::futures::StreamExt;
use iced::keyboard::{self, Key, Modifiers, key::Named};
use iced::theme::Base as _;
use iced::{Event, Point, Size, mouse, window};
use iced_runtime::user_interface::{self, UserInterface};
use iced_runtime::{Action, task};

use crate::app::{App, Message, iced_settings, unclaimed_key};

/// The app, headless.
pub struct Headless {
    pub app: App,
    renderer: iced::Renderer,
    cache: Option<user_interface::Cache>,
    /// The window's size in logical pixels.
    pub size: Size,
    pub cursor: mouse::Cursor,
    /// What the app last wrote to the clipboard, and what it reads.
    pub clipboard: Option<String>,
    /// The app asked to exit.
    pub exited: bool,
    window: window::Id,
}

struct Board<'a>(&'a mut Option<String>);

impl Clipboard for Board<'_> {
    fn read(&self, _kind: Kind) -> Option<String> {
        self.0.clone()
    }

    fn write(&mut self, _kind: Kind, contents: String) {
        *self.0 = Some(contents);
    }
}

/// Wakes the thread that waits in [`block_on`].
struct Unpark(std::thread::Thread);

impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

/// Runs a future to completion on this thread. Tasks here are short:
/// a save on a worker thread, a dialog answered by a script.
pub fn block_on<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::park();
    }
}

/// Loads the bundled fonts into the text system once per process, as the
/// windowed app does at start-up.
fn load_fonts() {
    static LOADED: OnceLock<()> = OnceLock::new();
    LOADED.get_or_init(|| {
        if let Ok(mut fs) = font_system().write() {
            for f in iced_settings().fonts {
                fs.load_font(f);
            }
        }
    });
}

/// A key press with the text it types, as the windowing layer reports it.
pub fn key_press(key: Key, modifiers: Modifiers, text: Option<&str>) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers,
        repeat: false,
        text: text.map(Into::into),
    })
}

pub fn key_release(key: Key, modifiers: Modifiers) -> Event {
    Event::Keyboard(keyboard::Event::KeyReleased {
        key: key.clone(),
        modified_key: key,
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers,
    })
}

/// The modifier that shortcuts use on this OS: Cmd on macOS, Ctrl
/// elsewhere.
pub fn command_modifier() -> Modifiers {
    if crate::keymap::MAC {
        Modifiers::LOGO
    } else {
        Modifiers::CTRL
    }
}

impl Headless {
    /// The app in a window of `size` logical pixels, drawn with tiny-skia.
    /// `None` when no headless renderer could be made.
    pub fn new(app: App, size: Size) -> Option<Headless> {
        Headless::with_renderer(app, size, "tiny-skia")
    }

    /// The same with a named renderer: "tiny-skia", or "wgpu" in a build
    /// with the `gpu` feature (the renderer comparison).
    pub fn with_renderer(app: App, size: Size, backend: &str) -> Option<Headless> {
        load_fonts();
        let s = iced_settings();
        let renderer = block_on(<iced::Renderer as renderer::Headless>::new(
            s.default_font,
            s.default_text_size,
            Some(backend),
        ))?;
        let mut h = Headless {
            app,
            renderer,
            cache: Some(user_interface::Cache::default()),
            size,
            cursor: mouse::Cursor::Unavailable,
            clipboard: None,
            exited: false,
            window: window::Id::unique(),
        };
        // A first frame, as a window gets before any input.
        h.events(vec![Event::Window(window::Event::RedrawRequested(
            Instant::now(),
        ))]);
        Some(h)
    }

    /// Sends events to the interface, then runs every message they cause.
    pub fn events(&mut self, events: Vec<Event>) {
        let mut messages = Vec::new();
        let cursor = self.cursor;
        let size = self.size;
        let cache = self.cache.take().unwrap_or_default();
        let mut ui = UserInterface::build(self.app.view(), size, cache, &mut self.renderer);
        let (_, statuses) = ui.update(
            &events,
            cursor,
            &mut self.renderer,
            &mut Board(&mut self.clipboard),
            &mut messages,
        );
        self.cache = Some(ui.into_cache());
        // What the app's subscription sees: events no widget captured.
        for (e, s) in events.into_iter().zip(statuses) {
            if let Some(m) = unclaimed_key(e, s, self.window) {
                messages.push(m);
            }
        }
        self.run(messages);
    }

    /// Runs messages through `update`, and the tasks they return, until
    /// nothing is left.
    pub fn run(&mut self, messages: Vec<Message>) {
        let mut queue: VecDeque<Message> = messages.into();
        while let Some(m) = queue.pop_front() {
            let task = self.app.update(m);
            let Some(mut stream) = task::into_stream(task) else {
                continue;
            };
            while let Some(action) = block_on(stream.next()) {
                match action {
                    Action::Output(m) => queue.push_back(m),
                    Action::Widget(op) => self.operate(op),
                    Action::Clipboard(iced_runtime::clipboard::Action::Read {
                        channel, ..
                    }) => {
                        let _ = channel.send(self.clipboard.clone());
                    }
                    Action::Clipboard(iced_runtime::clipboard::Action::Write {
                        contents, ..
                    }) => self.clipboard = Some(contents),
                    Action::LoadFont { channel, .. } => {
                        let _ = channel.send(Ok(()));
                    }
                    Action::Exit => self.exited = true,
                    Action::Window(_) | Action::System(_) | Action::Image(_) | Action::Reload => {}
                }
            }
        }
    }

    /// Runs whatever the agent link has delivered, as the windowed app's
    /// subscription would. Returns how many events there were.
    pub fn pump(&mut self) -> usize {
        let m = self.app.link_messages();
        let n = m.len();
        self.run(m);
        n
    }

    fn operate(&mut self, op: Box<dyn iced::advanced::widget::Operation>) {
        let mut current = Some(op);
        while let Some(mut op) = current.take() {
            let cache = self.cache.take().unwrap_or_default();
            let mut ui =
                UserInterface::build(self.app.view(), self.size, cache, &mut self.renderer);
            ui.operate(&self.renderer, op.as_mut());
            self.cache = Some(ui.into_cache());
            if let Outcome::Chain(next) = op.finish() {
                current = Some(next);
            }
        }
    }

    /// Presses and releases a key.
    pub fn key(&mut self, key: Key, modifiers: Modifiers, text: Option<&str>) {
        self.events(vec![
            key_press(key.clone(), modifiers, text),
            key_release(key, modifiers),
        ]);
    }

    pub fn named(&mut self, key: Named, modifiers: Modifiers) {
        self.key(Key::Named(key), modifiers, None);
    }

    /// A shortcut with the OS's command modifier, such as Mod+S.
    pub fn shortcut(&mut self, c: char, shift: bool) {
        let mut m = command_modifier();
        if shift {
            m |= Modifiers::SHIFT;
        }
        let s = c.to_string();
        self.key(Key::Character(s.as_str().into()), m, None);
    }

    /// Moves the caret to the start of the script with this OS's keys
    /// (editor.md §4): Cmd+Up on macOS, Ctrl+Home elsewhere.
    pub fn doc_start(&mut self) {
        if crate::keymap::MAC {
            self.named(Named::ArrowUp, Modifiers::LOGO);
        } else {
            self.named(Named::Home, Modifiers::CTRL);
        }
    }

    /// Moves the caret to the end of the script with this OS's keys
    /// (editor.md §4): Cmd+Down on macOS, Ctrl+End elsewhere.
    pub fn doc_end(&mut self) {
        if crate::keymap::MAC {
            self.named(Named::ArrowDown, Modifiers::LOGO);
        } else {
            self.named(Named::End, Modifiers::CTRL);
        }
    }

    /// Types text one key at a time, as a keyboard does. Each character is
    /// one key press that carries the character as its text, which is how
    /// accented letters arrive from a keyboard layout or dead keys.
    pub fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            match c {
                '\n' => self.named(Named::Enter, Modifiers::empty()),
                '\t' => self.named(Named::Tab, Modifiers::empty()),
                c => {
                    let s = c.to_string();
                    let shift = if c.is_uppercase() {
                        Modifiers::SHIFT
                    } else {
                        Modifiers::empty()
                    };
                    let key = Key::Character(s.to_lowercase().as_str().into());
                    self.key(key, shift, Some(&s));
                }
            }
        }
    }

    /// Text committed by an input method, such as an IME or the macOS
    /// accent menu.
    pub fn commit(&mut self, text: &str) {
        self.events(vec![Event::InputMethod(
            iced::advanced::input_method::Event::Commit(text.to_string()),
        )]);
    }

    /// Moves the mouse and clicks at a point in the window.
    pub fn click(&mut self, at: Point) {
        self.cursor = mouse::Cursor::Available(at);
        self.events(vec![
            Event::Mouse(mouse::Event::CursorMoved { position: at }),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ]);
    }

    /// Draws a frame and returns it as RGBA pixels, `scale` physical pixels
    /// per logical one, with its size in physical pixels.
    pub fn frame(&mut self, scale: f32) -> (Size<u32>, Vec<u8>) {
        let theme = self.app.theme();
        let base = theme.base();
        let cache = self.cache.take().unwrap_or_default();
        let mut ui = UserInterface::build(self.app.view(), self.size, cache, &mut self.renderer);
        let mut messages = Vec::new();
        let _ = ui.update(
            &[Event::Window(
                window::Event::RedrawRequested(Instant::now()),
            )],
            self.cursor,
            &mut self.renderer,
            &mut Board(&mut self.clipboard),
            &mut messages,
        );
        ui.draw(
            &mut self.renderer,
            &theme,
            &renderer::Style {
                text_color: base.text_color,
            },
            self.cursor,
        );
        self.cache = Some(ui.into_cache());
        let physical = Size::new(
            (self.size.width * scale).round() as u32,
            (self.size.height * scale).round() as u32,
        );
        let rgba = self
            .renderer
            .screenshot(physical, scale, base.background_color);
        self.run(messages);
        (physical, rgba)
    }
}
