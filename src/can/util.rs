use embedded_can::{ExtendedId, Id, StandardId};

pub fn raw_id(id: Id) -> u32 {
    match id {
        Id::Standard(id) => id.as_raw() as u32,
        Id::Extended(id) => id.as_raw(),
    }
}

pub fn make_id(from: Id, raw: u32) -> Id {
    match from {
        Id::Standard(_) => match StandardId::new(raw as u16) {
            Some(id) => Id::Standard(id),
            None => Id::Standard(StandardId::ZERO),
        },
        Id::Extended(_) => match ExtendedId::new(raw) {
            Some(id) => Id::Extended(id),
            None => Id::Extended(ExtendedId::ZERO),
        },
    }
}

/// use to search for known patterns in unknown signals
// TODO: just use bitvec
#[allow(unused)]
pub fn search_payload_unaligned(payload: &[u8], pattern: u64) -> bool {
    let search_len = pattern.ilog2() + 1;
    let mut current = 0u64;

    for &byte in payload {
        for b in 0u8..8u8 {
            current = (current << 1) | ((byte >> (7 - b)) & 1) as u64;
            current &= (1 << search_len) - 1;

            if current == pattern {
                return true;
            }
        }
    }

    false
}
