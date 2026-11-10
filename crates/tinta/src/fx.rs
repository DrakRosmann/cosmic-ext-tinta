//! Small helpers for what the app asks of the toolkit: focus a field by name, the clipboard.
//! The same names the GTK fork uses, so the logic in `app/` reads the same in both.

use crate::app::Message;
use cosmic::app::Task;

pub fn focus(id: &'static str) -> Task<Message> {
    cosmic::widget::text_input::focus(cosmic::widget::Id::new(id))
}

pub fn select_all(id: &'static str) -> Task<Message> {
    cosmic::widget::text_input::select_all(cosmic::widget::Id::new(id))
}

/// Focus and everything in it selected, ready to be typed over.
pub fn focus_all(id: &'static str) -> Task<Message> {
    Task::batch([focus(id), select_all(id)])
}

/// Back to the text: no field keeps the keyboard.
pub fn focus_editor() -> Task<Message> {
    cosmic::widget::text_input::focus(cosmic::widget::Id::new("no-such-field"))
}

pub fn clipboard_write(text: String) -> Task<Message> {
    cosmic::iced::clipboard::write(text)
}

pub fn clipboard_read(then: impl Fn(Option<String>) -> Message + Send + 'static) -> Task<Message> {
    cosmic::iced::clipboard::read().map(move |t| cosmic::Action::App(then(t)))
}
