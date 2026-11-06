//! The whole interface without a window: real widgets, real layout, real mouse and keyboard
//! events, drawn each frame into a software renderer. No compositor offers a virtual pointer, so
//! this is where the mouse gets exercised.

use crate::app::{App, Flags, Focus, Message, PaletteMode, SidebarView};
use crate::bus;
use crate::ui::editor_view::Geometry;
use cosmic::app::Core;
use cosmic::iced::advanced::clipboard;
use cosmic::iced::keyboard::{self, Modifiers};
use cosmic::iced::{Event, Font, Pixels, Point, Size, mouse, window};
use cosmic::{Application, Element, Renderer, Theme};
use iced_runtime::user_interface::{self, UserInterface};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, Once};
use std::time::Instant;
use tinta_core::keymap::Command;

/// The bus is global and settings live in the environment: one UI test at a time.
static LOCK: Mutex<()> = Mutex::new(());
static INIT: Once = Once::new();

const SIZE: Size = Size::new(1200.0, 800.0);

struct Ui {
    app: App,
    renderer: Renderer,
    cache: Option<user_interface::Cache>,
    cursor: Point,
    mods: Modifiers,
    theme: Theme,
    _guard: MutexGuard<'static, ()>,
}

fn temp_dir(name: &str) -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("tinta-ui-{}-{name}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

const SOURCE: &str = "fn main() {\n    let alpha = 1;\n    let beta = alpha + 2;\n    if beta > 2 {\n        println!(\"{beta}\");\n    }\n}\n\nfn other() {\n    let gamma = 3;\n}\n";

impl Ui {
    fn new(targets: Vec<PathBuf>) -> Self {
        let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        INIT.call_once(|| {
            let base = temp_dir("env");
            // SAFETY: done once, before this test's threads read the environment.
            unsafe {
                std::env::set_var("XDG_CONFIG_HOME", base.join("config"));
                std::env::set_var("XDG_STATE_HOME", base.join("state"));
                std::env::remove_var("TINTA_SCRIPT");
            }
        });
        // Earlier tests may have saved settings; each starts from the defaults.
        let _ = std::fs::remove_file(tinta_core::settings::settings_path());
        let _ = bus::drain();
        let flags = Flags { targets: targets.into_iter().map(|p| (p, None)).collect(), new_session: true };
        let (app, _) = App::init(Core::default(), flags);
        let renderer = Renderer::Secondary(iced_tiny_skia::Renderer::new(Font::DEFAULT, Pixels(14.0)));
        let mut ui = Ui {
            app,
            renderer,
            cache: Some(user_interface::Cache::new()),
            cursor: Point::ORIGIN,
            mods: Modifiers::empty(),
            theme: Theme::dark(),
            _guard: guard,
        };
        ui.frame();
        ui.frame();
        ui
    }

    /// Runs events through the widget tree, then feeds whatever they produced to `update`, until
    /// things settle. Draws, so painting can't quietly break.
    fn send(&mut self, events: Vec<Event>) {
        let mut pending = events;
        for _ in 0..12 {
            let mut messages = Vec::new();
            let element: Element<'_, Message> = self.app.view();
            let cache = self.cache.take().unwrap();
            let mut ui = UserInterface::build(element, SIZE, cache, &mut self.renderer);
            let mut clip = clipboard::Null;
            let cursor = mouse::Cursor::Available(self.cursor);
            let _ = ui.update(&pending, cursor, &mut self.renderer, &mut clip, &mut messages);
            let style = cosmic::iced::advanced::renderer::Style::default();
            ui.draw(&mut self.renderer, &self.theme, &style, cursor);
            let cache = ui.into_cache();
            self.cache = Some(cache);
            pending = Vec::new();
            messages.extend(bus::drain());
            if messages.is_empty() {
                return;
            }
            for m in messages {
                let _ = self.app.update(m);
            }
        }
    }

    fn frame(&mut self) {
        self.send(vec![Event::Window(window::Event::RedrawRequested(Instant::now()))]);
    }

    fn set_mods(&mut self, m: Modifiers) {
        self.mods = m;
        self.send(vec![Event::Keyboard(keyboard::Event::ModifiersChanged(m))]);
    }

    fn move_to(&mut self, p: Point) {
        self.cursor = p;
        self.send(vec![Event::Mouse(mouse::Event::CursorMoved { position: p })]);
    }

    fn press(&mut self, b: mouse::Button) {
        self.send(vec![Event::Mouse(mouse::Event::ButtonPressed(b))]);
    }

    fn release(&mut self, b: mouse::Button) {
        self.send(vec![Event::Mouse(mouse::Event::ButtonReleased(b))]);
    }

    fn click_at(&mut self, p: Point) {
        self.move_to(p);
        self.press(mouse::Button::Left);
        self.release(mouse::Button::Left);
    }

    /// Finds where the editor widget sits by probing: clicking its first cell moves the caret.
    fn editor_origin(&self) -> Point {
        // Rail + sidebar + sash when shown; tab bar + breadcrumbs on top.
        let x = if self.app.sidebar.is_some() { 44.0 + self.app.sidebar_w + 5.0 } else { 44.0 };
        Point::new(x, 36.0 + 24.0)
    }

    /// Screen position of a document cell, using the same geometry the widget paints with.
    fn cell(&self, line: usize, col: usize) -> Point {
        let (t, e) = self.app.active().unwrap();
        let g = Geometry::new(e.doc.len_lines(), t.view_w, &self.app.metrics, &self.app.settings);
        let o = self.origin_from_view();
        Point::new(
            o.x + g.text_x + (col as f32 + 0.2) * self.app.metrics.cell_w,
            o.y + (line as f32 + 0.5) * self.app.metrics.line_h - t.scroll_y,
        )
    }

    /// The editor fills what's left of the window, so its size gives away where it starts.
    fn origin_from_view(&self) -> Point {
        let (t, _) = self.app.active().unwrap();
        let o = self.editor_origin();
        Point::new(SIZE.width - t.view_w, o.y.max(SIZE.height - 26.0 - t.view_h))
    }

    fn head(&self) -> (usize, usize) {
        let (t, e) = self.app.active().unwrap();
        e.doc.line_col(t.cursors.head())
    }

    fn selection_text(&self) -> String {
        let (t, e) = self.app.active().unwrap();
        let s = t.cursors.primary();
        e.doc.slice_string(s.start(), s.end())
    }

    fn text(&self) -> String {
        self.app.active().unwrap().1.doc.rope.to_string()
    }

    fn cmd(&mut self, c: Command) {
        let _ = self.app.update(Message::Cmd(c));
        self.frame();
    }

    fn with_file() -> (Self, PathBuf) {
        let dir = temp_dir("file");
        let p = dir.join("a.rs");
        std::fs::write(&p, SOURCE).unwrap();
        let ui = Ui::new(vec![p.clone()]);
        (ui, p)
    }

    fn key(&mut self, name: &str, mods: Modifiers, text: Option<&str>) {
        let ev = crate::app::KeyEv { name: name.into(), shifted: None, mods, text: text.map(str::to_string), captured: false };
        let _ = self.app.update(Message::Key(ev));
        self.frame();
    }

    fn typed(&mut self, s: &str) {
        for c in s.chars() {
            self.key(&c.to_lowercase().to_string(), Modifiers::empty(), Some(&c.to_string()));
        }
    }
}

fn wait_for(ui: &mut Ui, mut done: impl FnMut(&App) -> bool) {
    for _ in 0..400 {
        ui.frame();
        if done(&ui.app) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("timed out waiting");
}

#[test]
fn the_editor_measures_itself_and_reports_its_size() {
    let (ui, _) = Ui::with_file();
    let (t, _) = ui.app.active().unwrap();
    assert!(t.view_w > 300.0 && t.view_h > 300.0, "{} x {}", t.view_w, t.view_h);
    assert!(ui.app.metrics.cell_w > 3.0, "{}", ui.app.metrics.cell_w);
}

#[test]
fn clicking_puts_the_caret_on_the_cell() {
    let (mut ui, _) = Ui::with_file();
    let p = ui.cell(2, 8);
    ui.click_at(p);
    assert_eq!(ui.head(), (2, 8));
    assert_eq!(ui.app.focus, Focus::Editor);
    // Past the end of a line lands at its end.
    let p = ui.cell(1, 60);
    ui.click_at(p);
    assert_eq!(ui.head(), (1, 18));
}

#[test]
fn double_and_triple_click_select_word_and_line() {
    let (mut ui, _) = Ui::with_file();
    let p = ui.cell(1, 9);
    ui.click_at(p);
    ui.click_at(p);
    assert_eq!(ui.selection_text(), "alpha");
    ui.click_at(p);
    assert_eq!(ui.selection_text(), "    let alpha = 1;\n");
}

#[test]
fn dragging_selects() {
    let (mut ui, _) = Ui::with_file();
    let a = ui.cell(1, 8);
    let b = ui.cell(2, 7);
    ui.move_to(a);
    ui.press(mouse::Button::Left);
    ui.move_to(b);
    ui.release(mouse::Button::Left);
    assert_eq!(ui.selection_text(), "alpha = 1;\n    let");
    // Moving afterwards doesn't keep selecting.
    let c = ui.cell(4, 3);
    ui.move_to(c);
    assert_eq!(ui.selection_text(), "alpha = 1;\n    let");
}

/// The click that closes a popup reaches the text, but its release goes to the popup. Found with
/// a real pointer: the drag never ended, and moving the mouse went on selecting.
#[test]
fn a_lost_release_does_not_leave_a_drag_running() {
    let (mut ui, _) = Ui::with_file();
    let a = ui.cell(1, 8);
    ui.move_to(a);
    ui.press(mouse::Button::Left);
    // No release: the pointer comes back to the window as the popup goes away.
    ui.send(vec![Event::Mouse(mouse::Event::CursorEntered)]);
    let b = ui.cell(4, 3);
    ui.move_to(b);
    assert_eq!(ui.selection_text(), "");
    assert_eq!(ui.head(), (1, 8));
}

#[test]
fn shift_click_extends_and_alt_click_adds_a_cursor() {
    let (mut ui, _) = Ui::with_file();
    let a = ui.cell(1, 8);
    ui.click_at(a);
    ui.set_mods(Modifiers::SHIFT);
    let b = ui.cell(1, 13);
    ui.click_at(b);
    assert_eq!(ui.selection_text(), "alpha");
    ui.set_mods(Modifiers::ALT);
    let c = ui.cell(2, 4);
    ui.click_at(c);
    assert_eq!(ui.app.active().unwrap().0.cursors.len(), 2);
    // Alt+click on a cursor takes it away (after long enough not to be a double click).
    std::thread::sleep(std::time::Duration::from_millis(420));
    ui.click_at(c);
    assert_eq!(ui.app.active().unwrap().0.cursors.len(), 1);
    ui.set_mods(Modifiers::empty());
}

#[test]
fn shift_alt_drag_makes_a_column_selection() {
    let (mut ui, _) = Ui::with_file();
    let a = ui.cell(1, 4);
    let b = ui.cell(2, 7);
    ui.set_mods(Modifiers::SHIFT | Modifiers::ALT);
    ui.move_to(a);
    ui.press(mouse::Button::Left);
    ui.move_to(b);
    ui.release(mouse::Button::Left);
    ui.set_mods(Modifiers::empty());
    let (t, e) = ui.app.active().unwrap();
    let parts: Vec<String> = t.cursors.iter().map(|s| e.doc.slice_string(s.start(), s.end())).collect();
    assert_eq!(parts, vec!["let", "let"]);
}

#[test]
fn wheel_scrolls_and_ctrl_wheel_zooms() {
    let dir = temp_dir("wheel");
    let p = dir.join("long.txt");
    std::fs::write(&p, "linha\n".repeat(500)).unwrap();
    let mut ui = Ui::new(vec![p]);
    let c = ui.cell(3, 1);
    ui.move_to(c);
    ui.send(vec![Event::Mouse(mouse::Event::WheelScrolled { delta: mouse::ScrollDelta::Lines { x: 0.0, y: -3.0 } })]);
    let y = ui.app.active().unwrap().0.scroll_y;
    assert!(y > 0.0, "{y}");
    let size = ui.app.settings.font_size;
    ui.set_mods(Modifiers::CTRL);
    ui.send(vec![Event::Mouse(mouse::Event::WheelScrolled { delta: mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 } })]);
    ui.set_mods(Modifiers::empty());
    assert_eq!(ui.app.settings.font_size, size + 1.0);
    assert_eq!(ui.app.active().unwrap().0.scroll_y, y, "ctrl+wheel doesn't scroll");
}

#[test]
fn fold_arrow_in_the_gutter_folds_and_unfolds() {
    let (mut ui, _) = Ui::with_file();
    let rows = |ui: &Ui| {
        let (t, e) = ui.app.active().unwrap();
        t.map.row_count(&e.doc)
    };
    let before = rows(&ui);
    let (t, e) = ui.app.active().unwrap();
    let g = Geometry::new(e.doc.len_lines(), t.view_w, &ui.app.metrics, &ui.app.settings);
    let o = ui.origin_from_view();
    let arrow = Point::new(o.x + g.gutter_w - 8.0, o.y + ui.app.metrics.line_h / 2.0);
    ui.click_at(arrow);
    assert!(rows(&ui) < before, "folded");
    ui.frame();
    // A fold doesn't count as a press on the text.
    assert_eq!(ui.head(), (0, 0));
    ui.click_at(arrow);
    assert_eq!(rows(&ui), before, "unfolded");
}

#[test]
fn line_number_click_selects_the_line() {
    let (mut ui, _) = Ui::with_file();
    let o = ui.origin_from_view();
    let p = Point::new(o.x + 12.0, o.y + ui.app.metrics.line_h * 1.5);
    ui.click_at(p);
    assert_eq!(ui.selection_text(), "    let alpha = 1;\n");
}

#[test]
fn minimap_and_scrollbar_scroll() {
    let dir = temp_dir("mini");
    let p = dir.join("long.txt");
    std::fs::write(&p, "linha\n".repeat(2000)).unwrap();
    let mut ui = Ui::new(vec![p]);
    let (t, e) = ui.app.active().unwrap();
    let g = Geometry::new(e.doc.len_lines(), t.view_w, &ui.app.metrics, &ui.app.settings);
    let (view_h, o) = (t.view_h, ui.origin_from_view());
    assert!(g.minimap_w > 0.0);
    let on_minimap = Point::new(o.x + g.minimap_x + 20.0, o.y + view_h - 10.0);
    ui.click_at(on_minimap);
    let y1 = ui.app.active().unwrap().0.scroll_y;
    assert!(y1 > 0.0, "{y1}");
    let on_bar = Point::new(o.x + g.scrollbar_x + 6.0, o.y + view_h / 2.0);
    ui.click_at(on_bar);
    let y2 = ui.app.active().unwrap().0.scroll_y;
    assert!(y2 > 0.0 && y2 != y1, "{y1} {y2}");
    // Dragging the thumb keeps following the pointer.
    ui.move_to(on_bar);
    ui.press(mouse::Button::Left);
    ui.move_to(Point::new(on_bar.x, on_bar.y + 100.0));
    ui.release(mouse::Button::Left);
    assert!(ui.app.active().unwrap().0.scroll_y > y2);
}

#[test]
fn typing_goes_through_keys_with_auto_pairs_and_undo() {
    let (mut ui, _) = Ui::with_file();
    ui.cmd(Command::SelectAll);
    ui.typed("f(");
    assert_eq!(ui.text(), "f()");
    ui.typed(")");
    assert_eq!(ui.text(), "f()");
    assert_eq!(ui.head(), (0, 3));
    ui.cmd(Command::Undo);
    ui.cmd(Command::Undo);
    assert_eq!(ui.text(), SOURCE);
}

#[test]
fn keyboard_shortcuts_reach_commands() {
    let (mut ui, _) = Ui::with_file();
    ui.key("d", Modifiers::CTRL | Modifiers::SHIFT, Some("D"));
    assert!(ui.text().starts_with("fn main() {\nfn main() {\n"), "{}", ui.text());
    ui.key("k", Modifiers::CTRL, None);
    assert!(ui.app.keymap.is_pending());
    ui.key("0", Modifiers::CTRL, None);
    assert!(!ui.app.active().unwrap().0.folds.ranges.is_empty(), "ctrl+k ctrl+0 folds everything");
    ui.key("k", Modifiers::CTRL, None);
    ui.key("j", Modifiers::CTRL, None);
    assert!(ui.app.active().unwrap().0.folds.ranges.is_empty());
}

#[test]
fn escape_collapses_cursors_and_arrows_move() {
    let (mut ui, _) = Ui::with_file();
    ui.cmd(Command::AddCursorBelow);
    assert_eq!(ui.app.active().unwrap().0.cursors.len(), 2);
    ui.key("escape", Modifiers::empty(), None);
    assert_eq!(ui.app.active().unwrap().0.cursors.len(), 1);
    // The newest cursor is the one that stays: it was on the second line.
    assert_eq!(ui.head().0, 1);
    ui.key("down", Modifiers::empty(), None);
    ui.key("right", Modifiers::CTRL, None);
    assert_eq!(ui.head().0, 2);
    assert!(ui.head().1 > 0);
}

#[test]
fn explorer_click_opens_files_and_folders_expand() {
    let dir = temp_dir("tree");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub/inner.txt"), "dentro\n").unwrap();
    std::fs::write(dir.join("top.txt"), "fora\n").unwrap();
    let mut ui = Ui::new(vec![dir]);
    assert_eq!(ui.app.sidebar, Some(SidebarView::Explorer));
    assert_eq!(ui.app.explorer.rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), vec!["sub", "top.txt"]);
    // Rows start under the sidebar's header; the first is about 40px down.
    let row0 = Point::new(100.0, 56.0);
    ui.click_at(row0);
    assert_eq!(ui.app.explorer.rows.len(), 3, "{:?}", ui.app.explorer.rows.iter().map(|r| &r.name).collect::<Vec<_>>());
    let row1 = Point::new(100.0, 56.0 + 26.0);
    ui.click_at(row1);
    assert_eq!(ui.app.active().unwrap().1.doc.file_name(), "inner.txt");
}

