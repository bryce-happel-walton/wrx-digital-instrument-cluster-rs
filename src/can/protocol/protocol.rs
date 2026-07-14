use super::super::can_backend::{CanFrame, TimestampedCanFrame};

use embedded_can::Id;

use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// We initiated this conversation
    Us,
    /// This is not our conversation, eavesdrop only
    External,
    /// Treated as external because we have not seen the request side
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConversationKey {
    pub request_id: u32,
    pub response_id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameDirection {
    Request,
    Response,
}

#[derive(Debug)]
pub struct MuxedMessage {
    pub protocol: &'static dyn CanProtocolDemuxer,
    pub conversation_key: ConversationKey,
    pub direction: FrameDirection,
    pub owner: Owner,
    pub data: Vec<u8>,
    pub request: Option<Vec<u8>>,
}

#[derive(Debug)]
pub enum RouterAction {
    None,
    Demux(MuxedMessage),
    Transmit(CanFrame),
    Ignored(IgnoreReason),
}

#[derive(Debug)]
pub enum IgnoreReason {
    UnknownProtocol,
    NotIsoTp,
    EmptyPayload,
    MalformedSingleFrame,
    MalformedFirstFrame,
    ConsecutiveWithoutFirst,
    SequenceMismatch { expected: usize, got: usize },
}

pub trait CanProtocol: Send + std::fmt::Debug {
    fn name(&self) -> &'static str;

    fn classify_id(&self, id: Id) -> Option<(FrameDirection, ConversationKey)>;

    fn note_outbound(&mut self, frame: &TimestampedCanFrame);

    fn ingest(
        &mut self,
        frame: &TimestampedCanFrame,
        direction: FrameDirection,
        conversation_key: ConversationKey,
        is_echo: bool,
    ) -> RouterAction;

    fn transmit(&mut self, id: Id, payload: &[u8], now: Instant) -> Option<Vec<CanFrame>> {
        let _ = (id, payload, now);
        None
    }

    fn due_transmissions(&mut self, now: Instant) -> Vec<CanFrame> {
        let _ = now;
        Vec::new()
    }

    fn last_external_activity(&self) -> Option<Instant> {
        None
    }

    fn cleanup_expired(&mut self, now: Instant);
}

pub trait CanProtocolDemuxer: Sync + std::fmt::Debug {
    fn dispatch_muxed_message(&self, message: &MuxedMessage);
}
