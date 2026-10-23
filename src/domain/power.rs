//! Power and battery domain models.

/// Strongly-typed battery charging state to eliminate boolean blindness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChargeState {
    #[default]
    StandbyOrDone,
    Charging,
}

impl ChargeState {
    #[inline]
    pub fn is_charging(&self) -> bool {
        matches!(self, Self::Charging)
    }

    #[inline]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Charging => "CHG",
            Self::StandbyOrDone => "HI",
        }
    }
}
