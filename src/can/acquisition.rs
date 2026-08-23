use crate::can::protocol::isotp::ISOTPService;

use super::can_backend::CanFrame;
use super::protocol::isotp::SERVICE_OFFSET;
use super::protocol::{FrameDirection, MuxedMessage};

use embedded_can::Id;

use std::collections::BTreeSet;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const DEFAULT_EAVESDROP_GRACE: Duration = Duration::from_millis(500);
const DEFAULT_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const DEFAULT_MAX_ATTEMPTS: usize = 5;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const PRUNE_INTERVAL: Duration = Duration::from_millis(100);

pub(crate) static ACQUISITIONS: AcquisitionQueue = AcquisitionQueue::new();

crate::parameter_struct! {static ACQUISITION_PROGRESS = AcquisitionProgress {
    [ro] queued: i32,
    [ro] completed: i32,
    [ro] skipped: i32,
    [ro] failed: i32,
    [ro] total: i32,
    [ro] complete: bool = false,
    [ro] current: String = String::new(),
}}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcquisitionGate {
    Ready,
    Blocked,
    Unreachable,
}

pub struct Acquisition {
    pub label: String,
    request_id: Id,
    request: Vec<u8>,
    matcher: Box<dyn Fn(&MuxedMessage) -> bool + Send + 'static>,
    gate: Option<Box<dyn Fn() -> AcquisitionGate + Send + 'static>>,
    on_abandon: Option<Box<dyn FnOnce() + Send + 'static>>,
    eavesdrop_grace: Duration,
    retry_interval: Duration,
    max_attempts: usize,
    timeout: Duration,
    attempts: usize,
    created_at: Instant,
    ready_since: Option<Instant>,
    last_attempt: Option<Instant>,
}

impl Acquisition {
    pub fn new(
        label: impl Into<String>,
        request_id: Id,
        request: impl Into<Vec<u8>>,
        matcher: impl Fn(&MuxedMessage) -> bool + Send + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            request_id,
            request: request.into(),
            matcher: Box::new(matcher),
            gate: None,
            on_abandon: None,
            eavesdrop_grace: DEFAULT_EAVESDROP_GRACE,
            retry_interval: DEFAULT_RETRY_INTERVAL,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            timeout: DEFAULT_TIMEOUT,
            attempts: 0,
            created_at: Instant::now(),
            ready_since: None,
            last_attempt: None,
        }
    }

    pub fn with_gate(mut self, gate: impl Fn() -> AcquisitionGate + Send + 'static) -> Self {
        self.gate = Some(Box::new(gate));
        self
    }

    pub fn on_abandon(mut self, callback: impl FnOnce() + Send + 'static) -> Self {
        self.on_abandon = Some(Box::new(callback));
        self
    }

    pub fn matches(&self, message: &MuxedMessage) -> bool {
        (self.matcher)(message)
    }

    pub fn gate(&self) -> AcquisitionGate {
        match &self.gate {
            Some(gate) => gate(),
            None => AcquisitionGate::Ready,
        }
    }

    fn poll_gate(&mut self, now: Instant) -> AcquisitionGate {
        let gate = self.gate();

        if (gate == AcquisitionGate::Ready) && self.ready_since.is_none() {
            self.ready_since = Some(now);
        }

        gate
    }

    fn expired(&self, now: Instant) -> bool {
        let attempts_spent = self.attempts >= self.max_attempts
            && self
                .last_attempt
                .is_some_and(|last| now.saturating_duration_since(last) >= self.retry_interval);

        let timed_out = self
            .ready_since
            .is_some_and(|since| now.saturating_duration_since(since) >= self.timeout);

        attempts_spent || timed_out
    }

    fn was_sent(&self) -> bool {
        self.attempts > 0
    }

    fn take_abandon(&mut self) -> Option<Box<dyn FnOnce() + Send + 'static>> {
        self.on_abandon.take()
    }

    pub fn due_request(&mut self, now: Instant) -> Option<(Id, Vec<u8>)> {
        let ready = self.poll_gate(now) == AcquisitionGate::Ready;

        let eavesdropping = now.saturating_duration_since(self.created_at) < self.eavesdrop_grace;

        let due = match self.last_attempt {
            None => true,
            Some(t) => now.saturating_duration_since(t) >= self.retry_interval,
        };

        if ready && !eavesdropping && due {
            self.attempts += 1;
            self.last_attempt = Some(now);
            Some((self.request_id, self.request.clone()))
        } else {
            None
        }
    }

    pub fn from_isotp<P: Copy + Into<u8>>(
        request_id: Id,
        service: &dyn ISOTPService,
        pids: &[P],
    ) -> Vec<Self> {
        const PIDS_PER_REQUEST: usize = CanFrame::MAX_FRAME_SIZE - 2;

        let service_byte = service.service_byte();
        let response_service = service_byte + SERVICE_OFFSET;

        let payloads: Vec<Vec<u8>> = if pids.is_empty() {
            vec![vec![service_byte]]
        } else {
            pids.chunks(PIDS_PER_REQUEST)
                .map(|chunk| {
                    let mut payload = vec![service_byte];
                    payload.extend(chunk.iter().map(|&pid| pid.into()));
                    payload
                })
                .collect()
        };

        payloads
            .into_iter()
            .map(|payload| {
                let label = format!("ISOTP {service:?} {:#02X?}", &payload[1..]);
                let request = payload.clone();

                Self::new(label, request_id, request, move |message| {
                    (message.direction == FrameDirection::Response)
                        && (message.data.first() == Some(&response_service))
                        && (message.request.as_deref() == Some(payload.as_slice()))
                })
            })
            .collect()
    }
}

