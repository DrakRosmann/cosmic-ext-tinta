//! The debugger's side panel and console.

use super::sidebar::header;
use super::workspace::dim_text;
use super::icons;
use crate::app::{App, DEBUG_INPUT, DebugMsg, Message, WATCH_INPUT};
use crate::i18n::tr;
use cosmic::iced::widget::text::Wrapping;
use cosmic::iced::{Alignment, Color, Length};
use cosmic::widget::{self, button, column, container, icon, row, scrollable, text};
use cosmic::{Element, theme};
use tinta_core::dap::{BpState, RunState, VarRow};
use tinta_core::keymap::Command;

fn tool<'a>(h: icon::Handle, tip: &'static str, msg: Option<Message>) -> Element<'a, Message> {
    let b = button::custom(icon::icon(h).size(16)).padding(5).class(theme::Button::Icon).on_press_maybe(msg);
    widget::tooltip(b, text(tr(tip)).size(12), widget::tooltip::Position::Bottom).into()
}

fn section(title: &'static str) -> Element<'static, Message> {
    container(text(tr(title).to_uppercase()).size(11).font(cosmic::font::semibold()).class(theme::Text::Color(dim_text())))
        .padding([10, 12, 3, 12])
        .into()
}

fn hint(s: &'static str) -> Element<'static, Message> {
    container(text(tr(s)).size(12).class(theme::Text::Color(dim_text()))).padding([1, 12]).into()
}

fn accent() -> Color {
    cosmic::theme::active().cosmic().accent.base.into()
}

fn tree_row(app: &App, r: VarRow, remove: Option<String>) -> Element<'_, Message> {
    let chevron: Element<'_, Message> = if r.expandable {
        icon::icon(if r.expanded { icons::chevron_down() } else { icons::chevron_right() }).size(10).into()
    } else {
        widget::Space::new().width(Length::Fixed(10.0)).into()
    };
    let is_scope = r.is_scope;
    let changed = r.changed;
    let mut line = row::with_capacity(5)
        .spacing(5)
        .align_y(Alignment::Center)
        .push(widget::Space::new().width(Length::Fixed(r.depth as f32 * 12.0)))
        .push(chevron);
    if is_scope {
        line = line.push(text(r.name.clone()).size(12).font(cosmic::font::semibold()).wrapping(Wrapping::None));
    } else {
        line = line.push(text(format!("{}:", r.name)).size(12).font(app.metrics.font).wrapping(Wrapping::None));
        let color = if changed { accent() } else { dim_text() };
        line = line.push(text(r.value.clone()).size(12).font(app.metrics.font).wrapping(Wrapping::None).class(theme::Text::Color(color)));
    }
    let long = r.value.chars().count() > 28;
    let value = r.value.clone();
    let mut b = button::custom(container(line).clip(true)).class(crate::ui::row_button(false)).padding([2, 6]).width(Length::Fill);
    if r.expandable {
        b = b.on_press(Message::Debug(DebugMsg::ToggleRow(r)));
    }
    let main: Element<'_, Message> = if long {
        widget::tooltip(b, text(value).size(12), widget::tooltip::Position::Bottom).into()
    } else {
        b.into()
    };
    match remove {
        Some(expr) => row::with_children(vec![
            container(main).width(Length::Fill).into(),
            button::custom(icon::icon(icons::close()).size(11)).padding(3).class(theme::Button::Icon).on_press(Message::Debug(DebugMsg::WatchRemove(expr))).into(),
        ])
        .align_y(Alignment::Center)
        .into(),
        None => main,
    }
}

