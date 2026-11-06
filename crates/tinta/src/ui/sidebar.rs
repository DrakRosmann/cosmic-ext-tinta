//! The panels on the left: files, search, source control, outline.

use super::menu::row_menu;
use super::workspace::dim_text;
use super::{icons, palette::highlighted};
use crate::app::{App, ExplorerMsg, GitMsg, Message, SEARCH_INPUT, SearchMsg, SearchRow, SidebarView};
use crate::i18n::tr;
use crate::state::{DocEntry, Tab};
use cosmic::iced::{Alignment, Color, Length};
use cosmic::widget::{self, button, column, container, icon, row, scrollable, text};
use cosmic::{Element, theme};
use tinta_core::git::StatusEntry;
use tinta_core::keymap::Command;
use tinta_core::outline;

const ROW_H: f32 = 26.0;

pub fn view(app: &App) -> Element<'_, Message> {
    match app.sidebar {
        Some(SidebarView::Search) => search_view(app),
        Some(SidebarView::Git) => git_view(app),
        Some(SidebarView::Outline) => outline_view(app),
        _ => explorer_view(app),
    }
}

fn header<'a>(title: String, buttons: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    let mut r = row::with_capacity(buttons.len() + 2).align_y(Alignment::Center).padding([6, 8, 4, 12]).spacing(2);
    r = r.push(text(title.to_uppercase()).size(11).font(cosmic::font::semibold()).width(Length::Fill).wrapping(cosmic::iced::widget::text::Wrapping::None));
    for b in buttons {
        r = r.push(b);
    }
    r.into()
}

fn small_btn<'a>(h: icon::Handle, tip: &'static str, msg: Message) -> Element<'a, Message> {
    let b = button::custom(icon::icon(h).size(14)).padding(4).class(theme::Button::Icon).on_press(msg);
    widget::tooltip(b, text(tr(tip)).size(12), widget::tooltip::Position::Bottom).into()
}

fn git_color(mark: char) -> Color {
    let t = cosmic::theme::active();
    let t = t.cosmic();
    match mark {
        'A' | '?' => t.success.base.into(),
        'D' | 'U' => t.destructive.base.into(),
        _ => t.warning.base.into(),
    }
}

/// Icons from the theme for the languages it knows.
pub fn file_icon_name(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    match name {
        "Makefile" | "makefile" | "GNUmakefile" => return "text-x-makefile",
        "Dockerfile" => return "text-x-dockerfile",
        "CMakeLists.txt" => return "text-x-cmake",
        "justfile" | "Justfile" => return "text-x-just",
        _ => {}
    }
    match ext.as_str() {
        "rs" => "text-x-rust",
        "py" | "pyi" => "text-x-python",
        "c" => "text-x-c",
        "h" | "hpp" | "hh" => "text-x-cheader",
        "cpp" | "cc" | "cxx" => "text-x-c++",
        "cs" => "text-x-c#",
        "go" => "text-x-go",
        "java" => "text-x-java",
        "kt" | "kts" => "text-x-kotlin",
        "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" => "text-x-javascript",
        "json" => "text-x-json",
        "md" | "markdown" => "text-x-markdown",
        "html" | "htm" => "text-html",
        "css" | "scss" | "less" => "text-x-css",
        "lua" => "text-x-lua",
        "rb" => "text-x-ruby",
        "pl" | "pm" => "text-x-perl",
        "r" => "text-x-r",
        "hs" => "text-x-haskell",
        "php" => "application-x-php",
        "sh" | "bash" | "zsh" | "fish" => "text-x-script",
        "xml" | "svg" | "xsl" => "text-x-xml",
        "ron" | "toml" | "yaml" | "yml" | "ini" | "cfg" => "text-x-ron",
        "csv" | "tsv" => "text-x-csv",
        "pdf" => "application-pdf",
        _ => "text-x-generic",
    }
}

