use super::can_backend::{CanBackend, CanFrame, CanInterface};
use super::protocol::isotp::parser::obd2::{OBD2Service, S1CurrentData, S9VehicleInformation};
use super::protocol::isotp::{
    FUNCTIONAL_REQUEST_ID, ISOTPProtocol, PHYSICAL_REQUEST_LO, REQUEST_TO_RESPONSE_OFFSET,
    SERVICE_OFFSET,
};
use super::protocol::{FrameDirection, RouterAction};
use super::router::ProtocolRouter;
use crate::application::global_state::GLOBAL_STATE;

use embedded_can::{Id, StandardId};
use rand::RngExt;

use std::time::Instant;

const REQUEST_ID: u32 = PHYSICAL_REQUEST_LO;
const RESPONSE_ID: u32 = REQUEST_ID + REQUEST_TO_RESPONSE_OFFSET;

const VIN: &str = "JF1VA1E60J9812345";
const ECU_NAME: &str = "ECM-EngineControl   ";

const _: () = assert!(VIN.len() == S9VehicleInformation::VIN.data_len());
const _: () = assert!(ECU_NAME.len() == S9VehicleInformation::ECU.data_len());

const SUPPORTED_CURRENT_DATA: &[S1CurrentData] = &[
    S1CurrentData::PIDs1,
    S1CurrentData::PIDs2,
    S1CurrentData::PIDs3,
    S1CurrentData::PIDs4,
    S1CurrentData::PIDs5,
    S1CurrentData::PIDs6,
    S1CurrentData::EngineLoad,
    S1CurrentData::EngineSpeed,
    S1CurrentData::IntakeManifoldAbsolutePressure,
    S1CurrentData::VehicleSpeed,
    S1CurrentData::TimingAdvance,
    S1CurrentData::MAFFlowRate,
    S1CurrentData::FuelTankLevel,
    S1CurrentData::ControlModuleVoltage,
    S1CurrentData::EngineFuelRate,
    S1CurrentData::Odometer,
];

const SUPPORTED_VEHICLE_INFORMATION: &[S9VehicleInformation] = &[
    S9VehicleInformation::PIDs,
    S9VehicleInformation::VIN,
    S9VehicleInformation::ECU,
];

pub struct EcuEmulator {
    backend: CanBackend,
    router: ProtocolRouter,
    response_id: Id,
}

impl EcuEmulator {
    pub fn new(backend: CanBackend) -> Self {
        let mut router = ProtocolRouter::default();
        router.register(ISOTPProtocol::default());

        Self {
            backend,
            router,
            response_id: StandardId::new(RESPONSE_ID as u16)
                .expect("response id is a valid standard id")
                .into(),
        }
    }

    pub fn process(&mut self) {
        if let Some(frame) = self.backend.read_frame() {
            match self.router.ingest(&frame) {
                RouterAction::Demux(message) => {
                    if (message.direction == FrameDirection::Request)
                        && addressed_to_us(message.conversation_key.request_id)
                        && let Some(response) = respond(&message.data)
                    {
                        self.send(&response);
                    }
                }
                RouterAction::Transmit(protocol_frame) => self.write(protocol_frame),
                _ => {}
            }
        }

        for frame in self.router.due_transmissions(Instant::now()) {
            self.write(frame);
        }
    }

    fn send(&mut self, payload: &[u8]) {
        let frames = self.router.transmit(self.response_id, payload);

        if frames.is_empty() {
            eprintln!("[ecu] no protocol took a {} byte response", payload.len());
        }

        for frame in frames {
            self.write(frame);
        }
    }

    fn write(&mut self, frame: CanFrame) {
        self.router.note_outbound(&frame);

        if let Err(e) = self.backend.write_frame(frame) {
            eprintln!("[ecu] failed to write frame: {e:?}");
        }
    }
}

pub fn spawn() {
    let interface = GLOBAL_STATE.can.selected_interface.value();

    if interface == CanInterface::Fake {
        match CanBackend::new(&interface, &GLOBAL_STATE.can.interface_path.value()) {
            Ok(backend) => {
                println!("[ecu] emulating a powertrain ECU on {RESPONSE_ID:#03X}");

                tokio::spawn(async move {
                    let mut ecu = EcuEmulator::new(backend);

                    loop {
                        ecu.process();
                        tokio::task::yield_now().await;
                    }
                });
            }
            Err(e) => eprintln!("[ecu] failed to open the fake bus: {e:?}"),
        }
    }
}

