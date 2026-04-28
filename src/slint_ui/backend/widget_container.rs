use crate::slint_generatedApp::{App, WidgetContainerState};

use slint::{ComponentHandle, Weak};

use std::sync::{Arc, LazyLock, RwLock};

struct WidgetState {
    page_number: usize,
    visible: bool,
}

fn recompute_page_numbers(handle_weak: Weak<App>, widgets: &mut [WidgetState]) {
    let mut page = 0;
    for widget in widgets.iter_mut() {
        if widget.visible {
            widget.page_number = page;
            page += 1;
        }
    }
    if let Some(handle) = handle_weak.upgrade() {
        let state = handle.global::<WidgetContainerState>();
        state.set_num_pages(widgets.len() as i32);
        state.set_num_visible_pages(page as i32);
    }
}

pub fn bridge(handle_weak: Weak<App>) {
    if let Some(handle) = handle_weak.upgrade() {
        static WIDGETS: LazyLock<Arc<RwLock<Vec<WidgetState>>>> =
            LazyLock::new(|| Default::default());
        let state = handle.global::<WidgetContainerState>();

        {
            let handle = handle.as_weak();
            state.on_create_widget(move || {
                if let Ok(mut widgets) = WIDGETS.write() {
                    let id = widgets.len();

                    widgets.push(WidgetState {
                        page_number: id,
                        visible: true,
                    });

                    recompute_page_numbers(handle.clone(), &mut widgets);

                    id as i32
                } else {
                    -1
                }
            });
        }

        state.on_reset(move || {
            if let Ok(mut widgets) = WIDGETS.write() {
                widgets.clear();
            }
        });

        state.on_get_widget_visible(move |id| {
            let Ok(idx) = usize::try_from(id) else {
                return false;
            };

            if let Ok(widgets) = WIDGETS.read() {
                if let Some(widget) = widgets.get(idx) {
                    return widget.visible;
                }
            }

            false
        });

        state.on_get_page_number_of_id(move |id| {
            let Ok(idx) = usize::try_from(id) else {
                return -1;
            };

            if let Ok(widgets) = WIDGETS.read() {
                if let Some(widget) = widgets.get(idx) {
                    if widget.visible {
                        return widget.page_number as i32;
                    }
                }
            }

            -1
        });

        {
            let handle = handle.as_weak();
            state.on_set_widget_visible(move |id, value| {
                let Ok(id) = usize::try_from(id) else { return };

                if let Ok(mut widgets) = WIDGETS.write() {
                    if let Some(widget) = widgets.get_mut(id) {
                        widget.visible = value;
                    }

                    recompute_page_numbers(handle.clone(), &mut widgets);
                }
            });
        }
    }
}
