use crate::hardware::hardware_backend::{self, HARDWARE_NAVIGATION_INPUT};
use crate::{CAR_DATA, GLOBAL_STATE, SETTINGS, SHUTDOWN_SIGNAL};

use tokio::time::{self, Duration};

pub fn debug_entry() {
    #[cfg(debug_assertions)]
    if GLOBAL_STATE.bin_args.debug.debug_cli.value() {
        tokio::spawn(cli_mode());
    }
}

#[cfg(debug_assertions)]
async fn cli_mode() {
    use std::io::{stdin, stdout};

    let mut buf = String::new();
    let cli_in = stdin();
    let mut cli_out = stdout();

    loop {
        use std::io::Write;

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