fn explorer_view(app: &App) -> Element<'_, Message> {
    let Some(folder) = &app.folder else {
        return column::with_children(vec![
            header(tr("Explorer").to_string(), Vec::new()),
            container(
                column::with_children(vec![
                    text(tr("No folder open")).size(13).class(theme::Text::Color(dim_text())).into(),
                    button::suggested(tr("Open Folder")).on_press(Message::Cmd(Command::OpenFolder)).into(),
                ])
                .spacing(10)
                .align_x(Alignment::Center),
            )
            .width(Length::Fill)
            .padding(16)
            .align_x(Alignment::Center)
            .into(),
        ])
        .into();
    };
    let name = folder.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let head = header(
        name,
        vec![
            small_btn(icons::new_file(), "New File", Message::Explorer(ExplorerMsg::NewFile(None))),
            small_btn(icons::new_folder(), "New Folder", Message::Explorer(ExplorerMsg::NewFolder(None))),
            small_btn(icons::refresh(), "Refresh", Message::Explorer(ExplorerMsg::Refresh)),
            small_btn(icons::collapse(), "Collapse All", Message::Explorer(ExplorerMsg::CollapseAll)),
        ],
    );
    let e = &app.explorer;
    let total = e.rows.len();
    let view_h = if e.view_h > 0.0 { e.view_h } else { 1000.0 };
    // Only what's on screen gets built, so a huge tree stays cheap on every keystroke.
    let first = (e.scroll_y / ROW_H).floor().max(0.0) as usize;
    let count = (view_h / ROW_H).ceil() as usize + 2;
    let last = (first + count).min(total);
    let first = first.min(last);
    let mut col = column::with_capacity(last - first + 2).width(Length::Fill);
    col = col.push(widget::Space::new().height(Length::Fixed(first as f32 * ROW_H)));
    let dim = dim_text();
    let active_path = app.active().and_then(|(_, d)| d.doc.path.clone());
    for i in first..last {
        let r = &e.rows[i];
        let indent = r.depth as f32 * 14.0 + 6.0;
        let lead: Element<'_, Message> = if r.is_dir {
            icon::icon(if r.expanded { icons::chevron_down() } else { icons::chevron_right() }).size(12).into()
        } else {
            widget::Space::new().width(Length::Fixed(12.0)).into()
        };
        let file_icon: Element<'_, Message> = if r.is_dir {
            icon::from_name(if r.expanded { "folder-open" } else { "folder" }).size(16).into()
        } else {
            icon::from_name(file_icon_name(&r.name)).size(16).into()
        };
        let mut label = text(r.name.as_str()).size(13).wrapping(cosmic::iced::widget::text::Wrapping::None);
        if let Some(m) = r.git {
            label = label.class(theme::Text::Color(git_color(m)));
        } else if r.ignored {
            label = label.class(theme::Text::Color(Color { a: 0.45, ..dim }));
        }
        let mut content = row::with_capacity(6).spacing(5).align_y(Alignment::Center).width(Length::Fill);
        content = content.push(widget::Space::new().width(Length::Fixed(indent)));
        content = content.push(lead);
        content = content.push(file_icon);
        content = content.push(container(label).clip(true).width(Length::Fill));
        if let Some(m) = r.git.filter(|_| !r.is_dir) {
            content = content.push(text(m.to_string()).size(11).class(theme::Text::Color(git_color(m))));
        }
        let selected = active_path.as_ref() == Some(&r.path) || e.selected.as_ref() == Some(&r.path);
        let b = button::custom(content)
            .class(crate::ui::row_button(selected))
            .padding([0, 8, 0, 0])
            .height(Length::Fixed(ROW_H))
            .width(Length::Fill)
            .on_press(Message::Explorer(ExplorerMsg::Click(i)));
        col = col.push(widget::context_menu(b, Some(row_menu(i, r.is_dir))));
    }
    col = col.push(widget::Space::new().height(Length::Fixed((total - last) as f32 * ROW_H)));
    column::with_children(vec![
        head,
        scrollable(col)
            .on_scroll(|v| Message::Explorer(ExplorerMsg::Scrolled(v.absolute_offset().y, v.bounds().height)))
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
    ])
    .into()
}

fn chip<'a>(label: &'static str, on: bool, tip: &'static str, msg: SearchMsg) -> Element<'a, Message> {
    let b = button::custom(text(label).size(12).font(cosmic::font::mono()))
        .padding([4, 6])
        .class(if on { theme::Button::Suggested } else { theme::Button::Text })
        .on_press(Message::Search(msg));
    widget::tooltip(b, text(tr(tip)).size(12), widget::tooltip::Position::Bottom).into()
}

