use super::parser::{obd2, ssm2, uds};
use super::subsystems;
use crate::can::can_backend::{CanFrame, TimestampedCanFrame};
use crate::can::protocol::{
    CanProtocol, CanProtocolDemuxer, ConversationKey, FrameDirection, IgnoreReason, MuxedMessage,
    Owner, RouterAction,
};
use crate::can::util::{make_id, raw_id};

use embedded_can::{Frame, Id};
use strum::FromRepr;

use std::collections::HashMap;
use std::time::{Duration, Instant};

subsystems![obd2::OBD2Subsystem, uds::UDSSubsystem, ssm2::SSM2Subsystem,];

pub const FUNCTIONAL_REQUEST_ID: u32 = 0x7DF;

pub const PHYSICAL_REQUEST_LO: u32 = 0x7E0;
pub const PHYSICAL_REQUEST_HI: u32 = 0x7E7;
pub const PHYSICAL_RESPONSE_LO: u32 = 0x7E8;
pub const PHYSICAL_RESPONSE_HI: u32 = 0x7EF;
pub const REQUEST_TO_RESPONSE_OFFSET: u32 = 0x8;

pub const SERVICE_OFFSET: u8 = 0x40;

/// Payload capacity of a FirstFrame (two PCI bytes)
pub const FIRST_FRAME_DATA_LEN: usize = CanFrame::MAX_FRAME_SIZE - 2;
/// Payload capacity of a ConsecutiveFrame (one PCI byte)
pub const CONSECUTIVE_FRAME_DATA_LEN: usize = CanFrame::MAX_FRAME_SIZE - 1;
/// A FirstFrame's length field is 12 bits
pub const MAX_MULTI_FRAME_PAYLOAD: usize = 0xFFF;

pub const CONVERSATION_TIMEOUT: Duration = Duration::from_secs(3);
const FUNCTIONAL_BROADCAST_TTL: Duration = Duration::from_millis(500);

/// ISO 15765-2 N_Bs: how long an outbound transfer waits on the receiver's
/// flow control before it is abandoned
const FLOW_CONTROL_TIMEOUT: Duration = Duration::from_secs(1);

pub const FUNCTIONAL_BROADCAST_KEY: ConversationKey = ConversationKey {
    request_id: FUNCTIONAL_REQUEST_ID,
    response_id: 0,
};

#[derive(Debug, Copy, Clone)]
pub enum ISOTPCanFrameType {
    SingleFrame { len: usize },
    FirstFrame { total_len: usize },
    ConsecutiveFrame { frame_index: usize },
    FlowControlFrame(ISOTPFlowControlFlag),
}

#[allow(unused)]
#[repr(u8)]
#[derive(Debug, Copy, Clone, FromRepr)]
pub enum ISOTPFlowControlFlag {
    Continue = 0x0,
    Wait = 0x1,
    Abort = 0x2,
}

pub enum ISOTPSeperationTime {
    /// milliseconds 0x00..=0x7F
    MS(u8),
    /// microseconds 0xf1..=0xf9
    US(u8),
}

impl ISOTPSeperationTime {
    pub fn from_byte(byte: u8) -> Self {
        match byte {
            0x00..=0x7F => Self::MS(byte),
            0xF1..=0xF9 => Self::US(byte),
            // reserved values: assume the slowest separation
            _ => Self::MS(0x7F),
        }
    }

    pub fn duration(&self) -> Duration {
        match self {
            Self::MS(ms) => Duration::from_millis(*ms as u64),
            Self::US(byte) => Duration::from_micros(((byte - 0xF0) as u64) * 100),
        }
    }
}

pub struct ISOTPCanFrame {
    pub frame_type: ISOTPCanFrameType,
    frame: CanFrame,
}

#[derive(Debug, Default)]
struct ISOTPReassembly {
    /// number of additional bytes required to complete the payload
    remaining: usize,
    /// expected sequence index of the next consecutive frame
    next_sequence: usize,
    /// payload accumulated so far
    data: Vec<u8>,
}

impl ISOTPReassembly {
    fn complete(data: Vec<u8>) -> Self {
        Self {
            remaining: 0,
            next_sequence: 0,
            data,
        }
    }

