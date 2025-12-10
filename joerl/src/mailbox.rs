//! Mailbox implementation for actors.
//!
//! Each actor has a mailbox that stores incoming messages. Mailboxes are
//! bounded to provide backpressure and prevent memory exhaustion.

use crate::message::Envelope;
use crate::telemetry::MessageMetrics;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::mpsc;
pub const DEFAULT_MAILBOX_CAPACITY: usize = 100;

/// Actor mailbox for receiving messages.
///
/// Mailboxes are bounded channels that provide backpressure when full.
/// This prevents fast senders from overwhelming slow receivers.
///
/// The mailbox now supports selective receive (similar to Erlang's `receive`)
/// by maintaining a pending queue for messages that don't match the current
/// receive pattern.
pub struct Mailbox {
    rx: mpsc::Receiver<Envelope>,
    /// Messages that didn't match a selective receive predicate.
    /// These are checked first on subsequent receives.
    /// Wrapped in Mutex to maintain Sync for ActorContext.
    pending: Mutex<VecDeque<Envelope>>,
}

impl Mailbox {
    /// Creates a new mailbox (for testing).
    #[cfg(test)]
    pub(crate) fn new(capacity: usize) -> (Self, MailboxSender) {
        Self::new_with_type(capacity, "test".to_string())
    }

    /// Creates a new mailbox with actor type for telemetry.
    pub(crate) fn new_with_type(capacity: usize, actor_type: String) -> (Self, MailboxSender) {
        let (tx, rx) = mpsc::channel(capacity);
        let mailbox = Mailbox {
            rx,
            pending: Mutex::new(VecDeque::new()),
        };
        let sender = MailboxSender {
            tx,
            actor_type,
            capacity,
        };
        (mailbox, sender)
    }

    /// Receives the next message from the mailbox.
    ///
    /// Returns `None` if all senders have been dropped.
    ///
    /// This now checks the pending queue first before receiving from the channel.
    pub(crate) async fn recv(&mut self) -> Option<Envelope> {
        // Check pending queue first
        if let Some(envelope) = self.pending.lock().unwrap().pop_front() {
            return Some(envelope);
        }
        self.rx.recv().await
    }

    /// Tries to receive a message without blocking.
    ///
    /// Returns `Ok(envelope)` if a message is available, `Err(TryRecvError)` otherwise.
    #[allow(dead_code)]
    pub(crate) fn try_recv(&mut self) -> Result<Envelope, mpsc::error::TryRecvError> {
        self.rx.try_recv()
    }

    /// Closes the mailbox, preventing any further messages from being sent.
    pub fn close(&mut self) {
        self.rx.close();
    }

    /// Selectively receive a message matching the predicate.
    ///
    /// This is similar to Erlang's `receive` with pattern matching.
    /// Messages that don't match are saved in the pending queue and
    /// will be checked again on subsequent receives.
    ///
    /// # Arguments
    ///
    /// * `predicate` - Function that returns `Some(T)` if the message matches
    /// * `timeout` - Optional timeout duration
    ///
    /// # Returns
    ///
    /// * `Some(T)` - A matching message was found
    /// * `None` - Timeout expired or all senders dropped
    pub(crate) async fn recv_matching<F, T>(
        &mut self,
        mut predicate: F,
        timeout: Option<Duration>,
    ) -> Option<T>
    where
        F: FnMut(&crate::message::Message) -> Option<T>,
    {
        use crate::message::EnvelopeContent;

        // First check pending messages
        {
            let mut pending = self.pending.lock().unwrap();
            for i in 0..pending.len() {
                if let Some(envelope) = pending.get(i)
                    && let EnvelopeContent::Message(msg) = &envelope.content
                    && let Some(result) = predicate(msg)
                {
                    // Found a match - remove it and return
                    pending.remove(i);
                    return Some(result);
                }
            }
        }

        // Then check incoming messages
        let deadline = timeout.map(|d| tokio::time::Instant::now() + d);

        loop {
            let envelope = if let Some(deadline) = deadline {
                match tokio::time::timeout_at(deadline, self.rx.recv()).await {
                    Ok(Some(env)) => env,
                    Ok(None) => return None, // Channel closed
                    Err(_) => return None,   // Timeout
                }
            } else {
                self.rx.recv().await?
            };

            match envelope.content {
                EnvelopeContent::Message(ref msg) => {
                    if let Some(result) = predicate(msg) {
                        // Found a match
                        return Some(result);
                    } else {
                        // Doesn't match - save for later
                        self.pending.lock().unwrap().push_back(envelope);
                    }
                }
                EnvelopeContent::Signal(_) => {
                    // Signals always go to pending - they should be processed by handle_signal
                    self.pending.lock().unwrap().push_back(envelope);
                }
            }
        }
    }

