//! Suggestions, hover docs, parameter hints and the dialogs.

use super::palette::highlighted;
use super::workspace::dim_text;
use crate::app::{App, Completion, DialogMsg, DialogState, Message, SignaturePopup};
use crate::i18n::tr;
use cosmic::iced::widget::{rich_text, span};
use cosmic::iced::{Alignment, Color, Font, Length};
use cosmic::widget::{self, button, column, container, row, scrollable, text};
use cosmic::{Element, theme};
use tinta_core::keymap::Command;

pub const DIALOG_INPUT: &str = "dialog-input";

pub const COMPLETION_ROWS: usize = 9;
pub const COMPLETION_ROW_H: f32 = 26.0;
pub const COMPLETION_W: f32 = 460.0;

pub fn completion(c: &Completion, mono: Font, rows: usize) -> Element<'static, Message> {
    let accent: Color = cosmic::theme::active().cosmic().accent.base.into();
    let dim = dim_text();
    let start = if c.selected >= rows / 2 { (c.selected - rows / 2).min(c.visible.len().saturating_sub(rows)) } else { 0 };
    let mut list = column::with_capacity(rows).width(Length::Fixed(COMPLETION_W - 12.0));
    for (i, idx) in c.visible.iter().enumerate().skip(start).take(rows) {
        let e = &c.entries[*idx];
        let r = row::with_children(vec![
            container(text(e.kind).size(10).font(mono).class(theme::Text::Color(dim))).width(Length::Fixed(34.0)).into(),
            highlighted(&e.label, &e.positions, 13.0, accent, Some(mono)),
            widget::Space::new().width(Length::Fixed(10.0)).into(),
            container(text(e.detail.clone()).size(11).class(theme::Text::Color(dim)).wrapping(cosmic::iced::widget::text::Wrapping::None))
                .clip(true)
                .width(Length::Fill)
                .align_x(Alignment::End)
                .into(),
        ])
        .align_y(Alignment::Center);
        list = list.push(
            button::custom(r)
                .class(crate::ui::row_button(i == c.selected))
                .padding([3, 6])
                .height(Length::Fixed(COMPLETION_ROW_H))
                .width(Length::Fill)
                .on_press(Message::CompletionPick(i)),
        );
    }
    container(list).padding(6).class(theme::Container::Dialog(false)).into()
}

/// Docs from language servers come as Markdown; fences switch to the code font.
pub fn hover(md: &str, mono: Font, view_w: f32) -> Element<'static, Message> {
    let width = 560.0_f32.min((view_w - 24.0).max(200.0));
    let mut items: Vec<Element<'static, Message>> = Vec::new();
    let mut in_code = false;
    let mut para = String::new();
    fn flush(items: &mut Vec<Element<'static, Message>>, para: &mut String) {
        if !para.trim().is_empty() {
            items.push(text(para.trim_end().to_string()).size(13).into());
        }
        para.clear();
    }
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            flush(&mut items, &mut para);
            in_code = !in_code;
            continue;
        }
        if in_code {
            items.push(text(line.to_string()).size(12.5).font(mono).into());
            continue;
        }
        if line.trim() == "---" {
            flush(&mut items, &mut para);
            items.push(widget::divider::horizontal::light().into());
            continue;
        }
        let clean = line.replace("**", "").replace('`', "").replace("\\_", "_").replace('\\', "");
        if clean.trim().is_empty() {
            flush(&mut items, &mut para);
        } else {
            para.push_str(&clean);
            para.push('\n');
        }
    }
    flush(&mut items, &mut para);
    let col = column::with_children(items).spacing(4).width(Length::Fill);
    container(scrollable(container(col).padding([0, 10, 0, 0])).height(Length::Shrink))
        .padding(10)
        .max_height(240.0)
        .width(Length::Fixed(width))
        .class(theme::Container::Dialog(false))
        .into()
}

pub fn signature(s: &SignaturePopup, mono: Font) -> Element<'static, Message> {
    let accent: Color = cosmic::theme::active().cosmic().accent.base.into();
    let chars: Vec<char> = s.label.chars().collect();
    let (a, b) = s.active.unwrap_or((chars.len(), chars.len()));
    let (a, b) = (a.min(chars.len()), b.min(chars.len()));
    let before: String = chars[..a].iter().collect();
    let active: String = chars[a..b].iter().collect();
    let after: String = chars[b..].iter().collect();
    let bold = Font { weight: cosmic::iced::font::Weight::Bold, ..mono };
    let spans = vec![
        span::<(), Font>(before).font(mono).size(13.0),
        span::<(), Font>(active).font(bold).color(accent).size(13.0),
        span::<(), Font>(after).font(mono).size(13.0),
    ];
    container(rich_text(spans))
        .padding([6, 10])
        .max_width(620.0)
        .class(theme::Container::Dialog(false))
        .into()
}