#[test]
fn palette_filters_and_runs_a_clicked_command() {
    let (mut ui, _) = Ui::with_file();
    ui.cmd(Command::CommandPalette);
    assert!(ui.app.palette.is_some());
    assert_eq!(ui.app.focus, Focus::Input);
    let title = Command::ToggleWordWrap.title(ui.app.pt()).to_string();
    let _ = ui.app.update(Message::Palette(crate::app::PaletteMsg::Input(title)));
    ui.frame();
    let items: Vec<String> = ui.app.palette.as_ref().unwrap().items.iter().map(|i| i.label.clone()).collect();
    assert!(!items.is_empty());
    // The card sits centred, 48px down; first row right below the input.
    let before = ui.app.settings.word_wrap;
    let target = ui.app.palette.as_ref().unwrap().items[0].clone();
    assert!(matches!(target.action, crate::app::PAction::Cmd(Command::ToggleWordWrap)), "{}", target.label);
    let p = Point::new(SIZE.width / 2.0, 48.0 + 10.0 + 44.0 + 12.0);
    ui.click_at(p);
    assert!(ui.app.palette.is_none(), "a click on a row closes it");
    assert_ne!(ui.app.settings.word_wrap, before);
    ui.cmd(Command::ToggleWordWrap);
}

#[test]
fn clicking_outside_the_palette_closes_it() {
    let (mut ui, _) = Ui::with_file();
    ui.cmd(Command::QuickOpen);
    assert_eq!(ui.app.palette.as_ref().unwrap().mode, PaletteMode::Files);
    ui.click_at(Point::new(10.0, SIZE.height - 60.0));
    assert!(ui.app.palette.is_none());
}

