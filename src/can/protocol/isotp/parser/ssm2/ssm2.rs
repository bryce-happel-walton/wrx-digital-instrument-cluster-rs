use super::cobb::*;
use super::ssm2_parameter_sets;
use crate::can::protocol::isotp::{DemuxContext, ISOTPService, isotp_subsystem};

use strum::{EnumIter, FromRepr, IntoEnumIterator, VariantArray};

isotp_subsystem!(SSM2Subsystem, SSM2Service, "SSM2");

ssm2_parameter_sets![CobbParameter];

#[repr(u8)]
#[derive(Debug, Clone, Copy, FromRepr, PartialEq, Eq, VariantArray)]
pub enum SSM2Service {
    ReadBlock = 0xA0,
    ReadAddresses = 0xA8,
    WriteAddress = 0xB0,
    WriteBlock = 0xB8,
    Init = 0xBF,
}

#[repr(u8)]
#[derive(Debug, FromRepr, PartialEq, Eq, EnumIter)]
pub enum SSM2Endpoint {
    Engine = 0x10,
    Transmission = 0x18,
    DiagnosticTool = 0xF0,
}

impl SSM2Service {
    pub const ADDRESS_CHUNK_SIZE: usize = 3;
    pub const REQUEST_HEADER_LEN: usize = 2;
}

impl ISOTPService for SSM2Service {
    fn service_byte(&self) -> u8 {
        *self as u8
    }

    fn demux(&self, context: &DemuxContext) {
        match self {
            Self::ReadAddresses => demux_read_addresses(context),
            other => println!(
                "Unimplemented SSM2 command {other:?}; data: {:#02X?}",
                context.data
            ),
        }
    }
}

fn demux_read_addresses(context: &DemuxContext) {
    let mut addresses = context
        .request
        .get(SSM2Service::REQUEST_HEADER_LEN..)
        .unwrap_or_default();
    let mut values = context.data;

    while let Some(parameter) = CobbParameter::iter()
        .filter(|parameter| addresses.starts_with(parameter.address()))
        .max_by_key(|parameter| parameter.address().len())
    {
        if parameter.data_len() <= values.len() {
            let (data, rest) = values.split_at(parameter.data_len());
            if let Some(value) = parameter.value(data) {
                println!("[SSM] {parameter:?} = {value}");
            }
            values = rest;
            addresses = &addresses[parameter.address().len()..];
        } else {
            break;
        }
    }
}
