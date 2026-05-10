use std::{
    collections::BTreeMap,
    sync::{Arc, LazyLock},
};

pub static STATIC_OBD_DATA: LazyLock<Arc<StaticObddata>> = crate::LazyLockDefault!();
pub static LIVE_OBD_DATA: LazyLock<Arc<LiveObddata>> = crate::LazyLockDefault!();

crate::parameter_struct!(StaticOBDData {
    [hidden] initialized: bool = false,

    s1_current_data {
        [ro] supported_pids: BTreeMap<u8, bool>,
    },

    s9_vehicle_information {
        [ro] vin: String,
        [ro] supported_pids: BTreeMap<u8, bool>,
        [ro] ecu_name: String,
    },
});

crate::parameter_struct!(LiveOBDData {
    [ro] engine_load: f32,
    [ro] engine_speed: u16,
    [ro] control_module_voltage: f32,
    [ro] engine_fuel_rate: f32,
    [ro] odometer: f32,
});
