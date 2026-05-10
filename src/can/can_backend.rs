use crossbeam::channel::{Receiver, RecvTimeoutError, Sender, TrySendError, unbounded};
use embedded_can::{Frame, Id};
use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::error::Error;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

#[cfg(target_os = "linux")]
use socketcan::Socket;

#[derive(Clone, Copy, Serialize, Deserialize, Default, PartialEq, PartialOrd)]
pub enum CanInterface {
    VirtualSocketCan,
    SocketCan,
    SerialCan,
    #[default]
    Fake,
}

impl CanInterface {
    #[allow(unused)]
    fn as_str(&self) -> &'static str {
        match self {
            Self::VirtualSocketCan => "vcan",
            Self::SocketCan => "can",
            Self::SerialCan => "slcan",
            Self::Fake => "fake",
        }
    }
}

impl std::fmt::Display for CanInterface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VirtualSocketCan => write!(f, "vcan(socketcan)"),
            Self::SocketCan => write!(f, "socketcan"),
            Self::SerialCan => write!(f, "slcan"),
            Self::Fake => write!(f, "fake"),
        }
    }
}

pub enum CanSocket {
    #[cfg(target_os = "linux")]
    SocketCan(socketcan::CanSocket),
    #[cfg(feature = "slcan")]
    Serial(slcan::CanSocket<serial::SystemPort>),
    Fake(FakeCanSocket),
}

#[derive(Clone, Copy, Debug)]
pub struct CanFrame {
    id: Id,
    dlc: usize,
    data: [u8; 8],
}

impl CanFrame {
    pub fn new(id: Id, dlc: usize, data: &[u8]) -> Self {
        let mut buffer = [0u8; 8];
        let len = data.len().min(8);
        buffer[..len].copy_from_slice(&data[..len]);

        Self {
            id,
            dlc,
            data: buffer,
        }
    }

    pub fn from_frame(frame: &impl Frame) -> Self {
        Self::new(frame.id(), frame.dlc(), frame.data())
    }
}

impl Frame for CanFrame {
    fn new(id: impl Into<Id>, data: &[u8]) -> Option<Self> {
        Some(CanFrame::new(id.into(), data.len(), data))
    }

    fn new_remote(_id: impl Into<Id>, _dlc: usize) -> Option<Self> {
        None
    }

    fn is_extended(&self) -> bool {
        matches!(self.id, Id::Extended(_))
    }

    fn is_remote_frame(&self) -> bool {
        false
    }

    fn id(&self) -> Id {
        self.id
    }

    fn dlc(&self) -> usize {
        self.dlc
    }

    fn data(&self) -> &[u8] {
        &self.data
    }
}

pub struct CanBackend {
    socket: CanSocket,
}

const DEFAULT_TIMEOUT: Duration = Duration::from_millis(1);

