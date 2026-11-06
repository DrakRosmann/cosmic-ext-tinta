//! Icons. Generic ones come from the desktop's theme; the few it has no idea about are bundled.

use cosmic::widget::icon;

macro_rules! svg {
    ($body:literal) => {
        concat!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">"#, $body, "</svg>").as_bytes()
    };
}

fn bundled(bytes: &'static [u8]) -> icon::Handle {
    icon::from_svg_bytes(bytes).symbolic(true)
}

pub fn named(name: &'static str) -> icon::Handle {
    icon::from_name(name).symbolic(true).handle()
}

pub fn files() -> icon::Handle {
    bundled(svg!(r#"<path d="M9 2H4.5A1.5 1.5 0 0 0 3 3.5v9A1.5 1.5 0 0 0 4.5 14h7a1.5 1.5 0 0 0 1.5-1.5V6z"/><path d="M9 2v4h4"/>"#))
}

pub fn search() -> icon::Handle {
    bundled(svg!(r#"<circle cx="7" cy="7" r="4.5"/><path d="m10.5 10.5 3.5 3.5"/>"#))
}

pub fn git() -> icon::Handle {
    bundled(svg!(r#"<circle cx="4.5" cy="3.5" r="1.7"/><circle cx="4.5" cy="12.5" r="1.7"/><circle cx="11.5" cy="5.5" r="1.7"/><path d="M4.5 5.2v5.6M11.5 7.2c0 2.3-2.5 2.8-7 3.4"/>"#))
}

pub fn outline() -> icon::Handle {
    bundled(svg!(r#"<path d="M2.5 3.5h3M2.5 8h3M2.5 12.5h3M8 3.5h5.5M8 8h5.5M8 12.5h5.5"/>"#))
}

pub fn terminal() -> icon::Handle {
    bundled(svg!(r#"<rect x="1.8" y="2.8" width="12.4" height="10.4" rx="1.6"/><path d="m4.5 6 2.5 2-2.5 2M8.5 10.5h3"/>"#))
}

pub fn chevron_right() -> icon::Handle {
    bundled(svg!(r#"<path d="m6 3.5 4.5 4.5L6 12.5"/>"#))
}

pub fn chevron_down() -> icon::Handle {
    bundled(svg!(r#"<path d="m3.5 6 4.5 4.5L12.5 6"/>"#))
}

pub fn chevron_up() -> icon::Handle {
    bundled(svg!(r#"<path d="m3.5 10 4.5-4.5 4.5 4.5"/>"#))
}

pub fn close() -> icon::Handle {
    bundled(svg!(r#"<path d="m4 4 8 8M12 4l-8 8"/>"#))
}

pub fn plus() -> icon::Handle {
    bundled(svg!(r#"<path d="M8 3v10M3 8h10"/>"#))
}

pub fn minus() -> icon::Handle {
    bundled(svg!(r#"<path d="M3 8h10"/>"#))
}

pub fn undo() -> icon::Handle {
    bundled(svg!(r#"<path d="M5.5 3 2.5 6l3 3"/><path d="M2.5 6h7a4 4 0 0 1 0 8h-3"/>"#))
}

pub fn refresh() -> icon::Handle {
    bundled(svg!(r#"<path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9L13.5 5.7"/><path d="M13.5 2.5v3.2h-3.2"/>"#))
}

pub fn new_file() -> icon::Handle {
    bundled(svg!(r#"<path d="M8.5 2H4.5A1.5 1.5 0 0 0 3 3.5v9A1.5 1.5 0 0 0 4.5 14h3"/><path d="M8.5 2 13 6.5V8"/><path d="M8.5 2v4.5H13"/><path d="M12 10v4M10 12h4"/>"#))
}

pub fn new_folder() -> icon::Handle {
    bundled(svg!(r#"<path d="M7.5 13H3a1.5 1.5 0 0 1-1.5-1.5v-7A1.5 1.5 0 0 1 3 3h3l1.5 1.8H13a1.5 1.5 0 0 1 1.5 1.5v1.5"/><path d="M12 10v4M10 12h4"/>"#))
}

pub fn collapse() -> icon::Handle {
    bundled(svg!(r#"<rect x="2.5" y="2.5" width="11" height="11" rx="1.5"/><path d="M5.5 8h5"/>"#))
}

pub fn settings() -> icon::Handle {
    named("preferences-system-symbolic")
}

pub fn error() -> icon::Handle {
    bundled(svg!(r#"<circle cx="8" cy="8" r="6"/><path d="m5.8 5.8 4.4 4.4M10.2 5.8l-4.4 4.4"/>"#))
}

pub fn warning() -> icon::Handle {
    bundled(svg!(r#"<path d="M8 2.5 14.5 13.5h-13z"/><path d="M8 6.5v3.5M8 11.8v.2"/>"#))
}

pub fn branch() -> icon::Handle {
    git()
}

pub fn push() -> icon::Handle {
    bundled(svg!(r#"<path d="M8 13V3M4 7l4-4 4 4"/>"#))
}

pub fn pull() -> icon::Handle {
    bundled(svg!(r#"<path d="M8 3v10M4 9l4 4 4-4"/>"#))
}

pub fn split() -> icon::Handle {
    bundled(svg!(r#"<rect x="2" y="2.8" width="12" height="10.4" rx="1.5"/><path d="M8 2.8v10.4"/>"#))
}

