//! Types into a Wayland compositor, the way a real keyboard does, so end-to-end tests cover the
//! whole path: compositor, xkb, dead keys, the toolkit's text fields.
//!
//!   tinta-vkbd LAYOUT < steps
//!
//! Each line of the steps:
//!   wait 200            pause in milliseconds
//!   key ctrl+shift+p    a chord; key names are xkb names (a, Return, Escape, F1, Up, BackSpace...)
//!   text olá mundo      types the text, finding each char in the layout, dead keys included

use rustix::fs::{MemfdFlags, ftruncate, memfd_create};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::os::fd::AsFd;
use std::time::{Duration, Instant};
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1, zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};
use xkbcommon::xkb;

#[derive(Default)]
struct App {
    seat: Option<wl_seat::WlSeat>,
    manager: Option<ZwpVirtualKeyboardManagerV1>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for App {
    fn event(s: &mut Self, r: &wl_registry::WlRegistry, e: wl_registry::Event, _: &(), _: &Connection, q: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, version } = e {
            match interface.as_str() {
                "wl_seat" => s.seat = Some(r.bind(name, version.min(5), q, ())),
                "zwp_virtual_keyboard_manager_v1" => s.manager = Some(r.bind(name, 1, q, ())),
                _ => {}
            }
        }
    }
}

macro_rules! ignore {
    ($($t:ty),*) => {$(
        impl Dispatch<$t, ()> for App {
            fn event(_: &mut Self, _: &$t, _: <$t as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
        }
    )*};
}
ignore!(wl_seat::WlSeat, ZwpVirtualKeyboardManagerV1, ZwpVirtualKeyboardV1);

const SHIFT: u32 = 1;
const CTRL: u32 = 4;
const ALT: u32 = 8;
const LOGO: u32 = 64;
const LEVEL3: u32 = 128;

// Linux keycodes (xkb minus 8) for the modifiers.
const KEY_SHIFT: u32 = 42;
const KEY_CTRL: u32 = 29;
const KEY_ALT: u32 = 56;
const KEY_LOGO: u32 = 125;
const KEY_ALTGR: u32 = 100;

struct Typer<'a> {
    kb: &'a ZwpVirtualKeyboardV1,
    conn: &'a Connection,
    start: Instant,
    /// Char → (keycode, modifier mask).
    chars: HashMap<char, (u32, u32)>,
    /// Keysym name → (keycode, modifier mask).
    names: HashMap<String, (u32, u32)>,
}

impl Typer<'_> {
    fn time(&self) -> u32 {
        self.start.elapsed().as_millis() as u32
    }

    fn raw(&self, key: u32, down: bool) {
        self.kb.key(self.time(), key, u32::from(down));
    }

    fn press(&self, key: u32, mods: u32) {
        let held: &[(u32, u32)] = &[(CTRL, KEY_CTRL), (SHIFT, KEY_SHIFT), (ALT, KEY_ALT), (LOGO, KEY_LOGO), (LEVEL3, KEY_ALTGR)];
        for (m, k) in held {
            if mods & m != 0 {
                self.raw(*k, true);
            }
        }
        self.kb.modifiers(mods, 0, 0, 0);
        self.flush();
        self.raw(key, true);
        self.flush();
        std::thread::sleep(Duration::from_millis(12));
        self.raw(key, false);
        for (m, k) in held {
            if mods & m != 0 {
                self.raw(*k, false);
            }
        }
        self.kb.modifiers(0, 0, 0, 0);
        self.flush();
        std::thread::sleep(Duration::from_millis(12));
    }

    fn flush(&self) {
        let _ = self.conn.flush();
    }

    fn chord(&self, spec: &str) {
        let mut mods = 0;
        let mut parts: Vec<&str> = spec.split('+').collect();
        // "ctrl++" and friends: an empty last part means the key is the plus sign.
        if spec.ends_with('+') && parts.len() > 1 {
            parts.retain(|p| !p.is_empty());
            parts.push("+");
        }
        let key = parts.pop().expect("a key");
        for m in parts {
            mods |= match m {
                "ctrl" => CTRL,
                "shift" => SHIFT,
                "alt" => ALT,
                "super" => LOGO,
                "altgr" => LEVEL3,
                other => panic!("unknown modifier {other}"),
            };
        }
        let found = self.names.get(key).copied().or_else(|| {
            let mut cs = key.chars();
            let c = cs.next()?;
            cs.next().is_none().then(|| self.chars.get(&c).copied()).flatten()
        });
        match found {
            Some((code, extra)) => self.press(code, mods | extra),
            None => panic!("no key named {key}"),
        }
    }

    fn text(&self, text: &str) {
        for ch in text.chars() {
            if let Some((code, mods)) = self.chars.get(&ch) {
                self.press(*code, *mods);
                continue;
            }
            // Not on a key directly: try dead key + base letter.
            let mut done = false;
            for mark in ['\u{301}', '\u{303}', '\u{302}', '\u{300}', '\u{308}'] {
                let dead = match mark {
                    '\u{301}' => "dead_acute",
                    '\u{303}' => "dead_tilde",
                    '\u{302}' => "dead_circumflex",
                    '\u{300}' => "dead_grave",
                    _ => "dead_diaeresis",
                };
                let Some(base) = strip_accent(ch, mark) else { continue };
                if let (Some(d), Some(b)) = (self.names.get(dead), self.chars.get(&base)) {
                    self.press(d.0, d.1);
                    self.press(b.0, b.1);
                    done = true;
                    break;
                }
            }
            if !done {
                panic!("cannot type {ch:?} on this layout");
            }
        }
    }
}

