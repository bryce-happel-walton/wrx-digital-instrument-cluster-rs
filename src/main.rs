mod application;
mod can;
mod data;
mod debug;
mod hardware;
mod slint_ui;
mod util;

use crate::application::global_state::GLOBAL_STATE;
use crate::application::settings::SETTINGS;
use crate::can::acquisition::Acquisition;
use crate::can::bus::CanBus;
use crate::can::can_backend::{CanBackend, CanInterface};
use crate::can::emulators::wrx_2018_emulator;
use crate::can::protocol::isotp::FUNCTIONAL_REQUEST_ID;
use crate::can::protocol::isotp::parser::obd2::{OBD2Service, S1CurrentData, S9VehicleInformation};
use crate::data::car::wrx_2018::CAN_DATA;
use crate::data::obd2_data::OBD_DATA;
use crate::debug::debug_entry;
use crate::hardware::hardware_backend::{self, HardwareBackend};
use crate::slint_ui::backend::{can_display, entry};

use embedded_can::{Id, StandardId};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, Pid, ProcessRefreshKind, RefreshKind, System};
use tokio::signal;
use tokio::sync::Notify;
use tokio::time::{self, Duration};

use std::env;
use std::sync::{Arc, LazyLock};

slint::include_modules!();

pub(crate) static SHUTDOWN_SIGNAL: LazyLock<Notify> = LazyLock::new(|| Notify::new());
pub(crate) static SHUTDOWN_FINISHED: LazyLock<Notify> = LazyLock::new(|| Notify::new());

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = tokio_runtime.enter();

    let ui = entry::make_app().unwrap();

    let can_backend = match CanBackend::new(
        &GLOBAL_STATE.can.selected_interface.value(),
        &GLOBAL_STATE.can.interface_path.value(),
    ) {
        Ok(can_backend) => Some(can_backend),
        Err(e) => {
            eprintln!("Error in can backend: {e:?}");
            GLOBAL_STATE.can.interface_path.set_value("err".to_string());
            None
        }
    };

    if let Some(can_backend) = can_backend {
        let mut bus = CanBus::new(GLOBAL_STATE.can.interface_path.value(), can_backend);

        let obd_functional: Id = StandardId::new(FUNCTIONAL_REQUEST_ID as u16)
            .unwrap()
            .into();
        for acq in [
            Acquisition::from_isotp(
                obd_functional,
                &OBD2Service::VehicleInformation,
                &[
                    S9VehicleInformation::PIDs,
                    S9VehicleInformation::VIN,
                    S9VehicleInformation::ECU,
                ],
            ),
            Acquisition::from_isotp(
                obd_functional,
                &OBD2Service::CurrentData,
                &[
                    S1CurrentData::PIDs1,
                    S1CurrentData::PIDs2,
                    // S1CurrentData::PIDs3,
                ],
            ),
        ]
        .into_iter()
        .flatten()
        {
            bus.acquire(acq);
        }

        bus.on_acquisitions_complete(|| {
            OBD_DATA.acquired.set_value(true);
        });

        let powertrain: Id = StandardId::new(0x7E0).unwrap().into();
        for pid in [
            S1CurrentData::EngineLoad as u8,
            S1CurrentData::EngineSpeed as u8,
            S1CurrentData::ControlModuleVoltage as u8,
            S1CurrentData::EngineFuelRate as u8,
            S1CurrentData::Odometer as u8,
        ] {
            bus.poll(
                powertrain,
                vec![OBD2Service::CurrentData as u8, pid],
                Duration::from_millis(100),
            );
        }

        let weak_ui = ui.as_weak();
        tokio::spawn(async move {
            let running_can = &SETTINGS.developer.can.running_can;

            loop {
                if running_can.value() {
                    if let Some(frame) = bus.process_frame() {
                        can_display::update(weak_ui.clone(), &frame, false);
                    }

                    tokio::task::yield_now().await;
                } else {
                    let _ = running_can.watch().wait_for(|v| *v).await;
                }
            }
        });

        if GLOBAL_STATE.bin_args.virtual_cluster.value() {
            let mut backend = CanBackend::new(
                &GLOBAL_STATE.can.selected_interface.value(),
                &GLOBAL_STATE.can.interface_path.value(),
            )
            .unwrap();

            if !matches!(
                &GLOBAL_STATE.can.selected_interface.value(),
                CanInterface::Fake | CanInterface::VirtualSocketCan
            ) {
                panic!("Do not run the can generator on a real socket!")
            }

            tokio::spawn(async move {
                let mut interval = time::interval(Duration::from_millis(10));

                let running_vcan = &SETTINGS.developer.simulation.running_simulation;
                let mut simulation_speed =
                    SETTINGS.developer.simulation.simulation_speed_ms.watch();

                loop {
                    let gen_frames = wrx_2018_emulator::generate_frames();

                    for frame in gen_frames {
                        if running_vcan.value() {
                            match backend.write_frame(frame) {
                                Ok(_) => crate::util::performance::record_can_frame_sent(),
                                Err(e) => {
                                    crate::util::performance::record_can_frame_lost();
                                    eprintln!("Failed to write simulated frame: {e:?}");
                                }
                            }
                        } else {
                            let _ = running_vcan.watch().wait_for(|v| *v).await;
                        }
                    }

                    if simulation_speed.has_changed().is_ok_and(|val| val) {
                        let value = simulation_speed.borrow_and_update().value() as u64;
                        interval = time::interval(Duration::from_millis(value));
                    }

                    interval.tick().await;
                }
            });

            #[cfg(debug_assertions)]
            tokio::spawn(bridge_performance_metrics());
        }
    }

    #[cfg(feature = "apalis_imx8")]
    let device = hardware::apalis_imx8::ApalisIMX8::new();
    #[cfg(feature = "apalis_imx8")]
    let hardware_backend = Arc::new(HardwareBackend::new(hardware_backend::Backend::ApalisIMX8(
        device,
    )));
    #[cfg(not(feature = "apalis_imx8"))]
    let hardware_backend = Arc::new(HardwareBackend::new(hardware_backend::Backend::Simulator));

    tokio::spawn(run_autosave_loop());
    tokio::spawn(bridge_system_info());

    // ui backend bridges
    entry::bridge(ui.as_weak(), hardware_backend.clone());

    // graceful shutdown
    {
        tokio::spawn(async {
            match signal::ctrl_c().await {
                Ok(_) => SHUTDOWN_SIGNAL.notify_one(),
                Err(e) => panic!("Failed to forward shutdown signal: {e:?}"),
            }
        });

        tokio::spawn(async {
            SHUTDOWN_SIGNAL.notified().await;
            shutdown();
        });
    }

    debug_entry::debug_entry();

    ui.run()?;

    // send shutdown after ui has closed
    SHUTDOWN_SIGNAL.notify_one();

    tokio_runtime.block_on(async { SHUTDOWN_FINISHED.notified().await });
    tokio_runtime.shutdown_background();

    Ok(())
}

