use crate::{App, ApplicationState, GLOBAL_STATE};

use slint::{ComponentHandle, Weak};

pub fn bridge(ui: Weak<App>) {
    if let Some(ui) = ui.upgrade() {
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
    }
}
