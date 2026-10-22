//! Strongly-typed physical orientation of the device to eliminate boolean blindness.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeviceOrientation {
    #[default]
    FaceUp,
    FaceDown,
}

impl DeviceOrientation {
    #[inline]
    pub const fn is_facedown(&self) -> bool {
        matches!(self, Self::FaceDown)
    }
}

impl From<bool> for DeviceOrientation {
    #[inline]
    fn from(facedown: bool) -> Self {
        if facedown {
            Self::FaceDown
        } else {
            Self::FaceUp
        }
    }
}