    /// Try to receive a matching message without blocking.
    ///
    /// Only checks the pending queue and tries one receive from the channel.
    pub(crate) fn try_recv_matching<F, T>(&mut self, mut predicate: F) -> Option<T>
    where
        F: FnMut(&crate::message::Message) -> Option<T>,
    {
        use crate::message::EnvelopeContent;

        // Check pending messages first
        {
            let mut pending = self.pending.lock().unwrap();
            for i in 0..pending.len() {
                if let Some(envelope) = pending.get(i)
                    && let EnvelopeContent::Message(msg) = &envelope.content
                    && let Some(result) = predicate(msg)
                {
                    pending.remove(i);
                    return Some(result);
                }
            }
        }

        // Try one receive without blocking
        if let Ok(envelope) = self.rx.try_recv() {
            match envelope.content {
                EnvelopeContent::Message(ref msg) => {
                    if let Some(result) = predicate(msg) {
                        return Some(result);
                    } else {
                        self.pending.lock().unwrap().push_back(envelope);
                    }
                }
                EnvelopeContent::Signal(_) => {
                    self.pending.lock().unwrap().push_back(envelope);
                }
            }
        }

        None
    }
}

/// Handle for sending messages to an actor's mailbox.
#[derive(Clone)]
pub struct MailboxSender {
    tx: mpsc::Sender<Envelope>,
    actor_type: String,
    capacity: usize,
}

impl MailboxSender {
    /// Sends a message to the mailbox.
    ///
    /// Returns an error if the mailbox is full or closed.
    pub(crate) async fn send(
        &self,
        envelope: Envelope,
    ) -> Result<(), mpsc::error::SendError<Envelope>> {
        let result = self.tx.send(envelope).await;

        // Update mailbox depth gauge with actor type
        if result.is_ok() {
            MessageMetrics::mailbox_depth_typed(&self.actor_type, self.len(), self.capacity);
        }

        result
    }

    /// Tries to send a message without blocking.
    ///
    /// Returns an error if the mailbox is full or closed.
    pub(crate) fn try_send(
        &self,
        envelope: Envelope,
    ) -> Result<(), mpsc::error::TrySendError<Envelope>> {
        let result = self.tx.try_send(envelope);

        match &result {
            Ok(_) => {
                MessageMetrics::mailbox_depth_typed(&self.actor_type, self.len(), self.capacity)
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                MessageMetrics::mailbox_full_typed(&self.actor_type)
            }
            _ => {}
        }

        result
    }

    /// Returns true if the mailbox is closed.
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    /// Returns the current capacity of the mailbox.
    pub fn capacity(&self) -> usize {
        self.tx.capacity()
    }

    /// Returns the number of messages currently in the mailbox.
    pub fn len(&self) -> usize {
        self.tx.max_capacity() - self.tx.capacity()
    }

    /// Returns true if the mailbox is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Envelope, Signal};

    #[tokio::test]
    async fn test_mailbox_send_recv() {
        let (mut mailbox, sender) = Mailbox::new_with_type(10, "test".to_string());

        sender.send(Envelope::signal(Signal::Stop)).await.unwrap();

        let envelope = mailbox.recv().await;
        assert!(envelope.is_some());
    }

    #[tokio::test]
    async fn test_mailbox_try_recv() {
        let (mut mailbox, sender) = Mailbox::new_with_type(10, "test".to_string());

        // Should be empty initially
        assert!(mailbox.try_recv().is_err());

        sender.send(Envelope::signal(Signal::Stop)).await.unwrap();

        // Should have a message now
        let envelope = mailbox.try_recv();
        assert!(envelope.is_ok());
    }

    #[tokio::test]
    async fn test_mailbox_bounded() {
        let (mut mailbox, sender) = Mailbox::new_with_type(2, "test".to_string());

        // Fill the mailbox
        sender.send(Envelope::signal(Signal::Stop)).await.unwrap();
        sender.send(Envelope::signal(Signal::Stop)).await.unwrap();

        // Should fail due to capacity
        let result = sender.try_send(Envelope::signal(Signal::Stop));
        assert!(result.is_err());

        // Drain one message
        mailbox.recv().await;

        // Should succeed now
        let result = sender.try_send(Envelope::signal(Signal::Stop));
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mailbox_close() {
        let (mut mailbox, sender) = Mailbox::new_with_type(10, "test".to_string());

        mailbox.close();

        // Send should fail
        let result = sender.send(Envelope::signal(Signal::Stop)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mailbox_sender_drop() {
        let (mut mailbox, sender) = Mailbox::new_with_type(10, "test".to_string());

        drop(sender);

        // Recv should return None when all senders dropped
        let result = mailbox.recv().await;
        assert!(result.is_none());
    }

    #[test]
    fn test_mailbox_sender_status() {
        let (_mailbox, sender) = Mailbox::new_with_type(10, "test".to_string());

        assert!(!sender.is_closed());
        assert_eq!(sender.capacity(), 10);
        assert!(sender.is_empty());
    }

    #[tokio::test]
    async fn test_mailbox_sender_clone() {
        let (mut mailbox, sender) = Mailbox::new_with_type(10, "test".to_string());
        let sender2 = sender.clone();

        sender.send(Envelope::signal(Signal::Stop)).await.unwrap();
        sender2.send(Envelope::signal(Signal::Stop)).await.unwrap();

        assert!(mailbox.recv().await.is_some());
        assert!(mailbox.recv().await.is_some());
    }
}