#[test]
fn every_command_runs_without_panicking_in_every_state() {
    let dir = temp_dir("all");
    std::fs::write(dir.join("a.rs"), SOURCE).unwrap();
    std::fs::write(dir.join("b.md"), "# t\n\ntexto\n").unwrap();
    let mut ui = Ui::new(vec![dir.clone()]);
    let skip = |c: Command| matches!(c, Command::Quit | Command::NewTerminal | Command::ToggleTerminal | Command::TogglePanel | Command::OpenFolderInFiles | Command::CloseFolder);
    // With nothing open.
    for c in Command::ALL.iter().copied().filter(|c| !skip(*c)) {
        eprintln!("-> {} on nothing", c.id());
        ui.cmd(c);
        ui.app.palette = None;
        ui.app.dialog = None;
        ui.app.focus = Focus::Editor;
    }
    // With a file open, then an empty file, each command from a fresh state.
    for name in ["a.rs", "b.md"] {
        for c in Command::ALL.iter().copied().filter(|c| !skip(*c)) {
            let _ = ui.app.close_tabs_where(|_, _| false);
            for t in ui.app.tabs.values_mut() {
                t.pinned = false;
            }
            ui.app.dialog = None;
            let ids: Vec<u64> = ui.app.tabs.keys().copied().collect();
            for id in ids {
                let _ = ui.app.close_tab(id);
            }
            let _ = ui.app.open_path(&dir.join(name));
            ui.frame();
            eprintln!("-> {} on {name}", c.id());
            ui.cmd(c);
            ui.app.palette = None;
            ui.app.dialog = None;
            ui.app.focus = Focus::Editor;
            ui.frame();
        }
    }
    assert!(Path::new(&dir).exists());
}