pub fn dialog(app: &App) -> Option<Element<'_, Message>> {
    let d = app.dialog.as_ref()?;
    let cancel = || button::standard(tr("Cancel")).on_press(Message::Dialog(DialogMsg::Cancel));
    let primary = |label: &'static str| button::suggested(tr(label)).on_press(Message::Dialog(DialogMsg::Primary));
    let danger = |label: &'static str| button::destructive(tr(label)).on_press(Message::Dialog(DialogMsg::Primary));
    let input = |placeholder: &'static str| -> Element<'_, Message> {
        widget::text_input(tr(placeholder), app.dialog_input.as_str())
            .id(widget::Id::new(DIALOG_INPUT))
            .on_input(|s| Message::Dialog(DialogMsg::Input(s)))
            .on_submit(|_| Message::Dialog(DialogMsg::Primary)).on_focus(Message::InputFocus(true)).on_unfocus(Message::InputFocus(false))
            .into()
    };
    let file_of = |p: &std::path::Path| p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let el: Element<'_, Message> = match d {
        DialogState::CloseUnsaved(id) => {
            let name = app.tabs.get(id).and_then(|t| app.docs.get(&t.doc)).map(|e| e.doc.file_name()).unwrap_or_default();
            widget::dialog()
                .title(name)
                .body(tr("Save changes before closing?"))
                .primary_action(primary("Save"))
                .secondary_action(cancel())
                .tertiary_action(button::destructive(tr("Don't Save")).on_press(Message::Dialog(DialogMsg::Secondary)))
                .into()
        }
        DialogState::QuitUnsaved => widget::dialog()
            .title(tr("Unsaved changes"))
            .body(tr("You have unsaved files. Save them before quitting?"))
            .primary_action(primary("Save All"))
            .secondary_action(cancel())
            .tertiary_action(button::destructive(tr("Don't Save")).on_press(Message::Dialog(DialogMsg::Secondary)))
            .into(),
        DialogState::ChangedOnDisk(doc) => {
            let name = app.docs.get(doc).map(|e| e.doc.file_name()).unwrap_or_default();
            widget::dialog()
                .title(format!("{}: {name}", tr("File changed on disk")))
                .body(tr("This file changed on disk and you have unsaved changes."))
                .primary_action(primary("Reload"))
                .secondary_action(button::standard(tr("Keep Mine")).on_press(Message::Dialog(DialogMsg::Cancel)))
                .into()
        }
        DialogState::NewFile(dir) => widget::dialog()
            .title(format!("{} · {}", tr("New File"), file_of(dir)))
            .control(input("Name"))
            .primary_action(primary("Create"))
            .secondary_action(cancel())
            .into(),
        DialogState::NewFolder(dir) => widget::dialog()
            .title(format!("{} · {}", tr("New Folder"), file_of(dir)))
            .control(input("Name"))
            .primary_action(primary("Create"))
            .secondary_action(cancel())
            .into(),
        DialogState::RenamePath(p) => widget::dialog()
            .title(format!("{} · {}", tr("Rename"), file_of(p)))
            .control(input("Name"))
            .primary_action(primary("Rename"))
            .secondary_action(cancel())
            .into(),
        DialogState::TrashPath(p) => widget::dialog()
            .title(file_of(p))
            .body(tr("Move to Trash"))
            .primary_action(danger("Move to Trash"))
            .secondary_action(cancel())
            .into(),
        DialogState::RenameSymbol => widget::dialog()
            .title(tr("Rename symbol to"))
            .control(input("Name"))
            .primary_action(primary("Rename"))
            .secondary_action(cancel())
            .into(),
        DialogState::ConfirmDiscard(entry) => widget::dialog()
            .title(entry.path.clone())
            .body(tr("Discard changes to this file? This cannot be undone."))
            .primary_action(danger("Discard Changes"))
            .secondary_action(cancel())
            .into(),
        DialogState::About => widget::dialog()
            .title(format!("Tinta {}", env!("CARGO_PKG_VERSION")))
            .body(format!("{}\n\nGPL-3.0-only\n{}", tr("A code editor for the COSMIC desktop, written in Rust."), env!("CARGO_PKG_REPOSITORY")))
            .primary_action(button::suggested(tr("OK")).on_press(Message::Dialog(DialogMsg::Cancel)))
            .into(),
        DialogState::Shortcuts => {
            let pt = app.pt();
            let filter = app.shortcuts_filter.to_lowercase();
            let mut list = column::with_capacity(64).spacing(2).width(Length::Fill);
            for c in Command::ALL {
                let title = c.title(pt);
                let Some(key) = app.keymap.label(*c) else { continue };
                if !filter.is_empty() && !title.to_lowercase().contains(&filter) && !key.to_lowercase().contains(&filter) {
                    continue;
                }
                list = list.push(
                    row::with_children(vec![
                        text(title).size(13).width(Length::Fill).into(),
                        text(key.to_string()).size(12).font(cosmic::font::mono()).class(theme::Text::Color(dim_text())).into(),
                    ])
                    .padding([2, 12, 2, 0]),
                );
            }
            let control = column::with_children(vec![
                widget::search_input(tr("Find"), app.shortcuts_filter.as_str())
                    .on_input(|s| Message::Dialog(DialogMsg::ShortcutsFilter(s)))
                    .into(),
                scrollable(list).height(Length::Fixed(420.0)).into(),
            ])
            .spacing(8);
            widget::dialog()
                .title(tr("Keyboard Shortcuts"))
                .control(control)
                .primary_action(button::suggested(tr("Close")).on_press(Message::Dialog(DialogMsg::Cancel)))
                .into()
        }
    };
    Some(el)
}
