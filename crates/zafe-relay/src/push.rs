//! Push coalescing (security review 2026-11, rule 7, "Pushes"): at most one push per device
//! token per [`PUSH_WINDOW_SECS`], plus one trailing push at the window's end when any were
//! held back.
//!
//! Pushes carry nothing ("vault activity"), share one collapse key, and one token (an FCM
//! installation id) serves every vault on a phone, so the key is the token: a burst across
//! all of a phone's vaults wakes it once. FCM limits sends per target (`QUOTA_EXCEEDED`), so
//! without this one member's writes could spend another member's push allowance. A held
//! push is never dropped: it stays pending until [`Pushes::due`] hands it out, which the
//! relay's ticker calls every few seconds ([`crate::Relay::flush_pushes`]).

use std::{collections::HashMap, sync::Mutex};

use zafe_proto::relay::PushPlatform;

/// At most one push per token in this many seconds.
pub const PUSH_WINDOW_SECS: u64 = 30;

struct Slot {
    platform: PushPlatform,
    /// When the last push to this token went out.
    sent_at: u64,
    /// A push was held back during the window.
    pending: bool,
}

#[derive(Default)]
pub(crate) struct Pushes {
    slots: Mutex<HashMap<String, Slot>>,
}

impl Pushes {
    /// Whether to send a push to `token` now; otherwise it is held for the window's end.
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

    /// The held pushes whose window has ended (each now counts as sent), and forgets tokens
    /// idle for a whole window, so memory stays bounded by recently active tokens.
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

    /// Forgets `token` (FCM said it is gone, or nobody holds it any more).
    pub(crate) fn forget(&self, token: &str) {
        self.slots.lock().expect("lock").remove(token);
    }
}