#[test]
fn split_views_of_one_document_stay_in_step() {
    let (mut ui, _) = Ui::with_file();
    let p = ui.cell(2, 0);
    ui.click_at(p);
    ui.cmd(Command::SplitRight);
    assert_eq!(ui.app.groups.len(), 2);
    assert_eq!(ui.app.docs.len(), 1);
    // Edit at the top in the new view: the first view's caret moves along.
    let first = ui.app.groups[0].active_tab().unwrap();
    let before = ui.app.tabs[&first].cursors.head();
    let _ = ui.app.update(Message::Key(crate::app::KeyEv { name: "home".into(), shifted: None, mods: Modifiers::CTRL, text: None, captured: false }));
    ui.typed("xyz");
    assert_eq!(ui.app.tabs[&first].cursors.head(), before + 3);
    ui.cmd(Command::CloseTab);
    assert_eq!(ui.app.groups.len(), 1);
    assert_eq!(ui.app.docs.len(), 1);
}

/// A split closing used to hand its tab bar's state to the next group, which panicked on draw.
#[test]
fn closing_a_split_with_tabs_on_either_side_draws_fine() {
    let dir = temp_dir("groups");
    for n in ["a.txt", "b.txt", "c.txt", "d.txt"] {
        std::fs::write(dir.join(n), n).unwrap();
    }
    let mut ui = Ui::new(vec![dir.join("a.txt")]);
    // Churn the first group's tab ids so they run ahead of the second's.
    for _ in 0..6 {
        let _ = ui.app.open_path(&dir.join("b.txt"));
        ui.frame();
        ui.cmd(Command::CloseTab);
    }
    ui.cmd(Command::SplitRight);
    let _ = ui.app.open_path(&dir.join("c.txt"));
    ui.frame();
    let _ = ui.app.update(Message::Cmd(Command::FocusPrevGroup));
    ui.frame();
    ui.cmd(Command::CloseGroup);
    assert_eq!(ui.app.groups.len(), 1);
    ui.frame();
    let _ = ui.app.open_path(&dir.join("d.txt"));
    ui.frame();
    ui.cmd(Command::SplitRight);
    ui.cmd(Command::SplitRight);
    ui.cmd(Command::CloseTab);
    ui.frame();
}

