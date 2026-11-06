//! The window's layout: activity rail, sidebar, editor groups, panel, status bar.

use super::editor_view::{EditorView, Geometry};
use super::markdown;
use super::sash::sash;
use super::{icons, palette, panel, popups, sidebar, status};
use crate::app::{App, FIND_INPUT, FindMsg, Focus, Message, REPLACE_INPUT, SidebarView};
use crate::i18n::tr;
use crate::state::TabKind;
use cosmic::iced::widget::{pin, stack};
use cosmic::iced::{Alignment, Color, Length};
use cosmic::widget::{self, button, column, container, icon, row, tab_bar, text};
use cosmic::{Element, theme};
use tinta_core::keymap::Command;

pub fn header_buttons(app: &App) -> Element<'_, Message> {
    let tip = |content: Element<'static, Message>, label: String| -> Element<'static, Message> {
        widget::tooltip(content, text(label).size(12), widget::tooltip::Position::Bottom).into()
    };
    let pt = app.pt();
    row::with_children(vec![
        tip(button::icon(icon::Handle::from(icons::split())).on_press(Message::Cmd(Command::SplitRight)).into(), Command::SplitRight.title(pt).to_string()),
        tip(button::icon(icon::Handle::from(icons::terminal())).on_press(Message::Cmd(Command::ToggleTerminal)).into(), Command::ToggleTerminal.title(pt).to_string()),
        tip(button::icon(icon::Handle::from(icons::settings())).on_press(Message::Cmd(Command::OpenSettings)).into(), tr("Settings").to_string()),
    ])
    .spacing(2)
    .align_y(Alignment::Center)
    .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let editors = editors_area(app);
    let center: Element<'_, Message> = if app.zen {
        editors
    } else if app.panel.is_some() {
        column::with_children(vec![
            container(editors).height(Length::Fill).into(),
            sash(false, Message::PanelResize).into(),
            container(panel::view(app)).height(Length::Fixed(app.panel_h)).width(Length::Fill).into(),
        ])
        .into()
    } else {
        editors
    };

    let main: Element<'_, Message> = if app.zen {
        center
    } else {
        let mut r = row::with_capacity(4).width(Length::Fill).height(Length::Fill);
        r = r.push(activity_rail(app));
        if app.sidebar.is_some() {
            r = r.push(container(sidebar::view(app)).width(Length::Fixed(app.sidebar_w)).height(Length::Fill));
            r = r.push(sash(true, Message::SidebarResize));
        }
        r.push(container(center).width(Length::Fill).height(Length::Fill)).into()
    };

    let body: Element<'_, Message> = if app.zen {
        main
    } else {
        column::with_children(vec![container(main).height(Length::Fill).into(), status::view(app)]).into()
    };

    let mut layers: Vec<Element<'_, Message>> = vec![files_drop_target(body)];
    if let Some(p) = palette::view(app) {
        layers.push(p);
    }
    if let Some(t) = &app.toast {
        layers.push(toast(t));
    }
    container(stack(layers).width(Length::Fill).height(Length::Fill))
        .class(theme::Container::Background)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Files dragged in from the file manager open like any others.
fn files_drop_target(content: Element<'_, Message>) -> Element<'_, Message> {
    cosmic::widget::dnd_destination::dnd_destination_for_data::<DroppedFiles, Message>(content, |data, _action| match data {
        Some(d) => Message::Dropped(d.0),
        None => Message::Noop,
    })
    .into()
}

#[derive(Debug, Clone)]
pub struct DroppedFiles(pub Vec<std::path::PathBuf>);

impl TryFrom<(Vec<u8>, String)> for DroppedFiles {
    type Error = ();
    fn try_from((bytes, _mime): (Vec<u8>, String)) -> Result<Self, ()> {
        let text = String::from_utf8_lossy(&bytes);
        let files: Vec<_> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .filter_map(tinta_core::lsp::uri_to_path)
            .collect();
        if files.is_empty() { Err(()) } else { Ok(Self(files)) }
    }
}

impl cosmic::iced::clipboard::mime::AllowedMimeTypes for DroppedFiles {
    fn allowed() -> std::borrow::Cow<'static, [String]> {
        std::borrow::Cow::Owned(vec!["text/uri-list".to_string()])
    }
}

fn toast(t: &str) -> Element<'_, Message> {
    container(
        container(text(t).size(13))
            .padding([8, 14])
            .class(theme::Container::Tooltip),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(Alignment::End)
    .align_y(Alignment::End)
    .padding([0, 18, 36, 0])
    .into()
}