fn search_view(app: &App) -> Element<'_, Message> {
    let s = &app.search;
    let head = header(tr("Search").to_string(), vec![small_btn(icons::refresh(), "Refresh", Message::Search(SearchMsg::Run))]);
    let query = widget::text_input(tr("Find"), s.query.pattern.as_str())
        .id(widget::Id::new(SEARCH_INPUT))
        .on_input(|v| Message::Search(SearchMsg::Query(v)))
        .on_submit(|_| Message::Search(SearchMsg::Run)).on_focus(Message::InputFocus(true)).on_unfocus(Message::InputFocus(false))
        .size(13);
    let toggles = row::with_children(vec![
        small_btn(if s.replace_open { icons::chevron_down() } else { icons::chevron_right() }, "Toggle replace", Message::Search(SearchMsg::ToggleReplace)),
        chip("Aa", s.query.case_sensitive, "Match case", SearchMsg::ToggleCase),
        chip("ab", s.query.whole_word, "Whole word", SearchMsg::ToggleWord),
        chip(".*", s.query.regex, "Regular expression", SearchMsg::ToggleRegex),
    ])
    .spacing(3);
    let mut form = column::with_capacity(6).spacing(5).padding([0, 10, 8, 10]);
    form = form.push(query).push(toggles);
    if s.replace_open {
        form = form.push(
            widget::text_input(tr("Replace"), s.replacement.as_str())
                .on_input(|v| Message::Search(SearchMsg::Replacement(v))).on_focus(Message::InputFocus(true)).on_unfocus(Message::InputFocus(false))
                .size(13),
        );
        form = form.push(button::standard(tr("Replace All")).on_press(Message::Search(SearchMsg::ReplaceAll)));
    }
    form = form.push(
        widget::text_input(tr("files to include"), s.include.as_str()).on_input(|v| Message::Search(SearchMsg::Include(v))).on_focus(Message::InputFocus(true)).on_unfocus(Message::InputFocus(false)).size(12),
    );
    form = form.push(
        widget::text_input(tr("files to exclude"), s.exclude.as_str()).on_input(|v| Message::Search(SearchMsg::Exclude(v))).on_focus(Message::InputFocus(true)).on_unfocus(Message::InputFocus(false)).size(12),
    );
    let dim = dim_text();
    let summary: String = if let Some(e) = &s.error {
        e.clone()
    } else if s.running {
        tr("Searching…").to_string()
    } else if s.query.pattern.is_empty() {
        String::new()
    } else if s.total == 0 {
        tr("No results").to_string()
    } else {
        let plus = if s.truncated { "+" } else { "" };
        format!("{}{plus} {} {} {}", s.total, tr("matches in"), s.results.len(), tr("files"))
    };
    form = form.push(text(summary).size(12).class(theme::Text::Color(dim)));

    let total = s.rows.len();
    let view_h = if s.view_h > 0.0 { s.view_h } else { 800.0 };
    let first = ((s.scroll_y / ROW_H).floor().max(0.0) as usize).min(total);
    let last = (first + (view_h / ROW_H).ceil() as usize + 2).min(total);
    let accent: Color = cosmic::theme::active().cosmic().accent.base.into();
    let mut col = column::with_capacity(last - first + 2).width(Length::Fill);
    col = col.push(widget::Space::new().height(Length::Fixed(first as f32 * ROW_H)));
    for r in &s.rows[first..last] {
        match r {
            SearchRow::File { path, count, collapsed } => {
                let rel = app.folder.as_ref().and_then(|f| path.strip_prefix(f).ok()).unwrap_or(path);
                let name = rel.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                let dir = rel.parent().map(|d| d.display().to_string()).unwrap_or_default();
                let content = row::with_children(vec![
                    icon::icon(if *collapsed { icons::chevron_right() } else { icons::chevron_down() }).size(12).into(),
                    icon::from_name(file_icon_name(&name)).size(16).into(),
                    text(name).size(13).wrapping(cosmic::iced::widget::text::Wrapping::None).into(),
                    container(text(dir).size(11).class(theme::Text::Color(dim)).wrapping(cosmic::iced::widget::text::Wrapping::None)).clip(true).width(Length::Fill).into(),
                    text(count.to_string()).size(11).class(theme::Text::Color(dim)).into(),
                ])
                .spacing(5)
                .align_y(Alignment::Center);
                col = col.push(
                    button::custom(content)
                        .class(crate::ui::row_button(false))
                        .padding([0, 8])
                        .height(Length::Fixed(ROW_H))
                        .width(Length::Fill)
                        .on_press(Message::Search(SearchMsg::ToggleFile(path.clone()))),
                );
            }
            SearchRow::Match { path, line, start, end, text: t } => {
                // Skip leading blanks and keep the match in view of a narrow sidebar.
                let ws = t.chars().take_while(|c| c.is_whitespace()).count();
                let keep_from = start.saturating_sub(16).max(ws);
                let shown: String = t.chars().skip(keep_from).collect();
                let positions: Vec<u32> = ((start - keep_from) as u32..(end - keep_from) as u32).collect();
                let content = row::with_children(vec![
                    widget::Space::new().width(Length::Fixed(22.0)).into(),
                    container(highlighted(&shown, &positions, 12.0, accent, None)).clip(true).width(Length::Fill).into(),
                ]);
                col = col.push(
                    button::custom(content)
                        .class(crate::ui::row_button(false))
                        .padding([0, 8])
                        .height(Length::Fixed(ROW_H))
                        .width(Length::Fill)
                        .on_press(Message::Search(SearchMsg::Open(path.clone(), *line, *start, *end))),
                );
            }
        }
    }
    col = col.push(widget::Space::new().height(Length::Fixed((total - last) as f32 * ROW_H)));
    column::with_children(vec![
        head,
        form.into(),
        scrollable(col)
            .on_scroll(|v| Message::Search(SearchMsg::Scrolled(v.absolute_offset().y, v.bounds().height)))
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
    ])
    .into()
}

