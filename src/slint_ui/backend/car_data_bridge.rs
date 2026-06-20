use crate::application::settings::SETTINGS;
use crate::data::{car_data::CarData, units::UnitSystem};
use crate::slint_generatedApp::*;

use pastey::paste;
use slint::{ComponentHandle, Weak};
use tokio::select;

use std::sync::Arc;

macro_rules! number_param_convertion_handle {
    ($car_data:ident, $ui_car_data:ident, $unit_system:ident, $param:ident: $type:ty = $value:expr) => {paste!(
        let units = $car_data.$param().units();

        let converted_value = units.convert_value_to($value as f64, $unit_system);
        let converted_min = units.convert_value_to($car_data.$param().min() as f64, $unit_system);
        let converted_max = units.convert_value_to($car_data.$param().max() as f64, $unit_system);

        if !converted_value.is_nan() && !converted_min.is_nan() && !converted_max.is_nan() {
            let mut sparam = $ui_car_data.[<get_ $param>]();
            sparam.value = converted_value as $type;
            sparam.min = converted_min as $type;
            sparam.max = converted_max as $type;
            // TODO: change unit str to use translations instead
            sparam.unit_str = units.convert_system_to($unit_system).get_short_str().into();
            $ui_car_data.[<set_ $param>](sparam);
        }
    )};
}

macro_rules! param_convertion_handle {
    ($car_data:ident, $ui_car_data:ident, $unit_system:ident, $param:ident: SIDataParameter = $value:expr) => {
        number_param_convertion_handle!{$car_data, $ui_car_data, $unit_system, $param: i32 = $value}
    };
    ($car_data:ident, $ui_car_data:ident, $unit_system:ident, $param:ident: SFDataParameter = $value:expr) => {
        number_param_convertion_handle!{$car_data, $ui_car_data, $unit_system, $param: f32 = $value}
    };
    ($car_data:ident, $ui_car_data:ident, $unit_system:ident, $param:ident: $type:tt = $value:expr) => {paste!(
        let mut sparam = $ui_car_data.[<get_ $param>]();
        sparam.value = $value.into();

        $ui_car_data.[<set_ $param>](sparam);
    )};
}

