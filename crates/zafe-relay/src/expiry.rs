pub const DAY_SECS: u64 = 86_400;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Expiry {
    pub idle_days: Option<u32>,
    pub unused_days: Option<u32>,
}

impl Expiry {
    pub const fn none() -> Self {
        Self {
            idle_days: None,
            unused_days: None,
        }
    }

    pub const fn hosted() -> Self {
        Self {
            idle_days: Some(365),
            unused_days: Some(30),
        }
    }

    pub fn is_off(&self) -> bool {
        self.idle_days.is_none() && self.unused_days.is_none()
    }
}

pub(crate) fn day(secs: u64) -> i64 {
    i64::try_from(secs / DAY_SECS).unwrap_or(i64::MAX)
}
