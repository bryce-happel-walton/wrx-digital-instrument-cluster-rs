use crate::application::settings::SETTINGS;
use crate::data::car_data::CAR_DATA;
use crate::slint_generatedApp::{App, BevyCarDisplayGlobal};
use crate::slint_ui::backend::bevy_adapter::slint_bevy_adapter;

use bevy::prelude::*;
use slint::ComponentHandle;
use tokio::time::{self, Duration, Instant};

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

const CAMERA_START_HEIGHT: f32 = 1.2;
const CAMERA_END_HEIGHT: f32 = 2.0;
const CAMERA_START_RADIUS: f32 = 4.0;
const CAMERA_END_RADIUS: f32 = 6.0;
const CAMERA_START_LOOK_AT: Vec3 = Vec3::ZERO;
const CAMERA_END_LOOK_AT: Vec3 = Vec3::new(0.0, 0.75, 6.0);
const ORBIT_DURATION_SECS: f32 = 2.25;
const ORBIT_HOLD_SECS: f32 = 0.5;

pub fn make_app() -> Result<App, Box<dyn std::error::Error>> {
    let mut wgpu_settings = slint::wgpu_27::WGPUSettings::default();
    wgpu_settings.device_required_limits = slint::wgpu_27::wgpu::Limits::default()
        .using_resolution(slint::wgpu_27::wgpu::Limits::downlevel_defaults());
    slint::BackendSelector::new()
        .require_wgpu_27(slint::wgpu_27::WGPUConfiguration::Automatic(wgpu_settings))
        .select()?;
    let app_window = App::new().unwrap();

    let bevy_channels: Rc<
        RefCell<
            Option<(
                crossbeam::channel::Receiver<slint::wgpu_27::wgpu::Texture>,
                crossbeam::channel::Sender<slint_bevy_adapter::ControlMessage>,
            )>,
        >,
    > = Rc::new(RefCell::new(None));
    let frames: Arc<AtomicU32> = Default::default();

    let app_weak = app_window.as_weak();
    let bevy_channels_setup = bevy_channels.clone();
    let restart_animation: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    let last_focused = Rc::new(Cell::new(false));

    {
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

                    frames.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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

    {
        let frames = frames.clone();
        tokio::spawn(async move {
            let mut last = Instant::now();
            let mut interval = time::interval(Duration::from_millis(100));

            loop {
                interval.tick().await;
                let secs = last.elapsed().as_secs_f32();

                SETTINGS.developer.system_info.fps.set_value(
                    ((frames.swap(0, std::sync::atomic::Ordering::Relaxed) as f32) / secs) as i32,
                );

                last = Instant::now();
            }
        });
    }
    app_window.show()?;

    Ok(app_window)
}

#[derive(Resource, Clone)]
struct AnimationControl {
    restart: Arc<AtomicBool>,
}

#[derive(Component)]
struct Hood;

#[derive(Component)]
struct Trunk;

#[derive(Component)]
struct LeftFrontDoor;

#[derive(Component)]
struct RightFrontDoor;

#[derive(Component)]
struct LeftRearDoor;

#[derive(Component)]
struct RightRearDoor;

#[derive(Component, Default)]
struct OpenAngle(f32);

#[derive(Component)]
struct Headlight;

#[derive(Component)]
struct Taillight;

#[derive(Component)]
struct LeftBlinker;

#[derive(Component)]
struct RightBlinker;

fn spawn_block(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    size: Vec3,
    transform: Transform,
    color: Color,
) -> Entity {
    commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::from_size(size))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                ..default()
            })),
            transform,
        ))
        .id()
}

fn spawn_lamp<M: Component>(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    size: Vec3,
    transform: Transform,
    base_color: Color,
    marker: M,
) {
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::from_size(size))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color,
            ..default()
        })),
        transform,
        marker,
    ));
}

