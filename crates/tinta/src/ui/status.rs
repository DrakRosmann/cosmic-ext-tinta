//! The bar at the bottom.

use super::icons;
use super::workspace::dim_text;
use crate::app::{App, Focus, Message, PanelView};
use crate::i18n::tr;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, button, container, icon, row, text};
use cosmic::{Element, theme};
use tinta_core::keymap::Command;
use tinta_core::lsp::Severity;

fn item<'a>(label: String, msg: Message) -> Element<'a, Message> {
    button::custom(text(label).size(12)).class(crate::ui::row_button(false)).padding([2, 7]).on_press(msg).into()
}

fn icon_item<'a>(h: icon::Handle, label: String, msg: Message) -> Element<'a, Message> {
    button::custom(
        row::with_children(vec![icon::icon(h).size(12).into(), text(label).size(12).into()])
            .spacing(4)
            .align_y(Alignment::Center),
    )
    .class(crate::ui::row_button(false))
    .padding([2, 7])
    .on_press(msg)
    .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let mut r = row::with_capacity(14).align_y(Alignment::Center).width(Length::Fill).spacing(1);
    if app.settings.vim_mode {
        r = r.push(container(text(app.vim.status()).size(12).font(cosmic::font::semibold())).padding([0, 8]));
    }
    if let Some(b) = &app.git.branch {
        let mut label = b.clone();
        if let Some((ahead, behind)) = app.git.ahead_behind
            && (ahead > 0 || behind > 0)
        {
            label.push_str(&format!(" ↑{ahead} ↓{behind}"));
        }
        r = r.push(icon_item(icons::branch(), label, Message::Cmd(Command::GitCheckout)));
    }
    let (mut errors, mut warnings) = (0, 0);
    for list in app.project_diags.values() {
        for d in list {
            match d.severity {
                Severity::Error => errors += 1,
                Severity::Warning => warnings += 1,
                _ => {}
            }
        }
    }
    r = r.push(icon_item(icons::error(), errors.to_string(), Message::PanelShow(PanelView::Problems)));
    r = r.push(icon_item(icons::warning(), warnings.to_string(), Message::PanelShow(PanelView::Problems)));
    if let Some(p) = &app.lsp_progress {
        r = r.push(container(text(p.clone()).size(12).class(theme::Text::Color(dim_text())).wrapping(cosmic::iced::widget::text::Wrapping::None)).clip(true).padding([0, 8]).max_width(420.0));
    }
    if let Some(k) = app.keymap.pending_label() {
        r = r.push(text(format!("{k} …")).size(12));
    }
    r = r.push(widget::Space::new().width(Length::Fill));

    let focus_terminal = app.focus == Focus::Terminal && app.panel == Some(PanelView::Terminal);
    if let Some(d) = app.active().and_then(|(t, _)| t.diff.as_ref()) {
        r = r.push(container(text(format!("+{} −{}", d.added, d.removed)).size(12)).padding([0, 7]));
        let pos = match d.current {
            Some(c) => format!("{} {} {}", c + 1, tr("of"), d.hunks.len()),
            None => format!("{}", d.hunks.len()),
        };
        r = r.push(item(pos, Message::Cmd(Command::NextChange)));
        r = r.push(item(d.right.language.name.clone(), Message::Noop));
    } else if let Some((t, e)) = app.active()
        && !focus_terminal
    {
        let d = &e.doc;
        let p = t.cursors.primary();
        let (line, col) = d.line_col(p.head);
        let mut pos = format!("{} {}, {} {}", tr("Ln"), line + 1, tr("Col"), col + 1);
        if t.cursors.is_multi() {
            pos.push_str(&format!("  ({} {})", t.cursors.len(), tr("cursors")));
        } else if !p.is_empty() {
            pos.push_str(&format!("  ({} {})", p.end() - p.start(), tr("selected")));
        }
        r = r.push(item(pos, Message::Cmd(Command::GoToLine)));
        let indent = if d.indent.spaces {
            format!("{}: {}", tr("Spaces"), d.indent.width)
        } else {
            format!("{}: {}", tr("Tab Size"), d.indent.width)
        };
        r = r.push(item(indent, Message::Cmd(Command::ChangeIndentSize)));
        r = r.push(item(d.encoding.name().to_string(), Message::Cmd(Command::ChangeEncoding)));
        let eol_toggle = if d.eol == tinta_core::buffer::LineEnding::Lf { Command::SetCrlf } else { Command::SetLf };
        r = r.push(item(d.eol.label().to_string(), Message::Cmd(eol_toggle)));
        r = r.push(item(d.language.name.clone(), Message::Cmd(Command::ChangeLanguage)));
        if e.lsp_key.is_some() {
            let name = e.lsp_key.as_deref().and_then(|k| k.split('@').next()).unwrap_or("").to_string();
            r = r.push(container(text(name).size(11).class(theme::Text::Color(dim_text()))).padding([0, 7]));
        }
        if d.readonly {
            r = r.push(container(text(if app.pt() { "somente leitura" } else { "read-only" }).size(11).class(theme::Text::Color(dim_text()))).padding([0, 7]));
        }
    }
    container(r).padding([1, 6]).width(Length::Fill).height(Length::Fixed(26.0)).align_y(Alignment::Center).into()
}
