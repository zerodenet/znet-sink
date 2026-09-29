//! Bounded diagnostic delivery. Transport threads only try to enqueue.
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc::{self, SyncSender},
    Arc,
};

pub(super) struct Worker<T> {
    sender: SyncSender<T>,
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
                while let Ok(frame) = receiver.recv() {
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

    pub fn submit(&self, item: T) -> Result<(), T> {
        self.sender.try_send(item).map_err(|error| {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            match error {
                mpsc::TrySendError::Full(item) | mpsc::TrySendError::Disconnected(item) => item,
            }
        })
    }
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