fn spawn_hinged_block<M: Component>(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    size: Vec3,
    hinge: Vec3,
    block_offset: Vec3,
    color: Color,
    marker: M,
) -> Entity {
    commands
        .spawn((
            Transform::from_translation(hinge),
            Visibility::default(),
            marker,
            OpenAngle::default(),
        ))
        .with_children(|p| {
            p.spawn((
                Mesh3d(meshes.add(Cuboid::from_size(size))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: color,
                    ..default()
                })),
                Transform::from_translation(block_offset),
            ));
        })
        .id()
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            ..default()
        },
        Transform::from_xyz(3.0, 6.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 6_000.0,
            ..default()
        },
        Transform::from_xyz(-4.0, 4.0, -3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, CAMERA_START_HEIGHT, CAMERA_START_RADIUS)
            .looking_at(Vec3::ZERO, Vec3::Y),
        AmbientLight {
            color: Color::WHITE,
            brightness: 1500.0,
            ..default()
        },
    ));

    let body_color = Color::srgb(0.85, 0.1, 0.15);
    let cabin_color = Color::srgb(0.2, 0.25, 0.35);
    let wheel_color = Color::srgb(0.05, 0.05, 0.05);

    let chassis_len = 4.0;
    let cabin_len = 2.0;
    let hood_len = 1.0;
    let trunk_len = 0.7;

    let body_y_top = 0.8;
    let cabin_y_center = body_y_top + 0.3;

    spawn_block(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::new(1.6, 0.5, chassis_len),
        Transform::from_xyz(0.0, 0.55, 0.0),
        body_color,
    );

    spawn_block(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::new(1.4, 0.6, cabin_len),
        Transform::from_xyz(0.0, cabin_y_center, 0.0),
        cabin_color,
    );

    spawn_hinged_block(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::new(1.6, 0.1, hood_len),
        Vec3::new(0.0, body_y_top + 0.05, cabin_len * 0.5),
        Vec3::new(0.0, 0.0, hood_len * 0.5),
        body_color,
        Hood,
    );

    spawn_hinged_block(
        &mut commands,
        &mut meshes,
        &mut materials,
        Vec3::new(1.6, 0.1, trunk_len),
        Vec3::new(0.0, body_y_top + 0.05, -cabin_len * 0.5),
        Vec3::new(0.0, 0.0, -trunk_len * 0.5),
        body_color,
        Trunk,
    );

    let door_height = 1.0;
    let door_panel = Vec3::new(0.05, door_height, cabin_len * 0.5);
    let door_x = 0.825;
    let door_y = 0.9;
    let front_hinge_z = cabin_len * 0.5;
    let rear_hinge_z = 0.0;
    let door_offset = Vec3::new(0.0, 0.0, -cabin_len * 0.25);

    spawn_hinged_block(
        &mut commands,
        &mut meshes,
        &mut materials,
        door_panel,
        Vec3::new(door_x, door_y, front_hinge_z),
        door_offset,
        body_color,
        LeftFrontDoor,
    );

    spawn_hinged_block(
        &mut commands,
        &mut meshes,
        &mut materials,
        door_panel,
        Vec3::new(-door_x, door_y, front_hinge_z),
        door_offset,
        body_color,
        RightFrontDoor,
    );

    spawn_hinged_block(
        &mut commands,
        &mut meshes,
        &mut materials,
        door_panel,
        Vec3::new(door_x, door_y, rear_hinge_z),
        door_offset,
        body_color,
        LeftRearDoor,
    );

    spawn_hinged_block(
        &mut commands,
        &mut meshes,
        &mut materials,
        door_panel,
        Vec3::new(-door_x, door_y, rear_hinge_z),
        door_offset,
        body_color,
        RightRearDoor,
    );

    let chassis_front = chassis_len * 0.5;
    let chassis_rear = -chassis_len * 0.5;
    let lamp_y = 0.6;
    let lamp_size = Vec3::new(0.18, 0.14, 0.08);
    let blinker_size = Vec3::new(0.12, 0.12, 0.08);
    let main_x = 0.5;
    let blinker_x = 0.72;
    let lamp_protrude = 0.04;

    spawn_lamp(
        &mut commands,
        &mut meshes,
        &mut materials,
        lamp_size,
        Transform::from_xyz(-main_x, lamp_y, chassis_front + lamp_protrude),
        Color::srgb(0.9, 0.9, 0.85),
        Headlight,
    );

    spawn_lamp(
        &mut commands,
        &mut meshes,
        &mut materials,
        lamp_size,
        Transform::from_xyz(main_x, lamp_y, chassis_front + lamp_protrude),
        Color::srgb(0.9, 0.9, 0.85),
        Headlight,
    );

    spawn_lamp(
        &mut commands,
        &mut meshes,
        &mut materials,
        lamp_size,
        Transform::from_xyz(-main_x, lamp_y, chassis_rear - lamp_protrude),
        Color::srgb(0.3, 0.0, 0.0),
        Taillight,
    );

    spawn_lamp(
        &mut commands,
        &mut meshes,
        &mut materials,
        lamp_size,
        Transform::from_xyz(main_x, lamp_y, chassis_rear - lamp_protrude),
        Color::srgb(0.3, 0.0, 0.0),
        Taillight,
    );

    spawn_lamp(
        &mut commands,
        &mut meshes,
        &mut materials,
        blinker_size,
        Transform::from_xyz(blinker_x, lamp_y, chassis_front + lamp_protrude),
        Color::srgb(0.4, 0.2, 0.0),
        LeftBlinker,
    );

    spawn_lamp(
        &mut commands,
        &mut meshes,
        &mut materials,
        blinker_size,
        Transform::from_xyz(-blinker_x, lamp_y, chassis_front + lamp_protrude),
        Color::srgb(0.4, 0.2, 0.0),
        RightBlinker,
    );

    spawn_lamp(
        &mut commands,
        &mut meshes,
        &mut materials,
        blinker_size,
        Transform::from_xyz(blinker_x, lamp_y, chassis_rear - lamp_protrude),
        Color::srgb(0.4, 0.2, 0.0),
        LeftBlinker,
    );

    spawn_lamp(
        &mut commands,
        &mut meshes,
        &mut materials,
        blinker_size,
        Transform::from_xyz(-blinker_x, lamp_y, chassis_rear - lamp_protrude),
        Color::srgb(0.4, 0.2, 0.0),
        RightBlinker,
    );

    let wheel_size = Vec3::new(0.4, 0.4, 0.4);
    for &(x, z) in &[(-0.85, 1.3), (0.85, 1.3), (-0.85, -1.3), (0.85, -1.3)] {
        spawn_block(
            &mut commands,
            &mut meshes,
            &mut materials,
            wheel_size,
            Transform::from_xyz(x, 0.2, z),
            wheel_color,
        );
    }
}