#[allow(unused)]
pub fn bridge(ui: Weak<App>, car_data: Arc<CarData>) {
    macro_rules! bridge {($($param:ident: $type:tt),+ $(,)? ) => {paste!{
        let ui = ui.clone();
        let car_data = car_data.clone();
        tokio::spawn(async move {
            $(
                let mut [<watch_ $param>] = car_data.$param().watch();
                let mut [<value_ $param>] = car_data.$param().value();
            )+

            let unit_system_parameter = &(SETTINGS.user.general.unit_system);
            let mut unit_system_changed = unit_system_parameter.watch();
            let mut unit_system: UnitSystem = unit_system_parameter.value();

            // TODO: we really should only update these before a frame is drawn, currently many cycles are wasted on updates that are not visible
            loop {
                {
                    let car_data = car_data.clone();
                    ui.upgrade_in_event_loop(move |ui| {
                        let ui_car_data = ui.global::<SCarData>();
                        $( param_convertion_handle!(car_data, ui_car_data, unit_system, $param: $type = [<value_ $param>]); )+
                    });
                }

                loop {
                    select! {
                        Ok(_) = unit_system_changed.changed() => {
                            unit_system = *unit_system_changed.borrow_and_update();
                            break;
                        },
                        $(
                            Ok(_) = [<watch_ $param>].changed() => {
                                [<value_ $param>] = *[<watch_ $param>].borrow_and_update();
                                let car_data = car_data.clone();
                                let value = [<value_ $param>];

                                #[cfg(debug_assertions)]
                                let parsed_ns = crate::util::performance::last_parse_instant_ns();
                                ui.upgrade_in_event_loop(move |ui| {
                                    let ui_car_data = ui.global::<SCarData>();
                                    param_convertion_handle!(car_data, ui_car_data, unit_system, $param: $type = value);
                                    #[cfg(debug_assertions)]
                                    crate::util::performance::record_ui_latency(parsed_ns);
                                });
                            },
                        )+
                    };
                }
            }
        });
    }}
}

    bridge! {
        engine_rpm: SIDataParameter,
        mt_gear: SStrDataParameter,

        engine_oil_temp: SIDataParameter,
        engine_coolant_temp: SIDataParameter,
        engine_boost_pressure: SFDataParameter,
        cruise_control_enabled: SBDataParameter,
        cruise_control_set_enabled: SBDataParameter,
        cruise_control_speed: SIDataParameter,

        oil_pressure_warning_light_enabled: SBDataParameter,
        check_engine_light_enabled: SBDataParameter,

        odometer: SFDataParameter,

        vehicle_speed: SFDataParameter,

        lowbeams_enabled: SBDataParameter,
        handbrake_sw: SBDataParameter,
        parking_lights_enabled: SBDataParameter,
        highbeams_enabled: SBDataParameter,
        reverse_sw: SBDataParameter,

        fuel_level: SFDataParameter,
        driver_seatbelt_warning_enabled: SBDataParameter,
        passenger_seatbelt_warning_enabled: SBDataParameter,
        left_turn_signal_enabled: SBDataParameter,
        right_turn_signal_enabled: SBDataParameter,

        mt_clutch_sw: SBDataParameter,

        fog_lights_enabled: SBDataParameter,
        tpms_warning_light_enabled: SBDataParameter,

        left_front_door_open: SBDataParameter,
        right_front_door_open: SBDataParameter,
        right_rear_door_open: SBDataParameter,
        left_rear_door_open: SBDataParameter,
        trunk_open: SBDataParameter,
        headlight_dimmer_enabled: SBDataParameter,
        dimmer_max_brightness_enabled: SBDataParameter,

        any_door_open: SBDataParameter,

        hill_assist_enabled: SBDataParameter,
        active_tq_vectoring_enabled: SBDataParameter,
        traction_control_disabled: SBDataParameter,

        rcta_disabled: SBDataParameter,
        rcta_left: SBDataParameter,
        rcta_right: SBDataParameter,
        bsd_left_adjacent: SBDataParameter,
        bsd_left_approaching: SBDataParameter,
        bsd_right_adjacent: SBDataParameter,
        bsd_right_approaching: SBDataParameter,

        srs_warning_light_enabled: SBDataParameter,

        hood_open: SBDataParameter,
        dimmer_dial_value: SStrDataParameter,
    }
}

// Special type conversion implementations

use crate::can::messages::wrx_2018::{DimmerAndHoodDimmerDialValue, EngineMtGear};

impl Into<slint::SharedString> for EngineMtGear {
    fn into(self) -> slint::SharedString {
        match &self {
            Self::Floating => " ".into(),
            Self::Neutral => "N".into(),
            Self::X1 => "1".into(),
            Self::X2 => "2".into(),
            Self::X3 => "3".into(),
            Self::X4 => "4".into(),
            Self::X5 => "5".into(),
            Self::X6 => "6".into(),
            _ => "?ERR_MT_GEAR".into(),
        }
    }
}

impl Into<slint::SharedString> for DimmerAndHoodDimmerDialValue {
    fn into(self) -> slint::SharedString {
        match &self {
            Self::X0 => "0".into(),
            Self::X1 => "1".into(),
            Self::X2 => "2".into(),
            Self::X3 => "3".into(),
            Self::X4 => "4".into(),
            Self::X5 => "5".into(),
            Self::_Other(val) => format!("?ERR_DIMMER{{ {} }}", *val).into(),
        }
    }
}

impl Into<UnitSystem> for SUnitSystem {
    fn into(self) -> UnitSystem {
        match self {
            Self::USCS => UnitSystem::USCS,
            Self::SI => UnitSystem::SI,
        }
    }
}

impl Into<SUnitSystem> for UnitSystem {
    fn into(self) -> SUnitSystem {
        match self {
            Self::USCS => SUnitSystem::USCS,
            Self::SI => SUnitSystem::SI,
        }
    }
}
