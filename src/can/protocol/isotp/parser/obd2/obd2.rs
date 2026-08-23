use crate::can::protocol::isotp::{DemuxContext, ISOTPService, isotp_subsystem};
use crate::data::obd2_data::OBD_DATA;

use bitvec::order::Msb0;
use bitvec::vec::BitVec;
use strum::{FromRepr, VariantArray};

use std::collections::BTreeMap;

isotp_subsystem!(OBD2Subsystem, OBD2Service, "OBD2");

pub trait OBD2ServiceData {
    // TRACKING: https://github.com/rust-lang/rust-project-goals/issues/106
    // const fn pid_demux_len(&self) -> usize;

    fn demux(self, data: &[u8]);
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, FromRepr, PartialEq, Eq, VariantArray)]
pub enum OBD2Service {
    CurrentData = 0x01,
    FreezeFrame = 0x02,
    StoredDTCs = 0x03,
    ClearDTCs = 0x04,
    TestResultsNonCan = 0x05,
    TestResultsCan = 0x06,
    PendingDTCs = 0x07,
    Control = 0x08,
    VehicleInformation = 0x09,
    PermanentDTCs = 0x0A,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, FromRepr)]
pub enum S1CurrentData {
    MonitorStatus = 0x01,
    EngineLoad = 0x04,
    EngineSpeed = 0x0C,
    IntakeManifoldAbsolutePressure = 0x0B,
    VehicleSpeed = 0x0D,
    TimingAdvance = 0x0E,
    MAFFlowRate = 0x10,
    FuelTankLevel = 0x2F,
    ControlModuleVoltage = 0x42,
    CommandLambda = 0x44,
    EngineFuelRate = 0x5E,
    Odometer = 0xA6,

    /// The pids that this vehicle supports
    PIDs1 = 0x0,
    PIDs2 = 0x20,
    PIDs3 = 0x40,
    PIDs4 = 0x60,
    PIDs5 = 0x80,
    PIDs6 = 0xA0,
    PIDs7 = 0xC0,
}

impl Into<u8> for S1CurrentData {
    fn into(self) -> u8 {
        self as u8
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, FromRepr)]
pub enum S9VehicleInformation {
    /// The pids that this vehicle supports
    PIDs = 0x0,
    /// the VIN reported by the car
    VIN = 0x02,
    /// The ECU name/id reported by the car
    ECU = 0x0A,
}

impl Into<u8> for S9VehicleInformation {
    fn into(self) -> u8 {
        self as u8
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, FromRepr)]
pub enum DTCCategory {
    Powertrain = 0b00,
    Chassis = 0b01,
    Body = 0b10,
    Undefined = 0b11,
}

#[allow(nonstandard_style)]
pub struct OBD_DTC {
    pub category: DTCCategory,
    pub number: u16,
}

impl OBD2Service {
    pub const PIDS_BITS: usize = S1CurrentData::PIDs1.data_len() * (u8::BITS as usize);
    /// How many PIDs a single support list reports on
    pub const SUPPORT_LIST_SPAN: u8 = Self::PIDS_BITS as u8;

