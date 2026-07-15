use crate::data::parameters::ParameterStruct;
use crate::hardware::hardware_backend::{self, HARDWARE_NAVIGATION_INPUT};
use crate::{CAN_DATA, GLOBAL_STATE, SHUTDOWN_SIGNAL};

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
                    const W: usize = 16;

                    println!();
                    println!("{:<W$} => show this help menu", "h | help");

                    println!("\n{:<W$} => close application", "q | quit");

                    println!("\nlist");
                    println!("{:<W$} => list all registered `<Parameter>`", "    params");
                    println!(
                        "{:<W$} => list the dependencies of the crate",
                        "    dependencies"
                    );

                    println!("\nnav");
                    println!("{:>W$} => force ui navigation up", "up");
                    println!("{:>W$} => force ui navigation down", "down");
                    println!("{:>W$} => force ui navigation enter", "enter");

                    println!(
                        "\n{:<W$} => set the value of any registered [`Parameter`]",
                        "set_param"
                    );
                    println!("{:>W$} |  set_param <path> <value>", "usage");
                    println!(
                        "{:>W$} |  set_param Settings.user.general.unit_system uscs",
                        "example"
                    );

                    println!(
                        "\n{:<W$} => get the value of any registered [`Parameter`]",
                        "get_param"
                    );
                    println!("{:>W$} |  get_param <path>", "usage");
                    println!(
                        "{:>W$} |  get_param Settings.user.general.unit_system",
                        "example"
                    );

                    println!(
                        "\n{:<W$} => set the value of any [`Parameter`] in [`CAN_DATA`]",
                        "set_can_data"
                    );
                    println!("{:>W$} |  set_can_data <param> <value>", "usage");
                    println!("{:>W$} |  set_can_data engine_rpm 1234", "example");
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
                        "params" => {
                            println!("{}", crate::PARAMETER_STRUCTS.get_page_layout())
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
                    crate::PARAMETER_STRUCTS.set_by_path(param_path, value);
                }
                "get_param" => {
                    let param_path = *cmd_splt.get(1).unwrap_or(&"");
                    if let Some((display_value, _)) =
                        crate::PARAMETER_STRUCTS.get_by_path(param_path)
                    {
                        println!("{display_value}")
                    }
                }
                "set_car_data" => {
                    // FIXME: if canbus and sim are both disabled, you must update multiple cardata params before the ui updates

                    let param = *cmd_splt.get(1).unwrap_or(&"");
                    let value = *cmd_splt.get(2).unwrap_or(&"");

                    CAN_DATA.set_param_by_name(param, value);
                }
                x => eprintln!("Unknown command `{x}`"),
            }
            buf.clear();
        }
    }
}
