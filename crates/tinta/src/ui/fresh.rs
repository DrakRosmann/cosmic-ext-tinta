//! Forces its content to start from clean widget state when asked.
//!
//! Widget state is matched to widgets by position. COSMIC's tab bar keeps its per-tab data in a
//! map keyed by its model's ids, so when a split closes and the next group's tab bar slides into
//! that position it gets a map full of keys from another model, and panics on the first draw.

use cosmic::iced::advanced::layout::{self, Layout};
use cosmic::iced::advanced::widget::{Operation, Tree, tree};
use cosmic::iced::advanced::{Clipboard, Shell, Widget, overlay, renderer};
use cosmic::iced::{Event, Length, Rectangle, Size, Vector, mouse};
use cosmic::{Element, Renderer, Theme};

pub struct Fresh<'a, Message> {
    /// Flipping this drops all state below.
    pub flip: bool,
    pub content: Element<'a, Message>,
}

struct Odd;
struct Even;

impl<Message> Widget<Message, Theme, Renderer> for Fresh<'_, Message> {
    fn tag(&self) -> tree::Tag {
        if self.flip { tree::Tag::of::<Odd>() } else { tree::Tag::of::<Even>() }
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(&mut self, tree: &mut Tree, r: &Renderer, limits: &layout::Limits) -> layout::Node {
        self.content.as_widget_mut().layout(&mut tree.children[0], r, limits)
    }

    fn draw(
        &self,
        tree: &Tree,
        r: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(&tree.children[0], r, theme, style, layout, cursor, viewport);
    }

    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, r: &Renderer, op: &mut dyn Operation) {
        self.content.as_widget_mut().operate(&mut tree.children[0], layout, r, op);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        r: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(&mut tree.children[0], event, layout, cursor, r, clipboard, shell, viewport);
    }

    fn mouse_interaction(&self, tree: &Tree, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle, r: &Renderer) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(&tree.children[0], layout, cursor, viewport, r)
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        r: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(&mut tree.children[0], layout, r, viewport, translation)
    }
}

impl<'a, Message: 'a> From<Fresh<'a, Message>> for Element<'a, Message> {
    fn from(f: Fresh<'a, Message>) -> Self {
        Element::new(f)
    }
}