    fn payload(&self) -> Option<&[u8]> {
        (self.remaining == 0).then(|| self.data.as_slice())
    }
}

#[derive(Debug)]
struct Conversation {
    owner: Owner,
    last_activity: Instant,
    request: Option<ISOTPReassembly>,
    response: Option<ISOTPReassembly>,
}

impl Conversation {
    fn new(owner: Owner, last_activity: Instant) -> Self {
        Self {
            owner,
            last_activity,
            request: None,
            response: None,
        }
    }

    fn reassembly_mut(&mut self, direction: FrameDirection) -> &mut Option<ISOTPReassembly> {
        match direction {
            FrameDirection::Request => &mut self.request,
            FrameDirection::Response => &mut self.response,
        }
    }
}

#[derive(Debug)]
struct FunctionalBroadcast {
    owner: Owner,
    instant: Instant,
    request: Vec<u8>,
}

#[derive(Debug)]
struct OutboundTransfer {
    /// the id we transmit on
    request_id: Id,
    /// payload still to send after the FirstFrame
    payload: Vec<u8>,
    /// bytes of `payload` already sent
    offset: usize,
    /// sequence number of the next consecutive frame
    next_sequence: usize,
    /// consecutive frames allowed before the next flow control; `None` once
    /// the receiver requests the rest without further flow control (BS 0)
    block_remaining: Option<usize>,
    /// receiver-requested minimum gap between consecutive frames (STmin)
    separation: Duration,
    /// earliest instant the next consecutive frame may be sent
    next_send: Instant,
    /// waiting on a flow control frame before sending more
    awaiting_flow_control: bool,
    last_activity: Instant,
}

#[derive(Debug, Default)]
pub struct ISOTPProtocol {
    conversations: HashMap<ConversationKey, Conversation>,
    outbound: HashMap<ConversationKey, OutboundTransfer>,
    last_functional_broadcast: Option<FunctionalBroadcast>,
    last_external_request: Option<Instant>,
}

pub struct DemuxContext<'a> {
    pub data: &'a [u8],
    pub request: &'a [u8],
}

pub trait ISOTPSubsystem: Sync {
    // TRACKING: https://github.com/rust-lang/rust-project-goals/issues/106
    // const NAME: &'static str;

    #[allow(unused)]
    fn name(&self) -> &'static str;

    fn demux(&self, service: u8, context: &DemuxContext) -> bool;

    #[allow(unused)]
    fn services(&self) -> Vec<u8>;
}

pub trait ISOTPService: std::fmt::Debug {
    /// The request-side service byte; responses add [`SERVICE_OFFSET`].
    fn service_byte(&self) -> u8;

    fn demux(&self, context: &DemuxContext);
}

#[derive(Debug)]
pub struct ISOTPDemuxer;

impl CanProtocolDemuxer for ISOTPDemuxer {
    fn dispatch_muxed_message(&self, message: &MuxedMessage) {
        if message.direction == FrameDirection::Response
            && let Some(request) = message.request.as_deref()
            && let Some((&service, data)) = message.data.split_first()
        {
            let normalized = if service >= SERVICE_OFFSET {
                service - SERVICE_OFFSET
            } else {
                service
            };

            let context = DemuxContext { data, request };

            if !SUBSYSTEMS
                .iter()
                .any(|subsystem| subsystem.demux(normalized, &context))
            {
                println!("[ISOTP] Unknown service 0x{service:02X}; data: 0x{data:02X?}");
            }
        }
    }
}

impl ISOTPCanFrameType {
    pub const SINGLE_FRAME: u8 = 0x00;
    pub const FIRST_FRAME: u8 = 0x10;
    pub const CONSECUTIVE_FRAME: u8 = 0x20;
    pub const FLOW_CONTROL_FRAME: u8 = 0x30;

