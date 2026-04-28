use crate::can::can_backend::{CanBackend, CanFrame, CanInterface};
use crate::can::parsers::{iso_tp::*, obd2::*, uds::*};
use crate::can::util::raw_id;

use bitvec::order::Msb0;
use bitvec::vec::BitVec;
use embedded_can::{Frame, Id};

use std::collections::BTreeMap;

#[derive(Debug)]
pub enum MuxParseError {
    UnknownMessageId,
    ConsecutiveFrameNoPriorData,
    /// ISOTP leaves an additional 2 bits for other frame types in the future
    InvalidISOTPFrameType,
}

#[derive(Debug)]
pub enum MuxParseResult {
    /// Message fully muxed
    ParseComplete,
    /// Requests from other broadcasters, not to be processed by us
    BroadcastFeedback,
    /// Waiting for our broadcaster to send a control frame
    AwaitingBroadcastAck,
    /// Waiting to receive a control from from bus
    AwaitingReceiveAck,
    /// Full mux is incomplete, continue until complete
    ConsecutiveFrameContinue,
}

pub struct MuxContext {
    can_backend: CanBackend,
    muxed_iso_tp_frames: BTreeMap<Id, Vec<ISOTPMux>>,
    waiting_for_responce: bool,
    conversation_owner: BTreeMap<Id, bool>,
    diagnostic_tool_detected: bool,
}

impl MuxContext {
    pub fn new(can_backend: CanBackend) -> Self {
        Self {
            can_backend,
            muxed_iso_tp_frames: Default::default(),
            waiting_for_responce: Default::default(),
            conversation_owner: Default::default(),
            diagnostic_tool_detected: Default::default(),
        }
    }

    pub fn is_waiting_for_responce(&self) -> bool {
        self.waiting_for_responce
    }

    pub fn parse_frame(&mut self, frame: &impl Frame) -> Result<MuxParseResult, MuxParseError> {
        if matches!(raw_id(frame.id()), (0x7DF..0x7E0) | (0x7E0..=0x7EF)) {
            self.parse_isotp_frame(frame)
        } else {
            Err(MuxParseError::UnknownMessageId)
        }
    }

    fn parse_isotp_frame(&mut self, frame: &impl Frame) -> Result<MuxParseResult, MuxParseError> {
        let id = frame.id();

        if (0x7DF..=0x7E0).contains(&raw_id(id)) {
            return Ok(MuxParseResult::BroadcastFeedback);
        }

        let payload = frame.data();
        let protocol_control: u8 = (payload[0] >> 4).into();

        if let Ok(isotp_frame) = ISOTPFrameType::try_from(protocol_control) {
            self.waiting_for_responce = false; // TODO: need some sort of timeout also

            match isotp_frame {
                ISOTPFrameType::SingleFrame => {
                    let mux_payload = ISOTPMux {
                        demux_len: (payload[0] & 0xF) as usize,
                        data: (&payload[1..]).to_vec(),
                        mux_complete: true,
                        next_sequence: 0,
                    };

                    self.push_iso_tp_mux(id, mux_payload);
                    self.demux_isotp(id);

                    Ok(MuxParseResult::ParseComplete)
                }
                ISOTPFrameType::FirstFrame => {
                    let data = &payload[2..];
                    let demux_len = ((payload[0] & 0xF) as usize) << 8 | (payload[1] as usize);

                    let mux_payload = ISOTPMux {
                        demux_len: demux_len - data.len(),
                        data: data.to_vec(),
                        mux_complete: false,
                        next_sequence: 1,
                    };

                    self.push_iso_tp_mux(id, mux_payload);

                    Ok(MuxParseResult::AwaitingBroadcastAck)
                }
                ISOTPFrameType::ConsecutiveFrame => {
                    if let Some(vec) = self.muxed_iso_tp_frames.get_mut(&id) {
                        for mux_payload in vec.iter_mut().rev() {
                            let frame_index = (payload[0] & 0xF) as usize;

                            if mux_payload.next_sequence == frame_index {
                                mux_payload.next_sequence += 1;
                            } else {
                                continue;
                            }

                            let data = &payload[1..];

                            mux_payload.demux_len -= mux_payload.demux_len.min(data.len());
                            mux_payload.data.extend_from_slice(data);

                            if mux_payload.demux_len == 0 {
                                mux_payload.mux_complete = true;
                                self.demux_isotp(id);

                                return Ok(MuxParseResult::ParseComplete);
                            } else {
                                self.waiting_for_responce = true;

                                return Ok(MuxParseResult::ConsecutiveFrameContinue);
                            }
                        }
                    }

                    Err(MuxParseError::ConsecutiveFrameNoPriorData)
                }
                ISOTPFrameType::FlowControlFrame => Ok(MuxParseResult::AwaitingReceiveAck), // TODO: this is functionally incomplete, but we are not acting as a reciever for the time being
            }
        } else {
            Err(MuxParseError::InvalidISOTPFrameType)
        }
    }

