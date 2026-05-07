use crate::application::settings::SETTINGS;
use crate::slint_generatedApp::{App as AppWindow, BevyCarDisplayGlobal};
#[cfg(feature = "bevy")]
use crate::slint_ui::backend::bevy::{bevy_widget::*, slint_bevy_adapter};

use slint::ComponentHandle;
use tokio::time::{self, Duration, Instant};

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub fn make_app() -> Result<AppWindow, Box<dyn std::error::Error>> {
    let app_window;
    let frames: Arc<AtomicU32> = Default::default();

    #[cfg(feature = "bevy")]
    {
        use bevy::prelude::*;

        let mut wgpu_settings = slint::wgpu_27::WGPUSettings::default();
        wgpu_settings.device_required_limits = slint::wgpu_27::wgpu::Limits::downlevel_defaults();

        slint::BackendSelector::new()
            .require_wgpu_27(slint::wgpu_27::WGPUConfiguration::Automatic(wgpu_settings))
            .select()?;
        app_window = AppWindow::new().unwrap();
        let app_weak = app_window.as_weak();

        let bevy_channels: Rc<
            RefCell<
                Option<(
                    crossbeam::channel::Receiver<slint::wgpu_27::wgpu::Texture>,
                    crossbeam::channel::Sender<slint_bevy_adapter::ControlMessage>,
                )>,
            >,
        > = Rc::new(RefCell::new(None));

        let bevy_channels_setup = bevy_channels.clone();
        let restart_animation: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
        let last_focused = Rc::new(Cell::new(false));

        let frames = frames.clone();
        let restart_animation_setup = restart_animation.clone();
        let restart_animation_frame = restart_animation.clone();
        let last_focused = last_focused.clone();
        app_window
            .window()
            .set_rendering_notifier(move |state, graphics_api| match state {
                slint::RenderingState::RenderingSetup => {
                    let slint::GraphicsAPI::WGPU27 {
                        instance,
                        device,
                        queue,
                        ..
                    } = graphics_api
                    else {
                        eprintln!("RenderingSetup: not a WGPU27 backend, skipping");
                        return;
                    };

                    let restart_animation = restart_animation_setup.clone();
                    let channels = slint_bevy_adapter::run_bevy_app_with_slint(
                        instance.clone(),
                        device.clone(),
                        queue.clone(),
                        |_app| {},
                        move |mut app| {
                            app.insert_resource(AnimationControl {
                                restart: restart_animation.clone(),
                            })
                            .add_systems(Startup, setup)
                            .add_systems(Update, (animate_camera, animate_openings, animate_lights))
                            .insert_resource(ClearColor(Color::NONE))
                            .run();
                        },
                    );

                    *bevy_channels_setup.borrow_mut() = Some(channels);
                }
                slint::RenderingState::BeforeRendering => {
                    let Some(app) = app_weak.upgrade() else {
                        return;
                    };

                    frames.fetch_add(1, Ordering::Relaxed);
                    app.window().request_redraw();

                    let bevy_global = app.global::<BevyCarDisplayGlobal>();
                    let focused = bevy_global.get_bevy_widget_focused();
                    if focused && !last_focused.get() {
                        restart_animation_frame.store(true, Ordering::Relaxed);
                    }
                    last_focused.set(focused);

                    let channels = bevy_channels_setup.borrow();
                    let Some((new_texture_receiver, control_message_sender)) = channels.as_ref()
                    else {
                        return;
                    };

                    let Ok(new_texture) = new_texture_receiver.try_recv() else {
                        return;
                    };

                    if let Some(old_texture) = bevy_global.get_texture().to_wgpu_27_texture() {
                        let _ = control_message_sender.try_send(
                            slint_bevy_adapter::ControlMessage::ReleaseFrontBufferTexture {
                                texture: old_texture,
                            },
                        );
                    }

                    let requested_width = bevy_global.get_requested_texture_width().round() as u32;
                    let requested_height =
                        bevy_global.get_requested_texture_height().round() as u32;
                    if requested_width > 0 && requested_height > 0 {
                        let _ = control_message_sender.try_send(
                            slint_bevy_adapter::ControlMessage::ResizeBuffers {
                                width: requested_width,
                                height: requested_height,
                            },
                        );
                    }

                    if let Ok(image) = new_texture.try_into() {
                        bevy_global.set_texture(image);
                    }
                }
                _ => {}
            })
            .unwrap_or_else(|e| eprintln!("set_rendering_notifier failed: {e:?}"));
    }
    #[cfg(not(feature = "bevy"))]
    {
        app_window = AppWindow::new().unwrap();

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
