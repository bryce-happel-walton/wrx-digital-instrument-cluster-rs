use crate::slint_generatedApp::{App, WidgetContainerState};

use slint::{ComponentHandle, Weak};

use std::sync::{Arc, LazyLock, RwLock};

struct WidgetState {
    page_number: usize,
    visible: bool,
}

fn recompute_page_numbers(handle_weak: Weak<App>, widgets: &mut [WidgetState]) {
    let mut current_page = 0;

    for widget in widgets.iter_mut() {
        if widget.visible {
            widget.page_number = current_page;
            current_page += 1;
        }
    }

    if let Some(handle) = handle_weak.upgrade() {
        let state = handle.global::<WidgetContainerState>();
        state.set_num_pages(widgets.len() as i32);
        state.set_num_visible_pages(current_page as i32);
    }
}

pub fn bridge(handle_weak: Weak<App>) {
    if let Some(handle) = handle_weak.upgrade() {
        static WIDGETS: LazyLock<Arc<RwLock<Vec<WidgetState>>>> = crate::LazyLockDefault!();
        let state = handle.global::<WidgetContainerState>();

        let create_widget_handle = handle.as_weak();
        state.on_create_widget(move || {
            if let Ok(mut widgets) = WIDGETS.write() {
                let id = widgets.len();

                widgets.push(WidgetState {
                    page_number: id,
                    visible: false,
                });

                recompute_page_numbers(create_widget_handle.clone(), &mut widgets);

                id as i32
            } else {
                -1
            }
        });

        state.on_reset(move || {
            if let Ok(mut widgets) = WIDGETS.write() {
                widgets.clear();
            }
        });

        state.on_get_widget_visible(move |id| {
            if let (Ok(idx), Ok(widgets)) = (usize::try_from(id), WIDGETS.read()) {
                if let Some(widget) = widgets.get(idx) {
                    return widget.visible;
                }
            }

            false
        });

        state.on_get_page_number_of_id(move |id| {
            if let (Ok(idx), Ok(widgets)) = (usize::try_from(id), WIDGETS.read()) {
                if let Some(widget) = widgets.get(idx) {
                    if widget.visible {
                        return widget.page_number as i32;
                    }
                }
            }

            -1
        });

        let widget_visible_handle = handle.as_weak();
        state.on_set_widget_visible(move |id, value| {
            if let (Ok(id), Ok(mut widgets)) = (usize::try_from(id), WIDGETS.write()) {
                if let Some(widget) = widgets.get_mut(id) {
                    widget.visible = value;
                }

                recompute_page_numbers(widget_visible_handle.clone(), &mut widgets);
            }
        });
    }
}
