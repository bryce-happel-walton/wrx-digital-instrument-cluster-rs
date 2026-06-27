mod application;
mod can;
mod data;
mod debug;
mod hardware;
mod slint_ui;
mod util;

use crate::application::global_state::GLOBAL_STATE;
use crate::application::settings::SETTINGS;
use crate::can::can_backend::{CanBackend, CanFrame, CanInterface};
use crate::can::emulators::wrx_2018_emulator;
use crate::can::messages::wrx_2018::CanError;
use crate::can::mux_context::{self, MuxContext, MuxParseResult};
use crate::can::parsers::iso_tp::*;
use crate::data::car_data::{CAR_DATA, ParseError};
use crate::debug::debug_entry;
use crate::hardware::hardware_backend::{self, HardwareBackend};
use crate::slint_ui::backend::{
    backend_lib, can_display, car_data_bridge, hardware_bridge, lang, rs_type_resolver,
    settings_bridge, state_bridge,
};

use sysinfo::{CpuRefreshKind, MemoryRefreshKind, Pid, ProcessRefreshKind, RefreshKind, System};
use tokio::signal;
use tokio::sync::Notify;
use tokio::time::{self, Duration};

use std::collections::VecDeque;
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

    let ui = App::new().unwrap();

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

    if let Some(mut can_backend) = can_backend {
        let weak_ui = ui.as_weak();
        tokio::spawn(async move {
            let obd_id =
                unsafe { embedded_can::Id::from(embedded_can::StandardId::new_unchecked(0x7E0)) };

            // TESTING
            // let mut mux_context = MuxContext::new(can_backend);
            // let mut queue = VecDeque::from(vec![
            //     // CanFrame::new(
            //     //     obd_id,
            //     //     8,
            //     //     &[
            //     //         0x02,
            //     //         OBDService::CurrentData.into(),
            //     //         S1CurrentData::PIDs1.into(),
            //     //     ],
            //     // ),
            //     // CanFrame::new(
            //     //     obd_id,
            //     //     8,
            //     //     &[
            //     //         0x02,
            //     //         OBDService::VehicleInformation.into(),
            //     //         S9VehicleInformation::PIDs.into(),
            //     //     ],
            //     // ),
            //     // CanFrame::new(
            //     //     obd_id,
            //     //     8,
            //     //     &[
            //     //         0x02,
            //     //         OBDService::VehicleInformation.into(),
            //     //         S9VehicleInformation::VIN.into(),
            //     //     ],
            //     // ),
            //     // CanFrame::new(obd_id, 8, &[0x01, OBDService::StoredDTCs.into()]),
            // ]);

            let running_can = &SETTINGS.developer.can.running_can;

            loop {
                if running_can.value() {
                    if let Some(frame) = can_backend.read_frame() {
                        can_display::update(weak_ui.clone(), &frame, false);

                        match CAR_DATA.parse_frame(&frame) {
                            Ok(result) => match result {
                                //? CAR_DARA.parse_frame does not do any demuxing currently
                                // ParseResult::Mux(result) => match result {
                                //     MuxParseResult::AwaitingBroadcastAck => {
                                //         let ack = ISOTPAckFrame::new(obd_id);
                                //         queue.push_front(CanFrame::from_frame(&ack));
                                //     }
                                //     _ => {}
                                // },
                                _ => {}
                            },
                            Err(e) => match e {
                                ParseError::CanError(e) => match e {
                                    CanError::UnknownMessageId(_id) => {
                                        //     match mux_context.parse_frame(&frame) {
                                        //         Ok(result) => match result {
                                        //             MuxParseResult::AwaitingBroadcastAck => {
                                        //                 // TODO: we need to track the current proprietor of the conversation for any given id
                                        //                 let ack = ISOTPAckFrame::new(obd_id);
                                        //                 queue.push_front(CanFrame::from_frame(&ack));
                                        //             }
                                        //             _ => {}
                                        //         },
                                        //         Err(e) => match e {
                                        //             mux_context::MuxParseError::UnknownMessageId => {
                                        //                 // TODO: propogate to other parsing mechanisms
                                        //             }
                                        //             _ => println!(
                                        //                 "Context failed to parse frame {frame:?}: {e:?}"
                                        //             ),
                                        //         },
                                        //     }
                                    }
                                    _ => {
                                        crate::util::performance::record_can_frame_lost();
                                        println!("Failed to parse frame {frame:?}: {e:?}");
                                    }
                                },
                                _ => {
                                    crate::util::performance::record_can_frame_lost();
                                    println!("Failed to parse frame {frame:?}: {e:?}");
                                }
                            },
                        }

                        crate::util::performance::record_can_frame_parsed();
                    };

                    // if !mux_context.is_waiting_for_responce() {
                    //     if let Some(frame) = queue.pop_front() {
                    //         match can_backend.write_frame(frame) {
                    //             Ok(_) => {
                    //                 // TODO: track this transaction to determine whether this conversation is ours
                    //                 println!("Wrote frame: {frame:?}")
                    //             }
                    //             Err(e) => {
                    //                 eprintln!("Failed to write to can_socket: {e:?}");
                    //             }
                    //         }
                    //     }
                    // }

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
    {
        rs_type_resolver::bridge(ui.as_weak());
        settings_bridge::bridge(ui.as_weak());
        car_data_bridge::bridge(ui.as_weak());
        hardware_bridge::bridge(ui.as_weak(), hardware_backend.clone());
        lang::bridge(ui.as_weak());
        backend_lib::bridge(ui.as_weak());
        state_bridge::bridge(ui.as_weak());
    }

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
