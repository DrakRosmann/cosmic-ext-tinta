//! Background threads talk to the app through one channel.

use crate::app::Message;
use cosmic::iced::Subscription;
use cosmic::iced::futures::{SinkExt, StreamExt};
use std::sync::{Mutex, OnceLock};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

struct Bus {
    tx: UnboundedSender<Message>,
    rx: Mutex<Option<UnboundedReceiver<Message>>>,
}

fn bus() -> &'static Bus {
    static BUS: OnceLock<Bus> = OnceLock::new();
    BUS.get_or_init(|| {
        let (tx, rx) = unbounded_channel();
        Bus { tx, rx: Mutex::new(Some(rx)) }
    })
}

pub fn send(msg: Message) {
    let _ = bus().tx.send(msg);
}

pub fn subscription() -> Subscription<Message> {
    Subscription::run(|| {
        cosmic::iced::stream::channel(64, async |mut out: cosmic::iced::futures::channel::mpsc::Sender<Message>| {
            let Some(mut rx) = bus().rx.lock().unwrap().take() else {
                std::future::pending::<()>().await;
                return;
            };
            let mut stream = tokio_stream_like(&mut rx);
            while let Some(m) = stream.next().await {
                if out.send(m).await.is_err() {
                    break;
                }
            }
        })
    })
}

fn tokio_stream_like(rx: &mut UnboundedReceiver<Message>) -> impl cosmic::iced::futures::Stream<Item = Message> + '_ {
    cosmic::iced::futures::stream::poll_fn(move |cx| rx.poll_recv(cx))
}

/// Tests have no event loop: they empty the channel themselves.
#[cfg(test)]
pub fn drain() -> Vec<Message> {
    let mut guard = bus().rx.lock().unwrap();
    let Some(rx) = guard.as_mut() else { return Vec::new() };
    let mut out = Vec::new();
    while let Ok(m) = rx.try_recv() {
        out.push(m);
    }
    out
}
