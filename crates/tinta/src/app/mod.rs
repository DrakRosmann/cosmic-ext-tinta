//! The application: all state lives here, every change goes through `update`.

mod commands;
mod debug;
mod diff;
mod docs;
mod editing;
mod explorer;
mod git;
mod keys;
mod lists;
mod lsp;
mod palette;
mod run;
mod script;
mod search;
mod tasks;
mod vim;

pub use explorer::{Explorer, RowAction};
pub use git::{GitMsg, GitPanel};
pub use keys::KeyEv;
pub use lsp::{Completion, HoverPopup, SignaturePopup};
pub use editing::{FIND_INPUT, MAX_SHOWN_MATCHES, REPLACE_INPUT};
pub use explorer::{EXPLORER_SCROLL, ExplorerMsg, ROW_H};
pub use palette::{PAction, PItem, PaletteMode, PaletteMsg, PaletteState, PALETTE_INPUT};
pub use debug::{DEBUG_INPUT, DebugMsg, DebugPanel, WATCH_INPUT};
pub use run::{RunPanel, out_dir as run_out_dir};
pub use tasks::{TaskMsg, TaskPanel};
pub use vim::{VIM_LINE, VimMsg};
pub use search::{SearchMsg, SearchPanel, SearchRow, SEARCH_INPUT};
pub use lists::GIT_INPUT;
pub use crate::ui::popups::DIALOG_INPUT;

use crate::bus;
use crate::fx;
use crate::i18n::tr;
use crate::state::{DocEntry, DocId, Group, Metrics, Tab, TabId};
use crate::ui;
use crate::ui::editor_view::EditorEvent;
use crate::ui::terminal::Session as TermSession;
use cosmic::app::{Core, Task, context_drawer};
use cosmic::iced::{Subscription, event, keyboard, window};
use cosmic::widget::segmented_button;
use cosmic::{Application, Element, Theme};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tinta_core::keymap::{Command, Keymap};
use tinta_core::settings::{AppTheme, AutoSave, Settings};
use tinta_core::{find, lsp as core_lsp};

pub const APP_ID: &str = "io.github.DrakRosmann.CosmicExtTinta";

