use crate::application::dependencies::DEPENDENCIES;
use crate::application::global_state::GLOBAL_STATE;
use crate::data::parameters::{Bound, ParameterStruct};
use crate::data::units::UnitSystem;
use crate::slint_generatedApp::{ClusterTheme, SettingsPageTarget};
use crate::slint_ui::backend::lang::StrLang;

use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tokio::time::{Duration, timeout};
use toml;

use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;

const CONFIG_NAME: &str = "settings.toml";
const LOAD_TIMEOUT: Duration = Duration::from_secs(10);

crate::parameter_struct! {
static SETTINGS [|this: Settings| this.init()] = Settings {
    [hidden] loaded: bool = false,

    user {
        general {
            disable_hill_assist_warning: bool = false,
            unit_system: UnitSystem,
            language: StrLang,
        },

        accessibility {
            animations_enabled: bool = true,
            accessible_switches: bool = false,
            selection_box_thickness: Bound<i32> = Bound::new(2, 1..=10, 1),
        },

        widgets {
            can_display_widget_enabled: bool = false,
        },

        theme {
            selected_theme: ClusterTheme = ClusterTheme::Default,
        },

        [hidden] save_state {
            last_widget_index: i32 = 0,
        },
    },

    [cfg!(debug_assertions)] developer {
        debug {
            [ro] debug_mode: bool = cfg!(debug_assertions),
            debug_highlights: bool = false,
            debug_overlay_enabled: bool = true,
            extra_debug_info: bool = false,
        },

        simulation {
            simulation_speed_ms: Bound<i32> = Bound::new(10, 10..=250, 10),
            running_simulation: bool = true,
        },

        can {
            running_can: bool = true,
        },

        hardware {
            [ro] adc_val: i32,
        },

        system_info {
            [ro] total_memory_mb: i32,
            [ro] used_memory_mb: i32,
            [ro] process_memory_mb: i32,
            [ro] process_memory_max_mb: i32,
            [ro] num_cpus: i32,
            [ro] cpu_usage: f32,
            [ro] fps: i32,
        },

        performance {
            [ro] can_frames_sent_per_sec: i32,
            [ro] can_frames_parsed_per_sec: i32,
            [ro] can_frame_parse_to_ui_latency_us: i32,
            [ro] can_frame_parse_to_ui_latency_max_us: i32,
            [ro] can_frames_lost_total: i32,
        },
    },

    diagnostics {
        obd2 {
            read_codes: FnTrigger = FnTrigger(FnTriggers::OBD2CodeRead),
            read_vin: FnTrigger = FnTrigger(FnTriggers::OBD2VinRead),
        },

        uds {
            read_codes: FnTrigger = FnTrigger(FnTriggers::UDSCodeRead),
        },

        ssm2 {
            // TODO: vehicle specific things should be covered by a feature config later
            cobb {},
        },
    },

    about {
        [ro] version: String = format!("{}", DEPENDENCIES.wrx_digital_instrument_cluster_rs),
        [ro] can_database_version: String = format!("{}", DEPENDENCIES.CAN_database),
        // [hidden] slint_version: String = format!("{}", DEPENDENCIES.slint), // hidden because it is accessed within the attributions page
        attributions: PageTrigger = PageTrigger(SettingsPageTarget::Attributions),
    },
}}

impl Settings {
    pub fn get_config_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let exe_dir = env::current_exe()?.to_path_buf();
        let exe_name = exe_dir
            .file_name()
            .ok_or("Failed to get executable's name")?
            .display();

        #[cfg(not(feature = "apalis_imx8"))]
        let config_dir = exe_dir
            .parent()
            .ok_or("Failed to get executable's parent")?;
        #[cfg(all(feature = "apalis_imx8"))]
        let config_dir = PathBuf::from("/data");

        let config_dir = config_dir.join(format!("{exe_name}_data/"));

