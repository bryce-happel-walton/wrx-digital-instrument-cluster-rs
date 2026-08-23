use crate::can::acquisition::ACQUISITION_PROGRESS;
use crate::slint_generatedApp::{AcquisitionProgress, App};

use slint::{ComponentHandle, Weak};
use tokio::select;

pub fn bridge(handle_weak: Weak<App>) {
    if let Some(handle) = handle_weak.upgrade() {
        let _ = slint::spawn_local(async_compat::Compat::new(async move {
            let ui_progress = handle.global::<AcquisitionProgress>();

            let mut watch_queued = ACQUISITION_PROGRESS.queued.watch();
            let mut watch_completed = ACQUISITION_PROGRESS.completed.watch();
            let mut watch_skipped = ACQUISITION_PROGRESS.skipped.watch();
            let mut watch_failed = ACQUISITION_PROGRESS.failed.watch();
            let mut watch_total = ACQUISITION_PROGRESS.total.watch();
            let mut watch_complete = ACQUISITION_PROGRESS.complete.watch();
            let mut watch_current = ACQUISITION_PROGRESS.current.watch();

            loop {
                // cloned up front so the acquisition queue is never blocked on the ui
                let current = watch_current.borrow_and_update().clone();

                ui_progress.set_queued(*watch_queued.borrow_and_update());
                ui_progress.set_completed(*watch_completed.borrow_and_update());
                ui_progress.set_skipped(*watch_skipped.borrow_and_update());
                ui_progress.set_failed(*watch_failed.borrow_and_update());
                ui_progress.set_total(*watch_total.borrow_and_update());
                ui_progress.set_complete(*watch_complete.borrow_and_update());
                ui_progress.set_current(current.as_str().into());

                select! {
                    Ok(_) = watch_queued.changed() => {},
                    Ok(_) = watch_completed.changed() => {},
                    Ok(_) = watch_skipped.changed() => {},
                    Ok(_) = watch_failed.changed() => {},
                    Ok(_) = watch_total.changed() => {},
                    Ok(_) = watch_complete.changed() => {},
                    Ok(_) = watch_current.changed() => {},
                    else => {break;}
                }
            }
        }));
    }
}