pub struct Flags {
    pub targets: Vec<(PathBuf, Option<(usize, usize)>)>,
    pub new_session: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Editor,
    Terminal,
    Input,
    /// The file tree, moved through with the arrows.
    Explorer,
    /// One of the sidebar's other lists: search results, git, outline or variables.
    List,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarView {
    Explorer,
    Search,
    Git,
    Outline,
    Debug,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelView {
    Terminal,
    Tasks,
    Problems,
    Output,
    Debug,
}

#[derive(Debug, Clone, Default)]
pub struct FindState {
    pub open: bool,
    pub replace_open: bool,
    pub query: find::Query,
    pub replacement: String,
    pub in_selection: Option<Vec<(usize, usize)>>,
    pub error: Option<String>,
    pub current: Option<usize>,
    pub generation: u64,
}

#[derive(Debug, Clone)]
pub enum DialogState {
    HeaderBar,
    CloseUnsaved(TabId),
    QuitUnsaved,
    ChangedOnDisk(DocId),
    NewFile(PathBuf),
    NewFolder(PathBuf),
    RenamePath(PathBuf),
    TrashPath(PathBuf),
    RenameSymbol,
    BreakpointCondition(PathBuf, usize),
    /// A new value for a variable, or a watched expression, of the paused program.
    SetValue(tinta_core::dap::VarRow),
    ConfirmDiscard(tinta_core::git::StatusEntry),
    /// Arguments for running a file.
    RunArgs(PathBuf),
    /// May this folder's `.tinta/` file run commands? Then what was asked for, again.
    TrustFolder(PathBuf, &'static str, Command),
    About,
    Shortcuts,
}

#[derive(Debug, Clone)]
pub enum DialogMsg {
    Input(String),
    Primary,
    Secondary,
    Cancel,
    ShortcutsFilter(String),
}

#[derive(Debug, Clone)]
pub enum FindMsg {
    Query(String),
    Replacement(String),
    ToggleCase,
    ToggleWord,
    ToggleRegex,
    ToggleInSelection,
    ToggleReplace,
    Next,
    Prev,
    ReplaceOne,
    ReplaceAll,
    Close,
}

#[derive(Debug, Clone)]
pub enum SettingMsg {
    UiTheme(AppTheme),
    SyntaxTheme(String),
    FontFamily(String),
    FontSize(f32),
    LineHeight(f32),
    TabSize(usize),
    Bool(BoolSetting, bool),
    LineNumbers(tinta_core::settings::LineNumbers),
    Whitespace(tinta_core::settings::Whitespace),
    CursorStyle(tinta_core::settings::CursorStyle),
    AutoSave(AutoSave),
    /// The header bar's buttons, left side and right side, as command ids.
    HeaderBar(Vec<String>, Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolSetting {
    Minimap,
    WordWrap,
    CurrentLine,
    BracketColors,
    IndentGuides,
    StickyScroll,
    Folding,
    Occurrences,
    CursorBlink,
    InsertSpaces,
    DetectIndent,
    AutoPairs,
    AutoIndent,
    AutoComplete,
    WordCompletion,
    Snippets,
    GitGutter,
    InlineBlame,
    FormatOnSave,
    TrimTrailing,
    FinalNewline,
    RestoreSession,
    HotExit,
    ShowHidden,
    HideBuildOutputs,
    Lsp,
    Vim,
}

#[derive(Debug, Clone)]
pub enum Message {
    Noop,
    Surface(cosmic::surface::Action),
    Key(KeyEv),
    Modifiers(keyboard::Modifiers),
    InputFocus(bool),
    Cmd(Command),
    Editor(TabId, EditorEvent),
    Diff(TabId, ui::diff_view::DiffEvent),
    TabActivate(usize, segmented_button::Entity),
    TabClose(usize, segmented_button::Entity),
    SidebarShow(SidebarView),
    SidebarResize(f32),
    PanelResize(f32),
    GroupResize(usize, f32),
    PanelShow(PanelView),
    PanelClose,
    TickFrame,
    TickSlow,
    Toast(String),
    OpenFolderPath(PathBuf),
    /// The window became too narrow for the sidebar beside the text, or wide enough again.
    Narrow(bool),
    /// The keyboard goes into a sidebar list, from the field above it.
    #[cfg_attr(not(test), allow(dead_code))]
    ListFocus(SidebarView),
    /// The banner about a missing language server: copy its command, if it has one, and close it.
    LspHintDone(String),
    OpenFiles(Vec<PathBuf>),
    SaveAsTo(TabId, PathBuf),
    Pasted(Option<String>),
    CopyText(String),
    Find(FindMsg),
    Palette(palette::PaletteMsg),
    Explorer(explorer::ExplorerMsg),
    Search(search::SearchMsg),
    Git(git::GitMsg),
    Task(TaskMsg),
    Vim(VimMsg),
    Debug(DebugMsg),
    TermLink(String, usize),
    OutlineJump(usize),
    OutlineScrolled(f32, f32),
    CompletionPick(usize),
    Lsp(core_lsp::Event),
    Dialog(DialogMsg),
    Setting(SettingMsg),
    SettingsClose,
    FsEvent(Vec<PathBuf>),
    TermWake(u64),
    TermTitle(u64, Option<String>),
    TermWrite(u64, Vec<u8>),
    /// A terminal's program ended, with its wait status.
    TermExited(u64, i32),
    TermResize(u64, u16, u16, f32, f32),
    TermFocus,
    TermSelect(usize),
    TermKill(usize),
    ProblemJump(PathBuf, usize, usize),
    CloseRequested,
    WindowFocused(bool),
    Dropped(Vec<PathBuf>),
    RemoteOpen(Vec<crate::instance::Target>),
    Script(script::Step),
    ThemeChanged,
}

pub struct App {
    pub core: Core,
    pub settings: Settings,
    pub base_font_size: f32,
    pub metrics: Metrics,
    pub keymap: Keymap,
    pub docs: HashMap<DocId, DocEntry>,
    pub tabs: HashMap<TabId, Tab>,
    pub groups: Vec<Group>,
    pub active_group: usize,
    pub next_id: u64,
    pub focus: Focus,
    pub folder: Option<PathBuf>,
    pub explorer: Explorer,
    pub search: SearchPanel,
    pub git: GitPanel,
    pub tasks: TaskPanel,
    pub debug: DebugPanel,
    pub runner: RunPanel,
    pub vim: tinta_core::vim::Vim,
    pub vim_line: Option<(char, String)>,
    pub vim_highlight: Option<String>,
    pub sidebar: Option<SidebarView>,
    pub last_sidebar: SidebarView,
    pub sidebar_w: f32,
    pub panel: Option<PanelView>,
    pub panel_h: f32,
    pub terms: Vec<TermSession>,
    pub active_term: usize,
    pub find: FindState,
    pub palette: Option<PaletteState>,
    pub completion: Option<Completion>,
    pub hover: Option<HoverPopup>,
    pub signature: Option<SignaturePopup>,
    pub dialog: Option<DialogState>,
    pub dialog_input: String,
    pub toast: Option<String>,
    pub toast_gen: u64,
    pub lsp: HashMap<String, lsp::LspEntry>,
    pub lsp_pending: HashMap<(String, i64), lsp::Pending>,
    pub lsp_progress: Option<String>,
    pub project_diags: HashMap<PathBuf, Vec<core_lsp::Diagnostic>>,
    pub output: Vec<String>,
    pub closed_tabs: Vec<PathBuf>,
    pub recent_files: Vec<PathBuf>,
    pub recent_folders: Vec<PathBuf>,
    pub nav_back: Vec<(PathBuf, usize)>,
    pub nav_forward: Vec<(PathBuf, usize)>,
    pub clip_meta: Option<(String, Vec<String>, bool)>,
    pub zen: bool,
    pub fullscreen: bool,
    pub syntax_theme: Arc<syntect::highlighting::Theme>,
    pub dark: bool,
    pub edit_tab: Option<TabId>,
    pub want_reveal: bool,
    pub watcher: Option<notify::RecommendedWatcher>,
    pub watched: Vec<PathBuf>,
    pub file_index: Arc<Vec<String>>,
    pub file_index_loading: bool,
    pub settings_open: bool,
    pub settings_mtime: Option<std::time::SystemTime>,
    pub keymap_mtime: Option<std::time::SystemTime>,
    pub font_names: Vec<String>,
    pub syntax_theme_names: Vec<String>,
    pub modifiers: keyboard::Modifiers,
    /// The window is narrow: the sidebar floats over the text, and goes away once a file opens.
    pub narrow: bool,
    /// The row the keyboard is on in the sidebar list shown, with `Focus::List`.
    pub list_cursor: usize,
    /// Where the outline is scrolled to, and how tall it is: to keep the keyboard's row in sight.
    pub outline_scroll: (f32, f32),
    /// Languages whose server isn't installed, by LSP id: the language's name and how to get one.
    pub lsp_missing: HashMap<String, (String, tinta_core::lsp::InstallHint)>,
    /// The ones the user closed the banner for, this session.
    pub lsp_hint_closed: std::collections::HashSet<String>,
    /// The sidebar as it was before the window got narrow, to bring back when it widens.
    pub(super) sidebar_wide: Option<SidebarView>,
    pub shortcuts_filter: String,
    pub title: String,
    pub session_dirty: Option<Instant>,
    pub user_snippets: HashMap<String, Vec<tinta_core::snippet::Definition>>,
    pub new_session: bool,
    pub groups_changed: std::cell::Cell<bool>,
    pub groups_flip: std::cell::Cell<bool>,
}

impl Application for App {
    type Executor = cosmic::executor::Default;
    type Flags = Flags;
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(mut core: Core, flags: Flags) -> (Self, Task<Message>) {
        core.set_keyboard_nav(false);
        core.window.content_container = false;
        core.window.show_context = false;
        let (settings, settings_err) = Settings::load();
        let mut keymap = Keymap::with_defaults();
        let mut key_errs = Vec::new();
        let keymap_mtime = file_mtime(&tinta_core::settings::keybindings_path());
        if let Ok(src) = std::fs::read_to_string(tinta_core::settings::keybindings_path()) {
            key_errs = keymap.apply_user(&src);
        }
        let dark = cosmic::theme::is_dark();
        let syntax_theme = tinta_core::syntax::theme_by_name(&settings.syntax_theme, dark);
        let metrics = Metrics::new(&editor_font(&settings), settings.font_size, settings.line_height);
        let mut app = App {
            core,
            base_font_size: settings.font_size,
            metrics,
            keymap,
            docs: HashMap::new(),
            tabs: HashMap::new(),
            groups: vec![Group::new()],
            active_group: 0,
            next_id: 1,
            focus: Focus::Editor,
            folder: None,
            explorer: Explorer::default(),
            search: SearchPanel::default(),
            git: GitPanel::default(),
            tasks: TaskPanel::default(),
            debug: DebugPanel::default(),
            runner: RunPanel::default(),
            vim: tinta_core::vim::Vim::new(),
            vim_line: None,
            vim_highlight: None,
            sidebar: None,
            last_sidebar: SidebarView::Explorer,
            sidebar_w: 280.0,
            panel: None,
            panel_h: 260.0,
            terms: Vec::new(),
            active_term: 0,
            find: FindState::default(),
            palette: None,
            completion: None,
            hover: None,
            signature: None,
            dialog: None,
            dialog_input: String::new(),
            toast: None,
            toast_gen: 0,
            lsp: HashMap::new(),
            lsp_pending: HashMap::new(),
            lsp_progress: None,
            project_diags: HashMap::new(),
            output: Vec::new(),
            closed_tabs: Vec::new(),
            recent_files: Vec::new(),
            recent_folders: Vec::new(),
            nav_back: Vec::new(),
            nav_forward: Vec::new(),
            clip_meta: None,
            zen: false,
            fullscreen: false,
            syntax_theme,
            dark,
            edit_tab: None,
            want_reveal: false,
            watcher: None,
            watched: Vec::new(),
            file_index: Arc::new(Vec::new()),
            file_index_loading: false,
            settings_open: false,
            settings_mtime: file_mtime(&tinta_core::settings::settings_path()),
            keymap_mtime,
            font_names: mono_font_names(),
            syntax_theme_names: tinta_core::syntax::theme_names(),
            modifiers: keyboard::Modifiers::default(),
            narrow: false,
            list_cursor: 0,
            outline_scroll: (0.0, 0.0),
            lsp_missing: HashMap::new(),
            lsp_hint_closed: Default::default(),
            sidebar_wide: None,
            shortcuts_filter: String::new(),
            title: "Tinta".into(),
            session_dirty: None,
            user_snippets: load_user_snippets(),
            new_session: flags.new_session,
            groups_changed: std::cell::Cell::new(false),
            groups_flip: std::cell::Cell::new(false),
            settings,
        };
        if let Some(e) = settings_err {
            app.output.push(format!("settings.json: {e}"));
            app.toast = Some(format!("settings.json: {e}"));
        }
        for e in key_errs {
            app.output.push(format!("keybindings.json: {e}"));
        }
        let mut tasks = vec![app.restore(flags.targets)];
        tasks.push(app.update_title());
        script::start();
        if app.settings.ui_theme != AppTheme::System {
            tasks.push(app.apply_ui_theme());
        }
        (app, Task::batch(tasks))
    }

    fn header_start(&self) -> Vec<Element<'_, Message>> {
        if self.zen { Vec::new() } else { vec![ui::menu::menu_bar(self), ui::headerbar::buttons(self, false)] }
    }

    fn header_center(&self) -> Vec<Element<'_, Message>> {
        Vec::new()
    }

    fn header_end(&self) -> Vec<Element<'_, Message>> {
        if self.zen {
            return Vec::new();
        }
        vec![ui::headerbar::buttons(self, true)]
    }

    fn context_drawer(&self) -> Option<context_drawer::ContextDrawer<'_, Message>> {
        if !self.settings_open {
            return None;
        }
        Some(
            context_drawer::context_drawer(ui::settings_page::view(self), Message::SettingsClose)
                .title(tr("Settings")),
        )
    }

    fn dialog(&self) -> Option<Element<'_, Message>> {
        ui::popups::dialog(self)
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        // Menus are popup surfaces, and this is called when they close too.
        (self.core.main_window_id() == Some(id)).then_some(Message::CloseRequested)
    }