#[test]
fn closing_a_dirty_tab_asks_first() {
    let (mut ui, _) = Ui::with_file();
    ui.typed("x");
    ui.cmd(Command::CloseTab);
    assert!(matches!(ui.app.dialog, Some(crate::app::DialogState::CloseUnsaved(_))));
    assert_eq!(ui.app.tabs.len(), 1);
    let _ = ui.app.update(Message::Dialog(crate::app::DialogMsg::Secondary));
    ui.frame();
    assert_eq!(ui.app.tabs.len(), 0);
}

#[test]
fn saving_writes_the_file_and_clears_dirty() {
    let (mut ui, p) = Ui::with_file();
    ui.typed("//");
    assert!(ui.app.active().unwrap().1.doc.is_dirty());
    ui.cmd(Command::Save);
    assert!(!ui.app.active().unwrap().1.doc.is_dirty());
    assert!(std::fs::read_to_string(&p).unwrap().starts_with("//fn main"));
}

#[test]
fn find_highlights_and_replace_all_works() {
    let (mut ui, _) = Ui::with_file();
    ui.cmd(Command::Find);
    let _ = ui.app.update(Message::Find(crate::app::FindMsg::Query("alpha".into())));
    ui.frame();
    assert_eq!(ui.app.active().unwrap().1.matches.len(), 2);
    assert_eq!(ui.selection_text(), "alpha");
    let _ = ui.app.update(Message::Find(crate::app::FindMsg::Replacement("omega".into())));
    let _ = ui.app.update(Message::Find(crate::app::FindMsg::ReplaceAll));
    ui.frame();
    assert_eq!(ui.text().matches("omega").count(), 2);
    assert_eq!(ui.app.active().unwrap().1.matches.len(), 0);
    ui.cmd(Command::Undo);
    assert_eq!(ui.text(), SOURCE);
}