    /// The support-list PIDs this service publishes, in the order they have to
    /// be read: each list reports whether the one after it exists. A service
    /// with no lists places no restriction on what may be requested.
    pub const fn support_lists(&self) -> &'static [u8] {
        match self {
            Self::CurrentData => &[
                S1CurrentData::PIDs1 as u8,
                S1CurrentData::PIDs2 as u8,
                S1CurrentData::PIDs3 as u8,
                S1CurrentData::PIDs4 as u8,
                S1CurrentData::PIDs5 as u8,
                S1CurrentData::PIDs6 as u8,
                S1CurrentData::PIDs7 as u8,
            ],
            Self::VehicleInformation => &[S9VehicleInformation::PIDs as u8],
            _ => &[],
        }
    }

    /// Position within [`Self::support_lists`] of the list reporting `pid`,
    /// or `None` when `pid` falls past every list this service publishes
    pub fn support_list_index(&self, pid: u8) -> Option<usize> {
        let index = (pid.saturating_sub(1) / Self::SUPPORT_LIST_SPAN) as usize;

        (index < self.support_lists().len()).then_some(index)
    }

    /// The support list reporting `pid`
    pub fn support_list_for(&self, pid: u8) -> Option<u8> {
        self.support_list_index(pid)
            .map(|index| self.support_lists()[index])
    }

    /// Whether the vehicle's answer for `list` is already cached
    pub fn support_list_cached(&self, list: u8) -> bool {
        list.checked_add(1)
            .and_then(|first| self.cached_support(first))
            .is_some()
    }

    /// The cached support bit for `pid`; `None` while the list reporting it
    /// has not been read
    pub fn cached_support(&self, pid: u8) -> Option<bool> {
        match self {
            Self::CurrentData => OBD_DATA
                .s1_current_data
                .supported_pids
                .with_value(|map| map.get(&pid).copied()),
            Self::VehicleInformation => OBD_DATA
                .s9_vehicle_information
                .supported_pids
                .with_value(|map| map.get(&pid).copied()),
            _ => None,
        }
    }

    /// Folds a support list's bitmap into the cache. A functional request is
    /// answered by every ECU that recognises it, so the bits are OR'd together
    /// rather than overwritten.
    pub fn merge_support(&self, list: u8, data: &[u8]) {
        let bits = BitVec::<u8, Msb0>::from_slice(data);

        self.update_support(|map| {
            for (index, bit) in bits.iter().enumerate().take(Self::PIDS_BITS) {
                if let Some(pid) = list
                    .checked_add(index as u8)
                    .and_then(|offset| offset.checked_add(1))
                {
                    *map.entry(pid).or_insert(false) |= *bit;
                }
            }
        });
    }

    /// Records a support list the vehicle never answered as reporting nothing,
    /// so everything behind it stops waiting on it
    pub fn mark_support_list_missing(&self, list: u8) {
        self.update_support(|map| {
            for offset in 1..=Self::SUPPORT_LIST_SPAN {
                if let Some(pid) = list.checked_add(offset) {
                    map.entry(pid).or_insert(false);
                }
            }
        });
    }

    fn update_support(&self, edit: impl FnOnce(&mut BTreeMap<u8, bool>)) {
        let supported_pids = match self {
            Self::CurrentData => Some(&OBD_DATA.s1_current_data.supported_pids),
            Self::VehicleInformation => Some(&OBD_DATA.s9_vehicle_information.supported_pids),
            _ => None,
        };

        if let Some(supported_pids) = supported_pids {
            let mut map = supported_pids.value();
            edit(&mut map);
            supported_pids.set_value(map);
        }
    }
}

impl ISOTPService for OBD2Service {
    fn service_byte(&self) -> u8 {
        *self as u8
    }

    fn demux(&self, context: &DemuxContext) {
        let data = context.data;

        match self {
            Self::VehicleInformation => {
                let mut remaining = data;

                while let Some((pid, data)) = remaining.split_first()
                    && let Some(vehicle_information) = S9VehicleInformation::from_repr(*pid)
                    && vehicle_information.data_len() <= data.len()
                {
                    let (data, rest) = data.split_at(vehicle_information.data_len());
                    vehicle_information.demux(data);

                    remaining = rest;
                }
            }
            Self::CurrentData => {
                let mut remaining = data;

                while let Some((pid, data)) = remaining.split_first()
                    && let Some(current_data) = S1CurrentData::from_repr(*pid)
                    && current_data.data_len() <= data.len()
                {
                    let (data, rest) = data.split_at(current_data.data_len());
                    current_data.demux(data);

                    remaining = rest;
                }
            }
            Self::StoredDTCs => {
                let (chunks, remainder) = data.as_chunks::<{ OBD_DTC::SIZE }>();
                for &chunk in chunks {
                    let dtc = OBD_DTC::from(chunk);
                    if dtc.number > 0 {
                        println!("{dtc}");
                    }
                }
                if !remainder.is_empty() {
                    println!(
                        "WARNING: {} remaining bytes in DTC payload — should be 0",
                        remainder.len()
                    );
                }
            }
            other => {
                println!(
                    "Unimplemented OBD2 service {:#02X}; data: {data:#02X?}",
                    other.clone() as u8
                );
            }
        }
    }
}

