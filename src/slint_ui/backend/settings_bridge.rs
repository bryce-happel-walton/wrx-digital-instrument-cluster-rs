use crate::application::settings::{FnTrigger, FnTriggers, PageTrigger, SETTINGS};
use crate::data::parameters::{Bound, Node};
use crate::slint_generatedApp::{
    AccessibilitySettings, App, ApplicationState, AttributionItem, DebugSettings, DerivedParamType,
    GeneralSettings, GlobalThemeData, SBoundInt, SettingsLayout, SettingsMenuItem,
    SettingsMenuItemType, SystemInfo,
};

use pastey::paste;
use slint::{ComponentHandle, Weak};
use tokio::select;

fn resolve_node<'a>(path: &'a str, layout_node: &'a Node) -> Option<&'a str> {
    if let Node::Page { name, items: _ } = layout_node {
        path.strip_prefix(&format!("{}.", *name))
    } else {
        None
    }
}

fn resolve_path(path: &str) -> Option<(String, Box<dyn std::any::Any>)> {
    resolve_node(path, SETTINGS.get_page_layout()).and_then(|path| SETTINGS.get_by_path(path))
}

pub fn bridge(handle_weak: Weak<App>) {
    {
        let handle_weak = handle_weak.clone();
        match slint::spawn_local(async_compat::Compat::new(async move {
            macro_rules! bind {
            {$slint_global:ident.$param:tt <=> $root:ident.$($tail:tt)+} => {{paste!{
                let handle_weak = handle_weak.clone();
                let root = $root.clone();

                if let Some(handle) = handle_weak.upgrade() {
                    let g = handle.global::<$slint_global>();

                    g.[<on_update_ $param>](move |value| {
                        root.$($tail)+.set_value(value.into());
                    });
                }

                bind!($slint_global.$param <=| $root.$($tail)+);
            }}};

            {$slint_global:ident.$param:tt <=| $root:ident.$($tail:tt)+} => {{paste!{
                let root = $root.clone();
                if let Some(handle) = handle_weak.upgrade() {
                    let _= slint::spawn_local(async_compat::Compat::new(async move {
                        let g = handle.global::<$slint_global>();
                        let mut watch = root.$($tail)+.watch();

                        loop {
                            g.[<set_ $param>](root.$($tail)+.value().into());

                            select!{
                                Ok(_) = watch.changed() => {},
                                else => {break;}
                            }
                        }
                    }));
                }
            }}};
        }

            // TODO: auto-generate bindings
            bind!(ApplicationState.user_unit <=> SETTINGS.user.general.unit_system);
            bind!(ApplicationState.running_simulation <=> SETTINGS.developer.simulation.running_simulation);
            bind!(ApplicationState.running_can <=> SETTINGS.developer.can.running_can);
            bind!(GeneralSettings.disable_hill_assist <=> SETTINGS.user.general.disable_hill_assist_warning);
            bind!(GlobalThemeData.current_theme <=> SETTINGS.user.theme.selected_theme);
            bind!(AccessibilitySettings.animations_enabled <=> SETTINGS.user.accessibility.animations_enabled);
            bind!(AccessibilitySettings.accessible_switches <=> SETTINGS.user.accessibility.accessible_switches);
            bind!(AccessibilitySettings.selection_box_thickness <=> SETTINGS.user.accessibility.selection_box_thickness);
            bind!(DebugSettings.debug_mode <=> SETTINGS.developer.debug.debug_mode);
            bind!(DebugSettings.debug_highlights <=> SETTINGS.developer.debug.debug_highlights);
            bind!(DebugSettings.debug_overlay_enabled <=> SETTINGS.developer.debug.debug_overlay_enabled);
            bind!(DebugSettings.extra_debug_info <=> SETTINGS.developer.debug.extra_debug_info);

            #[cfg(feature = "bevy")]
            bind!(GeneralSettings.use_bevy_car_display <=> SETTINGS.user.widgets.car_display_bevy.model_3d);
            #[cfg(not(feature = "bevy"))]
            bind!(GeneralSettings.use_bevy_car_display <=| SETTINGS.user.widgets.car_display.model_3d);

            bind!(SystemInfo.total_memory <=| SETTINGS.developer.system_info.total_memory_mb);
            bind!(SystemInfo.used_memory <=| SETTINGS.developer.system_info.used_memory_mb);
            bind!(SystemInfo.process_memory <=| SETTINGS.developer.system_info.process_memory_mb);
            bind!(SystemInfo.process_memory_max <=| SETTINGS.developer.system_info.process_memory_max_mb);
            bind!(SystemInfo.num_cpus <=| SETTINGS.developer.system_info.num_cpus);
            bind!(SystemInfo.cpu_usage <=| SETTINGS.developer.system_info.cpu_usage);
            bind!(SystemInfo.fps <=| SETTINGS.developer.system_info.fps);
        })) {
            Err(e) => eprintln!("Failure in settings loader: {e}"),
            _ => {}
        }
    }

    if let Some(handle) = handle_weak.upgrade() {
        let ui_layout = handle.global::<SettingsLayout>();

        ui_layout.on_get_page_layout(move |path| {
            let path: Vec<&str> = path.split_terminator(".").collect();
            let (layout, current_path) =
                unroll_path(SETTINGS.get_page_layout(), &path, String::new(), 0);

            let ret = unroll_layout(layout, &current_path, 0, 1);
            ret.as_slice()[1..].into() // remove the current page as an entry
        });

        ui_layout.on_get_page_parent(move |path| {
            path.rsplit_once(".")
                .map(|(rest, _)| rest.into())
                .unwrap_or(path)
        });

        ui_layout.on_set_by_path(move |path, value| {
            resolve_node(path.as_str(), SETTINGS.get_page_layout())
                .map(|path| SETTINGS.set_by_path(path, &value.as_str()));
        });

        ui_layout.on_get_by_path(move |path| {
            resolve_path(path.as_str())
                .map(|(display, _)| display)
                .unwrap_or(String::from("error"))
                .into()
        });

        ui_layout.on_get_bound_int(move |path| {
            resolve_path(path.as_str())
                .and_then(|(_, value)| value.downcast_ref::<Bound<i32>>().copied())
                .unwrap_or_default()
                .into()
        });

        ui_layout.on_get_bool(move |path| {
            resolve_path(path.as_str())
                .and_then(|(_, value)| value.downcast_ref::<bool>().copied())
                .unwrap_or_default()
                .into()
        });

        ui_layout.on_get_settings_special_page(move |path| {
            resolve_path(path.as_str())
                .and_then(|(_, value)| value.downcast_ref::<PageTrigger>().copied())
                .unwrap_or_default()
                .into()
        });

        ui_layout.on_trigger_fn(move |path| {
            match Into::<FnTriggers>::into(
                resolve_path(path.as_str())
                    .and_then(|(_, value)| value.downcast_ref::<FnTrigger>().copied())
                    .unwrap_or_default(),
            ) {
                FnTriggers::NoOp => {}
                //
                FnTriggers::OBD2CodeRead => {}
                FnTriggers::OBD2VinRead => {}
            }
        });

        ui_layout.on_get_attributions(move || {
            crate::application::dependencies::DEPENDENCIES.as_slice()[1..]
                .iter()
                .map(|dep| AttributionItem {
                    name: dep.name.into(),
                    version: format!("{}", dep).into(),
                })
                .collect::<Vec<_>>()
                .as_slice()
                .into()
        });

        ui_layout.on_stringify_derived_param_type(move |ty| ty.to_string().into());
    }
}