pub fn sidebar(app: &App) -> Element<'_, Message> {
    let d = &app.debug;
    let live = d.session.as_ref().filter(|s| s.is_live());
    let stopped = d.is_stopped();
    let cmd = |c: Command, on: bool| on.then_some(Message::Cmd(c));

    let mut body = column::with_capacity(16).width(Length::Fill);
    match live {
        Some(s) => {
            let mut bar = row::with_capacity(7).spacing(2).padding([0, 8]).align_y(Alignment::Center);
            bar = if s.state == RunState::Running {
                bar.push(tool(icons::pause(), "Pause", Some(Message::Cmd(Command::DebugPause))))
            } else {
                bar.push(tool(icons::play(), "Continue (F5)", cmd(Command::DebugStart, stopped)))
            };
            bar = bar
                .push(tool(icons::step_over(), "Step Over (F10)", cmd(Command::StepOver, stopped)))
                .push(tool(icons::step_in(), "Step Into (F11)", cmd(Command::StepInto, stopped)))
                .push(tool(icons::step_out(), "Step Out (Shift+F11)", cmd(Command::StepOut, stopped)))
                .push(tool(icons::refresh(), "Restart", Some(Message::Cmd(Command::DebugRestart))))
                .push(tool(icons::stop(), "Stop (Shift+F5)", Some(Message::Cmd(Command::DebugStop))));
            body = body.push(bar);
            let state = match &s.state {
                RunState::Starting => tr("Starting").to_string(),
                RunState::Running => tr("Running").to_string(),
                RunState::Stopped { reason, description } => {
                    let why = description.clone().unwrap_or_else(|| reason.clone());
                    format!("{}: {why}", tr("Paused"))
                }
                RunState::Ended => tr("Debug session ended").to_string(),
            };
            body = body.push(container(text(format!("{} · {state}", s.name)).size(12).class(theme::Text::Color(dim_text())).wrapping(Wrapping::None)).clip(true).padding([2, 12]));
        }
        None => {
            let label = match &d.selected {
                Some(n) => format!("{} ({n})", tr("Start Debugging")),
                None => tr("Start Debugging").to_string(),
            };
            let busy = d.after_build.as_ref().is_some_and(|(g, _)| *g == app.tasks.generation) && app.tasks.running.is_some();
            let start: Element<'_, Message> = if busy {
                text(tr("Building…")).size(13).into()
            } else {
                button::suggested(label).on_press(Message::Cmd(Command::DebugStart)).into()
            };
            body = body.push(container(start).padding([4, 12]));
            body = body.push(
                row::with_capacity(2)
                    .spacing(4)
                    .padding([2, 8])
                    .push(button::text(tr("Choose configuration")).on_press(Message::Cmd(Command::DebugSelectConfig)))
                    .push(button::text(tr("Edit launch.json")).on_press(Message::Cmd(Command::DebugConfigure))),
            );
        }
    }

    body = body.push(section("Variables"));
    match live {
        Some(s) if stopped => {
            for r in s.variable_rows() {
                body = body.push(tree_row(app, r, None));
            }
        }
        _ => body = body.push(hint("Shown when the program pauses")),
    }

    body = body.push(section("Watch"));
    // A row, not a container: a container takes on its child's id, and when the rows above this
    // one come and go, libcosmic hands the field the container's state and panics.
    body = body.push(
        row::with_children(vec![
            widget::text_input(tr("Add an expression"), d.watch_input.as_str())
                .id(widget::Id::new(WATCH_INPUT))
                .size(12)
                .on_input(|s| Message::Debug(DebugMsg::WatchInput(s)))
                .on_submit(|_| Message::Debug(DebugMsg::WatchSubmit))
                .on_focus(Message::InputFocus(true))
                .on_unfocus(Message::InputFocus(false))
                .into(),
        ])
        .padding([2, 12, 4, 12]),
    );
    match live {
        Some(s) if stopped => {
            for (i, r) in s.watch_rows() {
                let expr = (r.depth == 0).then(|| s.watches[i].expression.clone());
                body = body.push(tree_row(app, r, expr));
            }
        }
        _ => {
            for w in &d.watches {
                let r = VarRow { depth: 0, path: String::new(), name: w.clone(), value: String::new(), reference: 0, expandable: false, expanded: false, changed: false, is_scope: false };
                body = body.push(tree_row(app, r, Some(w.clone())));
            }
        }
    }

    body = body.push(section("Call Stack"));
    match live {
        Some(s) if stopped => {
            if s.threads.len() > 1 {
                for t in &s.threads {
                    let on = s.thread == Some(t.id);
                    body = body.push(
                        button::custom(text(format!("{} {}", t.id, t.name)).size(12).wrapping(Wrapping::None))
                            .class(crate::ui::row_button(on))
                            .padding([2, 12])
                            .width(Length::Fill)
                            .on_press(Message::Debug(DebugMsg::SelectThread(t.id))),
                    );
                }
            }
            for (i, f) in s.frames.iter().enumerate() {
                let place = match &f.path {
                    Some(p) => format!("{}:{}", p.file_name().unwrap_or_default().to_string_lossy(), f.line + 1),
                    None => tr("no source").to_string(),
                };
                body = body.push(
                    button::custom(
                        container(
                            row::with_children(vec![
                                text(f.name.clone()).size(12).font(app.metrics.font).wrapping(Wrapping::None).into(),
                                text(place).size(11).class(theme::Text::Color(dim_text())).wrapping(Wrapping::None).into(),
                            ])
                            .spacing(8)
                            .align_y(Alignment::Center),
                        )
                        .clip(true),
                    )
                    .class(crate::ui::row_button(i == s.frame))
                    .padding([2, 12])
                    .width(Length::Fill)
                    .on_press(Message::Debug(DebugMsg::SelectFrame(i))),
                );
            }
        }
        _ => body = body.push(hint("Shown when the program pauses")),
    }

    body = body.push(section("Breakpoints"));
    if d.breakpoints.is_empty() {
        body = body.push(hint("F9 or a click left of the line numbers"));
    }
    let t = cosmic::theme::active();
    let t = t.cosmic();
    let red: Color = t.destructive.base.into();
    for (path, list) in &d.breakpoints {
        for b in list {
            let color = if b.enabled && b.state != BpState::Pending { red } else { dim_text() };
            let dot = button::custom(text(if b.enabled { "●" } else { "○" }).size(12).class(theme::Text::Color(color)))
                .class(theme::Button::Icon)
                .padding([0, 3])
                .on_press(Message::Debug(DebugMsg::BreakpointEnable(path.clone(), b.line)));
            let name = format!("{}:{}", path.file_name().unwrap_or_default().to_string_lossy(), b.line + 1);
            let mut label = row::with_capacity(2).spacing(8).align_y(Alignment::Center).push(text(name).size(12).wrapping(Wrapping::None));
            if let Some(c) = &b.condition {
                label = label.push(text(c.clone()).size(11).font(app.metrics.font).class(theme::Text::Color(dim_text())).wrapping(Wrapping::None));
            }
            body = body.push(
                row::with_children(vec![
                    dot.into(),
                    button::custom(container(label).clip(true))
                        .class(crate::ui::row_button(false))
                        .padding([2, 4])
                        .width(Length::Fill)
                        .on_press(Message::Debug(DebugMsg::BreakpointJump(path.clone(), b.line)))
                        .into(),
                    button::custom(icon::icon(icons::close()).size(11))
                        .padding(3)
                        .class(theme::Button::Icon)
                        .on_press(Message::Debug(DebugMsg::BreakpointRemove(path.clone(), b.line)))
                        .into(),
                ])
                .padding([0, 8])
                .align_y(Alignment::Center),
            );
        }
    }

    column::with_children(vec![
        header(tr("Run and Debug").to_string(), Vec::new()),
        scrollable(body.padding([0, 0, 12, 0])).width(Length::Fill).height(Length::Fill).into(),
    ])
    .into()
}

