pub mod acquisition;
pub mod bus;
pub mod can_backend;
#[cfg(debug_assertions)]
pub mod ecu_emulator;
pub mod protocol;
pub mod router;
pub mod util;

pub mod emulators {
    include!(concat!(env!("OUT_DIR"), "/proj_gen/emulators/mod.rs"));
}

pub mod messages {
    include!(concat!(env!("OUT_DIR"), "/proj_gen/can/messages/mod.rs"));
}
