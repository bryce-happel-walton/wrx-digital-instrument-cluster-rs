use super::can_backend::{CanFrame, TimestampedCanFrame};
use super::protocol::{CanProtocol, IgnoreReason, RouterAction};

use embedded_can::{Frame, Id};

use std::time::{Duration, Instant};

const ECHO_MATCH_WINDOW: Duration = Duration::from_millis(100);
const RECENTLY_SENT_LIMIT: usize = 64;

#[derive(Default)]
pub struct ProtocolRouter {
    protocols: Vec<Box<dyn CanProtocol>>,
    recently_sent: Vec<TimestampedCanFrame>,
}

impl ProtocolRouter {
    pub fn register(&mut self, protocol: impl CanProtocol + 'static) {
        self.protocols.push(Box::new(protocol));
    }

    pub fn note_outbound(&mut self, frame: &impl Frame) {
        let frame = TimestampedCanFrame::now(frame);

        if let Some(idx) = self
            .protocols
            .iter()
            .position(|protocol| protocol.classify_id(frame.id()).is_some())
        {
            if self.recently_sent.len() >= RECENTLY_SENT_LIMIT {
                self.recently_sent.remove(0);
            }
            self.recently_sent.push(frame);

            self.protocols[idx].note_outbound(&frame);
        }
    }

    pub fn ingest(&mut self, frame: &impl Frame) -> RouterAction {
        let frame = TimestampedCanFrame::now(frame);

        self.cleanup_expired(frame.instant);

        if frame.data().is_empty() {
            RouterAction::Ignored(IgnoreReason::EmptyPayload)
        } else {
            let is_echo = self.is_echo(&frame);

            self.protocols
                .iter_mut()
                .find_map(|protocol| {
                    protocol
                        .classify_id(frame.id())
                        .map(|(direction, key)| protocol.ingest(&frame, direction, key, is_echo))
                })
                .unwrap_or(RouterAction::Ignored(IgnoreReason::UnknownProtocol))
        }
    }

    pub fn transmit(&mut self, id: Id, payload: &[u8]) -> Vec<CanFrame> {
        let now = Instant::now();

        self.protocols
            .iter_mut()
            .find_map(|protocol| protocol.transmit(id, payload, now))
            .unwrap_or_default()
    }

    pub fn due_transmissions(&mut self, now: Instant) -> Vec<CanFrame> {
        self.protocols
            .iter_mut()
            .flat_map(|protocol| protocol.due_transmissions(now))
            .collect()
    }

    pub fn last_external_activity(&self) -> Option<Instant> {
        self.protocols
            .iter()
            .filter_map(|protocol| protocol.last_external_activity())
            .max()
    }

    fn is_echo(&self, frame: &TimestampedCanFrame) -> bool {
        self.recently_sent.iter().any(|recent| {
            recent.id() == frame.id()
                && recent.dlc() == frame.dlc()
                && recent.data() == frame.data()
                && frame.instant.saturating_duration_since(recent.instant) <= ECHO_MATCH_WINDOW
        })
    }

    fn cleanup_expired(&mut self, now: Instant) {
        self.recently_sent
            .retain(|recent| now.saturating_duration_since(recent.instant) <= ECHO_MATCH_WINDOW);

        for protocol in &mut self.protocols {
            protocol.cleanup_expired(now);
        }
    }
}