fn git_entry<'a>(e: &'a StatusEntry, staged: bool) -> Element<'a, Message> {
    let mark = if staged { e.index } else if e.worktree == '?' { 'A' } else { e.worktree };
    let (name, dir) = match e.path.rsplit_once('/') {
        Some((d, n)) => (n, d),
        None => (e.path.as_str(), ""),
    };
    let mut r = row::with_capacity(6).spacing(5).align_y(Alignment::Center).width(Length::Fill);
    r = r.push(icon::from_name(file_icon_name(name)).size(16));
    r = r.push(text(name).size(13).class(theme::Text::Color(git_color(mark))).wrapping(cosmic::iced::widget::text::Wrapping::None));
    r = r.push(container(text(dir).size(11).class(theme::Text::Color(dim_text())).wrapping(cosmic::iced::widget::text::Wrapping::None)).clip(true).width(Length::Fill));
    let open = button::custom(r)
        .class(crate::ui::row_button(false))
        .padding([0, 4, 0, 10])
        .height(Length::Fixed(ROW_H))
        .width(Length::Fill)
        .on_press(Message::Git(GitMsg::Open(e.path.clone(), staged)));
    let mut outer = row::with_capacity(4).align_y(Alignment::Center).width(Length::Fill);
    outer = outer.push(open);
    if staged {
        outer = outer.push(small_btn(icons::minus(), "Unstage", Message::Git(GitMsg::Unstage(e.path.clone()))));
    } else {
        outer = outer.push(small_btn(icons::undo(), "Discard", Message::Git(GitMsg::Discard(e.clone()))));
        outer = outer.push(small_btn(icons::plus(), "Stage", Message::Git(GitMsg::Stage(e.path.clone()))));
    }
    outer = outer.push(text(mark.to_string()).size(11).class(theme::Text::Color(git_color(mark))).width(Length::Fixed(14.0)));
    outer.into()
}

fn git_view(app: &App) -> Element<'_, Message> {
    let g = &app.git;
    let head = header(
        tr("Source Control").to_string(),
        vec![
            small_btn(icons::pull(), "Pull", Message::Cmd(Command::GitPull)),
            small_btn(icons::push(), "Push", Message::Cmd(Command::GitPush)),
            small_btn(icons::refresh(), "Refresh status", Message::Cmd(Command::GitRefresh)),
        ],
    );
    if g.root.is_none() {
        return column::with_children(vec![
            head,
            container(text(tr("Not a git repository")).size(13).class(theme::Text::Color(dim_text()))).padding(14).into(),
        ])
        .into();
    }
    let input = widget::text_input(tr("Message"), g.message.as_str())
        .on_input(|s| Message::Git(GitMsg::MessageInput(s)))
        .on_submit(|_| Message::Git(GitMsg::Commit)).on_focus(Message::InputFocus(true)).on_unfocus(Message::InputFocus(false))
        .size(13);
    let branch = g.branch.clone().unwrap_or_default();
    let commit = button::suggested(format!("{} · {branch}", tr("Commit"))).width(Length::Fill).on_press_maybe((!g.message.trim().is_empty()).then_some(Message::Git(GitMsg::Commit)));
    let staged: Vec<&StatusEntry> = g.entries.iter().filter(|e| e.is_staged()).collect();
    let changed: Vec<&StatusEntry> = g.entries.iter().filter(|e| e.is_unstaged()).collect();
    let mut col = column::with_capacity(g.entries.len() + 4).width(Length::Fill);
    if !staged.is_empty() {
        col = col.push(container(text(format!("{} ({})", tr("Staged Changes"), staged.len())).size(12).font(cosmic::font::semibold())).padding([8, 12, 4, 12]));
        for e in staged {
            col = col.push(git_entry(e, true));
        }
    }
    col = col.push(
        row::with_children(vec![
            container(text(format!("{} ({})", tr("Changes"), changed.len())).size(12).font(cosmic::font::semibold())).padding([8, 0, 4, 12]).width(Length::Fill).into(),
            small_btn(icons::plus(), "Stage", Message::Git(GitMsg::StageAll)),
        ])
        .align_y(Alignment::Center),
    );
    if changed.is_empty() {
        col = col.push(container(text(tr("No changes")).size(12).class(theme::Text::Color(dim_text()))).padding([2, 14]));
    }
    for e in changed {
        col = col.push(git_entry(e, false));
    }
    column::with_children(vec![
        head,
        container(column::with_children(vec![input.into(), commit.into()]).spacing(6)).padding([0, 10, 6, 10]).into(),
        scrollable(col).width(Length::Fill).height(Length::Fill).into(),
    ])
    .into()
}

