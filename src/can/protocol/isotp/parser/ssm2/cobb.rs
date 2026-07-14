use super::ssm2_parameters;

ssm2_parameters!(CobbParameter {
    AfCorrection1 = [0x00, 0x00, 0x09],
    AfLearning1 = [0x00, 0x00, 0x0A],
    AfRatio = [0xF8, 0x8E, 0xD8, 0xF8, 0x8E, 0xD9],
    Battery = [0x00, 0x00, 0x1C],
    Boost = [0xF8, 0x3C, 0x6A, 0xF8, 0x3C, 0x6B, 0x00, 0x00, 0x23],
    CommandFuelFinal = [0xF8, 0xC4, 0x0A, 0xF8, 0xC4, 0x0B],
    DynamicAdvanceMultiplier = [0xF8, 0x11, 0x00],
    FeedbackKnockCorrection = [0xF8, 0xBB, 0xCA],
    FineKnockLearn = [0xF8, 0xBB, 0xDA],
    FuelPressure = [0xF8, 0x8C, 0x64, 0xF8, 0x8C, 0x65],
    InjectorDuty = [0x00, 0x00, 0x20, 0x00, 0x00, 0x0E, 0x00, 0x00, 0x0F],
    CalculatedLoad = [0xF8, 0xBA, 0x26],
    IntakeManifoldTemperature = [0xF8, 0x8C, 0xB4],
    VehicleSpeed = [0x00, 0x00, 0x10],
    IgnitionTiming = [0x00, 0x00, 0x11],
    WastegateDutyCycle = [0x00, 0x00, 0x30],
    EngineSpeed = [0xF8, 0xE3, 0xD4, 0xF8, 0xE3, 0xD5],
    InjectorPulseWidth = [0xF8, 0xE5, 0xBC, 0xF8, 0xE5, 0xBD],
    AtmosphericPressure = [0x00, 0x00, 0x23],
    ManifoldAbsolutePressure = [0xF8, 0x3C, 0x6A, 0xF8, 0x3C, 0x6B],
    Odometer = [0x00, 0x01, 0x0E],
    MafCorrFinal = [0xF8, 0x3C, 0xC4, 0xF8, 0x3C, 0xC5, 0xF8, 0x3C, 0xC6, 0xF8, 0x3C, 0xC7],
});

impl CobbParameter {
    pub fn value(&self, data: &[u8]) -> Option<f64> {
        match (self, data) {
            (Self::AfCorrection1, [value]) => Some(*value as f64 * 0.78125 - 100.0),
            (Self::AfLearning1, [value]) => Some(*value as f64 * 0.78125 - 100.0),
            (Self::AfRatio, [hi, lo]) => Some(u16::from_be_bytes([*hi, *lo]) as f64 * 0.00179),
            (Self::Battery, [value]) => Some(*value as f64 * 0.08),
            (Self::Boost, [a, b, c]) => {
                let raw = ((*a as u32) << 16 | (*b as u32) << 8 | *c as u32) as f64;
                Some(6451.6129 / raw - 14.65)
            }
            (Self::CommandFuelFinal, [hi, lo]) => {
                Some(3763.2 / u16::from_be_bytes([*hi, *lo]) as f64)
            }
            (Self::DynamicAdvanceMultiplier, [value]) => Some(*value as f64 * 0.0625),
            (Self::FeedbackKnockCorrection, [value]) => Some(*value as i8 as f64 * 0.3515625),
            (Self::FineKnockLearn, [value]) => Some(*value as i8 as f64 * 0.3515625),
            (Self::FuelPressure, [hi, lo]) => Some(u16::from_be_bytes([*hi, *lo]) as f64 * 0.10),
            (Self::InjectorDuty, [a, b, c]) => {
                let raw = ((*a as u32) << 16 | (*b as u32) << 8 | *c as u32) as f64;
                Some(raw * 0.000000006297 - 0.5321)
            }
            (Self::CalculatedLoad, [value]) => Some(*value as f64 * 0.01294),
            (Self::IntakeManifoldTemperature, [value]) => Some(*value as f64 * 1.8 - 57.8),
            (Self::VehicleSpeed, [value]) => Some(*value as f64 * 0.621371192),
            (Self::IgnitionTiming, [value]) => Some(*value as f64 * 0.5 - 64.0),
            (Self::WastegateDutyCycle, [value]) => Some(*value as f64 * 0.039092),
            (Self::EngineSpeed, [hi, lo]) => Some(u16::from_be_bytes([*hi, *lo]) as f64 * 0.2),
            (Self::InjectorPulseWidth, [hi, lo]) => {
                Some(u16::from_be_bytes([*hi, *lo]) as f64 * 0.0034 - 0.0789)
            }
            (Self::AtmosphericPressure, [value]) => Some(*value as f64 * 0.1451),
            (Self::ManifoldAbsolutePressure, [hi, lo]) => {
                Some(6451.6129 / u16::from_be_bytes([*hi, *lo]) as f64)
            }
            (Self::Odometer, [hi, lo]) => Some(u16::from_be_bytes([*hi, *lo]) as f64 * 1.242742384),
            (Self::MafCorrFinal, [a, b, c, d]) => {
                Some(f32::from_le_bytes([*a, *b, *c, *d]) as f64 * 0.01667)
            }
            _ => None,
        }
    }
}
