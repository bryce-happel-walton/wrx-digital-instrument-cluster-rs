use crate::can::protocol::isotp::ISOTPService;

use super::can_backend::CanFrame;
use super::protocol::isotp::SERVICE_OFFSET;
use super::protocol::{FrameDirection, MuxedMessage};

use embedded_can::Id;

use std::time::{Duration, Instant};

const DEFAULT_EAVESDROP_GRACE: Duration = Duration::from_millis(500);
const DEFAULT_RETRY_INTERVAL: Duration = Duration::from_secs(1);

pub struct Acquisition {
    pub label: String,
    request_id: Id,
    request: Vec<u8>,
    matcher: Box<dyn Fn(&MuxedMessage) -> bool + Send + 'static>,
    eavesdrop_grace: Duration,
    retry_interval: Duration,
    created_at: Instant,
    last_attempt: Option<Instant>,
}

impl Acquisition {
    pub fn new(
        label: impl Into<String>,
        request_id: Id,
        request: impl Into<Vec<u8>>,
        matcher: impl Fn(&MuxedMessage) -> bool + Send + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            request_id,
            request: request.into(),
            matcher: Box::new(matcher),
            eavesdrop_grace: DEFAULT_EAVESDROP_GRACE,
            retry_interval: DEFAULT_RETRY_INTERVAL,
            created_at: Instant::now(),
            last_attempt: None,
        }
    }

    pub fn matches(&self, message: &MuxedMessage) -> bool {
        (self.matcher)(message)
    }

    pub fn due_request(&mut self, now: Instant) -> Option<(Id, Vec<u8>)> {
        let eavesdropping = now.saturating_duration_since(self.created_at) < self.eavesdrop_grace;

        let due = match self.last_attempt {
            None => true,
            Some(t) => now.saturating_duration_since(t) >= self.retry_interval,
        };

        if !eavesdropping && due {
            self.last_attempt = Some(now);
            Some((self.request_id, self.request.clone()))
        } else {
            None
        }
    }

    pub fn from_isotp<P: Copy + Into<u8>>(
        request_id: Id,
        service: &dyn ISOTPService,
        pids: &[P],
    ) -> Vec<Self> {
        const PIDS_PER_REQUEST: usize = CanFrame::MAX_FRAME_SIZE - 2;

        let service_byte = service.service_byte();
        let response_service = service_byte + SERVICE_OFFSET;

        let payloads: Vec<Vec<u8>> = if pids.is_empty() {
            vec![vec![service_byte]]
        } else {
            pids.chunks(PIDS_PER_REQUEST)
                .map(|chunk| {
                    let mut payload = vec![service_byte];
                    payload.extend(chunk.iter().map(|&pid| pid.into()));
                    payload
                })
                .collect()
        };

        payloads
            .into_iter()
            .map(|payload| {
                let label = format!("ISOTP {service:?} {:#02X?}", &payload[1..]);
                let request = payload.clone();

                Self::new(label, request_id, request, move |message| {
                    (message.direction == FrameDirection::Response)
                        && (message.data.first() == Some(&response_service))
                        && (message.request.as_deref() == Some(payload.as_slice()))
                })
            })
            .collect()
    }
}