impl ToString for DerivedParamType {
    fn to_string(&self) -> String {
        match self {
            Self::Bool => String::from("bool"),
            Self::BoundInt => String::from("bound_int"),
            Self::Number => String::from("number"),
            Self::String => String::from("string"),
            Self::Page => String::from("page"),
            Self::Trigger => String::from("trigger"),
            Self::Enum => String::from("enum"),
        }
    }
}

/// Convert Rust types to Slint `DerivedParamType` so we can generate the user interaction in UI procedurally
impl From<&str> for DerivedParamType {
    fn from(value: &str) -> Self {
        match value.trim() {
            stringify!(bool) => Self::Bool,
            stringify!(Bound<i32>) => Self::BoundInt,
            stringify!(i32) | stringify!(f32) => Self::Number,
            stringify!(String) => Self::String,
            "page" => Self::Page,
            stringify!(PageTrigger) | stringify!(FnTrigger) => Self::Trigger,
            _ => Self::Enum, //* More complex types need to be dealt with by param_type
        }
    }
}

fn unroll_layout(
    node: &Node,
    current_path: &String,
    mut current_depth: usize,
    max_depth: usize,
) -> Vec<SettingsMenuItem> {
    let mut ret = Vec::new();

    if current_depth > max_depth {
        return ret;
    }

    match node {
        Node::HiddenParameter => {}
        Node::ReadOnlyParameter { name, ty } => ret.push(SettingsMenuItem {
            name: (*name).into(),
            param_type: (*ty).into(),
            param_path: format!("{current_path}{name}").into(),
            param_type_derived: DerivedParamType::from(*ty),
            item_type: SettingsMenuItemType::ReadOnlyParameter,
        }),
        Node::Parameter { name, ty } => ret.push(SettingsMenuItem {
            name: (*name).into(),
            param_type: (*ty).into(),
            param_path: format!("{current_path}{name}").into(),
            param_type_derived: DerivedParamType::from(*ty),
            item_type: SettingsMenuItemType::Parameter,
        }),
        Node::Page { name, items } => {
            let path: String = format!("{current_path}{name}");

            ret.push(SettingsMenuItem {
                name: (*name).into(),
                param_type: "page".into(),
                param_path: (&path).into(),
                param_type_derived: DerivedParamType::Page,
                item_type: SettingsMenuItemType::Page,
            });

            let child_path = format!("{path}.");
            current_depth += 1;

            for item in items {
                ret.extend_from_slice(
                    unroll_layout(item, &child_path, current_depth, max_depth).as_slice(),
                );
            }
        }
    }

    ret
}

fn unroll_path<'a>(
    node: &'a Node,
    split_path: &Vec<&str>,
    current_path: String,
    current_depth: usize,
) -> (&'a Node, String) {
    if split_path.len() != current_depth {
        if let Node::Page { name: _, items } = node {
            for item in items {
                if let Node::Page { name, items: _ } = item {
                    if let Some(current_item) = split_path.get(current_depth + 1) {
                        if current_item == name {
                            return unroll_path(
                                item,
                                split_path,
                                format!("{}.", split_path[..=current_depth].join(".")),
                                current_depth + 1,
                            );
                        }
                    }
                }
            }
        }
    }

    (node, current_path)
}