/// More matches than get highlighted: all of them still change.
#[test]
fn replace_all_goes_past_the_highlight_limit() {
    let dir = temp_dir("limit");
    let p = dir.join("many.txt");
    let n = crate::app::MAX_SHOWN_MATCHES + 500;
    std::fs::write(&p, "x\n".repeat(n)).unwrap();
    let mut ui = Ui::new(vec![p]);
    ui.cmd(Command::Find);
    let _ = ui.app.update(Message::Find(crate::app::FindMsg::Query("x".into())));
    ui.frame();
    assert_eq!(ui.app.active().unwrap().1.matches.len(), crate::app::MAX_SHOWN_MATCHES);
    let _ = ui.app.update(Message::Find(crate::app::FindMsg::Replacement("y".into())));
    let _ = ui.app.update(Message::Find(crate::app::FindMsg::ReplaceAll));
    ui.frame();
    assert_eq!(ui.text().matches('y').count(), n);
    assert_eq!(ui.text().matches('x').count(), 0);
}

/// Replacing in the folder used to act on the list on screen, which stops at a limit.
#[test]
fn replace_in_files_reaches_files_past_the_list_limit() {
    let dir = temp_dir("limit2");
    // One file fills the list; the next, alphabetically or not, would be left out of it.
    std::fs::write(dir.join("a.txt"), "x\n".repeat(60_000)).unwrap();
    std::fs::write(dir.join("b.txt"), "x\n").unwrap();
    std::fs::write(dir.join("c.txt"), "x\n").unwrap();
    let mut ui = Ui::new(vec![dir.clone()]);
    let _ = ui.app.update(Message::Search(crate::app::SearchMsg::Query("x".into())));
    wait_for(&mut ui, |a| !a.search.running);
    assert!(ui.app.search.truncated, "the list should say it stopped early");
    let _ = ui.app.update(Message::Search(crate::app::SearchMsg::Replacement("y".into())));
    let _ = ui.app.update(Message::Search(crate::app::SearchMsg::ReplaceAll));
    wait_for(&mut ui, |a| !a.search.replacing);
    for f in ["a.txt", "b.txt", "c.txt"] {
        assert!(!std::fs::read_to_string(dir.join(f)).unwrap().contains('x'), "{f}");
    }
}

