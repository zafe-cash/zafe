use std::{collections::HashMap, sync::Mutex};

use zafe_proto::relay::PushPlatform;

pub const PUSH_WINDOW_SECS: u64 = 30;

struct Slot {
    platform: PushPlatform,
    sent_at: u64,
    pending: bool,
}

#[derive(Default)]
pub(crate) struct Pushes {
    slots: Mutex<HashMap<String, Slot>>,
}

impl Pushes {
    pub(crate) fn admit(&self, platform: PushPlatform, token: &str, now: u64) -> bool {
        let mut slots = self.slots.lock().expect("lock");
        match slots.get_mut(token) {
            Some(slot) if now < slot.sent_at.saturating_add(PUSH_WINDOW_SECS) => {
                slot.platform = platform;
                slot.pending = true;
                false
            }
            Some(slot) => {
                slot.platform = platform;
                slot.sent_at = now;
                slot.pending = false;
                true
            }
            None => {
                slots.insert(
                    token.to_owned(),
                    Slot {
                        platform,
                        sent_at: now,
                        pending: false,
                    },
                );
                true
            }
        }
    }

    pub(crate) fn due(&self, now: u64) -> Vec<(PushPlatform, String)> {
        let mut slots = self.slots.lock().expect("lock");
        let mut out = Vec::new();
        slots.retain(|token, slot| {
            let ended = now >= slot.sent_at.saturating_add(PUSH_WINDOW_SECS);
            if !ended {
                return true;
            }
            if slot.pending {
                out.push((slot.platform, token.clone()));
                slot.sent_at = now;
                slot.pending = false;
                return true;
            }
            false
        });
        out
    }

    pub(crate) fn forget(&self, token: &str) {
        self.slots.lock().expect("lock").remove(token);
    }
}
