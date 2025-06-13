use std::num::{NonZero, NonZeroU32};

pub(super) const BASE_URL: &str = "https://api.warframe.market/v2";
pub(super) const V1_API: &str = "https://api.warframe.market/v1";
pub(super) const REQUESTS_PER_SECOND: NonZeroU32 = NonZero::new(3).unwrap();