    pub fn to_protocol_payload(self) -> Vec<u8> {
        match self {
            Self::SingleFrame { len } => vec![Self::SINGLE_FRAME | ((len as u8) & 0x0F)],
            Self::FirstFrame { total_len } => vec![
                Self::FIRST_FRAME | ((total_len >> 8) as u8 & 0x0F),
                (total_len & 0xFF) as u8,
            ],
            Self::ConsecutiveFrame { frame_index } => {
                vec![Self::CONSECUTIVE_FRAME | (frame_index as u8)]
            }
            Self::FlowControlFrame(flag) => vec![Self::FLOW_CONTROL_FRAME | (flag as u8)],
        }
    }

    pub fn try_from_frame(frame: &impl Frame) -> Result<Self, ()> {
        let data = frame.data();

        if data.is_empty() {
            Err(())
        } else {
            match data[0] & 0xF0 {
                Self::SINGLE_FRAME => {
                    let len = (data[0] & 0x0F) as usize;

                    if data.len() > len {
                        Ok(ISOTPCanFrameType::SingleFrame { len })
                    } else {
                        Err(())
                    }
                }
                Self::FIRST_FRAME => {
                    if data.len() < 2 {
                        Err(())
                    } else {
                        let total_len = ((data[0] & 0xF) as usize) << 8 | (data[1] as usize);
                        Ok(ISOTPCanFrameType::FirstFrame { total_len })
                    }
                }
                Self::CONSECUTIVE_FRAME => {
                    let frame_index = (data[0] & 0x0F) as usize;
                    Ok(ISOTPCanFrameType::ConsecutiveFrame { frame_index })
                }
                Self::FLOW_CONTROL_FRAME => {
                    if let Some(flag) = ISOTPFlowControlFlag::from_repr(data[0] & 0x0F) {
                        Ok(ISOTPCanFrameType::FlowControlFrame(flag))
                    } else {
                        Err(())
                    }
                }
                _ => Err(()),
            }
        }
    }
}

impl ISOTPCanFrame {
    fn new(id: Id, frame_type: ISOTPCanFrameType, data: Option<&[u8]>) -> Self {
        let mut payload = frame_type.to_protocol_payload();
        payload.extend_from_slice(data.unwrap_or(&[]));

        let frame = CanFrame::new(id, payload.len(), &payload);

        Self { frame_type, frame }
    }

    pub fn try_from_frame(frame: &impl Frame) -> Result<Self, ()> {
        ISOTPCanFrameType::try_from_frame(frame).map(|frame_type| Self {
            frame_type,
            frame: CanFrame::from_frame(frame),
        })
    }

    pub fn can_frame(&self) -> CanFrame {
        self.frame
    }

    /// Create an ISOTP flow control frame requesting the rest of the stream
    /// at once: block size 0 (no further flow control) and STmin 0
    pub fn new_ack_continue(id: Id) -> Self {
        Self::new(
            id,
            ISOTPCanFrameType::FlowControlFrame(ISOTPFlowControlFlag::Continue),
            Some(&[0x00, 0x00]),
        )
    }

    /// Create an ISOTP SingleFrame from `data`
    pub fn new_single_frame(id: Id, data: &[u8]) -> Option<Self> {
        if !data.is_empty() && (data.len() < CanFrame::MAX_FRAME_SIZE) {
            Some(Self::new(
                id,
                ISOTPCanFrameType::SingleFrame { len: data.len() },
                Some(data),
            ))
        } else {
            None
        }
    }

    /// Create an ISOTP FirstFrame announcing `total_len` and carrying the
    /// first [`FIRST_FRAME_DATA_LEN`] bytes of the payload
    pub fn new_first_frame(id: Id, total_len: usize, data: &[u8]) -> Self {
        Self::new(id, ISOTPCanFrameType::FirstFrame { total_len }, Some(data))
    }

    /// Create an ISOTP ConsecutiveFrame carrying up to
    /// [`CONSECUTIVE_FRAME_DATA_LEN`] bytes of the payload
    pub fn new_consecutive_frame(id: Id, frame_index: usize, data: &[u8]) -> Self {
        Self::new(
            id,
            ISOTPCanFrameType::ConsecutiveFrame { frame_index },
            Some(data),
        )
    }
}

