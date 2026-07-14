use crate::application::settings::{FnTrigger, FnTriggers, PageTrigger, SETTINGS};
use crate::data::parameters::{Bound, Node, ParameterStruct};
use crate::slint_generatedApp::{
    AccessibilitySettings, App, ApplicationState, AttributionItem, DebugSettings, DerivedParamType,
    GeneralSettings, GlobalThemeData, Performance, SBoundInt, SettingsLayout, SettingsMenuItem,
    SettingsMenuItemType, SystemInfo, WidgetContainerState, WidgetSettings,
};

use pastey::paste;
use slint::{ComponentHandle, Weak};
use tokio::select;

macro_rules! bind_settings {
    ($({$slint_global:ident.$param:tt <=$direction:tt $root:ident.$($tail:ident).+}),+ $(,)? ) => {paste! {
        #[allow(nonstandard_style)]
        pub fn bind_settings(handle_weak: Weak<App>) {
            if let Some(handle) = handle_weak.clone().upgrade() {
                $(bind_settings!(@from (handle) $slint_global.$param $direction $root.$($tail).+);)+

                let _ = slint::spawn_local(async_compat::Compat::new(async move {
                    $(
                        let [<g_ $slint_global _ $param>] = handle.global::<$slint_global>();
                        let mut [<watch_ $root _ $($tail)_+>] = $root.$($tail).+.watch();
                    )+

                    loop {
                        $(
                            let [<value_ $root _ $($tail)_+>] = *[<watch_ $root _ $($tail)_+>].borrow_and_update();
                            [<g_ $slint_global _ $param>].[<set_ $param>]([<value_ $root _ $($tail)_+>].into());
                        )+

                        select!{$(
                            Ok(_) = [<watch_ $root _ $($tail)_+>].changed() => {}
                        )+}
                    }

                }));
            }
        }
    }};

    {@from ($handle:ident) $slint_global:ident.$param:tt > $root:ident.$($tail:ident).+} => {paste!{
        $handle.global::<$slint_global>().[<on_update_ $param>](move |value| {
            $root.$($tail).+.set_value(value.into());
        });
    }};
    {@from ($handle:ident) $slint_global:ident.$param:tt | $root:ident.$($tail:ident).+} => {/* empty */};
}

bind_settings! {
    {ApplicationState.user_unit <=> SETTINGS.user.general.unit_system},
    {ApplicationState.running_simulation <=> SETTINGS.developer.simulation.running_simulation},
    {ApplicationState.running_can <=> SETTINGS.developer.can.running_can},
    {GeneralSettings.disable_hill_assist <=> SETTINGS.user.general.disable_hill_assist_warning},
    {WidgetSettings.can_display_widget_enabled <=> SETTINGS.user.widgets.can_display_widget_enabled},
    {GlobalThemeData.current_theme <=> SETTINGS.user.theme.selected_theme},
    {AccessibilitySettings.animations_enabled <=> SETTINGS.user.accessibility.animations_enabled},
    {AccessibilitySettings.accessible_switches <=> SETTINGS.user.accessibility.accessible_switches},
    {AccessibilitySettings.selection_box_thickness <=> SETTINGS.user.accessibility.selection_box_thickness},
    {DebugSettings.debug_mode <=> SETTINGS.developer.debug.debug_mode},
    {DebugSettings.debug_highlights <=> SETTINGS.developer.debug.debug_highlights},
    {DebugSettings.debug_overlay_enabled <=> SETTINGS.developer.debug.debug_overlay_enabled},
    {DebugSettings.extra_debug_info <=> SETTINGS.developer.debug.extra_debug_info},
    {WidgetContainerState.current_page <=> SETTINGS.user.save_state.last_widget_index},

    {SystemInfo.total_memory <=| SETTINGS.developer.system_info.total_memory_mb},
    {SystemInfo.used_memory <=| SETTINGS.developer.system_info.used_memory_mb},
    {SystemInfo.process_memory <=| SETTINGS.developer.system_info.process_memory_mb},
    {SystemInfo.process_memory_max <=| SETTINGS.developer.system_info.process_memory_max_mb},
    {SystemInfo.num_cpus <=| SETTINGS.developer.system_info.num_cpus},
    {SystemInfo.cpu_usage <=| SETTINGS.developer.system_info.cpu_usage},
    {SystemInfo.fps <=| SETTINGS.developer.system_info.fps},

    {Performance.can_frames_sent_per_sec <=| SETTINGS.developer.performance.can_frames_sent_per_sec},
    {Performance.can_frames_parsed_per_sec <=| SETTINGS.developer.performance.can_frames_parsed_per_sec},
    {Performance.can_frame_parse_to_ui_latency_us <=| SETTINGS.developer.performance.can_frame_parse_to_ui_latency_us},
    {Performance.can_frame_parse_to_ui_latency_max_us <=| SETTINGS.developer.performance.can_frame_parse_to_ui_latency_max_us},
    {Performance.can_frames_lost_total <=| SETTINGS.developer.performance.can_frames_lost_total},
}

pub fn bridge(handle_weak: Weak<App>) {
    bind_settings(handle_weak.clone());
    if let Some(handle) = handle_weak.upgrade() {
        let ui_layout = handle.global::<SettingsLayout>();

        ui_layout.on_get_page_layout(move |path| {
            let path: Vec<&str> = path.split_terminator(".").collect();
            let (layout, current_path) = SETTINGS.unroll_path(&path, String::new(), 0, None);
            let ret = flatten_layout(layout, &current_path, 0, 1);

            ret.as_slice()[1..].into() // remove the current page as an entry
        });

        ui_layout.on_get_page_parent(move |path| {
            path.rsplit_once(".")
                .map(|(rest, _)| rest.into())
                .unwrap_or(path)
        });

        ui_layout.on_set_by_path(move |path, value| {
            SETTINGS
                .resolve_node(path.as_str(), None)
                .map(|path| SETTINGS.set_by_path(path, &value.as_str()));
        });

        ui_layout.on_get_by_path(move |path| {
            SETTINGS
                .resolve_path(path.as_str())
                .map(|(display, _)| display)
                .unwrap_or(String::from("error"))
                .into()
        });

        ui_layout.on_get_bound_int(move |path| {
            SETTINGS
                .resolve_path(path.as_str())
                .and_then(|(_, value)| value.downcast_ref::<Bound<i32>>().copied())
                .unwrap_or_default()
                .into()
        });

        ui_layout.on_get_bool(move |path| {
            SETTINGS
                .resolve_path(path.as_str())
                .and_then(|(_, value)| value.downcast_ref::<bool>().copied())
                .unwrap_or_default()
                .into()
        });

        ui_layout.on_get_settings_special_page(move |path| {
            SETTINGS
                .resolve_path(path.as_str())
                .and_then(|(_, value)| value.downcast_ref::<PageTrigger>().copied())
                .unwrap_or_default()
                .into()
        });

        ui_layout.on_trigger_fn(move |path| {
            match Into::<FnTriggers>::into(
                SETTINGS
                    .resolve_path(path.as_str())
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

fn flatten_layout(
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
                    flatten_layout(item, &child_path, current_depth, max_depth).as_slice(),
                );
            }
        }
    }

    ret
}
