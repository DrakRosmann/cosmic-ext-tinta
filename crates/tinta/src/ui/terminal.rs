//! The integrated terminal: alacritty's emulator, drawn with the same grid approach as the editor.

use crate::app::Message;
use crate::bus;
use alacritty_terminal::event::{Event as TermEvent, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point as GridPoint, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::tty;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor, Rgb};
use cosmic::iced::advanced::layout::{self, Layout};
use cosmic::iced::advanced::renderer::{self, Quad};
use cosmic::iced::advanced::text::{self as adv_text, Paragraph as _, Renderer as _};
use cosmic::iced::advanced::widget::{Tree, tree};
use cosmic::iced::advanced::{Clipboard, Renderer as _, Shell, Widget};
use cosmic::iced::keyboard::Modifiers;
use cosmic::iced::{Color, Event, Font, Length, Pixels, Point, Rectangle, Size, alignment, keyboard, mouse, window};
use cosmic::{Element, Renderer, Theme};
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

#[derive(Clone)]
pub struct Proxy {
    id: u64,
    pending: Arc<AtomicBool>,
}

impl EventListener for Proxy {
    fn send_event(&self, ev: TermEvent) {
        match ev {
            TermEvent::Wakeup => {
                // One redraw at a time, or a noisy program would flood the queue.
                if !self.pending.swap(true, Ordering::AcqRel) {
                    bus::send(Message::TermWake(self.id));
                }
            }
            TermEvent::Title(t) => bus::send(Message::TermTitle(self.id, Some(t))),
            TermEvent::ResetTitle => bus::send(Message::TermTitle(self.id, None)),
            TermEvent::PtyWrite(s) => bus::send(Message::TermWrite(self.id, s.into_bytes())),
            TermEvent::ClipboardStore(_, s) => bus::send(Message::CopyText(s)),
            TermEvent::Exit | TermEvent::ChildExit(_) => bus::send(Message::TermExited(self.id)),
            _ => {}
        }
    }
}

pub struct Session {
    pub id: u64,
    pub term: Arc<FairMutex<Term<Proxy>>>,
    tx: EventLoopSender,
    pub title: String,
    pub exited: bool,
    pending: Arc<AtomicBool>,
    size: (u16, u16),
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Shutdown);
    }
}

impl Session {
    pub fn spawn(id: u64, cwd: Option<PathBuf>, shell: &str) -> Result<Self, String> {
        let pending = Arc::new(AtomicBool::new(false));
        let proxy = Proxy { id, pending: pending.clone() };
        let size = TermSize::new(80, 24);
        let window = WindowSize { num_lines: 24, num_cols: 80, cell_width: 8, cell_height: 18 };
        let term = Term::new(Config::default(), &size, proxy.clone());
        let term = Arc::new(FairMutex::new(term));
        tty::setup_env();
        let mut env = std::collections::HashMap::new();
        env.insert("TERM".to_string(), "xterm-256color".to_string());
        env.insert("COLORTERM".to_string(), "truecolor".to_string());
        env.insert("TERM_PROGRAM".to_string(), "tinta".to_string());
        let options = tty::Options {
            shell: (!shell.is_empty()).then(|| tty::Shell::new(shell.to_string(), Vec::new())),
            working_directory: cwd,
            drain_on_exit: false,
            env,
        };
        let pty = tty::new(&options, window, id).map_err(|e| e.to_string())?;
        let event_loop = EventLoop::new(term.clone(), proxy, pty, false, false).map_err(|e| e.to_string())?;
        let tx = event_loop.channel();
        event_loop.spawn();
        Ok(Self { id, term, tx, title: String::new(), exited: false, pending, size: (80, 24) })
    }

    pub fn write(&self, bytes: Vec<u8>) {
        let _ = self.tx.send(Msg::Input(Cow::Owned(bytes)));
        self.term.lock().scroll_display(Scroll::Bottom);
    }

    pub fn wake_done(&self) {
        self.pending.store(false, Ordering::Release);
    }