impl Frame for ISOTPCanFrame {
    fn data(&self) -> &[u8] {
        let data = self.frame.data();

        match self.frame_type {
            ISOTPCanFrameType::SingleFrame { len } => &data[1..data.len().min(1 + len)],
            ISOTPCanFrameType::FirstFrame { .. } => &data[2..],
            ISOTPCanFrameType::ConsecutiveFrame { .. } => &data[1..],
            ISOTPCanFrameType::FlowControlFrame(_) => &[],
        }
    }

    fn dlc(&self) -> usize {
        self.frame.dlc()
    }

    fn id(&self) -> Id {
        self.frame.id()
    }

    fn is_data_frame(&self) -> bool {
        self.frame.is_data_frame()
    }

    fn is_extended(&self) -> bool {
        self.frame.is_extended()
    }
    fn is_remote_frame(&self) -> bool {
        self.frame.is_remote_frame()
    }

    fn is_standard(&self) -> bool {
        self.frame.is_standard()
    }

    #[allow(unused)]
    /// Do not use this trait method, use [`ISOTPCanFrame::try_from_frame()`]
    fn new(id: impl Into<Id>, data: &[u8]) -> Option<Self> {
        unimplemented!()
    }

    #[allow(unused)]
    fn new_remote(id: impl Into<Id>, dlc: usize) -> Option<Self> {
        unimplemented!()
    }
}

impl ISOTPProtocol {
    pub const NAME: &'static str = "ISO-TP";

    fn single_frame_request_payload(frame: &impl Frame) -> Option<Vec<u8>> {
        if let Ok(isotp_frame) = ISOTPCanFrame::try_from_frame(frame)
            && let ISOTPCanFrameType::SingleFrame { .. } = isotp_frame.frame_type
        {
            Some(isotp_frame.data().to_vec())
        } else {
            None
        }
    }
}

impl CanProtocol for ISOTPProtocol {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn classify_id(&self, id: Id) -> Option<(FrameDirection, ConversationKey)> {
        let raw_id = raw_id(id);

        let frame_direction = match raw_id {
            FUNCTIONAL_REQUEST_ID | (PHYSICAL_REQUEST_LO..=PHYSICAL_REQUEST_HI) => {
                Some(FrameDirection::Request)
            }
            PHYSICAL_RESPONSE_LO..=PHYSICAL_RESPONSE_HI => Some(FrameDirection::Response),
            _ => None,
        }?;

        let conversation_key = match frame_direction {
            FrameDirection::Request => ConversationKey {
                request_id: raw_id,
                response_id: raw_id + REQUEST_TO_RESPONSE_OFFSET,
            },
            FrameDirection::Response => ConversationKey {
                request_id: raw_id - REQUEST_TO_RESPONSE_OFFSET,
                response_id: raw_id,
            },
        };

        Some((frame_direction, conversation_key))
    }

    fn note_outbound(&mut self, frame: &TimestampedCanFrame) {
        if raw_id(frame.id()) == FUNCTIONAL_REQUEST_ID {
            self.last_functional_broadcast = Some(FunctionalBroadcast {
                owner: Owner::Us,
                instant: frame.instant,
                request: Self::single_frame_request_payload(frame).unwrap_or_default(),
            });
        } else if let Some((_, key)) = self.classify_id(frame.id()) {
            let conversation = self
                .conversations
                .entry(key)
                .or_insert_with(|| Conversation::new(Owner::Us, frame.instant));

            if let Some(payload) = Self::single_frame_request_payload(frame) {
                conversation.request = Some(ISOTPReassembly::complete(payload));
            }
        }
    }

