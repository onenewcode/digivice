//! Power rail monitoring (ADC1 GPIO0), charging state (GPIO23), and BOOT key (GPIO9)

use esp_hal::Blocking;
use esp_hal::analog::adc::{Adc, AdcConfig, AdcPin, Attenuation};
use esp_hal::gpio::{Input, InputConfig, Pull};
use esp_hal::peripherals::{ADC1, GPIO0, GPIO9, GPIO23};

pub struct PowerKeyMonitor<'d> {
    adc: Adc<'d, ADC1<'d>, Blocking>,
    pin_cal: AdcPin<GPIO0<'d>, ADC1<'d>>,
    bc_state_pin: Input<'d>,
    boot_pin: Input<'d>,
}

impl<'d> PowerKeyMonitor<'d> {
    pub fn new(adc1: ADC1<'d>, gpio0: GPIO0<'d>, gpio23: GPIO23<'d>, gpio9: GPIO9<'d>) -> Self {
        let mut config = AdcConfig::<ADC1<'d>>::new();
        let pin_cal = config.enable_pin(gpio0, Attenuation::_11dB);
        let adc = Adc::new(adc1, config);

        let bc_state_pin = Input::new(gpio23, InputConfig::default().with_pull(Pull::Up));
        let boot_pin = Input::new(gpio9, InputConfig::default().with_pull(Pull::Up));

        Self {
            adc,
            pin_cal,
            bc_state_pin,
            boot_pin,
        }
    }

    /// Read raw ADC and convert to millivolts on the power rail.
    /// Voltage divider: R1 = 200k, R8 = 100k -> V_rail = V_adc * 3.0
    pub fn sample_v_rail_mv(&mut self) -> u16 {
        let raw: u16 = self.adc.read_oneshot(&mut self.pin_cal).unwrap_or(0);
        // At 11dB attenuation, max scale ~3100mV across 4096 counts (approx 0.757 mV/count)
        // Calibrated voltage: mV_adc = raw * 3100 / 4095
        // V_rail = mV_adc * 3000 / 1000 = raw * 3100 * 3 / 4095 = raw * 9300 / 4095
        ((raw as u32) * 9300 / 4095) as u16
    }

    /// Read ETA6098 charging status (GPIO23).
    /// Low = Charging in progress, High = Standby / Full / Not charging
    pub fn is_charging(&self) -> bool {
        self.bc_state_pin.is_low()
    }

    /// Read ETA6098 raw level (true if High)
    pub fn bc_state_high(&self) -> bool {
        self.bc_state_pin.is_high()
    }

    /// Check if BOOT button (GPIO9) is pressed (active low)
    pub fn is_boot_pressed(&self) -> bool {
        self.boot_pin.is_low()
    }

    /// Borrow the boot pin for async waiting
    pub fn boot_pin(&mut self) -> &mut Input<'d> {
        &mut self.boot_pin
    }
}
