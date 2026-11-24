//! [`Hub`]: the current snapshot, its sequence number and the subscriber
//! queues.
//!
//! Every [`publish`](Hub::publish) applies a delta to the snapshot, bumps
//! the sequence number by one and appends `(sequence, delta)` to every
//! subscriber's bounded queue, all under one lock, so a subscriber sees the
//! snapshot it started from followed by every later delta in order. A
//! subscriber whose queue is full loses its queued deltas and is marked
//! lagged; its connection then sends a fresh snapshot
//! ([`resync`](Hub::resync)) instead of a gap. Memory per subscriber is
//! bounded by its queue capacity.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use crate::snapshot::{CoreSnapshot, Delta};

/// The default per-subscriber queue capacity.
pub const DEFAULT_QUEUE_CAPACITY: usize = 256;

#[derive(Debug)]
struct HubState {
    snapshot: CoreSnapshot,
    sequence: u64,
    subscribers: Vec<Arc<Queue>>,
    closed: Option<String>,
}

/// The shared state clients read and subscribe to.
#[derive(Debug)]
pub struct Hub {
    state: Mutex<HubState>,
}

#[derive(Debug, Default)]
struct QueueState {
    deltas: VecDeque<(u64, Delta)>,
    lagged: bool,
    closed: Option<String>,
    detached: bool,
}

#[derive(Debug)]
struct Queue {
    capacity: usize,
    state: Mutex<QueueState>,
    ready: Condvar,
}

impl Queue {
    fn lock(&self) -> MutexGuard<'_, QueueState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// What a subscriber gets next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next {
    /// The next delta, in order.
    Delta(u64, Delta),
    /// Deltas were dropped because the queue was full; call
    /// [`Hub::resync`] for a fresh snapshot.
    Lagged,
    /// Nothing happened within the timeout.
    Idle,
    /// The hub closed and every queued delta has been taken.
    Closed(String),
}

/// One subscriber's queue. Dropping it detaches it from the hub.
#[derive(Debug)]
pub struct Subscriber {
    queue: Arc<Queue>,
}

impl Subscriber {
    /// Wait up to `timeout` for the next event.
    pub fn next(&self, timeout: Duration) -> Next {
        let guard = self.queue.lock();
        let (mut guard, _) = self
            .queue
            .ready
            .wait_timeout_while(guard, timeout, |state| {
                state.deltas.is_empty() && !state.lagged && state.closed.is_none()
            })
            .unwrap_or_else(PoisonError::into_inner);
        if guard.lagged {
            return Next::Lagged;
        }
        if let Some((sequence, delta)) = guard.deltas.pop_front() {
            return Next::Delta(sequence, delta);
        }
        match &guard.closed {
            Some(reason) => Next::Closed(reason.clone()),
            None => Next::Idle,
        }
    }
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        let mut state = self.queue.lock();
        state.detached = true;
        state.deltas.clear();
    }
}

impl Hub {
    /// A hub starting from `snapshot` at sequence 0.
    pub fn new(snapshot: CoreSnapshot) -> Self {
        Self {
            state: Mutex::new(HubState {
                snapshot,
                sequence: 0,
                subscribers: Vec::new(),
                closed: None,
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, HubState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Apply `delta`, give it the next sequence number and queue it for
    /// every subscriber. Returns the sequence number. After
    /// [`close`](Self::close) the snapshot still changes but nothing is
    /// queued.
    pub fn publish(&self, delta: Delta) -> u64 {
        let mut hub = self.lock();
        hub.snapshot.apply(&delta);
        hub.sequence += 1;
        let sequence = hub.sequence;
        hub.subscribers.retain(|queue| {
            let mut state = queue.lock();
            if state.detached {
                return false;
            }
            if state.closed.is_some() {
                return true;
            }
            if state.deltas.len() >= queue.capacity {
                state.deltas.clear();
                state.lagged = true;
            } else if !state.lagged {
                state.deltas.push_back((sequence, delta.clone()));
            }
            queue.ready.notify_all();
            true
        });
        sequence
    }

    /// The current sequence number and snapshot.
    pub fn snapshot(&self) -> (u64, CoreSnapshot) {
        let hub = self.lock();
        (hub.sequence, hub.snapshot.clone())
    }

    /// Why the hub closed, if it has.
    pub fn closed(&self) -> Option<String> {
        self.lock().closed.clone()
    }

    /// Register a subscriber with a queue of `capacity` deltas (at least
    /// one) and return it with the snapshot and sequence number it starts
    /// from. `None` once the hub is closed.
    pub fn subscribe(&self, capacity: usize) -> Option<(Subscriber, u64, CoreSnapshot)> {
        let mut hub = self.lock();
        if hub.closed.is_some() {
            return None;
        }
        let queue = Arc::new(Queue {
            capacity: capacity.max(1),
            state: Mutex::new(QueueState::default()),
            ready: Condvar::new(),
        });
        hub.subscribers.push(Arc::clone(&queue));
        Some((Subscriber { queue }, hub.sequence, hub.snapshot.clone()))
    }

    /// Clear a lagged subscriber's queue and return the snapshot it resumes
    /// from; later deltas follow it in order.
    pub fn resync(&self, subscriber: &Subscriber) -> (u64, CoreSnapshot) {
        let hub = self.lock();
        let mut state = subscriber.queue.lock();
        state.deltas.clear();
        state.lagged = false;
        (hub.sequence, hub.snapshot.clone())
    }

    /// The number of live subscribers.
    pub fn subscriber_count(&self) -> usize {
        let mut hub = self.lock();
        hub.subscribers.retain(|queue| !queue.lock().detached);
        hub.subscribers.len()
    }

    /// End every subscription: each subscriber takes its remaining deltas,
    /// then [`Next::Closed`]. Later [`subscribe`](Self::subscribe) calls
    /// return `None`.
    pub fn close(&self, reason: impl Into<String>) {
        let reason = reason.into();
        let mut hub = self.lock();
        if hub.closed.is_some() {
            return;
        }
        hub.closed = Some(reason.clone());
        for queue in &hub.subscribers {
            let mut state = queue.lock();
            state.closed = Some(reason.clone());
            queue.ready.notify_all();
        }
    }
}