fn strip_accent(c: char, mark: char) -> Option<char> {
    let table: &[(char, char, char)] = &[
        ('á', 'a', '\u{301}'), ('é', 'e', '\u{301}'), ('í', 'i', '\u{301}'), ('ó', 'o', '\u{301}'), ('ú', 'u', '\u{301}'),
        ('Á', 'A', '\u{301}'), ('É', 'E', '\u{301}'), ('ã', 'a', '\u{303}'), ('õ', 'o', '\u{303}'), ('Ã', 'A', '\u{303}'),
        ('â', 'a', '\u{302}'), ('ê', 'e', '\u{302}'), ('ô', 'o', '\u{302}'), ('à', 'a', '\u{300}'), ('ü', 'u', '\u{308}'),
    ];
    table.iter().find(|(x, _, m)| *x == c && *m == mark).map(|(_, b, _)| *b)
}

fn main() {
    let layout = std::env::args().nth(1).unwrap_or_else(|| "us".to_string());
    let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    let keymap = xkb::Keymap::new_from_names(&ctx, "", "pc105", &layout, "", None, xkb::KEYMAP_COMPILE_NO_FLAGS).expect("keymap");
    let text = keymap.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1);

    // What each key gives at each level.
    let mut chars = HashMap::new();
    let mut names = HashMap::new();
    let shift_i = keymap.mod_get_index(xkb::MOD_NAME_SHIFT);
    let l3_i = keymap.mod_get_index("Mod5");
    for code in 8u32..256 {
        let kc = xkb::Keycode::new(code);
        for (mods, mask) in [(0u32, 0u32), (1 << shift_i, SHIFT), (1 << l3_i, LEVEL3)] {
            let mut state = xkb::State::new(&keymap);
            state.update_mask(mods, 0, 0, 0, 0, 0);
            let sym = state.key_get_one_sym(kc);
            if sym.raw() == 0 {
                continue;
            }
            let name = xkb::keysym_get_name(sym);
            // A key that gives it with no modifier beats one that needs Shift or AltGr.
            let e = names.entry(name).or_insert((code - 8, mask));
            if e.1 != 0 && mask == 0 {
                *e = (code - 8, mask);
            }
            let s = state.key_get_utf8(kc);
            let mut it = s.chars();
            if let (Some(c), None) = (it.next(), it.next())
                && !c.is_control()
            {
                let e = chars.entry(c).or_insert((code - 8, mask));
                if e.1 != 0 && mask == 0 {
                    *e = (code - 8, mask);
                }
            }
        }
    }

    let conn = Connection::connect_to_env().expect("wayland");
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut app = App::default();
    queue.roundtrip(&mut app).expect("roundtrip");
    let seat = app.seat.clone().expect("a seat");
    let manager = app.manager.clone().expect("zwp_virtual_keyboard_manager_v1 is not offered");
    let kb = manager.create_virtual_keyboard(&seat, &qh, ());

    let fd = memfd_create("keymap", MemfdFlags::CLOEXEC).expect("memfd");
    let bytes = text.as_bytes();
    ftruncate(&fd, bytes.len() as u64 + 1).expect("truncate");
    {
        let mut f = std::fs::File::from(fd.try_clone().expect("dup"));
        f.write_all(bytes).expect("write");
        f.write_all(&[0]).expect("write");
    }
    kb.keymap(1, fd.as_fd(), bytes.len() as u32 + 1);
    queue.roundtrip(&mut app).expect("roundtrip");

    let t = Typer { kb: &kb, conn: &conn, start: Instant::now(), chars, names };
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        let line = line.trim_start();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        match word {
            "wait" => std::thread::sleep(Duration::from_millis(rest.trim().parse().unwrap_or(100))),
            "key" => t.chord(rest.trim()),
            "text" => t.text(rest),
            other => panic!("unknown step {other}"),
        }
        t.flush();
    }
    let _ = queue.roundtrip(&mut app);
    std::thread::sleep(Duration::from_millis(100));
}
