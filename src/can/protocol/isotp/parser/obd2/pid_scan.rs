use super::OBD2Service;
use crate::can::acquisition::{Acquisition, AcquisitionGate};

use embedded_can::Id;

/// Whether a PID may be requested, according to the vehicle's support lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PidAvailability {
    /// the vehicle reports the PID
    Supported,
    /// the vehicle reports the PID is absent, or no support list covers it
    Unsupported,
    /// the support list reporting the PID has not been read yet
    Unknown,
}

/// Resolves `pid` against the vehicle's support lists, walking back through
/// every list it depends on: a list is only readable once the list before it
/// reports it, so an unread ancestor leaves the answer
/// [`PidAvailability::Unknown`] and a missing one rules the PID out outright.
///
/// Nothing is requested here — the answer comes from what the lists already
/// cached, which is what makes a scan cost one read per list per vehicle.
pub fn availability(service: OBD2Service, pid: u8) -> PidAvailability {
    let lists = service.support_lists();

    if lists.is_empty() || (lists.first() == Some(&pid)) {
        // a service without support lists is unrestricted, and its first list
        // is always readable
        PidAvailability::Supported
    } else {
        match service.support_list_for(pid) {
            None => PidAvailability::Unsupported,
            Some(list) => match availability(service, list) {
                PidAvailability::Supported => match service.cached_support(pid) {
                    Some(true) => PidAvailability::Supported,
                    Some(false) => PidAvailability::Unsupported,
                    None => PidAvailability::Unknown,
                },
                unreadable => unreadable,
            },
        }
    }
}

pub fn is_supported(service: OBD2Service, pid: u8) -> bool {
    availability(service, pid) == PidAvailability::Supported
}

/// The support-list reads needed before any of `pids` can be judged, ordered
/// so each one unlocks the next. Lists already cached are left out, and a list
/// the vehicle turns out not to publish retires itself along with everything
/// behind it.
///
/// Use this for PIDs that are polled rather than acquired once: the scan tells
/// [`is_supported`] what to allow through.
pub fn support_chain<P: Copy + Into<u8>>(
    request_id: Id,
    service: OBD2Service,
    pids: &[P],
) -> Vec<Acquisition> {
    let lists = service.support_lists();

    pids.iter()
        .filter_map(|&pid| service.support_list_index(pid.into()))
        .max()
        .map(|depth| {
            lists[..=depth]
                .iter()
                .filter(|&&list| !service.support_list_cached(list))
                .flat_map(|&list| {
                    Acquisition::from_isotp(request_id, &service, &[list])
                        .into_iter()
                        .map(move |acquisition| {
                            acquisition
                                .with_gate(move || gate(service, list))
                                .on_abandon(move || service.mark_support_list_missing(list))
                        })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Reads `pids` once the vehicle confirms it supports them, preceded by
/// whatever support lists that judgement needs.
///
/// Each PID is requested on its own rather than batched, because a batch is
/// only as requestable as its least supported member.
pub fn acquire<P: Copy + Into<u8>>(
    request_id: Id,
    service: OBD2Service,
    pids: &[P],
) -> Vec<Acquisition> {
    let mut acquisitions = support_chain(request_id, service, pids);

    acquisitions.extend(pids.iter().flat_map(|&pid| {
        let pid: u8 = pid.into();

        Acquisition::from_isotp(request_id, &service, &[pid])
            .into_iter()
            .map(move |acquisition| acquisition.with_gate(move || gate(service, pid)))
    }));

    acquisitions
}

fn gate(service: OBD2Service, pid: u8) -> AcquisitionGate {
    match availability(service, pid) {
        PidAvailability::Supported => AcquisitionGate::Ready,
        PidAvailability::Unknown => AcquisitionGate::Blocked,
        PidAvailability::Unsupported => AcquisitionGate::Unreachable,
    }
}
