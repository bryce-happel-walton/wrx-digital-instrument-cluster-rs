use crate::CAN_DATA;
use crate::application::settings::SETTINGS;
use crate::data::units::UnitSystem;
use crate::slint_generatedApp::*;

use pastey::paste;
use slint::{ComponentHandle, Weak};

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::time::{self, Duration, Instant};

macro_rules! param_convertion_handle {
    (@number $data_root:ident, $ui_car_data:ident, $unit_system:ident, $param:ident: $type:ty = $value:expr) => {paste!(
        let units = $data_root.$param().units();

        let converted_value = units.convert_value_to($value as f64, $unit_system);
        let converted_min = units.convert_value_to($data_root.$param().min() as f64, $unit_system);
        let converted_max = units.convert_value_to($data_root.$param().max() as f64, $unit_system);

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
    ($data_root:ident, $ui_car_data:ident, $unit_system:ident, $param:ident: SIDataParameter = $value:expr) => {
        param_convertion_handle!{@number $data_root, $ui_car_data, $unit_system, $param: i32 = $value}
    };
    ($data_root:ident, $ui_car_data:ident, $unit_system:ident, $param:ident: SFDataParameter = $value:expr) => {
        param_convertion_handle!{@number $data_root, $ui_car_data, $unit_system, $param: f32 = $value}
    };
    ($data_root:ident, $ui_car_data:ident, $unit_system:ident, $param:ident: $type:tt = $value:expr) => {paste!(
        let mut sparam = $ui_car_data.[<get_ $param>]();
        sparam.value = $value.into();

        $ui_car_data.[<set_ $param>](sparam);
    )};
}

macro_rules! bridge {
    ($data_root:ident; $($param:ident: $type:tt),+ $(,)?) => {paste! {
        pub fn bridge(ui: Weak<App>) {
            let Some(ui) = ui.upgrade() else { return };

            let frames: Arc<AtomicU32> = Default::default();

            {
                let weak = ui.as_weak();
                let frames = frames.clone();

                $( let mut [<watch_ $param>] = $data_root.$param().watch(); )+
                let mut unit_system_watch = SETTINGS.user.general.unit_system.watch();
                let mut first_frame = true;

                ui.window()
                    .set_rendering_notifier(move |state, _| {
                        if !matches!(state, slint::RenderingState::BeforeRendering) {
                            return;
                        }

                        let Some(ui) = weak.upgrade() else { return };

                        frames.fetch_add(1, Ordering::Relaxed);

                        let ui_car_data = ui.global::<SCarData>();
                        let unit_system = SETTINGS.user.general.unit_system.value();
                        let refresh_all = first_frame || unit_system_watch.has_changed().unwrap_or(false);
                        unit_system_watch.mark_unchanged();
                        first_frame = false;

                        let mut updated = false;

                        $(
                            if refresh_all || [<watch_ $param>].has_changed().unwrap_or(false) {
                                let value = *[<watch_ $param>].borrow_and_update();
                                param_convertion_handle!($data_root, ui_car_data, unit_system, $param: $type = value);
                                updated = true;
                            }
                        )+

                        if updated {
                            #[cfg(debug_assertions)]
                            crate::util::performance::record_ui_latency(
                                crate::util::performance::last_parse_instant_ns(),
                            );

                            ui.window().request_redraw();
                        }
                    })
                    .unwrap();
            }

            {
                let frames = frames.clone();
                tokio::spawn(async move {
                    let mut last = Instant::now();
                    let mut interval = time::interval(Duration::from_millis(100));

                    loop {
                        interval.tick().await;
                        let secs = last.elapsed().as_secs_f32();

                        SETTINGS
                            .developer
                            .system_info
                            .fps
                            .set_value(((frames.swap(0, Ordering::Relaxed) as f32) / secs) as i32);

                        last = Instant::now();
                    }
                });
            }
        }
    }};
}

bridge! {
    CAN_DATA;

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