fn addressed_to_us(request_id: u32) -> bool {
    (request_id == FUNCTIONAL_REQUEST_ID) || (request_id == REQUEST_ID)
}

fn respond(request: &[u8]) -> Option<Vec<u8>> {
    request
        .split_first()
        .and_then(|(&service, pids)| Some((OBD2Service::from_repr(service)?, pids)))
        .and_then(|(service, pids)| {
            let mut payload = vec![(service as u8) + SERVICE_OFFSET];

            for &pid in pids {
                let data = match service {
                    OBD2Service::CurrentData => current_data(pid),
                    OBD2Service::VehicleInformation => vehicle_information(pid),
                    _ => None,
                };

                if let Some(data) = data {
                    payload.push(pid);
                    payload.extend(data);
                }
            }

            (payload.len() > 1).then_some(payload)
        })
}

fn current_data(pid: u8) -> Option<Vec<u8>> {
    supported(pid, SUPPORTED_CURRENT_DATA).map(|pid| match pid {
        S1CurrentData::PIDs1
        | S1CurrentData::PIDs2
        | S1CurrentData::PIDs3
        | S1CurrentData::PIDs4
        | S1CurrentData::PIDs5
        | S1CurrentData::PIDs6
        | S1CurrentData::PIDs7 => support_bitmap(pid as u8, SUPPORTED_CURRENT_DATA).to_vec(),
        S1CurrentData::EngineLoad => vec![rand::rng().random_range(0..=255u8)],
        S1CurrentData::EngineSpeed => (rand::rng().random_range(800..=6500u16) * 4)
            .to_be_bytes()
            .to_vec(),
        S1CurrentData::IntakeManifoldAbsolutePressure => {
            vec![rand::rng().random_range(20..=250u8)]
        }
        S1CurrentData::VehicleSpeed => vec![rand::rng().random_range(0..=160u8)],
        S1CurrentData::TimingAdvance => vec![rand::rng().random_range(80..=200u8)],
        S1CurrentData::MAFFlowRate => rand::rng()
            .random_range(100..=25000u16)
            .to_be_bytes()
            .to_vec(),
        S1CurrentData::FuelTankLevel => vec![rand::rng().random_range(0..=255u8)],
        S1CurrentData::ControlModuleVoltage => rand::rng()
            .random_range(11800..=14600u16)
            .to_be_bytes()
            .to_vec(),
        S1CurrentData::EngineFuelRate => {
            rand::rng().random_range(0..=600u16).to_be_bytes().to_vec()
        }
        S1CurrentData::Odometer => rand::rng()
            .random_range(500_000..=3_000_000u32)
            .to_be_bytes()
            .to_vec(),
        other => vec![0; other.data_len()],
    })
}

fn vehicle_information(pid: u8) -> Option<Vec<u8>> {
    supported(pid, SUPPORTED_VEHICLE_INFORMATION).map(|pid| match pid {
        S9VehicleInformation::PIDs => {
            support_bitmap(pid as u8, SUPPORTED_VEHICLE_INFORMATION).to_vec()
        }
        S9VehicleInformation::VIN => VIN.as_bytes().to_vec(),
        S9VehicleInformation::ECU => ECU_NAME.as_bytes().to_vec(),
    })
}

fn supported<P: Copy + Into<u8>>(pid: u8, known: &[P]) -> Option<P> {
    known
        .iter()
        .copied()
        .find(|&candidate| Into::<u8>::into(candidate) == pid)
}

fn support_bitmap<P: Copy + Into<u8>>(
    list: u8,
    supported: &[P],
) -> [u8; OBD2Service::PIDS_BITS / 8] {
    let mut bitmap = [0u8; OBD2Service::PIDS_BITS / 8];

    for &pid in supported {
        let offset = Into::<u8>::into(pid).wrapping_sub(list);

        if (1..=OBD2Service::SUPPORT_LIST_SPAN).contains(&offset) {
            let index = (offset - 1) as usize;
            bitmap[index / 8] |= 0x80 >> (index % 8);
        }
    }

    bitmap
}