    pub fn resize(&mut self, cols: u16, rows: u16, cell_w: f32, cell_h: f32) {
        if self.size == (cols, rows) || cols == 0 || rows == 0 {
            return;
        }
        self.size = (cols, rows);
        let w = WindowSize { num_lines: rows, num_cols: cols, cell_width: cell_w as u16, cell_height: cell_h as u16 };
        let _ = self.tx.send(Msg::Resize(w));
        self.term.lock().resize(TermSize::new(cols as usize, rows as usize));
    }

    pub fn selection_text(&self) -> Option<String> {
        self.term.lock().selection_to_string().filter(|s| !s.is_empty())
    }

    pub fn clear_selection(&self) {
        self.term.lock().selection = None;
    }

    pub fn is_app_cursor(&self) -> bool {
        self.term.lock().mode().contains(TermMode::APP_CURSOR)
    }

    pub fn is_bracketed_paste(&self) -> bool {
        self.term.lock().mode().contains(TermMode::BRACKETED_PASTE)
    }
}

/// What a key sends to the program in the terminal.
pub fn key_bytes(name: &str, mods: Modifiers, text: Option<&str>, app_cursor: bool) -> Option<Vec<u8>> {
    let modcode = 1 + u8::from(mods.shift()) + 2 * u8::from(mods.alt()) + 4 * u8::from(mods.control());
    let csi = |code: &str, tail: char| -> Vec<u8> {
        if modcode > 1 {
            format!("\x1b[{code};{modcode}{tail}").into_bytes()
        } else {
            format!("\x1b[{code}{tail}").into_bytes()
        }
    };
    let arrow = |c: char| -> Vec<u8> {
        if modcode > 1 {
            format!("\x1b[1;{modcode}{c}").into_bytes()
        } else if app_cursor {
            format!("\x1bO{c}").into_bytes()
        } else {
            format!("\x1b[{c}").into_bytes()
        }
    };
    let tilde = |n: u8| -> Vec<u8> {
        if modcode > 1 {
            format!("\x1b[{n};{modcode}~").into_bytes()
        } else {
            format!("\x1b[{n}~").into_bytes()
        }
    };
    let bytes = match name {
        "enter" => {
            if mods.alt() { b"\x1b\r".to_vec() } else { b"\r".to_vec() }
        }
        "backspace" => {
            if mods.control() {
                b"\x08".to_vec()
            } else if mods.alt() {
                b"\x1b\x7f".to_vec()
            } else {
                b"\x7f".to_vec()
            }
        }
        "tab" => {
            if mods.shift() { b"\x1b[Z".to_vec() } else { b"\t".to_vec() }
        }
        "escape" => b"\x1b".to_vec(),
        "up" => arrow('A'),
        "down" => arrow('B'),
        "right" => arrow('C'),
        "left" => arrow('D'),
        "home" => arrow('H'),
        "end" => arrow('F'),
        "insert" => tilde(2),
        "delete" => tilde(3),
        "pageup" => tilde(5),
        "pagedown" => tilde(6),
        "f1" => csi("1", 'P').pipe_f(mods, b"\x1bOP"),
        "f2" => csi("1", 'Q').pipe_f(mods, b"\x1bOQ"),
        "f3" => csi("1", 'R').pipe_f(mods, b"\x1bOR"),
        "f4" => csi("1", 'S').pipe_f(mods, b"\x1bOS"),
        "f5" => tilde(15),
        "f6" => tilde(17),
        "f7" => tilde(18),
        "f8" => tilde(19),
        "f9" => tilde(20),
        "f10" => tilde(21),
        "f11" => tilde(23),
        "f12" => tilde(24),
        _ => {
            // Ctrl+letter and friends become control codes.
            if mods.control() && name.chars().count() == 1 {
                let c = name.chars().next()?.to_ascii_lowercase();
                let code = match c {
                    'a'..='z' => (c as u8) - b'a' + 1,
                    ' ' | '2' | '@' => 0,
                    '3' | '[' => 27,
                    '4' | '\\' => 28,
                    '5' | ']' => 29,
                    '6' | '^' => 30,
                    '7' | '-' | '/' => 31,
                    '8' => 127,
                    _ => return None,
                };
                let mut v = Vec::new();
                if mods.alt() {
                    v.push(0x1b);
                }
                v.push(code);
                return Some(v);
            }
            let t = text?;
            if t.is_empty() || t.chars().all(|c| c.is_control()) {
                return None;
            }
            let mut v = Vec::new();
            if mods.alt() {
                v.push(0x1b);
            }
            v.extend_from_slice(t.as_bytes());
            return Some(v);
        }
    };
    Some(bytes)
}

