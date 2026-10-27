//! Fan-out of text frames to every subscribed WebSocket connection.

use std::sync::{mpsc, Arc, Mutex};

/// Delivers each published frame to every live subscriber, and remembers the
/// latest one for subscribers that join later.
///
/// Lock order, taken by both [`Hub::publish`] and [`Hub::subscribe`]:
/// `subscribers`, then `latest`. Registering a subscriber reads `latest` while
/// holding `subscribers`, so a concurrent `publish` cannot slip a frame past a
/// half-registered client - which would otherwise strand it on a stale frame
/// until the next publish.
pub struct Hub {
    queue_depth: usize,
    subscribers: Mutex<Vec<mpsc::SyncSender<Arc<String>>>>,
    latest: Mutex<Option<Arc<String>>>,
}

impl Hub {
    /// An empty hub whose subscribers may each fall `queue_depth` frames
    /// behind before they are dropped.
    pub fn new(queue_depth: usize) -> Self {
        Self {
            queue_depth,
            subscribers: Mutex::new(Vec::new()),
            latest: Mutex::new(None),
        }
    }

    /// Register a subscriber, its queue primed with the latest frame, if any.
    pub fn subscribe(&self) -> mpsc::Receiver<Arc<String>> {
        let (sender, receiver) = mpsc::sync_channel(self.queue_depth);
        let mut subscribers = self.subscribers.lock().expect("hub subscribers poisoned");
        if let Some(frame) = self.latest() {
            let _ = sender.try_send(frame);
        }
        subscribers.push(sender);
        receiver
    }

    /// Record `frame` as the latest and queue it for every subscriber,
    /// dropping each one whose queue is full or whose receiver is gone.
    pub fn publish(&self, frame: String) {
        let frame = Arc::new(frame);
        let mut subscribers = self.subscribers.lock().expect("hub subscribers poisoned");
        *self.latest.lock().expect("hub latest frame poisoned") = Some(Arc::clone(&frame));
        subscribers.retain(|sender| sender.try_send(Arc::clone(&frame)).is_ok());
    }

    /// The latest frame published, if any.
    pub fn latest(&self) -> Option<Arc<String>> {
        self.latest
            .lock()
            .expect("hub latest frame poisoned")
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::TryRecvError;

    use super::Hub;

    #[test]
    fn a_late_subscriber_is_primed_with_the_latest_frame() {
        let hub = Hub::new(2);
        assert!(hub.latest().is_none());
        hub.publish("first".to_owned());
        hub.publish("second".to_owned());
        let receiver = hub.subscribe();
        assert_eq!(
            receiver.try_recv().expect("primed frame").as_str(),
            "second"
        );
        assert_eq!(receiver.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn a_subscriber_whose_queue_is_full_is_dropped() {
        let hub = Hub::new(1);
        let receiver = hub.subscribe();
        hub.publish("kept".to_owned());
        hub.publish("overflow".to_owned());
        hub.publish("after".to_owned());
        assert_eq!(receiver.try_recv().expect("queued frame").as_str(), "kept");
        assert_eq!(receiver.try_recv(), Err(TryRecvError::Disconnected));
        assert_eq!(hub.latest().expect("latest").as_str(), "after");
    }
}