    fn ingest(
        &mut self,
        frame: &TimestampedCanFrame,
        direction: FrameDirection,
        conversation_key: ConversationKey,
        is_echo: bool,
    ) -> RouterAction {
        let raw_id = raw_id(frame.id());
        let instant = frame.instant;

        let owner = if (direction == FrameDirection::Request) && is_echo {
            Owner::Us
        } else {
            Owner::External
        };

        if let Ok(isotp_frame) = ISOTPCanFrame::try_from_frame(frame) {
            if raw_id == FUNCTIONAL_REQUEST_ID {
                if owner == Owner::External {
                    self.last_functional_broadcast = Some(FunctionalBroadcast {
                        owner,
                        instant,
                        request: Self::single_frame_request_payload(frame).unwrap_or_default(),
                    });
                    self.last_external_request = Some(instant);
                }
            }
            let conversation = self
                .conversations
                .entry(conversation_key)
                .or_insert_with(|| Conversation::new(Owner::Unknown, instant));

            match direction {
                FrameDirection::Request => {
                    if owner == Owner::External {
                        conversation.owner = owner;
                        self.last_external_request = Some(instant);
                    }
                }
                FrameDirection::Response => {
                    if conversation.owner == Owner::Unknown
                        && let Some(broadcast) = &self.last_functional_broadcast
                        && instant.saturating_duration_since(broadcast.instant)
                            < FUNCTIONAL_BROADCAST_TTL
                    {
                        conversation.owner = broadcast.owner;
                        conversation.request =
                            Some(ISOTPReassembly::complete(broadcast.request.clone()));
                    }
                }
            }

            let data = isotp_frame.data();
            let mut action = match isotp_frame.frame_type {
                ISOTPCanFrameType::SingleFrame { len } => {
                    if data.is_empty() || (data.len() != len) {
                        RouterAction::Ignored(IgnoreReason::MalformedSingleFrame)
                    } else {
                        RouterAction::Demux(MuxedMessage {
                            protocol: &ISOTPDemuxer,
                            conversation_key,
                            direction,
                            owner,
                            data: data.to_vec(),
                            request: None,
                        })
                    }
                }
                ISOTPCanFrameType::FirstFrame { total_len } => {
                    if data.is_empty() || (total_len < data.len()) {
                        RouterAction::Ignored(IgnoreReason::MalformedFirstFrame)
                    } else {
                        *conversation.reassembly_mut(direction) = Some(ISOTPReassembly {
                            remaining: total_len - data.len(),
                            next_sequence: 1,
                            data: data.to_vec(),
                        });

                        if (conversation.owner == Owner::Us)
                            && (direction == FrameDirection::Response)
                        {
                            let request_id = make_id(isotp_frame.id(), conversation_key.request_id);
                            RouterAction::Transmit(
                                ISOTPCanFrame::new_ack_continue(request_id).can_frame(),
                            )
                        } else {
                            RouterAction::None
                        }
                    }
                }
                ISOTPCanFrameType::ConsecutiveFrame { frame_index: got } => {
                    let reassembly_slot = conversation.reassembly_mut(direction);

                    if let Some(reassembly) = reassembly_slot.as_mut() {
                        let expected = reassembly.next_sequence & 0x0F;

                        if expected == got {
                            let take = reassembly.remaining.min(data.len());
                            reassembly.data.extend_from_slice(&data[..take]);
                            reassembly.remaining -= take;
                            reassembly.next_sequence += 1;

                            if reassembly.remaining == 0 {
                                let complete = reassembly_slot.take().expect("checked above");

                                RouterAction::Demux(MuxedMessage {
                                    protocol: &ISOTPDemuxer,
                                    conversation_key,
                                    direction,
                                    owner,
                                    data: complete.data,
                                    request: None,
                                })
                            } else {
                                RouterAction::None
                            }
                        } else {
                            *reassembly_slot = None;

                            RouterAction::Ignored(IgnoreReason::SequenceMismatch { expected, got })
                        }
                    } else {
                        RouterAction::Ignored(IgnoreReason::ConsecutiveWithoutFirst)
                    }
                }
                ISOTPCanFrameType::FlowControlFrame(flag) => {
                    if let Some(transfer) = self.outbound.get_mut(&conversation_key)
                        && frame.id() != transfer.request_id
                    {
                        match flag {
                            ISOTPFlowControlFlag::Continue => {
                                let data = frame.data();
                                let block_size = data.get(1).copied().unwrap_or(0);
                                let separation = data.get(2).copied().unwrap_or(0);

                                transfer.block_remaining =
                                    (block_size > 0).then_some(block_size as usize);
                                transfer.separation =
                                    ISOTPSeperationTime::from_byte(separation).duration();
                                transfer.next_send = instant;
                                transfer.awaiting_flow_control = false;
                                transfer.last_activity = instant;
                            }
                            ISOTPFlowControlFlag::Wait => {
                                transfer.last_activity = instant;
                            }
                            ISOTPFlowControlFlag::Abort => {
                                self.outbound.remove(&conversation_key);
                            }
                        }
                    }

                    RouterAction::None
                }
            };

            conversation.last_activity = instant;

            if let RouterAction::Demux(message) = &mut action {
                match direction {
                    FrameDirection::Request => {
                        conversation.request =
                            Some(ISOTPReassembly::complete(message.data.clone()));
                    }
                    FrameDirection::Response => {
                        message.request = conversation
                            .request
                            .as_ref()
                            .and_then(ISOTPReassembly::payload)
                            .map(<[u8]>::to_vec);
                    }
                }
            }

            action
        } else {
            RouterAction::Ignored(IgnoreReason::NotIsoTp)
        }
    }

