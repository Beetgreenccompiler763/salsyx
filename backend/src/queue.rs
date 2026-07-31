//! Event queue for the future crawler workers.
//!
//! Today this is an in-memory `async-channel` (works fine for a single
//! process). The crawler is designed to be its own service, so the queue
//! abstracts the transport: swapping in Redis streams or Postgres
//! LISTEN/NOTIFY later only changes this module.

use async_channel::{Receiver, Sender};
use salsyx_shared::events::Event;

/// Thread-safe wrapper around the event channel.
#[derive(Clone)]
pub struct EventQueue {
    sender: Sender<Event>,
    receiver: Receiver<Event>,
}

impl EventQueue {
    pub fn new(capacity: usize) -> Self {
        let (sender, receiver) = async_channel::bounded(capacity);
        Self { sender, receiver }
    }

    /// Enqueue an event. Bounded — if the queue is full the caller waits.
    pub async fn send(&self, event: Event) -> anyhow::Result<()> {
        self.sender.send(event).await?;
        Ok(())
    }

    /// Try to enqueue without blocking.
    pub fn try_send(&self, event: Event) -> anyhow::Result<()> {
        self.sender.try_send(event)?;
        Ok(())
    }

    /// Receive events from the channel (used by worker processes).
    pub fn receiver(&self) -> Receiver<Event> {
        self.receiver.clone()
    }

    /// Estimated queue depth (for observability).
    pub fn len(&self) -> usize {
        self.sender.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sender.is_empty()
    }
}
