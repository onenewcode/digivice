//! Power rail monitoring (ADC1 GPIO0), charging state (GPIO23), and BOOT key (GPIO9)

use esp_hal::Blocking;
use esp_hal::analog::adc::{Adc, AdcConfig, AdcPin, Attenuation};
use esp_hal::gpio::{Input, InputConfig, Pull};
use esp_hal::peripherals::{ADC1, GPIO0, GPIO9, GPIO23};

pub use crate::domain::ChargeState;

/// Strongly-typed physical press state of the button to eliminate boolean blindness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonPressState {
    #[default]
    Released,
    Pressed,
}

impl ButtonPressState {
    #[inline]
    pub const fn is_pressed(&self) -> bool {
        matches!(self, Self::Pressed)
    }
}

impl From<bool> for ButtonPressState {
    #[inline]
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

/// Strongly-typed power rail and button status to eliminate primitive/tuple blindness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PowerStatus {
    pub v_rail_mv: u16,
    pub charge_state: ChargeState,
    pub boot_press_state: ButtonPressState,
}

impl PowerStatus {
    #[inline]
    pub fn is_boot_pressed(&self) -> bool {
        self.boot_press_state.is_pressed()
    }
}

pub struct PowerKeyMonitor<'d> {
    adc: Adc<'d, ADC1<'d>, Blocking>,
    pin_cal: AdcPin<GPIO0<'d>, ADC1<'d>>,
    bc_state_pin: Input<'d>,
    boot_pin: Input<'d>,
    filtered_v_rail_mv: u16,
}

impl<'d> PowerKeyMonitor<'d> {
    pub fn new(adc1: ADC1<'d>, gpio0: GPIO0<'d>, gpio23: GPIO23<'d>, gpio9: GPIO9<'d>) -> Self {
        let mut config = AdcConfig::<ADC1<'d>>::new();
        let pin_cal = config.enable_pin(gpio0, Attenuation::_11dB);
        let adc = Adc::new(adc1, config);

        let bc_state_pin = Input::new(gpio23, InputConfig::default().with_pull(Pull::Up));
        let boot_pin = Input::new(gpio9, InputConfig::default().with_pull(Pull::Up));

        let mut monitor = Self {
            adc,
            pin_cal,
            bc_state_pin,
            boot_pin,
            filtered_v_rail_mv: 4180,
        };
        // Pre-warm ADC filter with initial measurement
        monitor.sample_v_rail_mv();
        monitor
    }

    /// Read raw ADC and convert to millivolts on the power rail with EMA smoothing.
    /// Voltage divider: R1 = 200k, R8 = 100k -> V_rail = V_adc * 3.0
    /// At 11dB attenuation, ESP32-C6 linear range is 0..2500mV across 4096 counts.
    pub fn sample_v_rail_mv(&mut self) -> u16 {
        // nb::read_oneshot is non-blocking and returns WouldBlock during active conversion.
        // Poll up to 100 times (~1ms max) to ensure we receive the actual converted value.
        let mut raw = None;
        for _ in 0..100 {
            match self.adc.read_oneshot(&mut self.pin_cal) {
                Ok(val) => {
                    raw = Some(val);
                    break;
                }
                Err(nb::Error::WouldBlock) => {
                    esp_hal::delay::Delay::new().delay_micros(15);
                }
                Err(_) => break,
            }
        }

        if let Some(val) = raw.filter(|&v| v > 0) {
            // Calibrated voltage: mV_adc = raw * 2500 / 4095
            // V_rail = mV_adc * 3 = raw * 7500 / 4095
            let instant_mv = (((val as u32) * 7500) / 4095).clamp(3000, 4350) as u16;
            // Exponential Moving Average filter: 7/8 historical + 1/8 latest sample
            self.filtered_v_rail_mv =
                (((self.filtered_v_rail_mv as u32 * 7) + (instant_mv as u32)) / 8) as u16;
        }

        self.filtered_v_rail_mv
    }

    /// Read ETA6098 charging status (GPIO23).
    /// Low = Charging in progress, High = Standby / Full / Not charging
    pub fn charge_state(&self) -> ChargeState {
        if self.bc_state_pin.is_low() {
            ChargeState::Charging
        } else {
            ChargeState::StandbyOrDone
        }
    }

    /// Read ETA6098 charging flag
    pub fn is_charging(&self) -> bool {
        self.charge_state().is_charging()
    }

    /// Read ETA6098 raw level (true if High)
    pub fn bc_state_high(&self) -> bool {
        self.bc_state_pin.is_high()
    }

    /// Check if BOOT button (GPIO9) is pressed (active low)
    pub fn is_boot_pressed(&self) -> bool {
        self.boot_pin.is_low()
    }

    /// Read comprehensive power and key state in a single strongly-typed struct.
    pub fn read_status(&mut self) -> PowerStatus {
        let v_rail_mv = self.sample_v_rail_mv();
        let charge_state = self.charge_state();
        let boot_press_state = ButtonPressState::from(self.is_boot_pressed());
        PowerStatus {
            v_rail_mv,
            charge_state,
            boot_press_state,
        }
    }

    /// Borrow the boot pin for async waiting
    pub fn boot_pin(&mut self) -> &mut Input<'d> {
        &mut self.boot_pin
    }
}