    fn transmit(&mut self, id: Id, payload: &[u8], now: Instant) -> Option<Vec<CanFrame>> {
        let (direction, conversation_key) = self.classify_id(id)?;

        if payload.len() < CanFrame::MAX_FRAME_SIZE {
            ISOTPCanFrame::new_single_frame(id, payload).map(|frame| vec![frame.can_frame()])
        } else if payload.len() <= MAX_MULTI_FRAME_PAYLOAD {
            let conversation = self
                .conversations
                .entry(conversation_key)
                .or_insert_with(|| Conversation::new(Owner::Unknown, now));

            if direction == FrameDirection::Request {
                conversation.owner = Owner::Us;
            }
            conversation.last_activity = now;
            *conversation.reassembly_mut(direction) =
                Some(ISOTPReassembly::complete(payload.to_vec()));

            let (first, rest) = payload.split_at(FIRST_FRAME_DATA_LEN);

            self.outbound.insert(
                conversation_key,
                OutboundTransfer {
                    request_id: id,
                    payload: rest.to_vec(),
                    offset: 0,
                    next_sequence: 1,
                    block_remaining: None,
                    separation: Duration::ZERO,
                    next_send: now,
                    awaiting_flow_control: true,
                    last_activity: now,
                },
            );

            Some(vec![
                ISOTPCanFrame::new_first_frame(id, payload.len(), first).can_frame(),
            ])
        } else {
            None
        }
    }

    fn due_transmissions(&mut self, now: Instant) -> Vec<CanFrame> {
        let mut frames = Vec::new();

        self.outbound.retain(|_, transfer| {
            while !transfer.awaiting_flow_control
                && (now >= transfer.next_send)
                && (transfer.offset < transfer.payload.len())
            {
                let end =
                    (transfer.offset + CONSECUTIVE_FRAME_DATA_LEN).min(transfer.payload.len());

                frames.push(
                    ISOTPCanFrame::new_consecutive_frame(
                        transfer.request_id,
                        transfer.next_sequence & 0x0F,
                        &transfer.payload[transfer.offset..end],
                    )
                    .can_frame(),
                );

                transfer.offset = end;
                transfer.next_sequence += 1;
                transfer.last_activity = now;

                if let Some(remaining) = &mut transfer.block_remaining {
                    *remaining -= 1;

                    if *remaining == 0 {
                        transfer.awaiting_flow_control = true;
                    }
                }

                if !transfer.separation.is_zero() {
                    transfer.next_send = now + transfer.separation;
                }
            }

            transfer.offset < transfer.payload.len()
        });

        frames
    }

    fn last_external_activity(&self) -> Option<Instant> {
        self.last_external_request
    }

    fn cleanup_expired(&mut self, now: Instant) {
        if let Some(broadcast) = &self.last_functional_broadcast {
            if now.saturating_duration_since(broadcast.instant) > FUNCTIONAL_BROADCAST_TTL {
                self.last_functional_broadcast = None;
            }
        }

        self.conversations.retain(|_, conversation| {
            now.saturating_duration_since(conversation.last_activity) <= CONVERSATION_TIMEOUT
        });

        self.outbound.retain(|_, transfer| {
            now.saturating_duration_since(transfer.last_activity) <= FLOW_CONTROL_TIMEOUT
        });
    }
}
