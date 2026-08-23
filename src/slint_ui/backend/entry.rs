use crate::application::settings::SETTINGS;
use crate::hardware::hardware_backend::HardwareBackend;
use crate::slint_generatedApp::App;

use slint::{ComponentHandle, Weak};
use tokio::time::{self, Duration, Instant};

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

pub fn make_app() -> Result<App, Box<dyn std::error::Error>> {
    let app_window;
    let frames: Arc<AtomicU32> = Default::default();

    {
        app_window = App::new().unwrap();

        let frames = frames.clone();
        app_window
            .window()
            .set_rendering_notifier(move |state, _| match state {
                slint::RenderingState::BeforeRendering => {
                    frames.fetch_add(1, Ordering::Relaxed);
                }
                _ => {}
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

    Ok(app_window)
}

pub fn bridge(weak_handle: Weak<App>, hardware_backend: Arc<HardwareBackend>) {
    super::rs_type_resolver::bridge(weak_handle.clone());
    super::settings_bridge::bridge(weak_handle.clone());
    super::car_data::wrx_2018_can::bridge(weak_handle.clone());
    super::acquisition_bridge::bridge(weak_handle.clone());
    super::hardware_bridge::bridge(weak_handle.clone(), hardware_backend.clone());
    super::lang::bridge(weak_handle.clone());
    super::backend_lib::bridge(weak_handle.clone());
    super::state_bridge::bridge(weak_handle.clone());
}
