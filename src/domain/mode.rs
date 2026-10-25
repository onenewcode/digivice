//! Device operating view modes for the Digivice Cyberpet.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    Pet,
    Status,
}

impl From<ViewMode> for u8 {
    fn from(mode: ViewMode) -> Self {
        match mode {
            ViewMode::Pet => 0,
            ViewMode::Status => 1,
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
            0 | 2 | 3 => Ok(ViewMode::Pet),
            1 => Ok(ViewMode::Status),
            other => Err(InvalidViewMode(other)),
        }
    }
}

impl ViewMode {
    /// Navigate to the view to the left.
    pub const fn prev(self) -> Self {
        match self {
            Self::Pet => Self::Status,
            Self::Status => Self::Pet,
        }
    }

    /// Navigate to the view to the right.
    pub const fn next(self) -> Self {
        match self {
            Self::Pet => Self::Status,
            Self::Status => Self::Pet,
        }
    }

    /// Return the next view mode in circular sequence (toggles Pet <-> Status).
    pub const fn cycle(self) -> Self {
        match self {
            Self::Pet => Self::Status,
            Self::Status => Self::Pet,
        }
    }
}
