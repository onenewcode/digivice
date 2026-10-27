//! Power and battery domain models.

/// Strongly-typed battery charging state to eliminate boolean blindness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChargeState {
    #[default]
    StandbyOrDone,
    Charging,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VoltageValidity {
    Valid,
    #[default]
    ConversionFailed,
    BelowRange,
    AboveRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoltageMeasurement {
    pub millivolts: u16,
    pub validity: VoltageValidity,
}

impl VoltageMeasurement {
    pub const fn from_adc(raw: u16) -> Self {
        let millivolts = ((raw as u32 * 7_500) / 4_095) as u16;
        let validity = if raw == 0 || millivolts < 3_000 {
            VoltageValidity::BelowRange
        } else if millivolts > 4_350 {
            VoltageValidity::AboveRange
        } else {
            VoltageValidity::Valid
        };
        Self {
            millivolts,
            validity,
        }
    }
}

/// Debounced active-low BOOT key state, independent of ADC sampling cadence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonDebouncer {
    stable: ButtonPressState,
    candidate: Option<ButtonPressState>,
    candidate_since: embassy_time::Instant,
}

/// Physical BOOT key state shared by the board adapter and application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonPressState {
    #[default]
    Released,
    Pressed,
}

impl ButtonPressState {
    pub const fn is_pressed(self) -> bool {
        matches!(self, Self::Pressed)
    }
}

impl From<bool> for ButtonPressState {
    fn from(pressed: bool) -> Self {
        if pressed {
            Self::Pressed
        } else {
            Self::Released
        }
    }
}

impl From<ButtonPressState> for bool {
    #[inline]
    fn from(state: ButtonPressState) -> Self {
        state.is_pressed()
    }
}

impl ButtonDebouncer {
    pub const DEBOUNCE_MS: u64 = 30;

    pub const fn new(initial: ButtonPressState, now: embassy_time::Instant) -> Self {
        Self {
            stable: initial,
            candidate: None,
            candidate_since: now,
        }
    }

    pub const fn state(self) -> ButtonPressState {
        self.stable
    }

    pub fn update(
        &mut self,
        observed: ButtonPressState,
        now: embassy_time::Instant,
    ) -> Option<ButtonPressState> {
        if observed == self.stable {
            self.candidate = None;
            return None;
        }
        if self.candidate != Some(observed) {
            self.candidate = Some(observed);
            self.candidate_since = now;
            return None;
        }
        if now
            .checked_duration_since(self.candidate_since)
            .is_some_and(|elapsed| {
                elapsed >= embassy_time::Duration::from_millis(Self::DEBOUNCE_MS)
            })
        {
            self.stable = observed;
            self.candidate = None;
            Some(observed)
        } else {
            None
        }
    }
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