fn activity_rail(app: &App) -> Element<'_, Message> {
    let item = |h: icon::Handle, view: SidebarView, tip: &'static str| -> Element<'_, Message> {
        let active = app.sidebar == Some(view);
        let b = button::custom(icon::icon(h).size(20))
            .padding(8)
            .class(if active { theme::Button::Suggested } else { theme::Button::Icon })
            .on_press(Message::SidebarShow(view));
        widget::tooltip(b, text(tr(tip)).size(12), widget::tooltip::Position::Right).into()
    };
    let changes = app.git.entries.len();
    let git_item = item(icons::git(), SidebarView::Git, "Source Control");
    let git_el: Element<'_, Message> = if changes > 0 {
        column::with_children(vec![git_item, container(text(changes.to_string()).size(10)).center_x(Length::Fixed(36.0)).into()]).into()
    } else {
        git_item
    };
    container(
        column::with_children(vec![
            item(icons::files(), SidebarView::Explorer, "Explorer"),
            item(icons::search(), SidebarView::Search, "Search"),
            git_el,
            item(icons::outline(), SidebarView::Outline, "Outline"),
        ])
        .spacing(4)
        .padding([6, 4])
        .align_x(Alignment::Center),
    )
    .width(Length::Fixed(44.0))
    .height(Length::Fill)
    .into()
}

fn editors_area(app: &App) -> Element<'_, Message> {
    let mut r = row::with_capacity(app.groups.len() * 2).width(Length::Fill).height(Length::Fill);
    for (i, g) in app.groups.iter().enumerate() {
        let weight = (g.weight * 100.0).round().max(1.0) as u16;
        r = r.push(container(group_view(app, i)).width(Length::FillPortion(weight)).height(Length::Fill));
        if i + 1 < app.groups.len() {
            r = r.push(sash(true, move |d| Message::GroupResize(i, d)));
        }
    }
    // Any change in the groups starts them from clean state: see `fresh`.
    if app.groups_changed.take() {
        app.groups_flip.set(!app.groups_flip.get());
    }
    super::fresh::Fresh { flip: app.groups_flip.get(), content: r.into() }.into()
}

fn group_view(app: &App, g: usize) -> Element<'_, Message> {
    let group = &app.groups[g];
    let active_group = g == app.active_group;
    let Some(tab_id) = group.active_tab() else {
        return welcome(app);
    };
    let Some(tab) = app.tabs.get(&tab_id) else { return welcome(app) };
    let Some(entry) = app.docs.get(&tab.doc) else { return welcome(app) };

    let tabs = tab_bar::horizontal(&group.model)
        .button_height(36)
        .on_activate(move |e| Message::TabActivate(g, e))
        .on_close(move |e| Message::TabClose(g, e))
        .on_middle_press(move |e| Message::TabClose(g, e));
    let mut col = column::with_capacity(4).width(Length::Fill).height(Length::Fill);
    col = col.push(container(tabs).width(Length::Fill));
    if !app.zen {
        col = col.push(breadcrumbs(app, g));
    }
    if active_group && app.find.open && tab.kind == TabKind::Text {
        col = col.push(find_bar(app));
    }
    if tab.kind == TabKind::Preview {
        const EMPTY: &[markdown::Block] = &[];
        let blocks = tab.preview_cache.as_ref().map_or(EMPTY, |(_, b)| b.as_slice());
        return col.push(markdown::view(blocks, app.metrics.font)).into();
    }
    let blame = entry.blame.as_ref().filter(|_| app.settings.inline_blame && active_group).map(|(_, s)| s.as_str()).filter(|s| !s.is_empty());
    let editor: Element<'_, Message> = EditorView {
        entry,
        tab,
        metrics: app.metrics,
        settings: &app.settings,
        focused: active_group && app.focus == Focus::Editor && app.palette.is_none() && app.dialog.is_none(),
        current_match: if active_group { app.find.current } else { None },
        inline_blame: blame,
        on_event: Box::new(move |ev| Message::Editor(tab_id, ev)),
    }
    .into();
    let editor: Element<'_, Message> =
        cosmic::widget::context_menu(editor, Some(super::menu::editor_menu(app))).into();

    let mut layers = vec![editor];
    if active_group {
        let geo = Geometry::new(entry.doc.len_lines(), tab.view_w, &app.metrics, &app.settings);
        let at = |pos: usize| -> (f32, f32) {
            let (row, v) = tab.map.pos_to_row(&entry.doc, pos);
            let pad = tab.map.row(&entry.doc, row).pad as f32;
            (
                geo.text_x + (v as f32 + pad) * app.metrics.cell_w - tab.scroll_x,
                row as f32 * app.metrics.line_h - tab.scroll_y,
            )
        };
        let place = |content: Element<'static, Message>, pos: usize, w: f32, h: f32| -> Element<'static, Message> {
            let (x, y) = at(pos);
            // Below the line when there's room, above when there isn't.
            let below = y + app.metrics.line_h + 2.0;
            let py = if below + h > tab.view_h && y - h - 2.0 > 0.0 { y - h - 2.0 } else { below };
            let px = x.min((tab.view_w - w - 8.0).max(0.0)).max(0.0);
            pin(content).x(px).y(py).width(Length::Fill).height(Length::Fill).into()
        };
        if let Some(c) = &app.completion
            && c.tab == tab_id
            && !c.visible.is_empty()
        {
            // Whichever side of the line has more room, with the list cut down to fit it.
            let (x, y) = at(c.anchor);
            let below = (tab.view_h - y - app.metrics.line_h - 16.0).max(0.0);
            let above = (y - 16.0).max(0.0);
            let room = below.max(above);
            let wanted = c.visible.len().min(popups::COMPLETION_ROWS);
            let rows = ((room / popups::COMPLETION_ROW_H) as usize).clamp(1, wanted.max(1));
            let h = rows as f32 * popups::COMPLETION_ROW_H + 12.0;
            let py = if h <= below || below >= above { y + app.metrics.line_h + 2.0 } else { y - h - 2.0 };
            let px = x.min((tab.view_w - popups::COMPLETION_W - 8.0).max(0.0)).max(0.0);
            layers.push(pin(popups::completion(c, app.metrics.font, rows)).x(px).y(py).width(Length::Fill).height(Length::Fill).into());
        } else if let Some(h) = &app.hover {
            layers.push(place(popups::hover(&h.text, app.metrics.font, tab.view_w), h.pos, 560.0, 220.0));
        }
        if let Some(s) = &app.signature {
            // Above the line, so it doesn't cover what's being typed in the next one.
            let (x, y) = at(s.pos);
            let px = x.min((tab.view_w - 500.0).max(0.0)).max(0.0);
            let py = (y - 44.0).max(0.0);
            layers.push(pin(popups::signature(s, app.metrics.font)).x(px).y(py).width(Length::Fill).height(Length::Fill).into());
        }
    }
    col.push(stack(layers).width(Length::Fill).height(Length::Fill)).into()
}