const OPEN_ANGLE_MAX: f32 = std::f32::consts::FRAC_PI_3;
const OPEN_LERP_RATE: f32 = 6.0;

fn lerp_open_angle(current: &mut f32, target_open: bool, dt: f32) -> f32 {
    let target = if target_open { OPEN_ANGLE_MAX } else { 0.0 };
    let alpha = 1.0 - ops::exp(-OPEN_LERP_RATE * dt);
    *current += (target - *current) * alpha;
    *current
}

fn animate_openings(
    time: Res<Time>,
    mut hood: Query<(&mut Transform, &mut OpenAngle), With<Hood>>,
    mut trunk: Query<(&mut Transform, &mut OpenAngle), (With<Trunk>, Without<Hood>)>,
    mut left_front: Query<
        (&mut Transform, &mut OpenAngle),
        (With<LeftFrontDoor>, Without<Hood>, Without<Trunk>),
    >,
    mut right_front: Query<
        (&mut Transform, &mut OpenAngle),
        (
            With<RightFrontDoor>,
            Without<Hood>,
            Without<Trunk>,
            Without<LeftFrontDoor>,
        ),
    >,
    mut left_rear: Query<
        (&mut Transform, &mut OpenAngle),
        (
            With<LeftRearDoor>,
            Without<Hood>,
            Without<Trunk>,
            Without<LeftFrontDoor>,
            Without<RightFrontDoor>,
        ),
    >,
    mut right_rear: Query<
        (&mut Transform, &mut OpenAngle),
        (
            With<RightRearDoor>,
            Without<Hood>,
            Without<Trunk>,
            Without<LeftFrontDoor>,
            Without<RightFrontDoor>,
            Without<LeftRearDoor>,
        ),
    >,
) {
    let dt = time.delta_secs();

    for (mut t, mut a) in &mut hood {
        let angle = lerp_open_angle(&mut a.0, CAR_DATA.hood_open().value(), dt);
        t.rotation = Quat::from_rotation_x(-angle);
    }
    for (mut t, mut a) in &mut trunk {
        let angle = lerp_open_angle(&mut a.0, CAR_DATA.trunk_open().value(), dt);
        t.rotation = Quat::from_rotation_x(angle);
    }
    for (mut t, mut a) in &mut left_front {
        let angle = lerp_open_angle(&mut a.0, CAR_DATA.left_front_door_open().value(), dt);
        t.rotation = Quat::from_rotation_y(-angle);
    }
    for (mut t, mut a) in &mut right_front {
        let angle = lerp_open_angle(&mut a.0, CAR_DATA.right_front_door_open().value(), dt);
        t.rotation = Quat::from_rotation_y(angle);
    }
    for (mut t, mut a) in &mut left_rear {
        let angle = lerp_open_angle(&mut a.0, CAR_DATA.left_rear_door_open().value(), dt);
        t.rotation = Quat::from_rotation_y(-angle);
    }
    for (mut t, mut a) in &mut right_rear {
        let angle = lerp_open_angle(&mut a.0, CAR_DATA.right_rear_door_open().value(), dt);
        t.rotation = Quat::from_rotation_y(angle);
    }
}