trait PipeF {
    fn pipe_f(self, mods: Modifiers, plain: &[u8]) -> Vec<u8>;
}

impl PipeF for Vec<u8> {
    /// F1–F4 use a different sequence when no modifier is held.
    fn pipe_f(self, mods: Modifiers, plain: &[u8]) -> Vec<u8> {
        if mods.is_empty() { plain.to_vec() } else { self }
    }
}

/// The 16 ANSI colours, close to COSMIC's terminal defaults.
fn ansi(i: usize, dark: bool, fg: Color, bg: Color) -> Color {
    const DARK: [u32; 16] = [
        0x1E1E1E, 0xF16161, 0x7CB987, 0xDDC74C, 0x6296BE, 0xBE6DEE, 0x49BAC8, 0xBEBEBE, 0x808080, 0xFF8985, 0x94D19F,
        0xF5DE64, 0x7CAED6, 0xD685FF, 0x63D2E0, 0xE7E7E7,
    ];
    const LIGHT: [u32; 16] = [
        0x1E1E1E, 0xA6222B, 0x2B6C33, 0x7B6400, 0x1F5F91, 0x7E2DAD, 0x006E7C, 0x7A7A7A, 0x5A5A5A, 0xC0303A, 0x3A8A45,
        0x9A7F00, 0x2A7AB8, 0x9A40D0, 0x008A9B, 0xE7E7E7,
    ];
    let t = if dark { DARK } else { LIGHT };
    match i {
        0..=15 => hex(t[i]),
        16..=231 => {
            let i = i - 16;
            let step = |n: usize| if n == 0 { 0 } else { (n * 40 + 55) as u8 };
            Color::from_rgb8(step(i / 36), step((i / 6) % 6), step(i % 6))
        }
        232..=255 => {
            let v = (8 + (i - 232) * 10) as u8;
            Color::from_rgb8(v, v, v)
        }
        256 => fg,
        _ => bg,
    }
}

fn hex(h: u32) -> Color {
    Color::from_rgb8((h >> 16) as u8, (h >> 8) as u8, h as u8)
}

fn resolve(c: AnsiColor, colors: &Colors, dark: bool, fg: Color, bg: Color) -> Color {
    let rgb = |r: Rgb| Color::from_rgb8(r.r, r.g, r.b);
    match c {
        AnsiColor::Spec(r) => rgb(r),
        AnsiColor::Indexed(i) => colors[i as usize].map(rgb).unwrap_or_else(|| ansi(i as usize, dark, fg, bg)),
        AnsiColor::Named(n) => {
            if let Some(r) = colors[n] {
                return rgb(r);
            }
            match n {
                NamedColor::Foreground | NamedColor::BrightForeground | NamedColor::Cursor => fg,
                NamedColor::DimForeground => Color { a: 0.7, ..fg },
                NamedColor::Background => bg,
                other => {
                    let i = other as usize;
                    if i < 16 {
                        ansi(i, dark, fg, bg)
                    } else if (NamedColor::DimBlack as usize..=NamedColor::DimWhite as usize).contains(&i) {
                        Color { a: 0.7, ..ansi(i - NamedColor::DimBlack as usize, dark, fg, bg) }
                    } else {
                        fg
                    }
                }
            }
        }
    }
}

pub struct TermView<'a, Msg> {
    pub session: &'a Session,
    pub font: Font,
    pub size: f32,
    pub focused: bool,
    pub on_resize: Box<dyn Fn(u16, u16, f32, f32) -> Msg + 'a>,
    pub on_focus: Msg,
}

#[derive(Default)]
struct State {
    cell_w: f32,
    measured: Option<(Font, f32)>,
    reported: Option<(u16, u16)>,
    selecting: bool,
    last_click: Option<(Instant, u8)>,
    mods: Modifiers,
}

