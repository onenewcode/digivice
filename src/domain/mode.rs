//! Device operating view modes.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    Pet,
    SelfTest,
}

impl From<ViewMode> for u8 {
    fn from(mode: ViewMode) -> Self {
        match mode {
            ViewMode::Pet => 3,
            ViewMode::SelfTest => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidViewMode(pub u8);

impl core::fmt::Display for InvalidViewMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Invalid ViewMode tag: {}", self.0)
    }
}

impl core::error::Error for InvalidViewMode {}

impl TryFrom<u8> for ViewMode {
    type Error = InvalidViewMode;

    fn try_from(val: u8) -> Result<Self, Self::Error> {
        match val {
            0 | 1 | 3 => Ok(ViewMode::Pet), // Legacy Island/Standby tags open the pet view
            2 => Ok(ViewMode::SelfTest),
            other => Err(InvalidViewMode(other)),
        }
    }
}

impl ViewMode {
    /// Navigate to the diagnostic view to the left.
    pub const fn prev(self) -> Self {
        match self {
            Self::Pet => Self::SelfTest,
            Self::SelfTest => Self::Pet,
        }
    }

    /// Navigate to the pet view to the right.
    pub const fn next(self) -> Self {
        match self {
            Self::SelfTest => Self::Pet,
            Self::Pet => Self::Pet,
        }
    }

    /// Return the next view mode in circular sequence.
    pub const fn cycle(self) -> Self {
        match self {
            Self::SelfTest => Self::Pet,
            Self::Pet => Self::SelfTest,
        }
    }
}
