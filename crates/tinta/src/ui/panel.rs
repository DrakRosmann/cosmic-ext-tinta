//! The bottom panel: terminals, problems, output.

use super::icons;
use super::terminal::TermView;
use super::workspace::dim_text;
use crate::app::{App, Focus, Message, PanelView};
use crate::i18n::tr;
use cosmic::iced::{Alignment, Color, Length};
use cosmic::widget::{self, button, column, container, icon, row, scrollable, text};
use cosmic::{Element, theme};
use tinta_core::keymap::Command;
use tinta_core::lsp::Severity;

fn tab_btn<'a>(label: String, active: bool, msg: Message) -> Element<'a, Message> {
    button::custom(text(label).size(12))
        .class(crate::ui::row_button(active))
        .padding([4, 10])
        .on_press(msg)
        .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let current = app.panel.unwrap_or(PanelView::Terminal);
    let mut header = row::with_capacity(8).spacing(2).align_y(Alignment::Center).width(Length::Fill);
    header = header.push(tab_btn(tr("Terminal").to_string(), current == PanelView::Terminal, Message::PanelShow(PanelView::Terminal)));
    let count: usize = app.project_diags.values().map(Vec::len).sum();
    let problems = if count > 0 { format!("{} ({count})", tr("Problems")) } else { tr("Problems").to_string() };
    header = header.push(tab_btn(problems, current == PanelView::Problems, Message::PanelShow(PanelView::Problems)));
    header = header.push(tab_btn(tr("Output").to_string(), current == PanelView::Output, Message::PanelShow(PanelView::Output)));
    if current == PanelView::Terminal {
        header = header.push(widget::Space::new().width(Length::Fixed(14.0)));
        for (i, s) in app.terms.iter().enumerate() {
            let title = if s.title.is_empty() { format!("{}", i + 1) } else { format!("{}: {}", i + 1, truncate(&s.title, 24)) };
            header = header.push(tab_btn(title, i == app.active_term, Message::TermSelect(i)));
        }
    }
    header = header.push(widget::Space::new().width(Length::Fill));
    if current == PanelView::Terminal {
        header = header.push(button::custom(icon::icon(icons::plus()).size(14)).padding(5).class(theme::Button::Icon).on_press(Message::Cmd(Command::NewTerminal)));
        header = header.push(button::custom(icon::icon(icons::minus()).size(14)).padding(5).class(theme::Button::Icon).on_press(Message::TermKill(app.active_term)));
    }
    header = header.push(button::custom(icon::icon(icons::close()).size(14)).padding(5).class(theme::Button::Icon).on_press(Message::PanelClose));

    let body: Element<'_, Message> = match current {
        PanelView::Terminal => terminal(app),
        PanelView::Problems => problems_view(app),
        PanelView::Output => output_view(app),
    };
    column::with_children(vec![container(header).padding([2, 8]).into(), container(body).width(Length::Fill).height(Length::Fill).into()]).into()
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let t: String = s.chars().take(n).collect();
    format!("{t}…")
}

fn terminal(app: &App) -> Element<'_, Message> {
    let Some(s) = app.terms.get(app.active_term) else {
        return container(text(tr("Terminal exited")).size(13)).padding(12).into();
    };
    let id = s.id;
    TermView {
        session: s,
        font: app.metrics.font,
        size: app.settings.terminal_font_size,
        focused: app.focus == Focus::Terminal && app.palette.is_none(),
        on_resize: Box::new(move |c, r, w, h| Message::TermResize(id, c, r, w, h)),
        on_focus: Message::TermFocus,
    }
    .into()
}

fn problems_view(app: &App) -> Element<'_, Message> {
    let t = cosmic::theme::active();
    let t = t.cosmic();
    let err: Color = t.destructive.base.into();
    let warn: Color = t.warning.base.into();
    let info: Color = t.accent.base.into();
    let list = app.problems();
    if list.is_empty() {
        return container(text(tr("No problems")).size(13).class(theme::Text::Color(dim_text()))).padding(12).into();
    }
    let mut col = column::with_capacity(list.len()).width(Length::Fill);
    for (path, line, ch, sev, msg) in list.into_iter().take(500) {
        let (h, color) = match sev {
            Severity::Error => (icons::error(), err),
            Severity::Warning => (icons::warning(), warn),
            _ => (icons::outline(), info),
        };
        let rel = app.folder.as_ref().and_then(|f| path.strip_prefix(f).ok()).unwrap_or(&path).display().to_string();
        let first = msg.lines().next().unwrap_or("").to_string();
        col = col.push(
            button::custom(
                row::with_children(vec![
                    icon::icon(h).size(13).class(theme::Svg::Custom(std::rc::Rc::new(move |_| cosmic::iced::widget::svg::Style { color: Some(color) }))).into(),
                    text(first).size(12).wrapping(cosmic::iced::widget::text::Wrapping::None).into(),
                    text(format!("{rel}:{}:{}", line + 1, ch + 1)).size(11).class(theme::Text::Color(dim_text())).wrapping(cosmic::iced::widget::text::Wrapping::None).into(),
                ])
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .class(crate::ui::row_button(false))
            .padding([2, 10])
            .width(Length::Fill)
            .on_press(Message::ProblemJump(path, line, ch)),
        );
    }
    scrollable(col).width(Length::Fill).height(Length::Fill).into()
}

fn output_view(app: &App) -> Element<'_, Message> {
    let start = app.output.len().saturating_sub(400);
    let mut col = column::with_capacity(app.output.len() - start).padding([4, 12]).width(Length::Fill);
    for l in &app.output[start..] {
        col = col.push(text(l.as_str()).size(12).font(app.metrics.font).wrapping(cosmic::iced::widget::text::Wrapping::None));
    }
    scrollable(col).width(Length::Fill).height(Length::Fill).anchor_bottom().into()
}
