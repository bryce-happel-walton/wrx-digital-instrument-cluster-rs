use serde::Serialize;

use crate::data::parameters::Parameter;
#[cfg(feature = "apalis_imx8")]
use crate::hardware::apalis_imx8;

use std::sync::{Arc, LazyLock};

pub(crate) static HARDWARE_NAVIGATION_INPUT: LazyLock<Arc<Parameter<HardwareNavigationState>>> =
    crate::LazyLockDefault!();

#[derive(Default)]
pub enum Backend {
    #[cfg(feature = "apalis_imx8")]
    ApalisIMX8(apalis_imx8::ApalisIMX8),
    #[default]
    Simulator,
}

#[derive(Clone, Copy, Default, Serialize, PartialEq, Debug)]
pub enum HardwareNavigationState {
    Forward,
    Backward,
    Enter,
    #[default]
    Idle,
}

#[derive(Default)]
pub struct HardwareBackend {
    backend: Backend,
    pub dbg_adc: Arc<Parameter<u32>>,
}

impl HardwareBackend {
    pub fn new(backend: Backend) -> Self {
        match &backend {
            #[cfg(feature = "apalis_imx8")]
            Backend::ApalisIMX8(apalis_imx8) => {
                use crate::hardware::apalis_imx8::ApalisIMX8ADC;

                apalis_imx8.register_adc_reader(ApalisIMX8ADC::ADC0);

                let adc0: Arc<Parameter<u32>> = apalis_imx8.get_adc_param(ApalisIMX8ADC::ADC0);

                {
                    const IDLE_RANGE: u32 = 1000;
                    const DOWN_RANGE: u32 = 900;
                    const ENTER_RANGE: u32 = 500;
                    const _UP_RANGE: u32 = 0;

                    let adc0 = adc0.clone();
                    tokio::spawn(async move {
                        let mut adc0_watch = adc0.watch();
                        loop {
                            use tokio::select;

                            let value = adc0.value();
                            if value >= IDLE_RANGE {
                                HARDWARE_NAVIGATION_INPUT.set_value(HardwareNavigationState::Idle);
                            } else if (value < IDLE_RANGE) && (value >= DOWN_RANGE) {
                                HARDWARE_NAVIGATION_INPUT
                                    .set_value(HardwareNavigationState::Forward);
                            } else if (value < DOWN_RANGE) && (value >= ENTER_RANGE) {
                                HARDWARE_NAVIGATION_INPUT.set_value(HardwareNavigationState::Enter);
                            } else {
                                HARDWARE_NAVIGATION_INPUT
                                    .set_value(HardwareNavigationState::Backward);
                            }

                            select! {
                                Ok(_) = adc0_watch.changed() => {},
                                else => {break;}
                            }
                        }
                    });
                }

                Self {
                    backend,
                    dbg_adc: adc0,
                    ..Default::default()
                }
            }
            Backend::Simulator => Self {
                backend,
                ..Default::default()
            },
        }
    }

    pub fn power_suspend(&self) {
        match &self.backend {
            #[cfg(feature = "apalis_imx8")]
            Backend::ApalisIMX8(_device) => {
                println!("Power suspend not yet implemented!");
                // device.power_suspend();
            }
            _ => println!("Simulator does not support `power_suspend`"),
        }
    }
}
