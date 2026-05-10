use crate::application::settings::SETTINGS;
use crate::slint_generatedApp::App;

use slint::ComponentHandle;
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

    app_window.show()?;
    Ok(app_window)
}
