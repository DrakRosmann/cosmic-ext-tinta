//! One running editor per user and session: a second `tinta file` hands the file to the first and
//! leaves. Two processes would each overwrite the other's session and delete the other's backups.

use crate::app::Message;
use crate::bus;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

pub type Target = (PathBuf, Option<(usize, usize)>);

#[derive(Serialize, Deserialize)]
struct Request {
    targets: Vec<Target>,
}

pub enum Claim {
    /// Another instance took the request.
    Forwarded,
    /// This process is the instance now.
    Primary(UnixListener, PathBuf),
    /// No socket could be made: run on our own rather than not at all.
    Alone,
}

/// Per user, per display and per state folder: a nested test compositor gets its own instance.
pub fn socket_path() -> PathBuf {
    let mut h = DefaultHasher::new();
    tinta_core::settings::state_dir().hash(&mut h);
    std::env::var("WAYLAND_DISPLAY").unwrap_or_default().hash(&mut h);
    std::env::var("DISPLAY").unwrap_or_default().hash(&mut h);
    let dir = dirs::runtime_dir().unwrap_or_else(std::env::temp_dir);
    dir.join(format!("tinta-{:016x}.sock", h.finish()))
}

fn forward(path: &Path, targets: &[Target]) -> std::io::Result<()> {
    let mut s = UnixStream::connect(path)?;
    let msg = serde_json::to_string(&Request { targets: targets.to_vec() })?;
    s.write_all(msg.as_bytes())?;
    s.write_all(b"\n")?;
    s.flush()
}

/// Forwards to a running instance, or becomes it.
pub fn claim(path: &Path, targets: &[Target]) -> Claim {
    // Paths are relative to where the command ran, which the running instance doesn't know.
    let absolute: Vec<Target> = targets
        .iter()
        .map(|(p, at)| {
            let abs = if p.is_absolute() { p.clone() } else { std::env::current_dir().unwrap_or_default().join(p) };
            (abs, *at)
        })
        .collect();
    // Twice: if someone else binds between our failed connect and our bind, the retry reaches them.
    for _ in 0..2 {
        if forward(path, &absolute).is_ok() {
            return Claim::Forwarded;
        }
        // Nobody answers, so any file there is left over from a crash.
        let _ = std::fs::remove_file(path);
        if let Ok(l) = UnixListener::bind(path) {
            return Claim::Primary(l, path.to_path_buf());
        }
    }
    Claim::Alone
}

/// Reads requests on a thread and hands them to `sink`.
pub fn serve(listener: UnixListener, sink: impl Fn(Vec<Target>) + Send + 'static) {
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut line = String::new();
            if BufReader::new(stream).read_line(&mut line).is_ok()
                && let Ok(req) = serde_json::from_str::<Request>(&line)
            {
                sink(req.targets);
            }
        }
    });
}

pub fn serve_app(listener: UnixListener) {
    serve(listener, |targets| bus::send(Message::RemoteOpen(targets)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    #[test]
    fn the_second_process_forwards_to_the_first() {
        let dir = std::env::temp_dir().join(format!("tinta-inst-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.sock");
        let Claim::Primary(listener, _) = claim(&path, &[]) else { panic!("first should be primary") };
        let (tx, rx) = channel();
        serve(listener, move |t| tx.send(t).unwrap());
        let targets = vec![(PathBuf::from("rel/a.rs"), Some((3, 4)))];
        assert!(matches!(claim(&path, &targets), Claim::Forwarded));
        let got = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(got[0].0.is_absolute() && got[0].0.ends_with("rel/a.rs"));
        assert_eq!(got[0].1, Some((3, 4)));
        // Opening with nothing still reaches it, to raise the window.
        assert!(matches!(claim(&path, &[]), Claim::Forwarded));
        assert!(rx.recv_timeout(Duration::from_secs(5)).unwrap().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_socket_left_by_a_crash_is_replaced() {
        let dir = std::env::temp_dir().join(format!("tinta-inst2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.sock");
        drop(UnixListener::bind(&path).unwrap()); // Leaves the file, with no one listening.
        assert!(path.exists());
        assert!(matches!(claim(&path, &[]), Claim::Primary(..)));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