        match fs::create_dir_all(&config_dir) {
            Err(ref e) if e.kind() == io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e.into()),
            _ => (),
        }

        Ok(config_dir)
    }

    pub fn get_config_path(&self) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let config_dir = Self::get_config_dir()?;

        let config_dir = config_dir.join(CONFIG_NAME);
        match File::create_new(&config_dir) {
            Ok(mut file) => {
                let toml_string = toml::to_string_pretty(&self.user)?;
                file.write(toml_string.as_bytes())?;
            }
            _ => {}
        };

        Ok(config_dir)
    }

    pub async fn load_from_fs(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_dir = self.get_config_path()?;
        let config_file = fs::read_to_string(&config_dir)?;

        if let Err(_) = timeout(LOAD_TIMEOUT, async move {
            if let Ok(loaded_config) = toml::from_str(config_file.as_str()) {
                self.user.apply(loaded_config);
                self.loaded.set_value(true);
            }
        })
        .await
        {
            eprintln!("Timeout on settings load");
        }

        Ok(())
    }

    pub fn save_to_fs(&self) -> Result<(), SaveError> {
        // TODO: add timestamp and settings version to file
        let toml_str =
            toml::to_string_pretty(&self.user).map_err(|e| SaveError::Error(e.into()))?;

        let config_path = self
            .get_config_path()
            .map_err(|e| SaveError::Error(e.into()))?;
        let dir = config_path.parent().ok_or(SaveError::DirError)?;
        let temp_dir = dir.join(format!("{CONFIG_NAME}.tmp"));
        let old_dir = dir.join(format!("{CONFIG_NAME}.old"));

        let mut temp_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp_dir)
            .map_err(|e| SaveError::Error(e.into()))?;

        temp_file
            .write_all(toml_str.as_bytes())
            .map_err(|e| SaveError::Error(e.into()))?;

        temp_file
            .sync_all()
            .map_err(|e| SaveError::Error(e.into()))?;

        fs::copy(&config_path, &old_dir).map_err(|e| SaveError::Error(e.into()))?;
        fs::rename(&temp_dir, &config_path).map_err(|e| SaveError::Error(e.into()))?;

        OpenOptions::new()
            .read(true)
            .open(dir)
            .map_err(|e| SaveError::Error(e.into()))?
            .sync_all()
            .map_err(|e| SaveError::Error(e.into()))?;

        Ok(())
    }

    #[allow(unused)]
    pub fn loaded(&self) -> watch::Receiver<bool> {
        self.loaded.watch()
    }

    pub fn init(self) -> Self {
        tokio::spawn(async move {
            if let Err(e) = SETTINGS.load_from_fs().await {
                eprintln!("Failed to load user settings from fs: {e:?}");
            }
        });

        #[cfg(debug_assertions)]
        {
            self.developer
                .debug
                .debug_highlights
                .set_value(GLOBAL_STATE.bin_args.debug.debug_highlights.value());

            self.developer
                .debug
                .debug_overlay_enabled
                .set_value(!GLOBAL_STATE.bin_args.debug.debug_no_overlay.value());

            self.developer
                .debug
                .extra_debug_info
                .set_value(GLOBAL_STATE.bin_args.debug.extra_debug_info.value());

            // if BIN_ARGS.get_flag("cli") {
            //     tokio::spawn(cli_mode());
            // }
        }

        self
    }
}

/// Define a triggerable struct that contains an enum for serde with the UI settings parser
macro_rules! ImplTriggerStruct {
    ($struct:ident <$inner:ident>) => {
        #[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug, Default)]
        pub struct $struct($inner);

        impl std::fmt::Display for $struct {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{:?}", self)
            }
        }

        impl Into<$inner> for $struct {
            fn into(self) -> $inner {
                self.0
            }
        }
    };
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug, Default)]
pub enum FnTriggers {
    #[default]
    NoOp,
    OBD2CodeRead,
    OBD2VinRead,
    UDSCodeRead,
}

ImplTriggerStruct!(PageTrigger<SettingsPageTarget>);
ImplTriggerStruct!(FnTrigger<FnTriggers>);

#[derive(Debug)]
pub enum SaveError {
    DirError, // TODO: make proper errors
    Error(Box<dyn std::error::Error>),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DirError => write!(f, "Directory error"),
            Self::Error(e) => write!(f, "Failed to save: {e:?}"),
        }
    }
}