fn breadcrumbs(app: &App, g: usize) -> Element<'_, Message> {
    let tab = app.groups[g].active_tab().and_then(|t| app.tabs.get(&t));
    let dim = dim_text();
    let mut parts: Vec<String> = Vec::new();
    if let Some(t) = tab
        && let Some(e) = app.docs.get(&t.doc)
    {
        match &e.doc.path {
            Some(p) => {
                let rel = app.folder.as_ref().and_then(|f| p.strip_prefix(f).ok()).unwrap_or(p);
                parts.extend(rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).filter(|s| s != "/"));
            }
            None => parts.push(e.doc.file_name()),
        }
        if t.kind == TabKind::Text {
            parts.extend(crate::ui::sidebar::current_symbols(e, t));
        }
    }
    let label = parts.join("  ›  ");
    container(text(label).size(12).class(theme::Text::Color(dim)))
        .padding([3, 12])
        .width(Length::Fill)
        .into()
}

pub fn dim_text() -> Color {
    let on: Color = cosmic::theme::active().cosmic().background(false).on.into();
    Color { a: 0.6, ..on }
}

fn welcome(app: &App) -> Element<'_, Message> {
    let pt = app.pt();
    let line = |c: Command| -> Element<'_, Message> {
        let hint = app.keymap.label(c).unwrap_or("").to_string();
        button::custom(
            row::with_children(vec![
                text(c.title(pt)).size(14).width(Length::Fill).into(),
                text(hint).size(13).class(theme::Text::Color(dim_text())).into(),
            ])
            .width(Length::Fixed(340.0)),
        )
        .class(crate::ui::row_button(false))
        .padding([6, 10])
        .on_press(Message::Cmd(c))
        .into()
    };
    let content = column::with_children(vec![
        text("Tinta").size(40).font(cosmic::font::light()).into(),
        text(tr("Open a file or folder to get started")).size(14).class(theme::Text::Color(dim_text())).into(),
        widget::Space::new().height(Length::Fixed(14.0)).into(),
        line(Command::NewFile),
        line(Command::OpenFile),
        line(Command::OpenFolder),
        line(Command::OpenRecent),
        line(Command::QuickOpen),
        line(Command::CommandPalette),
        line(Command::OpenSettings),
    ])
    .spacing(4)
    .align_x(Alignment::Center);
    container(content).width(Length::Fill).height(Length::Fill).center(Length::Fill).into()
}

