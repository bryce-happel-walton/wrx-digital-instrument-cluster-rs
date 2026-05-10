use crate::hardware::hardware_backend::{
    HARDWARE_NAVIGATION_INPUT, HardwareBackend, HardwareNavigationState,
};
use crate::slint_generatedApp::{App, DebugMenuState, HardwareBackendData};

use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle, Weak};
use tokio::select;

use std::sync::Arc;

// TODO: add ui debug for testing hardware backend interactions

pub fn bridge(ui: Weak<App>, backend: Arc<HardwareBackend>) {
    if let Some(ui) = ui.upgrade() {
        {
            let ui = ui.clone_strong();
            let backend = backend.clone();
            let debug_menu_state = ui.global::<DebugMenuState>();
            debug_menu_state.on_debug_suspend(move || backend.power_suspend());
        }

        let nav_handle = ui.clone_strong();
        let dispatch_navigation = move |state: HardwareNavigationState| match state {
            HardwareNavigationState::Backward => {
                nav_handle.window().dispatch_event(WindowEvent::KeyPressed {
                    text: Key::Backtab.into(),
                });
            }
            HardwareNavigationState::Enter => {
                nav_handle.window().dispatch_event(WindowEvent::KeyPressed {
                    text: Key::Space.into(),
                });
            }
            HardwareNavigationState::Forward => {
                nav_handle.window().dispatch_event(WindowEvent::KeyPressed {
                    text: Key::Tab.into(),
                });
            }
            HardwareNavigationState::Idle => {
                nav_handle
                    .window()
                    .dispatch_event(WindowEvent::KeyReleased {
                        text: Key::Tab.into(),
                    });
                nav_handle
                    .window()
                    .dispatch_event(WindowEvent::KeyReleased {
                        text: Key::Backtab.into(),
                    });
                nav_handle
                    .window()
                    .dispatch_event(WindowEvent::KeyReleased {
                        text: Key::Space.into(),
                    });
            }
        };

        {
            let ui = ui.clone_strong();
            let _ = slint::spawn_local(async_compat::Compat::new(async move {
                let hardware_backend_data = ui.global::<HardwareBackendData>();
                let mut navigation_state = HARDWARE_NAVIGATION_INPUT.watch();
                loop {
                    let value = *navigation_state.borrow_and_update();
                    hardware_backend_data.set_navigation_state(format!("{value:?}").into());

                    select! {
                        Ok(_) = navigation_state.changed() => {
                            if value != HardwareNavigationState::Idle {
                                dispatch_navigation(HardwareNavigationState::Idle);
                            }

                            dispatch_navigation(value);
                        },
                        else => {
                            break;
                        },
                    };
                }
            }));
        }

        {
            let ui = ui.clone_strong();
            let backend = backend.clone();
            let _ = slint::spawn_local(async_compat::Compat::new(async move {
                let hardware_backend_data = ui.global::<HardwareBackendData>();
                let mut adc0_watch = backend.dbg_adc.watch();
                loop {
                    use tokio::select;

                    let value = *adc0_watch.borrow_and_update();
                    hardware_backend_data.set_adc_val(value as i32);

                    select! {
                        Ok(_) = adc0_watch.changed() => {},
                        else => {break;}
                    }
                }
            }));
        }
    }
}