impl<'a, Msg2: Clone + 'a> TermView<'a, Msg2> {
    fn line_h(&self) -> f32 {
        (self.size * 1.3).round()
    }

    fn grid_point(&self, p: Point, bounds: Rectangle, cell_w: f32) -> (GridPoint, Side) {
        let term = self.session.term.lock();
        let cols = term.columns().max(1);
        let rows = term.screen_lines().max(1);
        let x = (p.x - bounds.x - 6.0).max(0.0);
        let y = (p.y - bounds.y - 2.0).max(0.0);
        let col = ((x / cell_w) as usize).min(cols - 1);
        let line = ((y / self.line_h()) as usize).min(rows - 1) as i32 - term.grid().display_offset() as i32;
        let side = if (x / cell_w).fract() < 0.5 { Side::Left } else { Side::Right };
        (GridPoint::new(Line(line), Column(col)), side)
    }
}

impl<'a, Msg2: Clone + 'a> Widget<Msg2, Theme, Renderer> for TermView<'a, Msg2> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(&mut self, tree: &mut Tree, _r: &Renderer, limits: &layout::Limits) -> layout::Node {
        let state = tree.state.downcast_mut::<State>();
        if state.measured != Some((self.font, self.size)) {
            let sample = "M".repeat(100);
            let p = <Renderer as adv_text::Renderer>::Paragraph::with_text(adv_text::Text {
                content: sample.as_str(),
                bounds: Size::INFINITE,
                size: Pixels(self.size),
                line_height: adv_text::LineHeight::Absolute(Pixels(self.line_h())),
                font: self.font,
                align_x: adv_text::Alignment::Left,
                align_y: alignment::Vertical::Top,
                shaping: adv_text::Shaping::Advanced,
                wrapping: adv_text::Wrapping::None,
                ellipsize: adv_text::Ellipsize::None,
            });
            state.cell_w = (p.min_bounds().width / 100.0).max(1.0);
            state.measured = Some((self.font, self.size));
        }
        layout::Node::new(limits.max())
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _r: &Renderer,
        _c: &mut dyn Clipboard,
        shell: &mut Shell<'_, Msg2>,
        _v: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        if state.cell_w > 0.0 {
            let cols = (((bounds.width - 12.0) / state.cell_w).floor() as u16).max(2);
            let rows = (((bounds.height - 4.0) / self.line_h()).floor() as u16).max(1);
            if state.reported != Some((cols, rows)) {
                state.reported = Some((cols, rows));
                shell.publish((self.on_resize)(cols, rows, state.cell_w, self.line_h()));
            }
        }
        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(m)) => state.mods = *m,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(p) = cursor.position_over(bounds) else { return };
                shell.publish(self.on_focus.clone());
                let (point, side) = self.grid_point(p, bounds, state.cell_w);
                let clicks = match state.last_click {
                    Some((t, n)) if t.elapsed().as_millis() < 400 => (n % 3) + 1,
                    _ => 1,
                };
                state.last_click = Some((Instant::now(), clicks));
                let kind = match clicks {
                    2 => SelectionType::Semantic,
                    3 => SelectionType::Lines,
                    _ => SelectionType::Simple,
                };
                let mut term = self.session.term.lock();
                term.selection = Some(Selection::new(kind, point, side));
                state.selecting = true;
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.selecting => {
                let Some(p) = cursor.position() else { return };
                let (point, side) = self.grid_point(p, bounds, state.cell_w);
                let mut term = self.session.term.lock();
                if let Some(sel) = &mut term.selection {
                    sel.update(point, side);
                }
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.selecting = false;
                let mut term = self.session.term.lock();
                if term.selection.as_ref().is_some_and(|s| s.is_empty()) {
                    term.selection = None;
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if cursor.position_over(bounds).is_none() {
                    return;
                }
                let lines = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => (*y * 3.0) as i32,
                    mouse::ScrollDelta::Pixels { y, .. } => (*y / self.line_h()) as i32,
                };
                if lines == 0 {
                    return;
                }
                let alt = self.session.term.lock().mode().contains(TermMode::ALT_SCREEN);
                if alt {
                    // Full-screen programs scroll with the arrow keys.
                    let code: &[u8] = if self.session.is_app_cursor() {
                        if lines > 0 { b"\x1bOA" } else { b"\x1bOB" }
                    } else if lines > 0 {
                        b"\x1b[A"
                    } else {
                        b"\x1b[B"
                    };
                    let mut bytes = Vec::new();
                    for _ in 0..lines.unsigned_abs() {
                        bytes.extend_from_slice(code);
                    }
                    let _ = self.session.tx.send(alacritty_terminal::event_loop::Msg::Input(Cow::Owned(bytes)));
                } else {
                    self.session.term.lock().scroll_display(Scroll::Delta(lines));
                }
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Window(window::Event::RedrawRequested(_)) => {}
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        _t: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _v: &Rectangle,
        _r: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) { mouse::Interaction::Text } else { mouse::Interaction::default() }
    }

    fn draw(
        &self,
        tree: &Tree,
        r: &mut Renderer,
        theme: &Theme,
        _s: &renderer::Style,
        layout: Layout<'_>,
        _c: mouse::Cursor,
        _v: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let t = theme.cosmic();
        let dark = t.is_dark;
        let fg: Color = Color { a: 1.0, ..t.background(false).on.into() };
        let bg: Color = Color { a: 1.0, ..t.background(false).base.into() };
        let accent: Color = t.accent.base.into();
        let (cw, lh) = (state.cell_w.max(1.0), self.line_h());
        let term = self.session.term.lock();
        let content = term.renderable_content();
        let origin = Point::new(bounds.x + 6.0, bounds.y + 2.0);
        let offset = content.display_offset as i32;
        let selection = content.selection;
        let colors = content.colors;
        let cur = content.cursor;
        let cursor_visible = cur.shape != alacritty_terminal::vte::ansi::CursorShape::Hidden;
        let text_of = |c: String, font: Font, size: f32, lh: f32| adv_text::Text {
            content: c,
            bounds: Size::new(f32::INFINITY, lh),
            size: Pixels(size),
            line_height: adv_text::LineHeight::Absolute(Pixels(lh)),
            font,
            align_x: adv_text::Alignment::Left,
            align_y: alignment::Vertical::Top,
            shaping: adv_text::Shaping::Advanced,
            wrapping: adv_text::Wrapping::None,
            ellipsize: adv_text::Ellipsize::None,
        };
        r.with_layer(bounds, |r| {
            let mut run = String::new();
            let mut run_col = 0usize;
            let mut run_line = i32::MIN;
            let mut run_fg = fg;
            let mut run_font = self.font;
            let flush = |r: &mut Renderer, run: &mut String, col: usize, line: i32, color: Color, font: Font| {
                if run.trim().is_empty() {
                    run.clear();
                    return;
                }
                let p = Point::new(origin.x + col as f32 * cw, origin.y + (line + offset) as f32 * lh);
                r.fill_text(text_of(std::mem::take(run), font, self.size, lh), p, color, bounds);
            };
            for cell in content.display_iter {
                let (col, line) = (cell.point.column.0, cell.point.line.0);
                let flags = cell.flags;
                if flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                let mut f = resolve(cell.fg, colors, dark, fg, bg);
                let mut b = resolve(cell.bg, colors, dark, fg, bg);
                if flags.contains(Flags::INVERSE) {
                    std::mem::swap(&mut f, &mut b);
                }
                if flags.intersects(Flags::DIM) {
                    f.a *= 0.7;
                }
                let selected = selection.is_some_and(|s| s.contains(cell.point));
                let y = origin.y + (line + offset) as f32 * lh;
                let x = origin.x + col as f32 * cw;
                let w = if flags.contains(Flags::WIDE_CHAR) { 2.0 } else { 1.0 } * cw;
                if selected {
                    r.fill_quad(Quad { bounds: Rectangle::new(Point::new(x, y), Size::new(w, lh)), snap: true, ..Quad::default() }, Color { a: 0.4, ..accent });
                } else if b != bg {
                    r.fill_quad(Quad { bounds: Rectangle::new(Point::new(x, y), Size::new(w, lh)), snap: true, ..Quad::default() }, b);
                }
                if flags.intersects(Flags::UNDERLINE | Flags::DOUBLE_UNDERLINE | Flags::UNDERCURL) {
                    r.fill_quad(Quad { bounds: Rectangle::new(Point::new(x, y + lh - 2.0), Size::new(w, 1.0)), snap: true, ..Quad::default() }, f);
                }
                let font = Font {
                    weight: if flags.contains(Flags::BOLD) { cosmic::iced::font::Weight::Bold } else { self.font.weight },
                    style: if flags.contains(Flags::ITALIC) { cosmic::iced::font::Style::Italic } else { self.font.style },
                    ..self.font
                };
                let hidden = flags.contains(Flags::HIDDEN);
                let ch = if hidden || cell.c == '\0' { ' ' } else { cell.c };
                let ascii = ch.is_ascii();
                let contiguous = line == run_line && col == run_col + run.chars().count() && f == run_fg && font == run_font && ascii;
                if !contiguous {
                    flush(r, &mut run, run_col, run_line, run_fg, run_font);
                    run_col = col;
                    run_line = line;
                    run_fg = f;
                    run_font = font;
                }
                run.push(ch);
                if let Some(zw) = cell.zerowidth() {
                    run.extend(zw.iter());
                }
                if !ascii {
                    flush(r, &mut run, run_col, run_line, run_fg, run_font);
                }
            }
            flush(r, &mut run, run_col, run_line, run_fg, run_font);
        });
        if cursor_visible {
            r.with_layer(bounds, |r| {
                let x = origin.x + cur.point.column.0 as f32 * cw;
                let y = origin.y + (cur.point.line.0 + offset) as f32 * lh;
                let q = |r: &mut Renderer, rect: Rectangle, c: Color| {
                    r.fill_quad(Quad { bounds: rect, snap: true, ..Quad::default() }, c);
                };
                if self.focused {
                    q(r, Rectangle::new(Point::new(x, y), Size::new(cw, lh)), Color { a: 0.55, ..accent });
                } else {
                    let c = Color { a: 0.5, ..fg };
                    q(r, Rectangle::new(Point::new(x, y), Size::new(cw, 1.0)), c);
                    q(r, Rectangle::new(Point::new(x, y + lh - 1.0), Size::new(cw, 1.0)), c);
                    q(r, Rectangle::new(Point::new(x, y), Size::new(1.0, lh)), c);
                    q(r, Rectangle::new(Point::new(x + cw - 1.0, y), Size::new(1.0, lh)), c);
                }
            });
        }
    }
}

