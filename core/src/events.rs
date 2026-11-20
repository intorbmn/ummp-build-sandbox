//! Infrastructure: event bus. Decouples pipeline steps and modules.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub trait EventHandler: Send + Sync {
    fn handle(&self, event: &Event);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: Uuid,
    pub kind: String,
    pub payload: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

pub struct EventBus {
    handlers: Mutex<Vec<Arc<dyn EventHandler>>>,
    queue: Mutex<VecDeque<Event>>,
}

impl EventBus {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            handlers: Mutex::new(Vec::new()),
            queue: Mutex::new(VecDeque::new()),
        })
    }

    pub fn subscribe(self: &Arc<Self>, handler: Arc<dyn EventHandler>) {
        self.handlers.lock().unwrap().push(handler);
    }

    pub fn publish(self: &Arc<Self>, kind: &str, payload: serde_json::Value) {
        let event = Event {
            id: Uuid::new_v4(),
            kind: kind.to_string(),
            payload,
            timestamp: chrono::Utc::now(),
        };
        for h in self.handlers.lock().unwrap().iter() {
            h.handle(&event);
        }
        self.queue.lock().unwrap().push_back(event);
    }

    pub fn drain(self: &Arc<Self>) -> Vec<Event> {
        let mut q = self.queue.lock().unwrap();
        let out: Vec<Event> = q.drain(..).collect();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingHandler(Arc<AtomicUsize>);

    impl EventHandler for CountingHandler {
        fn handle(&self, _event: &Event) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn test_publish_drain() {
        let bus = EventBus::new();
        bus.publish("step.start", serde_json::json!({"step": "import"}));
        bus.publish("step.done", serde_json::json!({"step": "import"}));

        let events = bus.drain();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, "step.start");
        assert_eq!(events[1].kind, "step.done");
        assert_ne!(events[0].id, events[1].id);
    }

    #[test]
    fn test_subscribe_handler() {
        let bus = EventBus::new();
        let count = Arc::new(AtomicUsize::new(0));
        bus.subscribe(Arc::new(CountingHandler(count.clone())));

        bus.publish("step.start", serde_json::json!({}));
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
}
