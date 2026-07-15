use std::collections::BTreeMap;

crate::parameter_struct! {static OBD_DATA = OBDData {
    [hidden] acquired: bool = false,

    s1_current_data {
        [ro] supported_pids: BTreeMap<u8, bool>,
        [ro] engine_load: f32,
        [ro] engine_speed: u16,
        [ro] control_module_voltage: f32,
        [ro] engine_fuel_rate: f32,
        [ro] odometer: f32,
    },

    s9_vehicle_information {
        [ro] vin: String,
        [ro] supported_pids: BTreeMap<u8, bool>,
        [ro] ecu_name: String,
    },
}}
