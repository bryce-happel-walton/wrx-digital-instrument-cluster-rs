use super::acquisition::ACQUISITIONS;
use super::can_backend::{CanBackend, CanFrame};
use super::messages::wrx_2018::CanError;
use super::protocol::isotp::ISOTPProtocol;
use super::protocol::{CanProtocol, IgnoreReason, MuxedMessage, RouterAction};
use super::router::ProtocolRouter;
use crate::data::car::wrx_2018::CAN_DATA;

use embedded_can::{Frame, Id};

use std::error::Error;
use std::time::{Duration, Instant};

const EXTERNAL_PEER_ACTIVE_WINDOW: Duration = Duration::from_secs(3);
const MAX_ACQUISITION_DEFERRAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy)]
enum PeerDeferral {
    Idle,
    Waiting { since: Instant },
    Overriding,
}

pub struct PolledRequest {
    pub request_id: Id,
    pub payload: Vec<u8>,
    pub interval: Duration,
    pub last_sent: Option<Instant>,
    gate: Option<Box<dyn Fn() -> bool + Send>>,
}

impl PolledRequest {
    fn is_due(&self, now: Instant) -> bool {
        match self.last_sent {
            None => true,
            Some(t) => now.saturating_duration_since(t) >= self.interval,
        }
    }

    fn is_allowed(&self) -> bool {
        self.gate.as_ref().is_none_or(|gate| gate())
    }
}

pub struct CanBus {
    pub name: String,
    backend: CanBackend,
    router: ProtocolRouter,
    polled: Vec<PolledRequest>,
    next_poll_idx: usize,
    peer_deferral: PeerDeferral,
}

impl CanBus {
    pub fn new(name: impl Into<String>, backend: CanBackend) -> Self {
        let mut router = ProtocolRouter::default();
        router.register(ISOTPProtocol::default());

        Self {
            name: name.into(),
            backend,
            router,
            polled: Vec::default(),
            next_poll_idx: 0,
            peer_deferral: PeerDeferral::Idle,
        }
    }

    #[allow(unused)]
    pub fn poll(&mut self, request_id: Id, payload: impl Into<Vec<u8>>, interval: Duration) {
        self.poll_gated(request_id, payload, interval, || true);
    }

    pub fn poll_gated(
        &mut self,
        request_id: Id,
        payload: impl Into<Vec<u8>>,
        interval: Duration,
        gate: impl Fn() -> bool + Send + 'static,
    ) {
        let payload = payload.into();

        self.polled.push(PolledRequest {
            request_id,
            payload,
            interval,
            last_sent: None,
            gate: Some(Box::new(gate)),
        });
    }

    pub fn write_frame(&mut self, frame: impl Frame) -> Result<(), Box<dyn Error>> {
        let captured = CanFrame::from_frame(&frame);
        self.router.note_outbound(&captured);
        self.backend.write_frame(captured)
    }

    pub fn process_frame(&mut self) -> Option<CanFrame> {
        let received = self.backend.read_frame();

        if let Some(frame) = &received {
            self.process_inbound(frame);
        }

        self.dispatch_outbound();

        received
    }

    fn process_inbound(&mut self, frame: &impl Frame) {
        match CAN_DATA.parse_frame(frame) {
            Ok(_) => {
                crate::util::performance::record_can_frame_parsed();
            }
            Err(CanError::UnknownMessageId(_)) => {
                let action = self.router.ingest(frame);
                self.execute(action);
            }
            Err(_) => {
                crate::util::performance::record_can_frame_lost();
            }
        }
    }

    fn execute(&mut self, action: RouterAction) {
        match action {
            RouterAction::None => {}
            RouterAction::Demux(message) => {
                self.match_acquisitions(&message);
                message.protocol.dispatch_muxed_message(&message);
            }
            RouterAction::Transmit(frame) => {
                if let Err(e) = self.write_frame(frame) {
                    eprintln!("[bus:{}] failed to send protocol frame: {e:?}", self.name);
                }
            }
            RouterAction::Ignored(reason) => match reason {
                IgnoreReason::NotIsoTp
                | IgnoreReason::EmptyPayload
                | IgnoreReason::UnknownProtocol => {}
                other => {
                    eprintln!("[bus:{}] frame ignored: {other:?}", self.name);
                }
            },
        }
    }

    fn match_acquisitions(&self, message: &MuxedMessage) {
        for label in ACQUISITIONS.resolve(message) {
            println!(
                "[bus:{}] acquired '{label}' via {:?} on {:#03X}",
                self.name, message.owner, message.conversation_key.response_id
            );
        }
    }

    fn is_external_peer_active(&self, now: Instant) -> bool {
        self.router.last_external_activity().is_some_and(|instant| {
            now.saturating_duration_since(instant) <= EXTERNAL_PEER_ACTIVE_WINDOW
        })
    }

    fn may_acquire(&mut self, now: Instant) -> bool {
        if self.is_external_peer_active(now) {
            match self.peer_deferral {
                PeerDeferral::Overriding => true,
                PeerDeferral::Waiting { since }
                    if now.saturating_duration_since(since) >= MAX_ACQUISITION_DEFERRAL =>
                {
                    println!(
                        "[bus:{}] peer still holding the bus after {MAX_ACQUISITION_DEFERRAL:?}; acquiring alongside it",
                        self.name
                    );
                    self.peer_deferral = PeerDeferral::Overriding;
                    true
                }
                PeerDeferral::Waiting { .. } => false,
                PeerDeferral::Idle => {
                    self.peer_deferral = PeerDeferral::Waiting { since: now };
                    false
                }
            }
        } else {
            self.peer_deferral = PeerDeferral::Idle;
            true
        }
    }

    fn dispatch_outbound(&mut self) {
        let now = Instant::now();

        for frame in self.router.due_transmissions(now) {
            if let Err(e) = self.write_frame(frame) {
                eprintln!("[bus:{}] failed to write ISO-TP frame: {e:?}", self.name);
            }
        }

        ACQUISITIONS.prune(now);

        if !ACQUISITIONS.is_empty() {
            if self.may_acquire(now) {
                if let Some((request_id, payload)) = ACQUISITIONS.due_request(now) {
                    self.send_isotp(request_id, &payload);
                }
            }
        } else {
            let n = self.polled.len();

            for offset in 0..n {
                let idx = (self.next_poll_idx + offset) % n;

                if self.polled[idx].is_due(now) {
                    self.polled[idx].last_sent = Some(now);

                    if self.polled[idx].is_allowed() {
                        let request_id = self.polled[idx].request_id;
                        let payload = self.polled[idx].payload.clone();

                        self.next_poll_idx = (idx + 1) % n;
                        self.send_isotp(request_id, &payload);
                        break;
                    }
                }
            }
        }
    }

    fn send_isotp(&mut self, id: Id, payload: &[u8]) {
        let frames = self.router.transmit(id, payload);

        if frames.is_empty() {
            eprintln!(
                "[bus:{}] no protocol accepted a payload of {} bytes for {id:?}",
                self.name,
                payload.len()
            );
        }

        for frame in frames {
            if let Err(e) = self.write_frame(frame) {
                eprintln!("[bus:{}] failed to write ISO-TP request: {e:?}", self.name);
            }
        }
    }
}
