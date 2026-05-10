use crate::can::messages::wrx_2018::{self, DimmerAndHoodDimmerDialValue, EngineMtGear, Messages};
use crate::can::mux_context::{MuxParseError, MuxParseResult};
use crate::data::parameters::DataParameter;
use crate::data::units::Unit::*;
use crate::data::units::UnitSystem::{self};

use embedded_can::Frame;

use std::str::FromStr;
use std::sync::{Arc, LazyLock};

pub static CAR_DATA: LazyLock<Arc<CarData>> = crate::LazyLockDefault!();

macro_rules! __generate_param_unit_system {
    ($unit:path| $system:expr) => {
        Some($unit($system))
    };
    ($unit:path|) => {
        Some($unit(UnitSystem::default()))
    };
    () => {
        Default::default()
    };
}

macro_rules! __generate_param_instantiation {
    (number $msg:path => $param:ident: $type:ty $([$unit:path| $($system:expr)?])? $(= $default_value:expr)?) => {pastey::paste!{
        DataParameter::new($msg::[<$param:upper _MIN>], $msg::[<$param:upper _MAX>], Some($crate::__default_value!($type| $($default_value)?)), __generate_param_unit_system!($($unit| $($system)?)?))
    }};
    ($msg:path => $param:ident: $type:ty $([$unit:path| $($system:expr)?])? $(= $default_value:expr)?) => {
        $crate::__default_value!($type| $($default_value)?)
    };
}

macro_rules! __param {
    ($msg:path => $param:ident: f32 $([$unit:path| $($system:expr)?])? $(= $default_value:expr)?) => {
        __generate_param_instantiation!(number $msg => $param: f32 $([$unit| $($system)?])? $(= $default_value)?)
    };
    ($msg:path => $param:ident: f64 $([$unit:path| $($system:expr)?])? $(= $default_value:expr)?) => {
        __generate_param_instantiation!(number $msg => $param: f64 $([$unit| $($system)?])? $(= $default_value)?)
    };
    ($msg:path => $param:ident: u8 $([$unit:path| $($system:expr)?])? $(= $default_value:expr)?) => {
        __generate_param_instantiation!(number $msg => $param: u8 $([$unit| $($system)?])? $(= $default_value)?)
    };
    ($msg:path => $param:ident: u16 $([$unit:path| $($system:expr)?])? $(= $default_value:expr)?) => {
        __generate_param_instantiation!(number $msg => $param: u16 $([$unit| $($system)?])? $(= $default_value)?)
    };
    ($msg:path => $param:ident: u32 $([$unit:path| $($system:expr)?])? $(= $default_value:expr)?) => {
        __generate_param_instantiation!(number $msg => $param: u32 $([$unit| $($system)?])? $(= $default_value)?)
    };
    ($msg:path => $param:ident: u64 $([$unit:path| $($system:expr)?])? $(= $default_value:expr)?) => {
        __generate_param_instantiation!(number $msg => $param: u64 $([$unit| $($system)?])? $(= $default_value)?)
    };
    ($msg:path => $param:ident: i8 $([$unit:path| $($system:expr)?])? $(= $default_value:expr)?) => {
        __generate_param_instantiation!(number $msg => $param: i8 $([$unit| $($system)?])? $(= $default_value)?)
    };
    ($msg:path => $param:ident: i16 $([$unit:path| $($system:expr)?])? $(= $default_value:expr)?) => {
        __generate_param_instantiation!(number $msg => $param: i16 $([$unit| $($system)?])? $(= $default_value)?)
    };
    ($msg:path => $param:ident: i32 $([$unit:path| $($system:expr)?])? $(= $default_value:expr)?) => {
        __generate_param_instantiation!(number $msg => $param: i32 $([$unit| $($system)?])? $(= $default_value)?)
    };
    ($msg:path => $param:ident: i64 $([$unit:path| $($system:expr)?])? $(= $default_value:expr)?) => {
        __generate_param_instantiation!(number $msg => $param: i64 $([$unit| $($system)?])? $(= $default_value)?)
    };
    ($msg:path => $param:ident: $type:tt $([$unit:path| $($system:expr)?])? $(= $default_value:expr)?) => {
        __generate_param_instantiation!($msg => $param: $type $([$unit:path| $($system:expr)?])? $(= $default_value)?)
    };
}

macro_rules! __handle_signal_process {
    ($self:ident, $sig:ident, $param:ident) => {
        $self.$param.set_value($sig.$param());
    };
    ($self:ident, $sig:ident, $param:ident, $process_override:ident) => {
        $self.$process_override(&$sig.$param(), $sig);
    };
}