    fn on_context_drawer(&mut self) -> Task<Message> {
        self.settings_open = self.core.window.show_context;
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            bus::subscription(),
            event::listen_with(|ev, status, _id| keys::from_event(ev, status)),
            cosmic::iced::time::every(Duration::from_millis(1000)).map(|_| Message::TickSlow),
        ];
        if self.needs_frame_tick() {
            subs.push(cosmic::iced::time::every(Duration::from_millis(16)).map(|_| Message::TickFrame));
        }
        Subscription::batch(subs)
    }

    fn system_theme_update(&mut self, _keys: &[&'static str], _new: &cosmic::cosmic_theme::Theme) -> Task<Message> {
        cosmic::task::message(cosmic::Action::App(Message::ThemeChanged))
    }

    fn system_theme_mode_update(&mut self, _keys: &[&'static str], _new: &cosmic::cosmic_theme::ThemeMode) -> Task<Message> {
        cosmic::task::message(cosmic::Action::App(Message::ThemeChanged))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        let after = self.after_update();
        Task::batch([task, after])
    }

    fn view(&self) -> Element<'_, Message> {
        ui::workspace::view(self)
    }
}

pub fn file_mtime(p: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

fn load_user_snippets() -> HashMap<String, Vec<tinta_core::snippet::Definition>> {
    let Ok(src) = std::fs::read_to_string(tinta_core::settings::snippets_path()) else { return HashMap::new() };
    serde_json::from_str(&tinta_core::settings::strip_jsonc(&src)).unwrap_or_default()
}

/// Monospace families installed on the system, for the settings page.
fn mono_font_names() -> Vec<String> {
    let mut names: Vec<String> = {
        let mut system = cosmic::iced::advanced::graphics::text::font_system().write().expect("font system");
        let fs = system.raw();
        let mut v: Vec<String> = fs
            .db()
            .faces()
            .filter(|f| f.monospaced)
            .filter_map(|f| f.families.first().map(|(n, _)| n.clone()))
            .collect();
        v.sort();
        v.dedup();
        v
    };
    if names.is_empty() {
        names.push("Noto Sans Mono".into());
    }
    names
}

/// The editor's font: the one in the settings, unless it's the default and COSMIC has its own
/// monospace font set.
pub fn editor_font(settings: &Settings) -> String {
    if settings.font_family == Settings::default().font_family {
        let m = cosmic::config::monospace_font().family;
        if !m.is_empty() {
            return m;
        }
    }
    settings.font_family.clone()
}

impl App {
    pub fn editor_font_family(&self) -> String {
        editor_font(&self.settings)
    }

    pub fn pt(&self) -> bool {
        crate::i18n::is_portuguese()
    }

    pub fn new_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn active_tab_id(&self) -> Option<TabId> {
        self.groups.get(self.active_group).and_then(Group::active_tab)
    }

    pub fn active(&self) -> Option<(&Tab, &DocEntry)> {
        let t = self.tabs.get(&self.active_tab_id()?)?;
        Some((t, self.docs.get(&t.doc)?))
    }

    pub fn active_mut(&mut self) -> Option<(&mut Tab, &mut DocEntry)> {
        let id = self.active_tab_id()?;
        let t = self.tabs.get_mut(&id)?;
        let d = self.docs.get_mut(&t.doc)?;
        Some((t, d))
    }

    pub fn toast(&mut self, text: impl Into<String>) -> Task<Message> {
        self.toast = Some(text.into());
        self.toast_gen += 1;
        let gen_id = self.toast_gen;
        cosmic::task::future(async move {
            tokio::time::sleep(Duration::from_secs(4)).await;
            bus::send(Message::Toast(format!("\0clear{gen_id}")));
            Message::Noop
        })
    }

    pub fn needs_frame_tick(&self) -> bool {
        for g in &self.groups {
            if let Some(d) = g.active_tab().and_then(|t| self.tabs.get(&t)).and_then(|t| t.diff.as_ref()) {
                for side in [&d.left, &d.right] {
                    if side.highlighter.as_ref().is_some_and(|h| !h.is_done(side.len_lines())) {
                        return true;
                    }
                }
            }
            if let Some(t) = g.active_tab().and_then(|t| self.tabs.get(&t))
                && let Some(e) = self.docs.get(&t.doc)
                && let Some(h) = &e.doc.highlighter
                && !h.is_done(e.doc.len_lines())
            {
                return true;
            }
        }
        false
    }

    pub fn mark_session_dirty(&mut self) {
        if self.session_dirty.is_none() {
            self.session_dirty = Some(Instant::now());
        }
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Noop => Task::none(),
            Message::Surface(a) => cosmic::task::message(cosmic::Action::Cosmic(cosmic::app::Action::Surface(a))),
            Message::Key(ev) => self.on_key(ev),
            Message::Modifiers(m) => {
                self.modifiers = m;
                Task::none()
            }
            Message::InputFocus(on) => {
                if on {
                    self.focus = Focus::Input;
                } else if self.focus == Focus::Input && self.palette.is_none() && self.dialog.is_none() {
                    self.focus = Focus::Editor;
                }
                Task::none()
            }
            Message::Cmd(c) => self.run(c),
            Message::Editor(tab, ev) => self.on_editor_event(tab, ev),
            Message::Diff(tab, ev) => self.on_diff_event(tab, ev),
            Message::TabActivate(g, e) => {
                self.active_group = g.min(self.groups.len() - 1);
                self.groups[self.active_group].model.activate(e);
                self.focus = Focus::Editor;
                self.close_popups();
                self.on_active_changed()
            }
            Message::TabClose(g, e) => {
                let Some(id) = self.groups.get(g).and_then(|gr| gr.model.data::<TabId>(e).copied()) else {
                    return Task::none();
                };
                self.request_close_tab(id)
            }
            Message::SidebarShow(v) => self.toggle_sidebar_view(v),
            Message::SidebarResize(d) => {
                self.sidebar_w = (self.sidebar_w + d).clamp(150.0, 800.0);
                self.mark_session_dirty();
                Task::none()
            }
            Message::PanelResize(d) => {
                self.panel_h = (self.panel_h - d).clamp(80.0, 900.0);
                self.mark_session_dirty();
                Task::none()
            }
            Message::GroupResize(g, d) => {
                self.resize_groups(g, d);
                Task::none()
            }
            Message::PanelShow(p) => {
                self.panel = Some(p);
                if p == PanelView::Terminal {
                    return self.ensure_terminal();
                }
                Task::none()
            }
            Message::PanelClose => {
                self.panel = None;
                if self.focus == Focus::Terminal {
                    self.focus = Focus::Editor;
                }
                Task::none()
            }
            Message::TickFrame => {
                self.advance_highlighting(Duration::from_millis(5));
                self.refresh_find_matches_lazily();
                Task::none()
            }
            Message::TickSlow => self.tick_slow(),
            Message::Toast(text) => {
                if let Some(g) = text.strip_prefix("\0clear") {
                    if g.parse::<u64>().ok() == Some(self.toast_gen) {
                        self.toast = None;
                    }
                    return Task::none();
                }
                self.toast(text)
            }
            Message::OpenFolderPath(p) => self.open_folder(p),
            Message::LspHintDone(lang) => {
                // "Got It": closes without copying.
                if let Some(key) = lang.strip_prefix("\0close") {
                    match key.strip_prefix("run:") {
                        Some(l) => {
                            self.runner.hint_closed.insert(l.to_string());
                        }
                        None => {
                            self.lsp_hint_closed.insert(key.to_string());
                        }
                    }
                    return Task::none();
                }
                if let Some(l) = lang.strip_prefix("run:") {
                    let l = l.to_string();
                    return match self.run_hint_done(&l) {
                        Some(c) => Task::batch([fx::clipboard_write(c), self.toast(tr("Command copied: run it in a terminal"))]),
                        None => Task::none(),
                    };
                }
                let command = self.lsp_missing.get(&lang).and_then(|(_, h)| h.command.clone());
                self.lsp_hint_closed.insert(lang);
                if let Some(c) = command {
                    return Task::batch([fx::clipboard_write(c), self.toast(tr("Command copied: run it in a terminal"))]);
                }
                Task::none()
            }
            Message::OutlineScrolled(y, h) => {
                self.outline_scroll = (y, h);
                Task::none()
            }
            Message::ListFocus(view) => {
                if self.sidebar == Some(view) && self.focus_list(view) {
                    return fx::focus_editor();
                }
                Task::none()
            }
            Message::Narrow(narrow) => {
                if narrow != self.narrow {
                    self.narrow = narrow;
                    if narrow {
                        self.sidebar_wide = self.sidebar.take();
                    } else {
                        let before = self.sidebar_wide.take();
                        if self.sidebar.is_none() {
                            self.sidebar = before;
                        }
                    }
                }
                Task::none()
            }
            Message::OpenFiles(ps) => {
                let mut tasks = Vec::new();
                for p in ps {
                    tasks.push(self.open_path(&p));
                }
                Task::batch(tasks)
            }
            Message::SaveAsTo(tab, p) => self.save_tab_as(tab, p),
            Message::Pasted(text) => self.on_pasted(text),
            Message::CopyText(s) => cosmic::iced::clipboard::write(s),
            Message::Find(m) => self.on_find(m),
            Message::Palette(m) => self.on_palette(m),
            Message::Explorer(m) => self.on_explorer(m),
            Message::Search(m) => self.on_search_msg(m),
            Message::Git(m) => self.on_git(m),
            Message::Task(m) => self.on_task(m),
            Message::Vim(m) => self.on_vim(m),
            Message::Debug(m) => self.on_debug(m),
            Message::TermLink(text, col) => self.on_term_link(text, col),
            Message::OutlineJump(line) => {
                self.push_nav();
                self.jump_to(line, 0);
                self.focus = Focus::Editor;
                Task::none()
            }
            Message::CompletionPick(i) => {
                if let Some(c) = &mut self.completion {
                    c.selected = i;
                }
                self.accept_completion()
            }
            Message::Lsp(ev) => self.on_lsp_event(ev),
            Message::Dialog(m) => self.on_dialog(m),
            Message::Setting(m) => self.on_setting(m),
            Message::SettingsClose => {
                self.settings_open = false;
                self.core.window.show_context = false;
                Task::none()
            }
            Message::FsEvent(paths) => self.on_fs_event(paths),
            Message::TermWake(id) => {
                if let Some(s) = self.terms.iter().find(|s| s.id == id) {
                    s.wake_done();
                }
                Task::none()
            }
            Message::TermTitle(id, t) => {
                if let Some(s) = self.terms.iter_mut().find(|s| s.id == id) {
                    s.title = t.unwrap_or_default();
                }
                Task::none()
            }
            Message::TermWrite(id, b) => {
                if let Some(s) = self.terms.iter().find(|s| s.id == id) {
                    s.write(b);
                }
                Task::none()
            }
            Message::TermExited(id, status) => {
                // The run's terminal stays, with what the program printed.
                if self.run_term_exited(id, status) {
                    return Task::none();
                }
                if let Some(i) = self.terms.iter().position(|s| s.id == id) {
                    self.terms.remove(i);
                    self.active_term = self.active_term.min(self.terms.len().saturating_sub(1));
                    if self.terms.is_empty() {
                        self.panel = None;
                        self.focus = Focus::Editor;
                    }
                }
                Task::none()
            }
            Message::TermResize(id, cols, rows, w, h) => {
                if let Some(s) = self.terms.iter_mut().find(|s| s.id == id) {
                    s.resize(cols, rows, w, h);
                }
                Task::none()
            }
            Message::TermFocus => {
                self.focus = Focus::Terminal;
                Task::none()
            }
            Message::TermSelect(i) => {
                self.active_term = i.min(self.terms.len().saturating_sub(1));
                self.focus = Focus::Terminal;
                Task::none()
            }
            Message::TermKill(i) => {
                if i < self.terms.len() {
                    let t = self.terms.remove(i);
                    self.run_term_closed(t.id);
                    if self.debug.tty.as_ref().is_some_and(|(id, _)| *id == t.id) {
                        self.debug.tty = None;
                    }
                    self.active_term = self.active_term.min(self.terms.len().saturating_sub(1));
                    if self.terms.is_empty() {
                        self.panel = None;
                        self.focus = Focus::Editor;
                    }
                }
                Task::none()
            }
            Message::ProblemJump(p, l, c) => {
                self.push_nav();
                let t = self.open_path(&p);
                self.jump_to(l, c);
                self.focus = Focus::Editor;
                t
            }
            Message::CloseRequested => self.request_quit(),
            Message::WindowFocused(on) => {
                if !on && self.settings.auto_save == AutoSave::Focus {
                    return self.save_all_dirty_named();
                }
                Task::none()
            }
            Message::Dropped(paths) => {
                let mut tasks = Vec::new();
                for p in paths {
                    if p.is_dir() {
                        tasks.push(self.open_folder(p));
                    } else {
                        tasks.push(self.open_path(&p));
                    }
                }
                Task::batch(tasks)
            }
            Message::RemoteOpen(targets) => {
                let mut tasks = Vec::new();
                for (path, at) in targets {
                    if path.is_dir() {
                        tasks.push(self.open_folder(path));
                    } else {
                        tasks.push(self.open_path(&path));
                        if let Some((l, c)) = at {
                            self.jump_to(l, c);
                        }
                    }
                }
                if let Some(id) = self.core.main_window_id() {
                    tasks.push(window::gain_focus(id));
                }
                Task::batch(tasks)
            }
            Message::Script(s) => self.on_script(s),
            Message::ThemeChanged => {
                self.refresh_syntax_theme();
                // COSMIC's monospace font, if the editor follows it, may have changed too.
                if self.metrics.font != crate::state::mono_font(&self.editor_font_family()) {
                    self.apply_font();
                }
                Task::none()
            }
        }
    }

    fn resize_groups(&mut self, g: usize, d: f32) {
        if g + 1 >= self.groups.len() {
            return;
        }
        let total_w = self.tabs_area_width().max(100.0);
        let total_weight: f32 = self.groups.iter().map(|g| g.weight).sum();
        let dw = d / total_w * total_weight;
        let min = 0.1 * total_weight;
        let a = (self.groups[g].weight + dw).max(min);
        let b = (self.groups[g + 1].weight - dw).max(min);
        self.groups[g].weight = a;
        self.groups[g + 1].weight = b;
    }

    fn tabs_area_width(&self) -> f32 {
        self.groups
            .iter()
            .filter_map(|g| g.active_tab().and_then(|t| self.tabs.get(&t)).map(|t| t.view_w))
            .sum()
    }

    pub fn refresh_syntax_theme(&mut self) {
        let dark = cosmic::theme::is_dark();
        let name = self.settings.syntax_theme.clone();
        if dark == self.dark && self.syntax_theme.name.as_deref() == Some(theme_name_for(&name, dark).as_str()) {
            return;
        }
        self.dark = dark;
        self.syntax_theme = tinta_core::syntax::theme_by_name(&name, dark);
        for e in self.docs.values_mut() {
            docs::attach_language(&mut e.doc, self.syntax_theme.clone());
        }
        for d in self.tabs.values_mut().filter_map(|t| t.diff.as_deref_mut()) {
            for side in [&mut d.left, &mut d.right] {
                side.highlighter = tinta_core::syntax::Highlighter::new(&side.language.name, self.syntax_theme.clone());
            }
        }
    }

    pub fn apply_ui_theme(&mut self) -> Task<Message> {
        let theme = match self.settings.ui_theme {
            AppTheme::System => cosmic::theme::system_preference(),
            AppTheme::Dark => Theme::dark(),
            AppTheme::Light => Theme::light(),
        };
        let t = cosmic::command::set_theme::<Message>(theme);
        Task::batch([t, cosmic::task::message(cosmic::Action::App(Message::ThemeChanged))])
    }
}

fn theme_name_for(name: &str, dark: bool) -> String {
    tinta_core::syntax::theme_by_name(name, dark).name.clone().unwrap_or_default()
}

pub fn save_session(app: &App) {
    let s = app.build_session();
    if let Err(e) = s.save() {
        log::warn!("could not save the session: {e}");
    }
}

/// What's listed in the outline: LSP's list when it's current, otherwise the tree's or the patterns'.
pub fn symbols_of(e: &DocEntry) -> Vec<(String, String, usize, usize)> {
    if let Some((rev, list)) = &e.lsp_symbols
        && *rev == e.doc.revision
        && !list.is_empty()
    {
        return list.iter().map(|s| (s.name.clone(), s.detail.clone(), s.line, s.depth)).collect();
    }
    e.symbols.iter().map(|s| (s.name.clone(), s.kind.to_string(), s.line, s.depth)).collect()
}
