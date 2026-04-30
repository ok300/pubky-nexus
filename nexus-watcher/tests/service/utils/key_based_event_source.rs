use std::collections::VecDeque;
use std::sync::Mutex;

use nexus_common::models::event::EventProcessorError;
use nexus_watcher::service::indexer::KeyBasedEventSource;
use pubky::{Event as StreamEvent, EventCursor, PublicKey};

#[derive(Default)]
pub struct MockKeyBasedEventSource {
    /// Event batches returned in fetch order.
    /// Useful when user ordering is not important and tests only care about processor flow.
    events: Mutex<VecDeque<Vec<StreamEvent>>>,

    /// User IDs requested from the mock, in fetch order.
    /// Useful for asserting the processor continued to, or stopped before, specific users.
    calls: Mutex<Vec<String>>,
}

impl MockKeyBasedEventSource {
    pub fn with_events(self, events: Vec<Vec<StreamEvent>>) -> Self {
        *self.events.lock().unwrap() = events.into();
        self
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl KeyBasedEventSource for MockKeyBasedEventSource {
    async fn fetch_events(
        &self,
        _hs_pk: &PublicKey,
        user_pk: &PublicKey,
        _cursor: EventCursor,
        _limit: u16,
    ) -> Result<Vec<StreamEvent>, EventProcessorError> {
        self.calls.lock().unwrap().push(user_pk.z32());

        Ok(self.events.lock().unwrap().pop_front().unwrap_or_default())
    }
}
