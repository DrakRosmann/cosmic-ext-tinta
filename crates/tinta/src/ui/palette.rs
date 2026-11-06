//! The palette overlay.

use crate::app::{App, Message, PALETTE_INPUT, PItem, PaletteMode, PaletteMsg};
use crate::i18n::tr;
use cosmic::iced::widget::{mouse_area, rich_text, span};
use cosmic::iced::{Alignment, Color, Font, Length};
use cosmic::widget::{self, button, column, container, row, text};
use cosmic::{Element, theme};

/// Text with the fuzzy-matched letters picked out.
pub fn highlighted<'a>(label: &str, positions: &[u32], size: f32, accent: Color, font: Option<Font>) -> Element<'a, Message> {
    let mut spans = Vec::new();
    let mut cur = String::new();
    let mut cur_on = false;
    let bold = Font { weight: cosmic::iced::font::Weight::Bold, ..font.unwrap_or_else(cosmic::font::default) };
    let base = font.unwrap_or_else(cosmic::font::default);
    let mut push = |s: String, on: bool| {
        if s.is_empty() {
            return;
        }
        let sp = span::<(), Font>(s).size(size);
        spans.push(if on { sp.color(accent).font(bold) } else { sp.font(base) });
    };
    for (i, ch) in label.chars().enumerate() {
        let on = positions.binary_search(&(i as u32)).is_ok();
        if on != cur_on {
            push(std::mem::take(&mut cur), cur_on);
            cur_on = on;
        }
        cur.push(ch);
    }
    push(cur, cur_on);
    rich_text(spans).wrapping(cosmic::iced::widget::text::Wrapping::None).into()
}

fn row_view<'a>(item: &PItem, index: usize, selected: bool, accent: Color, dim: Color) -> Element<'a, Message> {
    let mut r = row::with_capacity(3).spacing(10).align_y(Alignment::Center).width(Length::Fill);
    r = r.push(highlighted(&item.label, &item.positions, 13.0, accent, None));
    if !item.detail.is_empty() {
        r = r.push(
            container(text(item.detail.clone()).size(12).class(theme::Text::Color(dim)).wrapping(cosmic::iced::widget::text::Wrapping::None))
                .clip(true)
                .width(Length::Fill),
        );
    } else {
        r = r.push(widget::Space::new().width(Length::Fill));
    }
    if !item.hint.is_empty() {
        r = r.push(text(item.hint.clone()).size(11).class(theme::Text::Color(dim)));
    }
    button::custom(r)
        .class(crate::ui::row_button(selected))
        .padding([5, 10])
        .width(Length::Fill)
        .on_press(Message::Palette(PaletteMsg::Click(index)))
        .into()
}

pub fn view(app: &App) -> Option<Element<'_, Message>> {
    let p = app.palette.as_ref()?;
    let placeholder = match p.mode {
        PaletteMode::Commands => tr("Type a command…"),
        PaletteMode::Files => tr("Search files by name… (> commands, : line, @ symbol)"),
        PaletteMode::Line => tr("Go to line…"),
        PaletteMode::Symbols => tr("Go to symbol…"),
        _ => tr("Select…"),
    };
    let input = widget::text_input(placeholder, p.query.as_str())
        .id(widget::Id::new(PALETTE_INPUT))
        .on_input(|s| Message::Palette(PaletteMsg::Input(s)))
        .on_submit(|_| Message::Palette(PaletteMsg::Submit)).on_focus(Message::InputFocus(true)).on_unfocus(Message::InputFocus(false))
        .width(Length::Fill);

    let t = cosmic::theme::active();
    let accent: Color = t.cosmic().accent.base.into();
    let dim = crate::ui::workspace::dim_text();
    const WINDOW: usize = 14;
    let start = if p.selected >= WINDOW / 2 { (p.selected - WINDOW / 2).min(p.items.len().saturating_sub(WINDOW)) } else { 0 };
    let mut list = column::with_capacity(WINDOW).spacing(1).width(Length::Fill);
    for (i, item) in p.items.iter().enumerate().skip(start).take(WINDOW) {
        list = list.push(row_view(item, i, i == p.selected, accent, dim));
    }
    let mut body = column::with_capacity(3).spacing(6).width(Length::Fill);
    body = body.push(input);
    if p.items.is_empty() && !p.query.is_empty() && p.mode != PaletteMode::Line {
        body = body.push(container(text(tr("No results")).size(13).class(theme::Text::Color(dim))).padding(10));
    } else {
        body = body.push(list);
    }
    if p.items.len() > WINDOW {
        body = body.push(
            container(text(format!("{} / {}", p.selected + 1, p.items.len())).size(11).class(theme::Text::Color(dim)))
                .width(Length::Fill)
                .align_x(Alignment::End),
        );
    }
    let card = container(body)
        .padding(10)
        .width(Length::Fixed(640.0))
        .class(theme::Container::Dialog(false));

    let backdrop = mouse_area(container(widget::Space::new()).width(Length::Fill).height(Length::Fill)).on_press(Message::Palette(PaletteMsg::Close));
    Some(
        cosmic::iced::widget::stack(vec![
            backdrop.into(),
            container(mouse_area(card)).width(Length::Fill).align_x(Alignment::Center).padding([48, 0, 0, 0]).into(),
        ])
        .width(Length::Fill)
        .height(Length::Fill)
        .into(),
    )
}