pub fn console(app: &App) -> Element<'_, Message> {
    let t = cosmic::theme::active();
    let t = t.cosmic();
    let err: Color = t.destructive.base.into();
    let d = &app.debug;
    let start = d.console.len().saturating_sub(800);
    let mut lines = column::with_capacity(d.console.len() - start).padding([4, 12]).width(Length::Fill);
    for l in &d.console[start..] {
        let color = match l.category.as_str() {
            "stderr" => Some(err),
            "input" | "console" => Some(dim_text()),
            "result" => Some(accent()),
            _ => None,
        };
        let mut label = text(l.text.as_str()).size(12).font(app.metrics.font).wrapping(Wrapping::None);
        if let Some(c) = color {
            label = label.class(theme::Text::Color(c));
        }
        lines = lines.push(label);
    }
    column::with_children(vec![
        container(scrollable(lines).width(Length::Fill).height(Length::Fill).anchor_bottom()).height(Length::Fill).into(),
        row::with_children(vec![
            widget::text_input(tr("Expression to evaluate; -exec for a debugger command"), d.input.as_str())
                .id(widget::Id::new(DEBUG_INPUT))
                .size(12)
                .on_input(|s| Message::Debug(DebugMsg::Input(s)))
                .on_submit(|_| Message::Debug(DebugMsg::Submit))
                .on_focus(Message::InputFocus(true))
                .on_unfocus(Message::InputFocus(false))
                .into(),
        ])
        .padding([2, 10, 6, 10])
        .into(),
    ])
    .into()
}