#[test]
fn a_separate_instance_leaves_session_and_backups_alone() {
    let (mut ui, _) = Ui::with_file();
    assert!(ui.app.new_session);
    tinta_core::session::write_backup("of-the-main-instance", "texto").unwrap();
    ui.typed("x");
    ui.app.persist_on_quit();
    assert_eq!(tinta_core::session::read_backup("of-the-main-instance").as_deref(), Some("texto"));
    tinta_core::session::remove_backup("of-the-main-instance");
    // And since nothing is kept for it, quitting with unsaved work asks.
    let _ = ui.app.request_quit();
    assert!(matches!(ui.app.dialog, Some(crate::app::DialogState::QuitUnsaved)));
}

#[test]
fn opening_a_file_does_not_lose_the_session_it_would_have_restored() {
    let (mut ui, p) = Ui::with_file();
    ui.typed("rascunho ");
    ui.app.write_backups(false);
    crate::app::save_session(&ui.app);
    let text = ui.text();
    drop(ui);
    let other = temp_dir("other").join("b.txt");
    std::fs::write(&other, "b").unwrap();
    let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let flags = Flags { targets: vec![(other, None)], new_session: false };
    let (app, _) = App::init(Core::default(), flags);
    drop(guard);
    assert_eq!(app.active().unwrap().1.doc.file_name(), "b.txt", "the requested file is in front");
    let id = app.doc_by_path(&p).expect("the old tab came back");
    assert_eq!(app.docs[&id].doc.rope.to_string(), text);
    assert!(app.docs[&id].doc.is_dirty());
}

#[test]
fn word_wrap_keeps_clicks_on_the_right_cell() {
    let dir = temp_dir("wrap");
    let p = dir.join("w.txt");
    std::fs::write(&p, "palavra ".repeat(200)).unwrap();
    let mut ui = Ui::new(vec![p]);
    ui.cmd(Command::ToggleWordWrap);
    ui.frame();
    let (t, e) = ui.app.active().unwrap();
    assert!(t.map.row_count(&e.doc) > 3, "{}", t.map.row_count(&e.doc));
    // Row 2 starts at a word: its first cell is the start of that word.
    let row = t.map.row(&e.doc, 2);
    let start = row.start as usize;
    let p = ui.cell(2, 0);
    ui.click_at(p);
    assert_eq!(ui.head(), (0, start));
    ui.cmd(Command::ToggleWordWrap);
}

#[test]
fn hot_exit_brings_unsaved_text_back() {
    let (mut ui, p) = Ui::with_file();
    ui.typed("rascunho ");
    ui.app.write_backups(false);
    ui.app.settings.restore_session = true;
    crate::app::save_session(&ui.app);
    let doc_text = ui.text();
    drop(ui);
    // A new run, restoring: same text, still dirty, nothing written to disk.
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    drop(_g);
    let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let flags = Flags { targets: Vec::new(), new_session: false };
    let (app, _) = App::init(Core::default(), flags);
    drop(guard);
    let (_, e) = app.active().expect("the tab came back");
    assert_eq!(e.doc.rope.to_string(), doc_text);
    assert!(e.doc.is_dirty());
    assert_eq!(std::fs::read_to_string(&p).unwrap(), SOURCE);
}