impl<'a, Msg2: Clone + 'a> From<TermView<'a, Msg2>> for Element<'a, Msg2> {
    fn from(v: TermView<'a, Msg2>) -> Self {
        Element::new(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none() -> Modifiers {
        Modifiers::empty()
    }

    #[test]
    fn keys_become_the_right_sequences() {
        assert_eq!(key_bytes("enter", none(), None, false).unwrap(), b"\r");
        assert_eq!(key_bytes("up", none(), None, false).unwrap(), b"\x1b[A");
        assert_eq!(key_bytes("up", none(), None, true).unwrap(), b"\x1bOA");
        assert_eq!(key_bytes("up", Modifiers::CTRL, None, false).unwrap(), b"\x1b[1;5A");
        assert_eq!(key_bytes("c", Modifiers::CTRL, Some("c"), false).unwrap(), &[3]);
        assert_eq!(key_bytes("a", none(), Some("a"), false).unwrap(), b"a");
        assert_eq!(key_bytes("a", Modifiers::ALT, Some("a"), false).unwrap(), b"\x1ba");
        assert_eq!(key_bytes("ç", none(), Some("ç"), false).unwrap(), "ç".as_bytes());
        assert_eq!(key_bytes("f1", none(), None, false).unwrap(), b"\x1bOP");
        assert_eq!(key_bytes("delete", none(), None, false).unwrap(), b"\x1b[3~");
        assert!(key_bytes("shift", none(), None, false).is_none());
    }

    #[test]
    fn color_cube_and_greys() {
        let c = ansi(16, true, Color::WHITE, Color::BLACK);
        assert_eq!(c, Color::from_rgb8(0, 0, 0));
        let c = ansi(231, true, Color::WHITE, Color::BLACK);
        assert_eq!(c, Color::from_rgb8(255, 255, 255));
        assert_eq!(ansi(232, true, Color::WHITE, Color::BLACK), Color::from_rgb8(8, 8, 8));
    }
}