// TODO: derive from parameter_struct!
/// Example:
///```rust
/// CarData! {
///     {
///         // normal struct stuff here
///     };
///
///     MessageEnum => {
///         <Unit(:UnitSystem)?>? [OverrideSetterFn]? param_name: type (= default)?,+
///         <Speed:USCS> [SpeedOverride] cruise_speed: u16 = 91,
///     };+
/// }
///
/// impl CarData {
///     (fn SpeedOverride(&mut self, input: u16 /* &dyn Any */, param: MessageEnum) {
///         ...
///     })?
/// }
///```
///
/// Note: ```bool``` data types default to `true` unless otherwise stated
///
macro_rules! CarData {
    { $($msg:ident => { $($(<$unit:path$(:$unit_system:path)?>)? $([$process_override:ident])? $param:ident: $type:tt $(= $init:expr)?),+ $(,)? } );+; } => {
        pub struct CarData {
            $($($param: DataParameter<$type>,)*)*
        }

        impl CarData {
            pub fn process_message(&self, message: &Messages) {
                match message {
                    $(
                        Messages::$msg(sig) => {
                        $(
                            __handle_signal_process!(self, sig, $param $(, $process_override)?);
                        )*
                    })*
                    _ => {}
                }
            }

            $($(
                pub fn $param(&self) -> &DataParameter<$type> {
                    &self.$param
                }
            )*)*

            pub fn set_param_by_name(&self, param_name: &str, value: &str) {
                match param_name {
                    $($(stringify!($param) => {
                        match value.parse::<$type>() {
                            Ok(value) => self.$param.set_value(value),
                            Err(e) => eprintln!("Failed to set {} to {value}: {e:?}", stringify!($param))
                        }
                    })*)*
                    _ => {
                        eprintln!("Failed to set {param_name} to {value}: {param_name} is not a valid parameter")
                    }
                }
            }

        }

        impl Default for CarData {
            fn default() -> Self {
                Self {
                    $($($param: __param!(wrx_2018::$msg => $param: $type $([$unit| $($unit_system)?])? $(= $init)?),)*)*
                }
            }
        }
    }
}

// TODO: make our own dbc codegen lib and generate the dbc parser along with the `CarData` `DataParameter` struct for each car
CarData! {
    Engine => {
        engine_rpm: u16,
        mt_gear: EngineMtGear,
    };

    EngineStatus2 => {
        <Temperature> engine_oil_temp: i16,
        <Temperature> engine_coolant_temp: i16,
        <Pressure> engine_boost_pressure: f32,
        cruise_control_enabled: bool,
        cruise_control_set_enabled: bool,
        /* <Speed:USCS> */ cruise_control_speed: u8,
    };

    EngineWarningLights => {
        oil_pressure_warning_light_enabled: bool,
        check_engine_light_enabled: bool,
    };

    Odometer => {
        <Distance> odometer: f32,
    };

    BrakePedal => {
        <Speed> vehicle_speed: f32,
    };

    StatusSwitches => {
        lowbeams_enabled: bool,
        brake_sw: bool = false,
        handbrake_sw: bool = false,
        parking_lights_enabled: bool,
        highbeams_enabled: bool,
        reverse_sw: bool,
    };

    Cluster => {
        fuel_level: f32,
        driver_seatbelt_warning_enabled: bool,
        passenger_seatbelt_warning_enabled: bool,
        left_turn_signal_enabled: bool,
        right_turn_signal_enabled: bool,
    };

    MotorControl => {
        mt_clutch_sw: bool = false,
    };

    Cluster2 => {
        fog_lights_enabled: bool,
        tpms_warning_light_enabled: bool,
    };

    Cabin => {
        left_front_door_open: bool,
        right_front_door_open: bool,
        right_rear_door_open: bool,
        left_rear_door_open: bool,
        trunk_open: bool,
        headlight_dimmer_enabled: bool,
        dimmer_max_brightness_enabled: bool,
    };

    XxxMsg340 => {
        any_door_open: bool,
    };

    DriverRoadAssists => {
        hill_assist_enabled: bool,
        active_tq_vectoring_enabled: bool,
        traction_control_disabled: bool,
    };

    BsdRcta => {
        rcta_disabled: bool,
        rcta_left: bool,
        rcta_right: bool,
        bsd_left_adjacent: bool,
        bsd_left_approaching: bool,
        bsd_right_adjacent: bool,
        bsd_right_approaching: bool,
    };

    SrsStatus => {
        srs_warning_light_enabled: bool,
    };

    DimmerAndHood => {
        [dimmer_dial_override] dimmer_dial_value: DimmerAndHoodDimmerDialValue,
        hood_open: bool,
    };
}

