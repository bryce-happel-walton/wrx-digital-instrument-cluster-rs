mod application;
mod can;
mod data;
mod hardware;
mod lib;
mod slint_ui;

use crate::application::global_state::GLOBAL_STATE;
use crate::application::settings::SETTINGS;
use crate::can::can_backend::{CanBackend, CanFrame, CanInterface};
use crate::can::emulators::wrx_2018_emulator;
use crate::can::messages::wrx_2018::CanError;
use crate::can::mux_context::{self, MuxContext, MuxParseResult};
use crate::can::parsers::iso_tp::*;
use crate::data::car_data::{CAR_DATA, ParseError};
use crate::hardware::hardware_backend::{self, HARDWARE_NAVIGATION_INPUT, HardwareBackend};
use crate::slint_ui::backend::{
    backend_lib, can_display, car_data_bridge, entry, hardware_bridge, lang, rs_type_resolver,
    settings_bridge, widget_container,
};

use sysinfo::{CpuRefreshKind, MemoryRefreshKind, Pid, ProcessRefreshKind, RefreshKind, System};
use tokio::signal;
use tokio::sync::Notify;
use tokio::time::{self, Duration};

use std::collections::VecDeque;
use std::env;
use std::io::Write;
use std::sync::{Arc, LazyLock};

slint::include_modules!();

pub static SHUTDOWN_SIGNAL: LazyLock<Notify> = LazyLock::new(|| Notify::new());
pub static SHUTDOWN_FINISHED: LazyLock<Notify> = LazyLock::new(|| Notify::new());

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = tokio_runtime.enter();

    let ui = entry::make_app()?;

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
                                    _ => println!("Failed to parse frame {frame:?}: {e:?}"),
                                },
                                _ => println!("Failed to parse frame {frame:?}: {e:?}"),
                            },
                        }
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
                            if let Err(e) = backend.write_frame(frame) {
                                eprintln!("Failed to write simulated frame: {e:?}");
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
        let application_state = ui.global::<ApplicationState>();
        application_state.set_virtual_cluster(GLOBAL_STATE.bin_args.virtual_cluster.value());
        application_state.set_interface_type(
            format!(
                "{}: {}",
                GLOBAL_STATE.can.selected_interface.value(),
                GLOBAL_STATE.can.interface_path.value()
            )
            .into(),
        );

        rs_type_resolver::bridge(ui.as_weak());
        settings_bridge::bridge(ui.as_weak());
        car_data_bridge::bridge(ui.as_weak(), CAR_DATA.clone());
        hardware_bridge::bridge(ui.as_weak(), hardware_backend.clone());
        lang::bridge(ui.as_weak());
        backend_lib::bridge(ui.as_weak());
        widget_container::bridge(ui.as_weak());
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

    if GLOBAL_STATE.bin_args.debug.debug_cli.value() {
        tokio::spawn(cli_mode());
    }

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

async fn run_autosave_loop() {
    let mut interval = time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        save_config();
    }
}

#[cfg(debug_assertions)]
async fn cli_mode() {
    use std::io::{stdin, stdout};

    let mut buf = String::new();
    let cli_in = stdin();
    let mut cli_out = stdout();

    loop {
        print!("CLI > ");
        let _ = cli_out.flush();

        if cli_in.read_line(&mut buf).is_ok() {
            if buf.ends_with('\n') {
                buf.pop();
                if buf.ends_with('\r') {
                    buf.pop();
                }
            }

            let cmd_splt: Vec<&str> = buf.as_str().split(" ").collect();
            match *cmd_splt.get(0).unwrap_or(&"") {
                "q" | "quit" => {
                    SHUTDOWN_SIGNAL.notify_one();
                    break;
                }
                "h" | "help" => {
                    println!();
                    println!("h | help     => show this help menu");

                    println!("\nq | quit     => close application");

                    println!("\nlist");
                    println!(
                        "    parameters     => list the layout of {}",
                        stringify!(SETTINGS)
                    );
                    println!("    dependencies   => list the dependencies of the crate");

                    println!("\nnav up       => force ui navigation up");
                    println!("    down     => force ui navigation down");
                    println!("    enter    => force ui navigation enter");

                    println!(
                        "\nset_param    => set the value of any `<Parameter>` in `{}`",
                        stringify!(SETTINGS)
                    );
                    println!("    usage    |  set_param <path> <value>");
                    println!("  example    |  set_param user.general.unit_system uscs");

                    println!(
                        "\nget_param    => get the value of any `<Parameter>` in `{}`",
                        stringify!(SETTINGS)
                    );
                    println!("    usage    |  get_param <path>");
                    println!("  example    |  get_param user.general.unit_system");

                    println!("\nset_car_data => set the value of any `<Parameter>` in `CAR_DATA`");
                    println!("    usage    |  set_car_data <param> <value>");
                    println!("  example    |  set_car_data engine_rpm 1234");
                    println!("     note    |  does not update immediately due to unknown bug");
                    println!();
                }
                "nav" => {
                    match *cmd_splt.get(1).unwrap_or(&"") {
                        "up" => {
                            HARDWARE_NAVIGATION_INPUT
                                .set_value(hardware_backend::HardwareNavigationState::Backward);
                        }
                        "down" => {
                            HARDWARE_NAVIGATION_INPUT
                                .set_value(hardware_backend::HardwareNavigationState::Forward);
                        }
                        "enter" => {
                            HARDWARE_NAVIGATION_INPUT
                                .set_value(hardware_backend::HardwareNavigationState::Enter);
                        }
                        _ => {}
                    };
                    let mut interval = time::interval(Duration::from_millis(100));
                    interval.tick().await;
                    interval.tick().await;
                    println!("resetting input");
                    HARDWARE_NAVIGATION_INPUT
                        .set_value(hardware_backend::HardwareNavigationState::Idle);
                }
                "list" => {
                    match *cmd_splt.get(1).unwrap_or(&"") {
                        "parameters" => {
                            println!("\n{}", SETTINGS.get_page_layout());
                        }
                        "dependencies" => {
                            for dep in crate::application::dependencies::DEPENDENCIES.as_slice() {
                                println!("{dep:?}");
                            }
                        }
                        _ => {}
                    };
                }
                "set_param" => {
                    let param_path = *cmd_splt.get(1).unwrap_or(&"");
                    let value = *cmd_splt.get(2).unwrap_or(&"");
                    SETTINGS.set_by_path(param_path, value);
                }
                "get_param" => {
                    let param_path = *cmd_splt.get(1).unwrap_or(&"");
                    if let Some((display_value, _)) = SETTINGS.get_by_path(param_path) {
                        println!("{display_value}")
                    }
                }
                "param_layout" => {
                    println!("\n{}", SETTINGS.get_page_layout());
                }
                "set_car_data" => {
                    // FIXME: if canbus and sim are both disabled, you must update multiple cardata params before the ui updates
                    let param = *cmd_splt.get(1).unwrap_or(&"");
                    let value = *cmd_splt.get(2).unwrap_or(&"");

                    CAR_DATA.set_param_by_name(param, value);
                }
                x => eprintln!("Unknown command `{x}`"),
            }
            buf.clear();
        }
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
