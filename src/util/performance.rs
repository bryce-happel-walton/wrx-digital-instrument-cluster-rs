#[cfg(debug_assertions)]
use std::sync::LazyLock;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
#[cfg(debug_assertions)]
use std::time::Instant;

#[cfg(debug_assertions)]
static EPOCH: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Frames written by the simulator since the last sample
#[cfg(debug_assertions)]
static FRAMES_SENT: AtomicU32 = AtomicU32::new(0);
/// Frames drained by the CAN read loop since the last sample
#[cfg(debug_assertions)]
static FRAMES_PARSED: AtomicU32 = AtomicU32::new(0);
/// Cumulative count of CAN frames lost
#[cfg(debug_assertions)]
static FRAMES_LOST: AtomicU64 = AtomicU64::new(0);
/// Nanos (since `EPOCH`) at which the most recent frame was parsed
#[cfg(debug_assertions)]
static LAST_PARSE_NS: AtomicU64 = AtomicU64::new(0);
/// Sum of parse->UI latencies (ns) observed since the last sample
#[cfg(debug_assertions)]
static UI_LATENCY_SUM_NS: AtomicU64 = AtomicU64::new(0);
/// Count of parse->UI latency samples since the last sample
#[cfg(debug_assertions)]
static UI_LATENCY_COUNT: AtomicU32 = AtomicU32::new(0);
/// Worst parse->UI latency (ns) observed since the last sample
#[cfg(debug_assertions)]
static UI_LATENCY_MAX_NS: AtomicU64 = AtomicU64::new(0);

#[cfg(debug_assertions)]
fn now_ns() -> u64 {
    EPOCH.elapsed().as_nanos() as u64
}

pub fn record_can_frame_sent() {
    #[cfg(debug_assertions)]
    FRAMES_SENT.fetch_add(1, Ordering::Relaxed);
}

pub fn record_can_frame_parsed() {
    #[cfg(debug_assertions)]
    {
        FRAMES_PARSED.fetch_add(1, Ordering::Relaxed);
        LAST_PARSE_NS.store(now_ns(), Ordering::Relaxed);
    }
}

pub fn record_can_frame_lost() {
    #[cfg(debug_assertions)]
    FRAMES_LOST.fetch_add(1, Ordering::Relaxed);
}

#[cfg(debug_assertions)]
pub fn last_parse_instant_ns() -> u64 {
    LAST_PARSE_NS.load(Ordering::Relaxed)
}

#[cfg(debug_assertions)]
pub fn record_ui_latency(parse_ns: u64) {
    if parse_ns == 0 {
        return;
    }
    let latency = now_ns().saturating_sub(parse_ns);
    UI_LATENCY_SUM_NS.fetch_add(latency, Ordering::Relaxed);
    UI_LATENCY_COUNT.fetch_add(1, Ordering::Relaxed);
    UI_LATENCY_MAX_NS.fetch_max(latency, Ordering::Relaxed);
}

#[cfg(debug_assertions)]
pub struct Sample {
    pub sent_per_sec: i32,
    pub parsed_per_sec: i32,
    pub ui_latency_avg_us: i32,
    pub ui_latency_max_us: i32,
    pub total_frames_lost: i32,
}

#[cfg(debug_assertions)]
pub fn sample(elapsed_secs: f32) -> Sample {
    let sent = FRAMES_SENT.swap(0, Ordering::Relaxed) as f32;
    let parsed = FRAMES_PARSED.swap(0, Ordering::Relaxed) as f32;

    let sum_ns = UI_LATENCY_SUM_NS.swap(0, Ordering::Relaxed);
    let count = UI_LATENCY_COUNT.swap(0, Ordering::Relaxed);
    let max_ns = UI_LATENCY_MAX_NS.swap(0, Ordering::Relaxed);
    let lost = FRAMES_LOST.load(Ordering::Relaxed);

    let avg_ns = if count > 0 { sum_ns / count as u64 } else { 0 };

    Sample {
        sent_per_sec: (sent / elapsed_secs) as i32,
        parsed_per_sec: (parsed / elapsed_secs) as i32,
        ui_latency_avg_us: (avg_ns / 1_000) as i32,
        ui_latency_max_us: (max_ns / 1_000) as i32,
        total_frames_lost: lost.min(i32::MAX as u64) as i32,
    }
}
