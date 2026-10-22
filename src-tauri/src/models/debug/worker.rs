//! Bounded diagnostic delivery. Transport threads only try to enqueue.
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc::{self, SyncSender},
    Arc,
};

enum Message<T> {
    Item(T),
    Barrier(mpsc::Sender<()>),
}

pub(super) struct Worker<T> {
    sender: SyncSender<Message<T>>,
    dropped: Arc<AtomicU64>,
}

impl<T: Send + 'static> Worker<T> {
    pub fn new(
        capacity: usize,
        consume: impl Fn(T, u64) + Send + 'static,
    ) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let dropped = Arc::new(AtomicU64::new(0));
        let losses = Arc::clone(&dropped);
        std::thread::Builder::new()
            .name("ipc-debug-store".into())
            .spawn(move || {
                while let Ok(message) = receiver.recv() {
                    let frame = match message {
                        Message::Item(frame) => frame,
                        Message::Barrier(ack) => {
                            let _ = ack.send(());
                            continue;
                        }
                    };
                    let skipped = losses.swap(0, Ordering::Relaxed);
                    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        consume(frame, skipped)
                    }))
                    .is_err()
                    {
                        losses.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })?;
        Ok(Self { sender, dropped })
    }

    pub fn flush(&self) -> Result<(), ()> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let (sender, receiver) = mpsc::channel();
        let mut message = Message::Barrier(sender);
        loop {
            match self.sender.try_send(message) {
                Ok(()) => break,
                Err(mpsc::TrySendError::Full(waiting)) => {
                    if std::time::Instant::now() >= deadline {
                        return Err(());
                    }
                    message = waiting;
                    std::thread::park_timeout(std::time::Duration::from_millis(5));
                }
                Err(mpsc::TrySendError::Disconnected(_)) => return Err(()),
            }
        }
        receiver
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .map_err(|_| ())
    }
    pub fn submit(&self, item: T) -> Result<(), T> {
        self.sender.try_send(Message::Item(item)).map_err(|error| {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            match error {
                mpsc::TrySendError::Full(Message::Item(item))
                | mpsc::TrySendError::Disconnected(Message::Item(item)) => item,
                _ => unreachable!("submit sends only items"),
            }
        })
    }
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