fn save_config() {
    if let Err(e) = SETTINGS.save_to_fs() {
        eprintln!("Failed to save user settings: {e:?}");
    }
}

fn shutdown() {
    if let Err(e) = slint::quit_event_loop() {
        panic!("Failed to quit Slint event loop: {e:?}");
    }

    save_config();

    SHUTDOWN_FINISHED.notify_one();
}

#[cfg(debug_assertions)]
async fn bridge_performance_metrics() {
    let mut last = time::Instant::now();
    let mut interval = time::interval(Duration::from_millis(500));

    loop {
        interval.tick().await;
        let secs = last.elapsed().as_secs_f32();
        last = time::Instant::now();

        let sample = crate::util::performance::sample(secs);

        let performance = &SETTINGS.developer.performance;
        performance
            .can_frames_sent_per_sec
            .set_value(sample.sent_per_sec);
        performance
            .can_frames_parsed_per_sec
            .set_value(sample.parsed_per_sec);
        performance
            .can_frame_parse_to_ui_latency_us
            .set_value(sample.ui_latency_avg_us);
        performance
            .can_frame_parse_to_ui_latency_max_us
            .set_value(sample.ui_latency_max_us);
        performance
            .can_frames_lost_total
            .set_value(sample.total_frames_lost);
    }
}

async fn run_autosave_loop() {
    let mut interval = time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        save_config();
    }
}

async fn bridge_system_info() {
    let pid = Pid::from_u32(std::process::id());

    let mut sys = System::new_all();
    sys.refresh_all();

    let mut process_memory_max = 0;

    let sys_refresh = RefreshKind::nothing()
        .with_memory(MemoryRefreshKind::nothing().with_ram())
        .with_cpu(CpuRefreshKind::nothing().with_cpu_usage())
        .with_processes(ProcessRefreshKind::nothing().with_memory());

    SETTINGS
        .developer
        .system_info
        .num_cpus
        .set_value(sys.cpus().len() as i32);

    SETTINGS
        .developer
        .system_info
        .total_memory_mb
        .set_value((sys.total_memory() / 1_048_576) as i32);

    let mut interval = time::interval(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    loop {
        sys.refresh_specifics(sys_refresh);
        sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), false);

        let used_memory = (sys.used_memory() / 1_048_576) as i32;

        if let Some(this) = sys.process(pid) {
            let process_memory = (this.memory() / 1_048_576) as i32;
            process_memory_max = std::cmp::max(process_memory, process_memory_max);

            SETTINGS
                .developer
                .system_info
                .process_memory_mb
                .set_value(process_memory);

            SETTINGS
                .developer
                .system_info
                .process_memory_max_mb
                .set_value(process_memory_max);
        }

        SETTINGS
            .developer
            .system_info
            .used_memory_mb
            .set_value(used_memory);

        SETTINGS
            .developer
            .system_info
            .cpu_usage
            .set_value(sys.global_cpu_usage());

        interval.tick().await;
    }
}
