//! The thin handle between two panes.

use cosmic::iced::advanced::layout::{self, Layout};
use cosmic::iced::advanced::renderer::{self, Quad};
use cosmic::iced::advanced::widget::{Tree, tree};
use cosmic::iced::advanced::{Clipboard, Renderer as _, Shell, Widget};
use cosmic::iced::{Color, Event, Length, Point, Rectangle, Size, mouse};
use cosmic::{Element, Renderer, Theme};

pub struct Sash<'a, Message> {
    vertical: bool,
    on_drag: Box<dyn Fn(f32) -> Message + 'a>,
}

/// `vertical` means the bar stands upright and drags sideways.
pub fn sash<'a, Message: 'a>(vertical: bool, on_drag: impl Fn(f32) -> Message + 'a) -> Sash<'a, Message> {
    Sash { vertical, on_drag: Box::new(on_drag) }
}

#[derive(Default)]
struct State {
    drag_from: Option<Point>,
}

const THICKNESS: f32 = 5.0;

impl<Message: Clone> Widget<Message, Theme, Renderer> for Sash<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        if self.vertical {
            Size::new(Length::Fixed(THICKNESS), Length::Fill)
        } else {
            Size::new(Length::Fill, Length::Fixed(THICKNESS))
        }
    }

    fn layout(&mut self, _t: &mut Tree, _r: &Renderer, limits: &layout::Limits) -> layout::Node {
        let s = if self.vertical { Size::new(THICKNESS, limits.max().height) } else { Size::new(limits.max().width, THICKNESS) };
        layout::Node::new(s)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _r: &Renderer,
        _c: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _v: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(p) = cursor.position_over(layout.bounds()) {
                    state.drag_from = Some(p);
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position, .. }) => {
                if let Some(from) = state.drag_from {
                    let d = if self.vertical { position.x - from.x } else { position.y - from.y };
                    if d != 0.0 {
                        state.drag_from = Some(*position);
                        shell.publish((self.on_drag)(d));
                    }
                    shell.capture_event();
                }
            }
            // The pointer is grabbed during a drag: entering means the release was lost.
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left) | mouse::Event::CursorEntered) => {
                state.drag_from = None;
            }
            _ => {}
        }
    }

    fn mouse_interaction(&self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, _v: &Rectangle, _r: &Renderer) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        if state.drag_from.is_some() || cursor.is_over(layout.bounds()) {
            if self.vertical { mouse::Interaction::ResizingHorizontally } else { mouse::Interaction::ResizingVertically }
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(&self, tree: &Tree, r: &mut Renderer, theme: &Theme, _s: &renderer::Style, layout: Layout<'_>, cursor: mouse::Cursor, _v: &Rectangle) {
        let state = tree.state.downcast_ref::<State>();
        let b = layout.bounds();
        let t = theme.cosmic();
        let active = state.drag_from.is_some() || cursor.is_over(b);
        let on: Color = t.background(false).on.into();
        let accent: Color = t.accent.base.into();
        let (color, thick) = if active { (accent, 2.0) } else { (Color { a: 0.14, ..on }, 1.0) };
        let line = if self.vertical {
            Rectangle::new(Point::new(b.x + (b.width - thick) / 2.0, b.y), Size::new(thick, b.height))
        } else {
            Rectangle::new(Point::new(b.x, b.y + (b.height - thick) / 2.0), Size::new(b.width, thick))
        };
        r.fill_quad(Quad { bounds: line, snap: true, ..Quad::default() }, color);
    }
}

impl<'a, Message: Clone + 'a> From<Sash<'a, Message>> for Element<'a, Message> {
    fn from(s: Sash<'a, Message>) -> Self {
        Element::new(s)
    }
}
