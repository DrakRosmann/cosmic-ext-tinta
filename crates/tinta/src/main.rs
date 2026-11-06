//! Tinta: a code editor for the COSMIC desktop.

mod app;
mod bus;
mod i18n;
mod state;
#[cfg(test)]
mod ui_tests;
mod ui;

use app::{App, Flags};
use cosmic::app::Settings;
use cosmic::iced::{Limits, Size};
use std::path::PathBuf;

fn main() -> cosmic::iced::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let mut targets = Vec::new();
    let mut new_session = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("tinta {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--help" | "-h" => {
                println!("{}", USAGE);
                return Ok(());
            }
            "--new-session" => new_session = true,
            other => targets.push(parse_target(other)),
        }
    }

    let (settings, _) = tinta_core::settings::Settings::load();
    let theme = match settings.ui_theme {
        tinta_core::settings::AppTheme::System => cosmic::theme::system_preference(),
        tinta_core::settings::AppTheme::Dark => cosmic::Theme::dark(),
        tinta_core::settings::AppTheme::Light => cosmic::Theme::light(),
    };
    let window = Settings::default()
        .size(Size::new(1280.0, 820.0))
        .size_limits(Limits::NONE.min_width(480.0).min_height(320.0))
        .theme(theme)
        .exit_on_close(false);
    cosmic::app::run::<App>(window, Flags { targets, new_session })
}

const USAGE: &str = "tinta: code editor for the COSMIC desktop

Usage: cosmic-ext-tinta [--new-session] [FILE[:LINE[:COL]] | FOLDER]...

  --new-session   ignore the saved session
  --version       print the version
";

/// `src/main.rs:10:4` opens at that spot, as compilers print it.
fn parse_target(arg: &str) -> (PathBuf, Option<(usize, usize)>) {
    let path = PathBuf::from(arg);
    if path.exists() {
        return (path, None);
    }
    let mut parts = arg.rsplitn(3, ':');
    let last = parts.next().and_then(|s| s.parse::<usize>().ok());
    let mid = parts.next();
    let rest = parts.next();
    match (last, mid, rest) {
        (Some(col), Some(line), Some(file)) if line.parse::<usize>().is_ok() => {
            (PathBuf::from(file), Some((line.parse::<usize>().unwrap().saturating_sub(1), col.saturating_sub(1))))
        }
        (Some(line), Some(file), _) => (PathBuf::from(file), Some((line.saturating_sub(1), 0))),
        _ => (path, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_with_positions() {
        assert_eq!(parse_target("nope/a.rs:10:4"), (PathBuf::from("nope/a.rs"), Some((9, 3))));
        assert_eq!(parse_target("nope/a.rs:10"), (PathBuf::from("nope/a.rs"), Some((9, 0))));
        assert_eq!(parse_target("nope/a.rs"), (PathBuf::from("nope/a.rs"), None));
    }
}
