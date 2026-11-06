//! Fan-out of text frames to every subscribed WebSocket connection.

use std::sync::{mpsc, Arc, Mutex};

/// Delivers each published frame to every live subscriber.
///
/// `subscribers` is the hub's only lock. The caller serialises priming against
/// publishing: the dashboard's `Broadcaster` holds its `feed` lock across both,
/// so no frame can fall between a subscriber's primer and its registration.
pub struct Hub {
    queue_depth: usize,
    subscribers: Mutex<Vec<mpsc::SyncSender<Arc<String>>>>,
}

impl Hub {
    /// An empty hub whose subscribers may each fall `queue_depth` frames
    /// behind before they are dropped.
    pub fn new(queue_depth: usize) -> Self {
        Self {
            queue_depth,
            subscribers: Mutex::new(Vec::new()),
        }
    }

    /// Register a subscriber, its queue primed with `primer`, if any.
    pub fn subscribe(&self, primer: Option<Arc<String>>) -> mpsc::Receiver<Arc<String>> {
        let (sender, receiver) = mpsc::sync_channel(self.queue_depth);
        let mut subscribers = self.subscribers.lock().expect("hub subscribers poisoned");
        if let Some(frame) = primer {
            let _ = sender.try_send(frame);
        }
        subscribers.push(sender);
        receiver
    }

    /// Queue `frame` for every subscriber, dropping each one whose queue is
    /// full or whose receiver is gone.
    pub fn publish(&self, frame: Arc<String>) {
        let mut subscribers = self.subscribers.lock().expect("hub subscribers poisoned");
        subscribers.retain(|sender| sender.try_send(Arc::clone(&frame)).is_ok());
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::TryRecvError;
    use std::sync::Arc;

    use super::Hub;

    #[test]
    fn a_subscriber_is_primed_with_the_frame_it_is_given() {
        let hub = Hub::new(2);
        hub.publish(Arc::new("before".to_owned()));
        let receiver = hub.subscribe(Some(Arc::new("primer".to_owned())));
        assert_eq!(
            receiver.try_recv().expect("primed frame").as_str(),
            "primer"
        );
        assert_eq!(receiver.try_recv(), Err(TryRecvError::Empty));
        hub.publish(Arc::new("after".to_owned()));
        assert_eq!(receiver.try_recv().expect("live frame").as_str(), "after");
    }

    #[test]
    fn a_subscriber_whose_queue_is_full_is_dropped() {
        let hub = Hub::new(1);
        let receiver = hub.subscribe(None);
        hub.publish(Arc::new("kept".to_owned()));
        hub.publish(Arc::new("overflow".to_owned()));
        hub.publish(Arc::new("after".to_owned()));
        assert_eq!(receiver.try_recv().expect("queued frame").as_str(), "kept");
        assert_eq!(receiver.try_recv(), Err(TryRecvError::Disconnected));
    }
}
