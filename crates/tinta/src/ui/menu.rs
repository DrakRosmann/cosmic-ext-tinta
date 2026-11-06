//! The menu bar and right-click menus: COSMIC's own menu widgets, fed from the command table.

use crate::app::{App, ExplorerMsg, Message, RowAction};
use crate::i18n::tr;
use cosmic::Element;
use cosmic::iced::keyboard::key::Named;
use cosmic::iced::keyboard::Key;
use cosmic::widget::menu::key_bind::Modifier;
use cosmic::widget::menu::{self, Item, KeyBind};
use std::collections::HashMap;
use tinta_core::keymap::{Command, Stroke};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Act(pub Command);

impl menu::Action for Act {
    type Message = Message;
    fn message(&self) -> Message {
        Message::Cmd(self.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RowAct(pub usize, pub RowAction);

impl menu::Action for RowAct {
    type Message = Message;
    fn message(&self) -> Message {
        Message::Explorer(ExplorerMsg::RowMenu(self.0, self.1))
    }
}

fn key_of(name: &str) -> Key {
    let n = match name {
        "enter" => Named::Enter,
        "tab" => Named::Tab,
        "space" => return Key::Character(" ".into()),
        "backspace" => Named::Backspace,
        "delete" => Named::Delete,
        "insert" => Named::Insert,
        "up" => Named::ArrowUp,
        "down" => Named::ArrowDown,
        "left" => Named::ArrowLeft,
        "right" => Named::ArrowRight,
        "home" => Named::Home,
        "end" => Named::End,
        "pageup" => Named::PageUp,
        "pagedown" => Named::PageDown,
        "f1" => Named::F1,
        "f2" => Named::F2,
        "f3" => Named::F3,
        "f4" => Named::F4,
        "f5" => Named::F5,
        "f6" => Named::F6,
        "f7" => Named::F7,
        "f8" => Named::F8,
        "f9" => Named::F9,
        "f10" => Named::F10,
        "f11" => Named::F11,
        "f12" => Named::F12,
        other => return Key::Character(other.into()),
    };
    Key::Named(n)
}

/// The shortcut shown next to each entry. Chords don't fit KeyBind, so those go without.
fn binds(app: &App) -> HashMap<KeyBind, Act> {
    let mut out = HashMap::new();
    for c in Command::ALL {
        let Some(label) = app.keymap.label(*c) else { continue };
        if label.contains(' ') {
            continue;
        }
        let Some(s) = Stroke::parse(label) else { continue };
        let mut modifiers = Vec::new();
        if s.logo {
            modifiers.push(Modifier::Super);
        }
        if s.ctrl {
            modifiers.push(Modifier::Ctrl);
        }
        if s.alt {
            modifiers.push(Modifier::Alt);
        }
        if s.shift {
            modifiers.push(Modifier::Shift);
        }
        out.insert(KeyBind { modifiers, key: key_of(&s.key) }, Act(*c));
    }
    out
}

pub fn menu_bar(app: &App) -> Element<'_, Message> {
    let pt = app.pt();
    let b = |c: Command| Item::Button(c.title(pt), None, Act(c));
    let check = |c: Command, on: bool| Item::CheckBox(c.title(pt), None, on, Act(c));
    let d = || Item::Divider;
    let keys = binds(app);
    let s = &app.settings;
    use Command as C;
    let mut recent_or_open = vec![b(C::NewFile), b(C::OpenFile), b(C::OpenFolder), b(C::OpenRecent), d()];
    recent_or_open.extend([
        b(C::Save),
        b(C::SaveAs),
        b(C::SaveAll),
        b(C::Revert),
        b(C::CompareSaved),
        d(),
        b(C::CloseTab),
        b(C::CloseOtherTabs),
        b(C::CloseAllTabs),
        b(C::ReopenTab),
        b(C::PinTab),
        b(C::CloseFolder),
        d(),
        b(C::OpenSettings),
        Item::Folder(
            tr("Reference"),
            vec![b(C::OpenSettingsJson), b(C::OpenKeybindings), b(C::OpenSnippets)],
        ),
        d(),
        b(C::Quit),
    ]);
    let tree = |label: &'static str, items| {
        menu::Tree::with_children(
            Element::from(menu::root(label)),
            menu::items(&keys, items),
        )
    };
    menu::bar(vec![
        tree(tr("File"), recent_or_open),
        tree(
            tr("Edit"),
            vec![
                b(C::Undo),
                b(C::Redo),
                d(),
                b(C::Cut),
                b(C::Copy),
                b(C::Paste),
                d(),
                b(C::Find),
                b(C::Replace),
                b(C::FindInFiles),
                b(C::ReplaceInFiles),
                d(),
                b(C::ToggleComment),
                b(C::ToggleBlockComment),
                b(C::InsertSnippet),
                d(),
                Item::Folder(
                    if pt { "Linhas" } else { "Lines" },
                    vec![
                        b(C::DuplicateSelection),
                        b(C::CopyLineUp),
                        b(C::CopyLineDown),
                        b(C::MoveLineUp),
                        b(C::MoveLineDown),
                        b(C::DeleteLine),
                        b(C::JoinLines),
                        d(),
                        b(C::SortAsc),
                        b(C::SortDesc),
                        b(C::SortCaseless),
                        b(C::ReverseLines),
                        b(C::UniqueLines),
                        b(C::ShuffleLines),
                        b(C::TrimTrailing),
                    ],
                ),
                Item::Folder(
                    if pt { "Transformar" } else { "Transform" },
                    vec![
                        b(C::UpperCase),
                        b(C::LowerCase),
                        b(C::TitleCase),
                        b(C::SnakeCase),
                        b(C::CamelCase),
                        b(C::PascalCase),
                        b(C::KebabCase),
                        b(C::Transpose),
                    ],
                ),
            ],
        ),
        tree(
            tr("Selection"),
            vec![
                b(C::SelectAll),
                b(C::SelectLine),
                b(C::ExpandSelection),
                b(C::ShrinkSelection),
                d(),
                b(C::AddCursorAbove),
                b(C::AddCursorBelow),
                b(C::SplitIntoLines),
                d(),
                b(C::AddNextOccurrence),
                b(C::SkipOccurrence),
                b(C::SelectAllOccurrences),
            ],
        ),
        tree(
            tr("View"),
            vec![
                b(C::CommandPalette),
                b(C::ChangeTheme),
                d(),
                check(C::ToggleSidebar, app.sidebar.is_some()),
                b(C::ShowExplorer),
                b(C::ShowSearch),
                b(C::ShowGit),
                b(C::ShowOutline),
                d(),
                check(C::TogglePanel, app.panel.is_some()),
                b(C::ShowProblems),
                b(C::SplitRight),
                b(C::CloseGroup),
                d(),
                check(C::ToggleWordWrap, s.word_wrap),
                check(C::ToggleMinimap, s.minimap),
                check(C::ToggleStickyScroll, s.sticky_scroll),
                check(C::ToggleBlame, s.inline_blame),
                b(C::ToggleWhitespace),
                b(C::ToggleLineNumbers),
                b(C::MarkdownPreview),
                d(),
                b(C::Fold),
                b(C::Unfold),
                b(C::FoldAll),
                b(C::UnfoldAll),
                d(),
                b(C::ZoomIn),
                b(C::ZoomOut),
                b(C::ZoomReset),
                check(C::ToggleFullscreen, app.fullscreen),
                check(C::ToggleZen, app.zen),
            ],
        ),
        tree(
            tr("Go"),
            vec![
                b(C::QuickOpen),
                b(C::GoToLine),
                b(C::GoToSymbol),
                d(),
                b(C::GoBack),
                b(C::GoForward),
                b(C::JumpToBracket),
                d(),
                b(C::NextTab),
                b(C::PrevTab),
                b(C::FocusNextGroup),
                b(C::FocusPrevGroup),
                d(),
                b(C::NextDiagnostic),
                b(C::PrevDiagnostic),
                b(C::NextChange),
                b(C::PrevChange),
                d(),
                b(C::ToggleBookmark),
                b(C::NextBookmark),
                b(C::PrevBookmark),
                b(C::ClearBookmarks),
            ],
        ),
        tree(
            tr("Code"),
            vec![
                b(C::TriggerCompletion),
                b(C::ParameterHints),
                b(C::ShowHover),
                d(),
                b(C::GoToDefinition),
                b(C::GoToDeclaration),
                b(C::GoToImplementation),
                b(C::GoToTypeDefinition),
                b(C::FindReferences),
                d(),
                b(C::Rename),
                b(C::FormatDocument),
                b(C::CodeAction),
                d(),
                b(C::ChangeLanguage),
                b(C::ChangeIndentSize),
                b(C::ChangeEncoding),
                b(C::RestartLsp),
            ],
        ),
        tree(
            "Git",
            vec![
                b(C::GitCommit),
                b(C::GitStageAll),
                b(C::GitPush),
                b(C::GitPull),
                b(C::GitFetch),
                b(C::GitCheckout),
                d(),
                b(C::GitDiffFile),
                b(C::GitLog),
                b(C::RevertHunk),
                b(C::GitRefresh),
            ],
        ),
        tree(tr("Terminal"), vec![b(C::ToggleTerminal), b(C::NewTerminal)]),
        tree(tr("Help"), vec![b(C::ShowShortcuts), b(C::About)]),
    ])
    .item_height(menu::ItemHeight::Dynamic(36))
    .item_width(menu::ItemWidth::Uniform(320))
    .spacing(2.0)
    .into()
}

/// Right click in the text.
pub fn editor_menu(app: &App) -> Vec<menu::Tree<Message>> {
    let pt = app.pt();
    let b = |c: Command| Item::Button(c.title(pt), None, Act(c));
    let d = || Item::Divider;
    use Command as C;
    menu::items(
        &binds(app),
        vec![
            b(C::GoToDefinition),
            b(C::FindReferences),
            b(C::Rename),
            b(C::CodeAction),
            b(C::FormatDocument),
            d(),
            b(C::Cut),
            b(C::Copy),
            b(C::Paste),
            d(),
            b(C::ToggleComment),
            b(C::AddNextOccurrence),
            b(C::SelectAllOccurrences),
            d(),
            b(C::CommandPalette),
        ],
    )
}

/// Right click in the file tree.
pub fn row_menu(index: usize, is_dir: bool) -> Vec<menu::Tree<Message>> {
    let empty: HashMap<KeyBind, RowAct> = HashMap::new();
    let b = |label: &'static str, a: RowAction| Item::Button(tr(label), None, RowAct(index, a));
    let mut items = Vec::new();
    if !is_dir {
        items.push(b("Open to the Side", RowAction::OpenSide));
    }
    items.extend([
        b("New File", RowAction::NewFile),
        b("New Folder", RowAction::NewFolder),
        Item::Divider,
        b("Rename", RowAction::Rename),
        b("Move to Trash", RowAction::Trash),
        Item::Divider,
        b("Copy Path", RowAction::CopyPath),
        b("Copy Relative Path", RowAction::CopyRelative),
        b("Reveal in File Manager", RowAction::Reveal),
        b("Open in Terminal", RowAction::Terminal),
    ]);
    menu::items(&empty, items)
}