#[allow(unused)]
#[derive(Debug)]
pub enum ParseResult {
    Mux(MuxParseResult),
    Ok,
}

#[allow(unused)]
#[derive(Debug)]
pub enum ParseError {
    MuxError(MuxParseError),
    CanError(wrx_2018::CanError),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MuxError(e) => e.fmt(f),
            Self::CanError(e) => e.fmt(f),
        }
    }
}

impl CarData {
    pub fn parse_frame(&self, frame: &impl Frame) -> Result<ParseResult, ParseError> {
        let data = &frame.data()[..frame.dlc()];

        match Messages::from_can_message(frame.id(), data) {
            Ok(message) => {
                self.process_message(&message);
                return Ok(ParseResult::Ok);
            }
            Err(e) => match e {
                _ => return Err(ParseError::CanError(e)),
            },
        };
    }

    pub fn dimmer_dial_override(
        &self,
        value: &DimmerAndHoodDimmerDialValue,
        _message: &crate::can::messages::wrx_2018::DimmerAndHood,
    ) {
        let raw = u8::from(*value);

        if raw <= u8::from(DimmerAndHoodDimmerDialValue::X0) {
            self.dimmer_dial_value
                .set_value(DimmerAndHoodDimmerDialValue::X0);
        } else if raw <= u8::from(DimmerAndHoodDimmerDialValue::X1) {
            self.dimmer_dial_value
                .set_value(DimmerAndHoodDimmerDialValue::X1);
        } else if raw <= u8::from(DimmerAndHoodDimmerDialValue::X2) {
            self.dimmer_dial_value
                .set_value(DimmerAndHoodDimmerDialValue::X2);
        } else if raw <= u8::from(DimmerAndHoodDimmerDialValue::X3) {
            self.dimmer_dial_value
                .set_value(DimmerAndHoodDimmerDialValue::X3);
        } else if raw <= u8::from(DimmerAndHoodDimmerDialValue::X4) {
            self.dimmer_dial_value
                .set_value(DimmerAndHoodDimmerDialValue::X4);
        } else {
            // there is a different parameter for max brightness
            self.dimmer_dial_value
                .set_value(DimmerAndHoodDimmerDialValue::X5);
        }
    }
}

//
// EngineStatusMtGear implementations for DataParameter
//

impl Default for EngineMtGear {
    fn default() -> Self {
        EngineMtGear::Floating
    }
}

impl PartialOrd for EngineMtGear {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let v1 = u8::from(*self);
        let v2 = u8::from(*other);

        if v1 > v2 {
            Some(std::cmp::Ordering::Greater)
        } else if v1 < v2 {
            Some(std::cmp::Ordering::Less)
        } else if v1 == v2 {
            Some(std::cmp::Ordering::Equal)
        } else {
            std::option::Option::None
        }
    }
}

impl Default for DimmerAndHoodDimmerDialValue {
    fn default() -> Self {
        DimmerAndHoodDimmerDialValue::X5
    }
}

impl PartialOrd for DimmerAndHoodDimmerDialValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let v1 = u8::from(*self);
        let v2 = u8::from(*other);

        if v1 > v2 {
            Some(std::cmp::Ordering::Greater)
        } else if v1 < v2 {
            Some(std::cmp::Ordering::Less)
        } else if v1 == v2 {
            Some(std::cmp::Ordering::Equal)
        } else {
            std::option::Option::None
        }
    }
}

impl FromStr for EngineMtGear {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "1" => Ok(Self::X1),
            "2" => Ok(Self::X2),
            "3" => Ok(Self::X3),
            "4" => Ok(Self::X4),
            "5" => Ok(Self::X5),
            "6" => Ok(Self::X6),
            "f" | "floating" => Ok(Self::Floating),
            "n" | "neutral" => Ok(Self::Neutral),
            _ => Err(()),
        }
    }
}

impl FromStr for DimmerAndHoodDimmerDialValue {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "0" => Ok(Self::X0),
            "1" => Ok(Self::X1),
            "2" => Ok(Self::X2),
            "3" => Ok(Self::X3),
            "4" => Ok(Self::X4),
            "5" => Ok(Self::X5),
            _ => Err(()),
        }
    }
}