impl CanBackend {
    pub fn new(
        interface_type: &CanInterface,
        interface_path: &str,
    ) -> Result<Self, Box<dyn Error>> {
        let socket: Option<CanSocket> = match interface_type {
            #[cfg(target_os = "linux")]
            CanInterface::VirtualSocketCan | CanInterface::SocketCan => {
                let interface = match socketcan::CanInterface::open(&interface_path) {
                    Ok(can_interface) => Some(can_interface),
                    _ => {
                        let can_if_str = interface_type.as_str();
                        match socketcan::CanInterface::create(&interface_path, None, can_if_str) {
                            Ok(can_interface) => {
                                println!("Created CAN interface {interface_path}");
                                Some(can_interface)
                            }
                            Err(e) => {
                                eprintln!("Failed to create CAN interface {interface_path}: {e}");
                                None
                            }
                        }
                    }
                };

                if let Some(interface) = interface {
                    let can_bitrate = 500000;
                    let details = interface.details()?;

                    let is_up = if (&details).is_up {
                        true
                    } else {
                        match interface.set_bitrate(can_bitrate, None) {
                            Ok(_) => {}
                            Err(e) => match interface_type {
                                CanInterface::VirtualSocketCan => {} // vcan does not allow setting bitrate
                                _ => eprintln!("Failed to set can bitrate: {e:?}"),
                            },
                        }

                        match interface.bring_up() {
                            Ok(_) => true,
                            Err(e) => {
                                eprintln!(
                                    "Failed to bring up interface {}: {e:?}",
                                    details.name.unwrap()
                                );
                                false
                            }
                        }
                    };

                    if is_up {
                        let socket = socketcan::CanSocket::open_iface(details.index)?;
                        let _ = socket.set_read_timeout(Some(DEFAULT_TIMEOUT));

                        Some(CanSocket::SocketCan(socket))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }

            #[cfg(feature = "slcan")]
            CanInterface::SerialCan => match serial::SystemPort::open(Path::new(interface_path)) {
                Ok(port) => {
                    let mut socket = slcan::CanSocket::<serial::SystemPort>::new(port);

                    socket.close()?;
                    socket.open(slcan::BitRate::Setup500Kbit)?;

                    Some(CanSocket::Serial(socket))
                }
                Err(e) => {
                    eprintln!("Error opening serial device {interface_path}: {e}");
                    None
                }
            },

            CanInterface::Fake => {
                let mut socket = FakeCanSocket::open(interface_path);
                socket.set_read_timeout(Some(DEFAULT_TIMEOUT));
                Some(CanSocket::Fake(socket))
            }

            #[allow(unreachable_patterns)]
            _ => return Err("Interface unsupported".into()),
        };

        if let Some(socket) = socket {
            Ok(Self { socket })
        } else {
            Err("No socket".into())
        }
    }

    pub fn read_frame(&mut self) -> Option<CanFrame> {
        match self.socket {
            #[cfg(target_os = "linux")]
            CanSocket::SocketCan(ref socket) => {
                if let Ok(frame) = socket.read_frame() {
                    return Some(CanFrame::from_frame(&frame));
                }
            }

            #[cfg(feature = "slcan")]
            CanSocket::Serial(ref mut socket) => {
                if let Ok(frame) = socket.read() {
                    return Some(CanFrame::from_frame(&frame));
                }
            }

            CanSocket::Fake(ref mut socket) => return socket.read().ok(),
        }

        None
    }

    pub fn write_frame(&mut self, frame: impl Frame) -> Result<(), Box<dyn std::error::Error>> {
        match self.socket {
            #[cfg(target_os = "linux")]
            CanSocket::SocketCan(ref socket) => {
                let socketcan_frame =
                    socketcan::CanFrame::new(frame.id(), &frame.data()[..frame.dlc()]);
                if let Some(frame) = socketcan_frame {
                    match socket.write_frame(&frame) {
                        Ok(_) => Ok(()),
                        Err(e) => Err(e.into()),
                    }
                } else {
                    Err("Failed to create socketcan frame".into())
                }
            }

            #[cfg(feature = "slcan")]
            CanSocket::Serial(ref mut socket) => {
                match socket.write(frame.id(), &frame.data()[..frame.dlc()]) {
                    Ok(_) => Ok(()),
                    Err(e) => Err(e.into()),
                }
            }

            CanSocket::Fake(ref mut socket) => match socket.write(CanFrame::from_frame(&frame)) {
                Ok(_) => Ok(()),
                Err(e) => Err(e.into()),
            },
        }
    }
}

static FAKE_CAN_BUSSES: LazyLock<Mutex<HashMap<String, Arc<FakeCanBus>>>> =
    crate::LazyLockDefault!();

#[derive(Default)]
pub struct FakeCanBus {
    subscribers: Mutex<Vec<Sender<CanFrame>>>,
    loopback: AtomicBool,
}

impl FakeCanBus {
    pub fn subscribe(
        &self,
    ) -> Result<(Sender<CanFrame>, Receiver<CanFrame>), Box<dyn std::error::Error>> {
        match self.subscribers.lock() {
            Ok(mut subscribers) => {
                let (tx, rx) = unbounded::<CanFrame>();
                subscribers.push(tx.clone());
                Ok((tx, rx))
            }
            Err(e) => Err(format!("{e:?}").into()),
        }
    }

    fn broadcast(
        &self,
        tx: &Sender<CanFrame>,
        frame: CanFrame,
    ) -> Result<(), TrySendError<CanFrame>> {
        let subscribers = self
            .subscribers
            .lock()
            .ok()
            .and_then(|subscribers| Some(subscribers.clone()));

        if let Some(subscribers) = subscribers {
            let loopback = self.loopback.load(Ordering::Relaxed);
            for sub in subscribers {
                if !sub.same_channel(&tx) || loopback {
                    // NOTE: if the loopback doesn't have a consumer, then this may fail to fully broadcast
                    if loopback {
                        let _ = sub.try_send(frame.clone());
                    } else {
                        sub.try_send(frame.clone())?;
                    }
                }
            }
        }

        Ok(())
    }
}

pub struct FakeCanSocket {
    bus: Arc<FakeCanBus>,
    tx: Sender<CanFrame>,
    rx: Receiver<CanFrame>,
    timeout: Option<Duration>,
}

impl FakeCanSocket {
    pub fn open(name: &str) -> Self {
        let bus = FAKE_CAN_BUSSES
            .lock()
            .unwrap()
            .entry(name.to_owned())
            .or_default()
            .clone();

        let (tx, rx) = bus.subscribe().ok().unwrap();

        Self {
            bus,
            tx,
            rx,
            timeout: None,
        }
    }

    pub fn set_read_timeout(&mut self, timeout: Option<Duration>) {
        self.timeout = timeout;
    }

    pub fn read(&mut self) -> Result<CanFrame, RecvTimeoutError> {
        if let Some(timeout) = self.timeout {
            Ok(self.rx.recv_timeout(timeout)?)
        } else {
            Ok(self.rx.recv()?)
        }
    }

    pub fn write(&mut self, frame: CanFrame) -> Result<(), TrySendError<CanFrame>> {
        Ok(self.bus.broadcast(&self.tx, frame)?)
    }
}