fn toggle_chip<'a>(label: &'static str, on: bool, tip: &'static str, msg: FindMsg) -> Element<'a, Message> {
    let b = button::custom(text(label).size(12).font(cosmic::font::mono()))
        .padding([4, 7])
        .class(if on { theme::Button::Suggested } else { theme::Button::Text })
        .on_press(Message::Find(msg));
    widget::tooltip(b, text(tr(tip)).size(12), widget::tooltip::Position::Bottom).into()
}

fn icon_btn<'a>(h: icon::Handle, tip: &'static str, msg: Message) -> Element<'a, Message> {
    let b = button::custom(icon::icon(h).size(14)).padding(5).class(theme::Button::Icon).on_press(msg);
    widget::tooltip(b, text(tr(tip)).size(12), widget::tooltip::Position::Bottom).into()
}

fn find_bar(app: &App) -> Element<'_, Message> {
    let f = &app.find;
    let total = app.active().map_or(0, |(_, e)| e.matches.len());
    let count = if f.query.pattern.is_empty() {
        String::new()
    } else if total == 0 {
        tr("No results").to_string()
    } else {
        format!("{} {} {total}", f.current.map_or(0, |c| c + 1), tr("of"))
    };
    let input = widget::text_input(tr("Find"), f.query.pattern.as_str())
        .id(widget::Id::new(FIND_INPUT))
        .on_input(|s| Message::Find(FindMsg::Query(s)))
        .on_submit(|_| Message::Find(FindMsg::Next)).on_focus(Message::InputFocus(true)).on_unfocus(Message::InputFocus(false))
        .size(13)
        .width(Length::Fill);
    let mut line1 = row::with_capacity(10).spacing(3).align_y(Alignment::Center);
    line1 = line1.push(icon_btn(
        if f.replace_open { icons::chevron_down() } else { icons::chevron_right() },
        "Toggle replace",
        Message::Find(FindMsg::ToggleReplace),
    ));
    line1 = line1.push(container(input).width(Length::FillPortion(3)));
    line1 = line1.push(toggle_chip("Aa", f.query.case_sensitive, "Match case", FindMsg::ToggleCase));
    line1 = line1.push(toggle_chip("ab", f.query.whole_word, "Whole word", FindMsg::ToggleWord));
    line1 = line1.push(toggle_chip(".*", f.query.regex, "Regular expression", FindMsg::ToggleRegex));
    line1 = line1.push(toggle_chip("≡", f.in_selection.is_some(), "Find in selection", FindMsg::ToggleInSelection));
    line1 = line1.push(container(text(count).size(12).class(theme::Text::Color(dim_text()))).padding([0, 6]));
    line1 = line1.push(icon_btn(icons::chevron_up(), "Previous match", Message::Find(FindMsg::Prev)));
    line1 = line1.push(icon_btn(icons::chevron_down(), "Next match", Message::Find(FindMsg::Next)));
    line1 = line1.push(icon_btn(icons::close(), "Close", Message::Find(FindMsg::Close)));
    let mut col = column::with_capacity(3).spacing(4);
    col = col.push(line1);
    if let Some(e) = &f.error {
        col = col.push(text(e.as_str()).size(11).class(theme::Text::Color(cosmic::theme::active().cosmic().destructive.base.into())));
    }
    if f.replace_open {
        let rep = widget::text_input(tr("Replace"), f.replacement.as_str())
            .id(widget::Id::new(REPLACE_INPUT))
            .on_input(|s| Message::Find(FindMsg::Replacement(s)))
            .on_submit(|_| Message::Find(FindMsg::ReplaceOne)).on_focus(Message::InputFocus(true)).on_unfocus(Message::InputFocus(false))
            .size(13)
            .width(Length::Fill);
        col = col.push(
            row::with_children(vec![
                widget::Space::new().width(Length::Fixed(27.0)).into(),
                container(rep).width(Length::FillPortion(3)).into(),
                button::text(tr("Replace")).on_press(Message::Find(FindMsg::ReplaceOne)).into(),
                button::text(tr("Replace All")).on_press(Message::Find(FindMsg::ReplaceAll)).into(),
            ])
            .spacing(3)
            .align_y(Alignment::Center),
        );
    }
    container(col).padding([6, 10]).width(Length::Fill).class(theme::Container::Card).into()
}
