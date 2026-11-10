//! The header bar's buttons, which the user picks: any command of the table, on either side, in
//! any order. The menu bar stays first on the left; a right click on the buttons, or "Customize
//! Header Bar…" in the File menu, opens the editor for the rest.

use super::icons;
use super::workspace::dim_text;
use crate::app::{App, DIALOG_INPUT, DialogMsg, Message, SettingMsg};
use crate::i18n::tr;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, button, column, container, icon, row, scrollable, text};
use cosmic::{Element, theme};
use tinta_core::keymap::Command;
use tinta_core::settings::{default_header_end, default_header_start};

/// An icon for the commands that have an obvious one; the rest show their name.
pub fn icon_for(c: Command) -> Option<icon::Handle> {
    use Command as C;
    let named = icons::named;
    Some(match c {
        C::NewFile => named("document-new-symbolic"),
        C::OpenFile => named("document-open-symbolic"),
        C::OpenFolder => named("folder-open-symbolic"),
        C::Save => named("document-save-symbolic"),
        C::SaveAs => named("document-save-as-symbolic"),
        C::CloseTab => icons::close(),
        C::Undo => named("edit-undo-symbolic"),
        C::Redo => named("edit-redo-symbolic"),
        C::Cut => named("edit-cut-symbolic"),
        C::Copy => named("edit-copy-symbolic"),
        C::Paste => named("edit-paste-symbolic"),
        C::Find => named("edit-find-symbolic"),
        C::Replace => named("edit-find-replace-symbolic"),
        C::FindInFiles | C::ShowSearch => icons::search(),
        C::CommandPalette => icons::grid(),
        C::QuickOpen => named("document-open-recent-symbolic"),
        C::ToggleSidebar => icons::sidebar(),
        C::ShowExplorer => icons::files(),
        C::ShowGit => icons::git(),
        C::ShowOutline => icons::outline(),
        C::ShowDebug => icons::debug(),
        C::TogglePanel => icons::panel_bottom(),
        C::ShowProblems => icons::warning(),
        C::SplitRight => icons::split(),
        C::ToggleWordWrap => named("format-justify-left-symbolic"),
        C::MarkdownPreview => icons::eye(),
        C::ZoomIn => named("zoom-in-symbolic"),
        C::ZoomOut => named("zoom-out-symbolic"),
        C::ZoomReset => named("zoom-original-symbolic"),
        C::ToggleFullscreen => named("view-fullscreen-symbolic"),
        C::ToggleZen => named("weather-clear-night-symbolic"),
        C::GoBack => named("go-previous-symbolic"),
        C::GoForward => named("go-next-symbolic"),
        C::FormatDocument => named("format-indent-more-symbolic"),
        C::GitCommit => named("object-select-symbolic"),
        C::GitPush => icons::push(),
        C::GitPull => icons::pull(),
        C::ToggleTerminal | C::NewTerminal => icons::terminal(),
        C::RunBuild => named("system-run-symbolic"),
        C::RunFile => icons::play(),
        C::RunStop | C::StopTask | C::DebugStop => icons::stop(),
        C::DebugStart => named("media-seek-forward-symbolic"),
        C::DebugPause => icons::pause(),
        C::DebugRestart => icons::refresh(),
        C::StepOver => icons::step_over(),
        C::StepInto => icons::step_in(),
        C::StepOut => icons::step_out(),
        C::OpenSettings => icons::settings(),
        C::EditHeaderBar => icons::pencil(),
        C::ShowShortcuts => icons::keyboard(),
        C::About => named("help-about-symbolic"),
        _ => return None,
    })
}

fn commands(ids: &[String]) -> Vec<Command> {
    ids.iter().filter_map(|id| Command::from_id(id)).collect()
}

/// A header button that runs `cmd`, with its name and shortcut as the tooltip.
fn command_button(app: &App, c: Command) -> Element<'_, Message> {
    let b: Element<'_, Message> = match icon_for(c) {
        Some(h) => button::icon(h).on_press(Message::Cmd(c)).into(),
        None => button::text(c.title(app.pt())).on_press(Message::Cmd(c)).into(),
    };
    let tip = match app.keymap.label(c) {
        Some(k) => format!("{} ({k})", c.title(app.pt())),
        None => c.title(app.pt()).to_string(),
    };
    widget::tooltip(b, text(tip).size(12), widget::tooltip::Position::Bottom).into()
}