fn animate_lights(
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    headlights: Query<&MeshMaterial3d<StandardMaterial>, With<Headlight>>,
    taillights: Query<&MeshMaterial3d<StandardMaterial>, (With<Taillight>, Without<Headlight>)>,
    left_blinkers: Query<
        &MeshMaterial3d<StandardMaterial>,
        (With<LeftBlinker>, Without<Headlight>, Without<Taillight>),
    >,
    right_blinkers: Query<
        &MeshMaterial3d<StandardMaterial>,
        (
            With<RightBlinker>,
            Without<Headlight>,
            Without<Taillight>,
            Without<LeftBlinker>,
        ),
    >,
) {
    let head_on = CAR_DATA.lowbeams_enabled().value()
        || CAR_DATA.highbeams_enabled().value()
        || CAR_DATA.parking_lights_enabled().value();
    let high_beams = CAR_DATA.highbeams_enabled().value();
    let blink_on = ops::sin(time.elapsed_secs() * std::f32::consts::TAU * 1.5) > 0.0;
    let left_on = CAR_DATA.left_turn_signal_enabled().value() && blink_on;
    let right_on = CAR_DATA.right_turn_signal_enabled().value() && blink_on;

    let head_emissive = if high_beams {
        LinearRgba::rgb(12.0, 12.0, 10.0)
    } else if head_on {
        LinearRgba::rgb(6.0, 6.0, 5.0)
    } else {
        LinearRgba::BLACK
    };
    let tail_emissive = if head_on {
        LinearRgba::rgb(4.0, 0.0, 0.0)
    } else {
        LinearRgba::BLACK
    };
    let amber = LinearRgba::rgb(8.0, 3.0, 0.0);

    for h in &headlights {
        if let Some(mat) = materials.get_mut(&h.0) {
            mat.emissive = head_emissive;
        }
    }
    for h in &taillights {
        if let Some(mat) = materials.get_mut(&h.0) {
            mat.emissive = tail_emissive;
        }
    }
    for h in &left_blinkers {
        if let Some(mat) = materials.get_mut(&h.0) {
            mat.emissive = if left_on { amber } else { LinearRgba::BLACK };
        }
    }
    for h in &right_blinkers {
        if let Some(mat) = materials.get_mut(&h.0) {
            mat.emissive = if right_on { amber } else { LinearRgba::BLACK };
        }
    }
}

fn animate_camera(
    mut cameras: Query<&mut Transform, With<Camera3d>>,
    time: Res<Time>,
    control: Res<AnimationControl>,
    mut start_secs: Local<f32>,
) {
    if control.restart.swap(false, Ordering::Relaxed) {
        *start_secs = time.elapsed_secs();
    }
    let elapsed = (time.elapsed_secs() - *start_secs - ORBIT_HOLD_SECS).max(0.0);
    let raw = (elapsed / ORBIT_DURATION_SECS).clamp(0.0, 1.0);
    let t = raw * raw * (3.0 - 2.0 * raw);
    let theta = t * std::f32::consts::PI;
    let (sin, cos) = ops::sin_cos(theta);
    let radius = CAMERA_START_RADIUS + (CAMERA_END_RADIUS - CAMERA_START_RADIUS) * t;
    let height = CAMERA_START_HEIGHT + (CAMERA_END_HEIGHT - CAMERA_START_HEIGHT) * t;

    let look_at = CAMERA_START_LOOK_AT.lerp(CAMERA_END_LOOK_AT, t);

    for mut transform in cameras.iter_mut() {
        transform.translation = Vec3::new(sin * radius, height, cos * radius);
        transform.look_at(look_at, Vec3::Y);
    }
}
