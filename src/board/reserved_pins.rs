//! Type-level seal and ownership sink for ESP32-C6 internal strapping & SPI flash pins.

use esp_hal::peripherals::{GPIO24, GPIO25, GPIO26, GPIO27, GPIO28, GPIO29, GPIO30};

/// Zero-access, non-constructible type guard for reserved hardware pins.
///
/// On ESP32-C6 (specifically ESP32-C6-WROOM-1 / ESP32-C6-LCD-1.28):
/// - GPIO24 ~ GPIO30 are internally connected to SPI Flash (and PSRAM) or reserved strapping functions.
/// - Reconfiguring or outputting on these pins corrupts flash bus transactions or causes CPU cache exceptions.
///
/// By enforcing that `Board::init` consumes these peripheral singletons into private, unexposed fields,
/// the Rust compiler's linear type system guarantees at compile time that these pins cannot be borrowed,
/// re-initialized, or aliased anywhere in the application.
pub struct ReservedPins {
    _p24: GPIO24<'static>,
    _p25: GPIO25<'static>,
    _p26: GPIO26<'static>,
    _p27: GPIO27<'static>,
    _p28: GPIO28<'static>,
    _p29: GPIO29<'static>,
    _p30: GPIO30<'static>,
}

impl ReservedPins {
    /// Seal and consume the reserved strapping pins into an inaccessible ownership sink.
    #[inline(always)]
    pub(crate) const fn seal(
        p24: GPIO24<'static>,
        p25: GPIO25<'static>,
        p26: GPIO26<'static>,
        p27: GPIO27<'static>,
        p28: GPIO28<'static>,
        p29: GPIO29<'static>,
        p30: GPIO30<'static>,
    ) -> Self {
        Self {
            _p24: p24,
            _p25: p25,
            _p26: p26,
            _p27: p27,
            _p28: p28,
            _p29: p29,
            _p30: p30,
        }
    }
}