/// The buttons of one side of the bar, from the settings. A right click on them opens the editor.
pub fn buttons(app: &App, end: bool) -> Element<'_, Message> {
    let ids = if end { &app.settings.header_end } else { &app.settings.header_start };
    let r = row::with_children(commands(ids).into_iter().map(|c| command_button(app, c)).collect::<Vec<_>>()).spacing(2).align_y(Alignment::Center);
    widget::mouse_area(r).on_right_press(Message::Cmd(Command::EditHeaderBar)).into()
}

/// The two sides of the bar, left and right, each as command ids from left to right.
pub type Sides = [Vec<String>; 2];

/// What the editor's buttons do to the two lists. Kept apart from the widgets so the tests can
/// go through every one of them.
pub mod edit {
    use super::Sides;

    /// One place to the left (`-1`) or to the right (`1`) on the same side.
    pub fn shift(s: &mut Sides, side: usize, i: usize, by: isize) {
        let j = i as isize + by;
        if i < s[side].len() && j >= 0 && (j as usize) < s[side].len() {
            s[side].swap(i, j as usize);
        }
    }

    /// To the other side, toward the middle of the bar: the end of the left, the start of the right.
    pub fn across(s: &mut Sides, side: usize, i: usize) {
        if i >= s[side].len() {
            return;
        }
        let id = s[side].remove(i);
        if side == 0 { s[1].insert(0, id) } else { s[0].push(id) }
    }

    pub fn remove(s: &mut Sides, side: usize, i: usize) {
        if i < s[side].len() {
            s[side].remove(i);
        }
    }

    /// At the end of a side, if it isn't on the bar already.
    pub fn add(s: &mut Sides, side: usize, id: &str) {
        if !s.iter().flatten().any(|x| x == id) {
            s[side].push(id.to_string());
        }
    }
}

fn set(s: Sides) -> Message {
    let [a, b] = s;
    Message::Setting(SettingMsg::HeaderBar(a, b))
}