    fn demux_isotp(&mut self, can_id: Id) {
        if let Some(vec) = self.muxed_iso_tp_frames.get_mut(&can_id) {
            vec.retain(|isotp_payload| {
                if isotp_payload.mux_complete {
                    let service = isotp_payload.data[0];
                    let demuxed_data = &(isotp_payload.data[1..]);
                    Self::parse_demux(can_id, service, demuxed_data);
                }

                !isotp_payload.mux_complete
            });
        }
    }

    fn parse_demux(can_id: Id, service: u8, data: &[u8]) {
        let service_offset = if service >= SERVICE_OFFSET {
            service - SERVICE_OFFSET
        } else {
            service
        };

        if let Some(obd_service) = OBDService::from_repr(service_offset) {
            match obd_service {
                OBDService::VehicleInformation => {
                    if let Some((pid, data)) = data.split_first() {
                        if let Some(vehicle_information) = S9VehicleInformation::from_repr(*pid) {
                            match vehicle_information {
                                // TODO: remove this and make it parameterized
                                S9VehicleInformation::PIDs => {
                                    println!("S9 PIDS Supported:");

                                    let bits = BitVec::<u8, Msb0>::from_vec(data.to_vec());

                                    // let mut pids: BTreeMap<u32, bool> = BTreeMap::new();
                                    for (i, bit) in bits.iter().enumerate() {
                                        let pid = i as u32 + 1;
                                        let value = *bit;
                                        // pids.insert(pid, value);

                                        if value {
                                            print!("[{pid:02X}] ")
                                        }
                                    }

                                    println!();
                                }
                                S9VehicleInformation::VIN => {
                                    let vin = String::from_utf8_lossy(data);
                                    println!("VIN: {vin}");
                                }
                                S9VehicleInformation::ECU => {
                                    let ecu = String::from_utf8_lossy(data);
                                    println!("ECU Name: {ecu}");
                                }
                            }
                        }
                    }
                }
                OBDService::CurrentData => {
                    if let Some((pid, data)) = data.split_first() {
                        if let Some(current_data) = S1CurrentData::from_repr(*pid) {
                            match current_data {
                                S1CurrentData::PIDs1
                                | S1CurrentData::PIDs2
                                | S1CurrentData::PIDs3
                                | S1CurrentData::PIDs4
                                | S1CurrentData::PIDs5
                                | S1CurrentData::PIDs6
                                | S1CurrentData::PIDs7 => {
                                    const _LEN: usize = 4;

                                    // println!("S1 PIDS Supported:");

                                    // let bits = BitVec::<u8, Msb0>::from_vec(data.to_vec());

                                    // // let mut pids: BTreeMap<u32, bool> = BTreeMap::new();
                                    // for (i, bit) in bits.iter().enumerate() {
                                    //     let pid = i as u32 + 1;
                                    //     let value = *bit;
                                    //     // pids.insert(pid, value);

                                    //     if value {
                                    //         print!("[{pid:02X}] ")
                                    //     }
                                    // }

                                    // println!();
                                }
                                S1CurrentData::EngineLoad => {
                                    const _LEN: usize = 1;
                                    let _value = data[0] as f32 * (100f32 / 255f32);
                                }
                                S1CurrentData::ControlModuleVoltage => {
                                    const _LEN: usize = 2;

                                    let value = ((data[0] as u16) << 8) | (data[1] as u16);
                                    let _voltage = value as f32 * 0.001;
                                }
                                S1CurrentData::EngineFuelRate => {
                                    const _LEN: usize = 2;

                                    let value = ((data[0] as u16) << 8) | (data[1] as u16);
                                    // L/h
                                    let _rate = value as f32 / 20f32;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                OBDService::StoredDTCs => {
                    let (chunks, _remainder) = data.as_chunks::<DTC_SIZE>();

                    for &chunk in chunks {
                        let dtc = DTC::from(chunk);
                        if dtc.number > 0 {
                            println!("{dtc}")
                        }
                    }

                    let remaining_bytes = _remainder.len();
                    if remaining_bytes > 0 {
                        println!(
                            "WARNING: {remaining_bytes} remaining bytes in DTC payload--should be 0"
                        )
                    }
                }
                _ => {
                    println!(
                        "Demuxed complete data in OBD2 format | Id: 0x{:02X}, Service: 0x{:02X}, demux: 0x{data:02X?}",
                        raw_id(can_id),
                        obd_service as u8
                    );
                }
            }
        } else if let Some(uds_service) = UDSService::from_repr(service_offset) {
            match uds_service {
                UDSService::NegativeResponce => {
                    if let Some(negative_responce) = UDSNegativeResponce::from_repr(data[0]) {
                        println!(
                            "Received negative UDS responce: {negative_responce:?}; data: 0x{data:02X?}"
                        )
                    } else {
                        println!(
                            "Received negative UDS responce: Unknown Service: 0x{service:02X}; data: 0x{data:02X?}"
                        )
                    }
                }
                _ => {
                    println!(
                        "Demuxed complete data in UDS format | Id: 0x{:02X}, Service: 0x{:02X}({:?}), PID: 0x{:02X}, data: 0x{:02X?}",
                        raw_id(can_id),
                        service_offset,
                        uds_service,
                        data[0],
                        &data[1..]
                    )
                }
            }
        } else {
            println!(
                "Demuxed complete ISOTP data in unknown format | Id: 0x{:02X}, Service: 0x{service:02X}, Data: 0x{data:02X?}",
                raw_id(can_id)
            );
        }
    }

    fn push_iso_tp_mux(&mut self, can_id: Id, mux: ISOTPMux) {
        self.muxed_iso_tp_frames
            .entry(can_id)
            .or_default()
            .push(mux);
    }
}

/// use to search for known patterns in unknown signals
// TODO: just use bitvec
#[allow(unused)]
pub fn search_payload_unaligned(payload: &[u8], pattern: u64) -> bool {
    let search_len = pattern.ilog2() + 1;
    let mut current = 0u64;

    for &byte in payload {
        for b in 0u8..8u8 {
            current = (current << 1) | ((byte >> (7 - b)) & 1) as u64;
            current &= (1 << search_len) - 1;

            if current == pattern {
                return true;
            }
        }
    }

    false
}

impl std::fmt::Display for MuxParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConsecutiveFrameNoPriorData => {
                write!(f, "Consecutive frame does not have prior data to append")
            }
            Self::InvalidISOTPFrameType => {
                write!(f, "Frame protocol control is not an ISO-TP type")
            }
            Self::UnknownMessageId => write!(f, "Frame is not a known mux id"),
        }
    }
}

impl std::error::Error for MuxParseError {}