pub struct AcquisitionQueue {
    inner: Mutex<Queue>,
}

struct Queue {
    pending: Vec<Acquisition>,
    seen: BTreeSet<String>,
    completed: usize,
    skipped: usize,
    failed: usize,
    announced: bool,
    last_prune: Option<Instant>,
    on_complete: Option<Box<dyn FnOnce() + Send>>,
}

impl AcquisitionQueue {
    const fn new() -> Self {
        Self {
            inner: Mutex::new(Queue::new()),
        }
    }

    pub fn extend(&self, acquisitions: impl IntoIterator<Item = Acquisition>) {
        {
            let mut queue = self.inner.lock().unwrap();

            for acquisition in acquisitions {
                if queue.seen.insert(acquisition.label.clone()) {
                    queue.pending.push(acquisition);
                }
            }
        }

        self.settle();
    }

    #[allow(unused)]
    pub fn push(&self, acquisition: Acquisition) {
        self.extend([acquisition]);
    }

    pub fn on_complete(&self, callback: impl FnOnce() + Send + 'static) {
        self.inner.lock().unwrap().on_complete = Some(Box::new(callback));
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().pending.is_empty()
    }

    pub fn resolve(&self, message: &MuxedMessage) -> Vec<String> {
        let acquired = {
            let mut queue = self.inner.lock().unwrap();
            let mut acquired = Vec::new();

            queue.pending.retain_mut(|acquisition| {
                if acquisition.matches(message) {
                    acquired.push(std::mem::take(&mut acquisition.label));
                    false
                } else {
                    true
                }
            });

            queue.completed += acquired.len();
            acquired
        };

        if !acquired.is_empty() {
            self.settle();
        }

        acquired
    }

    pub fn prune(&self, now: Instant) {
        let mut abandoned = Vec::new();
        let mut retired = 0;

        {
            let mut queue = self.inner.lock().unwrap();

            let due = !queue.pending.is_empty()
                && queue
                    .last_prune
                    .is_none_or(|last| now.saturating_duration_since(last) >= PRUNE_INTERVAL);

            if due {
                queue.last_prune = Some(now);

                let (mut skipped, mut failed) = (0, 0);

                queue.pending.retain_mut(|acquisition| {
                    let drop = if acquisition.poll_gate(now) == AcquisitionGate::Unreachable {
                        skipped += 1;
                        println!(
                            "[acquisition] skipping '{}': unsupported by the vehicle",
                            acquisition.label
                        );
                        true
                    } else if acquisition.expired(now) {
                        failed += 1;

                        if acquisition.was_sent() {
                            println!(
                                "[acquisition] giving up on '{}' after {} unanswered attempts",
                                acquisition.label, acquisition.attempts
                            );
                        } else {
                            println!(
                                "[acquisition] giving up on '{}': the bus never came free to ask on",
                                acquisition.label
                            );
                        }

                        true
                    } else {
                        false
                    };

                    if drop {
                        abandoned.extend(acquisition.take_abandon());
                    }

                    !drop
                });

                queue.skipped += skipped;
                queue.failed += failed;
                retired = skipped + failed;
            }
        }

        for callback in abandoned {
            callback();
        }

        if retired > 0 {
            self.settle();
        }
    }

    pub fn due_request(&self, now: Instant) -> Option<(Id, Vec<u8>)> {
        let due = {
            let mut queue = self.inner.lock().unwrap();

            queue.pending.iter_mut().find_map(|acquisition| {
                acquisition
                    .due_request(now)
                    .map(|request| (acquisition.label.clone(), request))
            })
        };

        due.map(|(label, request)| {
            ACQUISITION_PROGRESS.current.set_value(label);
            request
        })
    }

    fn settle(&self) {
        let callback = {
            let mut queue = self.inner.lock().unwrap();

            let queued = queue.pending.len();
            let finished = queue.completed + queue.skipped + queue.failed;
            let complete = (queued == 0) && (finished > 0);

            ACQUISITION_PROGRESS.queued.set_value(queued as i32);
            ACQUISITION_PROGRESS
                .completed
                .set_value(queue.completed as i32);
            ACQUISITION_PROGRESS.skipped.set_value(queue.skipped as i32);
            ACQUISITION_PROGRESS.failed.set_value(queue.failed as i32);
            ACQUISITION_PROGRESS
                .total
                .set_value((queued + finished) as i32);
            ACQUISITION_PROGRESS.complete.set_value(complete);

            if queued == 0 {
                ACQUISITION_PROGRESS.current.set_value(String::new());
            }

            let callback = if complete && !queue.announced {
                queue.announced = true;
                println!(
                    "[acquisition] complete: {} acquired, {} unsupported, {} unanswered",
                    queue.completed, queue.skipped, queue.failed
                );
                queue.on_complete.take()
            } else {
                None
            };

            callback
        };

        if let Some(callback) = callback {
            callback();
        }
    }
}

impl Queue {
    const fn new() -> Self {
        Self {
            pending: Vec::new(),
            seen: BTreeSet::new(),
            completed: 0,
            skipped: 0,
            failed: 0,
            announced: false,
            last_prune: None,
            on_complete: None,
        }
    }
}