impl S1CurrentData {
    pub const fn data_len(&self) -> usize {
        match self {
            Self::EngineLoad
            | Self::IntakeManifoldAbsolutePressure
            | Self::VehicleSpeed
            | Self::TimingAdvance
            | Self::FuelTankLevel => 1,
            Self::EngineSpeed
            | Self::MAFFlowRate
            | Self::ControlModuleVoltage
            | Self::EngineFuelRate => 2,
            Self::MonitorStatus
            | Self::CommandLambda
            | Self::Odometer
            | Self::PIDs1
            | Self::PIDs2
            | Self::PIDs3
            | Self::PIDs4
            | Self::PIDs5
            | Self::PIDs6
            | Self::PIDs7 => 4,
        }
    }
}

impl S9VehicleInformation {
    pub const fn data_len(&self) -> usize {
        match self {
            Self::PIDs => 4,
            Self::VIN => 17,
            Self::ECU => 20,
        }
    }
}

impl OBD2ServiceData for S1CurrentData {
    fn demux(self, data: &[u8]) {
        match (&self, data) {
            (
                Self::PIDs1
                | Self::PIDs2
                | Self::PIDs3
                | Self::PIDs4
                | Self::PIDs5
                | Self::PIDs6
                | Self::PIDs7,
                [data @ ..],
            ) => {
                OBD2Service::CurrentData.merge_support(self as u8, data);
            }
            (Self::EngineLoad, [byte]) => {
                let value = (*byte as f32) * (100f32 / 255f32);
                OBD_DATA.s1_current_data.engine_load.set_value(value);
            }
            (Self::EngineSpeed, [hi, lo]) => {
                let value = (((*hi as u16) << 8) | (*lo as u16)) / 4;
                OBD_DATA.s1_current_data.engine_speed.set_value(value);
            }
            (Self::ControlModuleVoltage, [hi, lo]) => {
                let value = (((*hi as u16) << 8) | (*lo as u16)) as f32 * 0.001;
                OBD_DATA
                    .s1_current_data
                    .control_module_voltage
                    .set_value(value);
            }
            (Self::EngineFuelRate, [hi, lo]) => {
                let value = (((*hi as u16) << 8) | (*lo as u16)) as f32 / 20f32;
                OBD_DATA.s1_current_data.engine_fuel_rate.set_value(value);
            }
            (Self::Odometer, [a, b, c, d]) => {
                let value =
                    (((*a as u32) << 24) | ((*b as u32) << 16) | ((*c as u32) << 8) | (*d as u32))
                        as f32
                        * 0.1;
                OBD_DATA.s1_current_data.odometer.set_value(value);
            }
            _ => {}
        }
    }
}

impl OBD2ServiceData for S9VehicleInformation {
    fn demux(self, data: &[u8]) {
        match (&self, data) {
            (Self::PIDs, [data @ ..]) => {
                OBD2Service::VehicleInformation.merge_support(self as u8, data);
            }
            (Self::VIN, [data @ ..]) => {
                let value = String::from_utf8_lossy(data).to_string();
                OBD_DATA.s9_vehicle_information.vin.set_value(value);
            }
            (Self::ECU, [data @ ..]) => {
                let value = String::from_utf8_lossy(data).to_string();
                OBD_DATA.s9_vehicle_information.ecu_name.set_value(value);
            }
        }
    }
}

impl Into<char> for DTCCategory {
    fn into(self) -> char {
        match self {
            Self::Powertrain => 'P',
            Self::Chassis => 'C',
            Self::Body => 'B',
            Self::Undefined => 'U',
        }
    }
}

impl OBD_DTC {
    pub const SIZE: usize = 2;
}

impl From<[u8; OBD_DTC::SIZE]> for OBD_DTC {
    fn from(value: [u8; OBD_DTC::SIZE]) -> Self {
        let category = value[0] >> 6;

        let mut number = (value[0] & 0x3F) as u16;
        for byte in &value[1..] {
            number = (number << 8) | *byte as u16;
        }

        Self {
            category: DTCCategory::from_repr(category).expect("infallible"),
            number,
        }
    }
}

impl std::fmt::Display for OBD_DTC {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{:04X}",
            Into::<char>::into(self.category).to_uppercase(),
            self.number
        )
    }
}
