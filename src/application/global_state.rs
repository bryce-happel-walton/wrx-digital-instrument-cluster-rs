use crate::can::can_backend::CanInterface;

use clap::ArgMatches;

use std::sync::{Arc, LazyLock};

const VCAN_IF_NAME: &str = "vcan0";
const CAN_IF_NAME: &str = "can0";
#[cfg(target_os = "linux")]
const DEFAULT_SL_DEV: &str = "/dev/ttyACM0";
#[cfg(target_vendor = "apple")]
const DEFAULT_SL_DEV: &str = "/dev/tty.usbmodem101";

pub static BIN_ARGS: LazyLock<ArgMatches> = LazyLock::new(|| {
    let conflicting_args = vec![
        "fakedev",
        #[cfg(target_os = "linux")]
        "candev",
        "sldev",
        #[cfg(target_os = "linux")]
        "virtual",
    ];

    clap::Command::new("").version(env!("CARGO_PKG_VERSION"))
        .args([
            #[cfg(target_os = "linux")]
            clap::arg!(-v --virtual "Runs the application in virtual mode using socketcan vcan")
                .required(false)
                .conflicts_with_all(conflicting_args.iter().filter(|&x| x != &"virtual")),
            clap::arg!(-f --fakedev "Runs the application in virtual mode using a fake can socket emulator")
                .required(false)
                .conflicts_with_all(conflicting_args.iter().filter(|&x| x != &"fakedev")),
            #[cfg(target_os = "linux")]
            clap::arg!(-c --candev <PATH> "Path to the desired CAN device to use")
                .required(false)
                .default_value(CAN_IF_NAME)
                .conflicts_with_all(conflicting_args.iter().filter(|&x| x != &"candev")),
            clap::arg!(-s --sldev <PATH> "Path to the desired serial CAN device to use")
                .required(false)
                .default_value(DEFAULT_SL_DEV)
                .conflicts_with_all(conflicting_args.iter().filter(|&x| x != &"sldev")),
            #[cfg(debug_assertions)]
            clap::arg!(--cli "Enable internal cli")
                .required(false),
            #[cfg(debug_assertions)]
            clap::arg!(--debug_highlight "Enables UI highlights")
                .required(false),
                #[cfg(debug_assertions)]
            clap::arg!(--debug_info "Enables UI extra-debug-info")
                .required(false),
            #[cfg(debug_assertions)]
            clap::arg!(--debug_no_overlay "Disables debug overlay")
                .required(false),
            ])
        .get_matches()
});

pub static GLOBAL_STATE: LazyLock<Arc<GlobalState>> = LazyLock::new(|| {
    let ret = GlobalState::default();
    ret.init();
    Arc::new(ret)
});

crate::parameter_struct! {GlobalState {
    bin_args {
        fake_dev: bool = BIN_ARGS.get_flag("fakedev"),
        virtual_cluster: bool,

        [cfg!(debug_assertions)] debug {
            debug_highlights: bool = get_debug_bin_arg("debug_highlight"),
            debug_no_overlay: bool = get_debug_bin_arg("debug_no_overlay"),
            extra_debug_info: bool = get_debug_bin_arg("debug_info"),
            debug_cli: bool = get_debug_bin_arg("cli"),
        },
    },

    can {
        selected_interface: CanInterface,
        interface_path: String,
    },
}}

impl GlobalState {
    fn init(&self) {
        let virtual_cluster = {
            let ret = BIN_ARGS.get_flag("fakedev");
            #[cfg(target_os = "linux")]
            {
                BIN_ARGS.get_flag("virtual") || ret
            }
            #[cfg(not(target_os = "linux"))]
            {
                ret
            }
        };

        let selected_interface = if BIN_ARGS.get_flag("fakedev") {
            CanInterface::Fake
        } else if virtual_cluster {
            #[cfg(target_os = "linux")]
            {
                CanInterface::VirtualSocketCan
            }
            #[cfg(not(target_os = "linux"))]
            {
                CanInterface::Fake
            }
        } else if cfg!(target_os = "linux")
            && (BIN_ARGS.value_source("candev") == Some(clap::parser::ValueSource::CommandLine))
        {
            CanInterface::SocketCan
        } else if BIN_ARGS.value_source("sldev") == Some(clap::parser::ValueSource::CommandLine) {
            CanInterface::SerialCan
        } else {
            #[cfg(feature = "apalis_imx8")]
            {
                CanInterface::SocketCan
            }
            #[cfg(not(feature = "apalis_imx8"))]
            {
                CanInterface::SerialCan
            }
        };

        let interface_path = match &selected_interface {
            CanInterface::VirtualSocketCan | CanInterface::Fake => VCAN_IF_NAME.to_owned(),
            CanInterface::SerialCan => BIN_ARGS
                .get_one::<String>("sldev")
                .cloned()
                .unwrap_or_else(|| DEFAULT_SL_DEV.to_owned()),
            CanInterface::SocketCan => BIN_ARGS
                .get_one::<String>("candev")
                .cloned()
                .unwrap_or_else(|| CAN_IF_NAME.to_owned()),
        };

        self.bin_args.virtual_cluster.set_value(virtual_cluster);
        self.can.selected_interface.set_value(selected_interface);
        self.can.interface_path.set_value(interface_path);
    }
}

fn get_debug_bin_arg(arg: &str) -> bool {
    #[cfg(debug_assertions)]
    {
        BIN_ARGS.get_flag(arg)
    }
    #[cfg(not(debug_assertions))]
    {
        false
    }
}