/// Not an assertion so much as a measurement: `cargo test --release large_file -- --ignored --nocapture`.
#[test]
#[ignore]
fn large_file_stays_responsive() {
    let dir = temp_dir("large");
    let p = dir.join("big.rs");
    let line = "    let value = compute(alpha, beta) + other_thing(\"texto\", 42); // comentário\n";
    let mut src = String::from("fn main() {\n");
    for _ in 0..300_000 {
        src.push_str(line);
    }
    src.push_str("}\n");
    std::fs::write(&p, &src).unwrap();
    let t = Instant::now();
    let mut ui = Ui::new(vec![p]);
    eprintln!("open + first frames: {:?} ({} MB)", t.elapsed(), src.len() / 1_000_000);
    let measure = |ui: &mut Ui, label: &str, n: usize, f: &mut dyn FnMut(&mut Ui)| {
        let mut worst = std::time::Duration::ZERO;
        let t = Instant::now();
        for _ in 0..n {
            let one = Instant::now();
            f(ui);
            worst = worst.max(one.elapsed());
        }
        eprintln!("{label}: mean {:?}, worst {:?}", t.elapsed() / n as u32, worst);
    };
    measure(&mut ui, "idle frame", 30, &mut |ui| ui.frame());
    measure(&mut ui, "highlight tick + frame", 30, &mut |ui| {
        let _ = ui.app.update(Message::TickFrame);
        ui.frame();
    });
    let _ = ui.app.update(Message::Cmd(Command::GoToLine));
    let _ = ui.app.update(Message::Palette(crate::app::PaletteMsg::Close));
    ui.app.jump_to(150_000, 10);
    ui.frame();
    measure(&mut ui, "typing mid-file", 30, &mut |ui| ui.typed("x"));
    measure(&mut ui, "newline mid-file", 10, &mut |ui| ui.key("enter", Modifiers::empty(), None));
    measure(&mut ui, "arrow down", 30, &mut |ui| ui.key("down", Modifiers::empty(), None));
    measure(&mut ui, "undo", 10, &mut |ui| ui.cmd(Command::Undo));
    let _ = ui.app.update(Message::Cmd(Command::Find));
    let t = Instant::now();
    let _ = ui.app.update(Message::Find(crate::app::FindMsg::Query("alpha".into())));
    ui.frame();
    eprintln!("find 300k matches: {:?} ({} found)", t.elapsed(), ui.app.active().unwrap().1.matches.len());
    measure(&mut ui, "frame with matches", 10, &mut |ui| ui.frame());
    measure(&mut ui, "typing with find open", 5, &mut |ui| {
        ui.app.focus = Focus::Editor;
        ui.typed("y");
    });
    let _ = ui.app.update(Message::Find(crate::app::FindMsg::Close));
    let t = Instant::now();
    ui.cmd(Command::SelectAll);
    eprintln!("select all: {:?}", t.elapsed());
    ui.key("escape", Modifiers::empty(), None);
    let t = Instant::now();
    ui.cmd(Command::ToggleWordWrap);
    eprintln!("turn on word wrap: {:?}", t.elapsed());
    measure(&mut ui, "typing with wrap", 5, &mut |ui| ui.typed("z"));
}

/// Menus are surfaces of their own, and closing one asks the same question as closing the window.
#[test]
fn closing_a_menu_popup_does_not_quit() {
    let (ui, _) = Ui::with_file();
    assert!(ui.app.on_close_requested(window::Id::unique()).is_none());
    let main = ui.app.core.main_window_id();
    if let Some(id) = main {
        assert!(ui.app.on_close_requested(id).is_some());
    }
}

#[test]
fn menu_labels_are_really_drawn() {
    let (ui, _) = Ui::with_file();
    let el: Element<'_, Message> = crate::ui::menu::menu_bar(&ui.app);
    let mut r = Renderer::Secondary(iced_tiny_skia::Renderer::new(Font::DEFAULT, Pixels(14.0)));
    let mut bar = UserInterface::build(el, Size::new(1000.0, 48.0), user_interface::Cache::new(), &mut r);
    let mut msgs = Vec::new();
    let mut clip = clipboard::Null;
    let cursor = mouse::Cursor::Available(Point::new(900.0, 40.0));
    let _ = bar.update(&[Event::Window(window::Event::RedrawRequested(Instant::now()))], cursor, &mut r, &mut clip, &mut msgs);
    let style = cosmic::iced::advanced::renderer::Style { icon_color: cosmic::iced::Color::WHITE, text_color: cosmic::iced::Color::WHITE, scale_factor: 1.0 };
    bar.draw(&mut r, &ui.theme, &style, cursor);
    drop(bar);
    let Renderer::Secondary(sk) = &mut r else { panic!() };
    let texts: Vec<String> = sk.layers().iter().flat_map(|l| l.text.iter().map(|t| format!("{t:?}").chars().take(160).collect::<String>())).collect();
    assert_eq!(texts.len(), 9, "{texts:?}");
}