/// The dialog that edits the header bar: two lists, left and right, whose rows move left, right
/// and across, or go; and a searchable list of every command to add on either side.
pub fn editor(app: &App) -> Element<'_, Message> {
    let pt = app.pt();
    let sides: Sides = [app.settings.header_start.clone(), app.settings.header_end.clone()];
    let small = |h: icon::Handle, tip: String, msg: Option<Message>| -> Element<'static, Message> {
        let b = button::icon(h).extra_small().on_press_maybe(msg);
        widget::tooltip(b, text(tip).size(12), widget::tooltip::Position::Top).into()
    };
    let mut body = column::with_capacity(8).spacing(6).width(Length::Fill);
    for (side, title) in [(0usize, tr("Beside the menu bar")), (1, tr("Beside the window controls"))] {
        body = body.push(text::heading(format!("{} · {title}", tr(if side == 0 { "Left" } else { "Right" }))));
        let list = commands(&sides[side]);
        if list.is_empty() {
            body = body.push(text(tr("No buttons")).size(12).class(theme::Text::Color(dim_text())));
        }
        let n = list.len();
        for (i, c) in list.into_iter().enumerate() {
            let with = |f: &dyn Fn(&mut Sides)| {
                let mut s = sides.clone();
                f(&mut s);
                set(s)
            };
            let glyph: Element<'_, Message> = match icon_for(c) {
                Some(h) => icon::icon(h).size(16).into(),
                None => widget::Space::new().width(Length::Fixed(16.0)).into(),
            };
            let keys = app.keymap.label(c).unwrap_or("").to_string();
            let across_tip = tr(if side == 0 { "Move to the Right Side" } else { "Move to the Left Side" }).to_string();
            let across_icon = icons::named(if side == 0 { "go-next-symbolic" } else { "go-previous-symbolic" });
            body = body.push(
                container(
                    row::with_children(vec![
                        glyph,
                        text(c.title(pt)).size(13).width(Length::Fill).into(),
                        text(keys).size(11).font(cosmic::font::mono()).class(theme::Text::Color(dim_text())).into(),
                        small(icons::named("go-up-symbolic"), tr("Move Left").to_string(), (i > 0).then(|| with(&|s| edit::shift(s, side, i, -1)))),
                        small(icons::named("go-down-symbolic"), tr("Move Right").to_string(), (i + 1 < n).then(|| with(&|s| edit::shift(s, side, i, 1)))),
                        small(across_icon, across_tip, Some(with(&|s| edit::across(s, side, i)))),
                        small(icons::named("user-trash-symbolic"), tr("Remove").to_string(), Some(with(&|s| edit::remove(s, side, i)))),
                    ])
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .padding([4, 8])
                .class(theme::Container::Card),
            );
        }
    }

    // The commands not on the bar yet, by what's typed in the field.
    let q = app.dialog_input.to_lowercase();
    let mut entries: Vec<Command> = Command::ALL
        .iter()
        .copied()
        .filter(|c| !sides.iter().flatten().any(|id| id == c.id()))
        .filter(|c| q.is_empty() || c.title(pt).to_lowercase().contains(&q) || c.id().to_lowercase().contains(&q))
        .collect();
    entries.sort_by_key(|c| c.title(pt).to_lowercase());
    let mut add = column::with_capacity(entries.len()).spacing(2);
    for c in entries {
        let glyph: Element<'_, Message> = match icon_for(c) {
            Some(h) => icon::icon(h).size(16).into(),
            None => widget::Space::new().width(Length::Fixed(16.0)).into(),
        };
        let to = |side: usize| {
            let mut s = sides.clone();
            edit::add(&mut s, side, c.id());
            set(s)
        };
        add = add.push(
            row::with_children(vec![
                glyph,
                text(c.title(pt)).size(13).width(Length::Fill).into(),
                small(icons::named("go-previous-symbolic"), tr("Add Button on the Left").to_string(), Some(to(0))),
                small(icons::named("go-next-symbolic"), tr("Add Button on the Right").to_string(), Some(to(1))),
            ])
            .spacing(8)
            .padding([2, 8])
            .align_y(Alignment::Center),
        );
    }
    body = body.push(text::heading(tr("Add Button")));
    body = body.push(
        widget::search_input(tr("Search commands"), app.dialog_input.as_str())
            .id(widget::Id::new(DIALOG_INPUT))
            .on_input(|s| Message::Dialog(DialogMsg::Input(s)))
            .on_focus(Message::InputFocus(true))
            .on_unfocus(Message::InputFocus(false)),
    );
    body = body.push(scrollable(add).height(Length::Fixed(200.0)));

    widget::dialog()
        .title(tr("Customize Header Bar"))
        .control(scrollable(body).height(Length::Fixed(520.0)))
        .primary_action(button::suggested(tr("Close")).on_press(Message::Dialog(DialogMsg::Cancel)))
        .secondary_action(button::standard(tr("Restore Defaults")).on_press(set([default_header_start(), default_header_end()])))
        .into()
}

#[cfg(test)]
mod tests {
    use super::Sides;
    use super::edit;

    fn sides(l: &[&str], r: &[&str]) -> Sides {
        [l.iter().map(|s| s.to_string()).collect(), r.iter().map(|s| s.to_string()).collect()]
    }

    #[test]
    fn arrows_move_within_a_side_and_stop_at_the_ends() {
        let mut s = sides(&["a", "b", "c"], &[]);
        edit::shift(&mut s, 0, 0, 1);
        assert_eq!(s, sides(&["b", "a", "c"], &[]));
        edit::shift(&mut s, 0, 0, -1);
        assert_eq!(s, sides(&["b", "a", "c"], &[]));
        edit::shift(&mut s, 0, 2, 1);
        assert_eq!(s, sides(&["b", "a", "c"], &[]));
    }

    #[test]
    fn across_goes_toward_the_middle_of_the_bar() {
        let mut s = sides(&["a", "b"], &["c", "d"]);
        edit::across(&mut s, 0, 0);
        assert_eq!(s, sides(&["b"], &["a", "c", "d"]));
        edit::across(&mut s, 1, 2);
        assert_eq!(s, sides(&["b", "d"], &["a", "c"]));
    }

    #[test]
    fn remove_and_add_without_duplicates() {
        let mut s = sides(&["a"], &["b"]);
        edit::add(&mut s, 1, "a");
        assert_eq!(s, sides(&["a"], &["b"]));
        edit::add(&mut s, 1, "c");
        assert_eq!(s, sides(&["a"], &["b", "c"]));
        edit::remove(&mut s, 0, 0);
        edit::remove(&mut s, 0, 5);
        assert_eq!(s, sides(&[], &["b", "c"]));
    }
}