/// What's listed in the outline: LSP's list when it's current, the pattern matcher otherwise.
pub fn symbols_of(e: &DocEntry) -> Vec<(String, String, usize, usize)> {
    if let Some((rev, list)) = &e.lsp_symbols
        && *rev == e.doc.revision
        && !list.is_empty()
    {
        return list.iter().map(|s| (s.name.clone(), s.detail.clone(), s.line, s.depth)).collect();
    }
    outline::symbols(&e.doc).into_iter().map(|s| (s.name, s.kind.to_string(), s.line, s.depth)).collect()
}

/// The symbols around the caret, outermost first, for the breadcrumbs.
pub fn current_symbols(e: &DocEntry, t: &Tab) -> Vec<String> {
    let line = e.doc.char_to_line(t.cursors.head());
    let mut chain: Vec<(usize, &str)> = Vec::new();
    for s in e.symbols.iter().take_while(|s| s.line <= line) {
        while chain.last().is_some_and(|(d, _)| *d >= s.depth) {
            chain.pop();
        }
        chain.push((s.depth, s.name.as_str()));
    }
    // A symbol further out than the caret's own line has ended.
    let tab = e.doc.indent.width.max(1);
    let indent = tinta_core::display::line_indent_vcols(&e.doc, line, tab) / tab;
    if !e.doc.line_is_blank(line) {
        let last_here = e.symbols.iter().any(|s| s.line == line);
        chain.retain(|(d, _)| *d < indent || (last_here && *d == indent));
    }
    chain.into_iter().map(|(_, n)| n.to_string()).collect()
}

fn outline_view(app: &App) -> Element<'_, Message> {
    let head = header(tr("Outline").to_string(), Vec::new());
    let Some((t, e)) = app.active() else {
        return column::with_children(vec![head, container(text(tr("No symbols")).size(13).class(theme::Text::Color(dim_text()))).padding(14).into()]).into();
    };
    let symbols = symbols_of(e);
    let line = e.doc.char_to_line(t.cursors.head());
    let current = symbols.iter().rposition(|s| s.2 <= line);
    let mut col = column::with_capacity(symbols.len()).width(Length::Fill);
    if symbols.is_empty() {
        col = col.push(container(text(tr("No symbols")).size(13).class(theme::Text::Color(dim_text()))).padding(14));
    }
    let dim = dim_text();
    for (i, (name, kind, l, depth)) in symbols.into_iter().enumerate().take(5000) {
        let content = row::with_children(vec![
            widget::Space::new().width(Length::Fixed(depth as f32 * 12.0 + 6.0)).into(),
            text(name).size(13).wrapping(cosmic::iced::widget::text::Wrapping::None).into(),
            container(text(kind).size(11).class(theme::Text::Color(dim)).wrapping(cosmic::iced::widget::text::Wrapping::None)).clip(true).width(Length::Fill).into(),
        ])
        .spacing(8)
        .align_y(Alignment::Center);
        col = col.push(
            button::custom(content)
                .class(crate::ui::row_button(Some(i) == current))
                .padding([0, 8])
                .height(Length::Fixed(ROW_H))
                .width(Length::Fill)
                .on_press(Message::OutlineJump(l)),
        );
    }
    column::with_children(vec![head, scrollable(col).width(Length::Fill).height(Length::Fill).into()]).into()
}
