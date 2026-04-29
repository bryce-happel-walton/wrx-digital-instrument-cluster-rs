use super::acquisition::Acquisition;
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

#[derive(Debug)]
pub struct PolledRequest {
    pub request_id: Id,
    pub payload: Vec<u8>,
    pub interval: Duration,
    pub last_sent: Option<Instant>,
}

impl PolledRequest {
    fn is_due(&self, now: Instant) -> bool {
        match self.last_sent {
            None => true,
            Some(t) => now.saturating_duration_since(t) >= self.interval,
        }
    }
}

pub struct CanBus {
    pub name: String,
    backend: CanBackend,
    router: ProtocolRouter,
    acquisitions: Vec<Acquisition>,
    on_acquisitions_complete: Option<Box<dyn FnOnce() + Send>>,
    polled: Vec<PolledRequest>,
    next_poll_idx: usize,
}

impl CanBus {
    pub fn new(name: impl Into<String>, backend: CanBackend) -> Self {
        let mut router = ProtocolRouter::default();
        router.register(ISOTPProtocol::default());

        Self {
            name: name.into(),
            backend,
            router,
            acquisitions: Vec::default(),
            on_acquisitions_complete: None,
            polled: Vec::default(),
            next_poll_idx: 0,
        }
    }

    pub fn register_protocol(&mut self, protocol: impl CanProtocol + 'static) {
        self.router.register(protocol);
    }

    pub fn acquire(&mut self, acquisition: Acquisition) {
        self.acquisitions.push(acquisition);
    }

    pub fn on_acquisitions_complete(&mut self, callback: impl FnOnce() + Send + 'static) {
        self.on_acquisitions_complete = Some(Box::new(callback));
    }

    pub fn pending_acquisitions(&self) -> usize {
        self.acquisitions.len()
    }

    pub fn poll(&mut self, request_id: Id, payload: impl Into<Vec<u8>>, interval: Duration) {
        let payload = payload.into();

        self.polled.push(PolledRequest {
            request_id,
            payload,
            interval,
            last_sent: None,
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

    fn match_acquisitions(&mut self, message: &MuxedMessage) {
        if !self.acquisitions.is_empty() {
            let bus_name = &self.name;
            self.acquisitions.retain(|acquisition| {
                if acquisition.matches(message) {
                    println!(
                        "[bus:{bus_name}] acquired '{}' via {:?} on 0x{:03X}",
                        acquisition.label, message.owner, message.conversation_key.response_id
                    );
                    false
                } else {
                    true
                }
            });

            if self.acquisitions.is_empty() {
                println!("[bus:{}] initial acquisitions complete", self.name);

                if let Some(callback) = self.on_acquisitions_complete.take() {
                    callback();
                }
            }
        }
    }

    fn is_external_peer_active(&self, now: Instant) -> bool {
        self.router.last_external_activity().is_some_and(|instant| {
            now.saturating_duration_since(instant) <= EXTERNAL_PEER_ACTIVE_WINDOW
        })
    }

    fn dispatch_outbound(&mut self) {
        let now = Instant::now();

        for frame in self.router.due_transmissions(now) {
            if let Err(e) = self.write_frame(frame) {
                eprintln!("[bus:{}] failed to write ISO-TP frame: {e:?}", self.name);
            }
        }

        if !self.acquisitions.is_empty() {
            if !self.is_external_peer_active(now) {
                let due = self
                    .acquisitions
                    .iter_mut()
                    .find_map(|acquisition| acquisition.due_request(now));

                if let Some((request_id, payload)) = due {
                    self.send_isotp(request_id, &payload);
                }
            }
        } else {
            let n = self.polled.len();

            for offset in 0..n {
                let idx = (self.next_poll_idx + offset) % n;

                if self.polled[idx].is_due(now) {
                    let request_id = self.polled[idx].request_id;
                    let payload = self.polled[idx].payload.clone();

                    self.polled[idx].last_sent = Some(now);
                    self.next_poll_idx = (idx + 1) % n;
                    self.send_isotp(request_id, &payload);
                    break;
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
