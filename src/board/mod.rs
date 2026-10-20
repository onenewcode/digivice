//! Board Support Package (BSP) for Waveshare ESP32-C6-LCD-1.28

pub mod display;
pub mod imu;
pub mod power_key;
pub mod reserved_pins;
pub mod rtc;

use core::cell::RefCell;
use critical_section::Mutex;
use embedded_hal_bus::i2c::CriticalSectionDevice;
use esp_hal::Blocking;
use esp_hal::gpio::{Input, InputConfig, Pull};
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use esp_hal::peripherals::{
    ADC1, GPIO0, GPIO4, GPIO5, GPIO6, GPIO7, GPIO8, GPIO9, GPIO10, GPIO11, GPIO15, GPIO18, GPIO19,
    GPIO20, GPIO21, GPIO23, GPIO24, GPIO25, GPIO26, GPIO27, GPIO28, GPIO29, GPIO30, I2C0, LEDC,
    SPI2,
};
use esp_hal::time::Rate;
use static_cell::StaticCell;

pub use display::{Backlight, BandDisplay, BandTarget, DISPLAY_HEIGHT, DISPLAY_WIDTH};
pub use imu::{ImuError, ImuSample, Qmi8658};
pub use power_key::PowerKeyMonitor;
pub use reserved_pins::ReservedPins;
pub use rtc::{DateTime, Pcf85063, RtcStatus, datetime_to_epoch, epoch_to_datetime};

pub type SharedI2c = Mutex<RefCell<I2c<'static, Blocking>>>;
pub type SharedI2cDevice<'a> = CriticalSectionDevice<'a, I2c<'static, Blocking>>;

static SHARED_I2C: StaticCell<SharedI2c> = StaticCell::new();

pub struct BoardPeripherals<'d> {
    pub spi2: SPI2<'d>,
    pub ledc: LEDC<'static>,
    pub i2c0: I2C0<'d>,
    pub adc1: ADC1<'d>,
    pub gpio0: GPIO0<'d>,
    pub gpio4: GPIO4<'d>,
    pub gpio5: GPIO5<'d>,
    pub gpio6: GPIO6<'d>,
    pub gpio7: GPIO7<'d>,
    pub gpio8: GPIO8<'d>,
    pub gpio9: GPIO9<'d>,
    pub gpio10: GPIO10<'d>,
    pub gpio11: GPIO11<'d>,
    pub gpio15: GPIO15<'d>,
    pub gpio18: GPIO18<'d>,
    pub gpio19: GPIO19<'d>,
    pub gpio20: GPIO20<'d>,
    pub gpio21: GPIO21<'d>,
    pub gpio23: GPIO23<'d>,
    // Mandatory handover of bootstrapping / internal flash pins
    pub gpio24: GPIO24<'d>,
    pub gpio25: GPIO25<'d>,
    pub gpio26: GPIO26<'d>,
    pub gpio27: GPIO27<'d>,
    pub gpio28: GPIO28<'d>,
    pub gpio29: GPIO29<'d>,
    pub gpio30: GPIO30<'d>,
}

pub struct Board<'d> {
    pub display: BandDisplay<'d>,
    pub backlight: Backlight<'d>,
    pub imu: Qmi8658<SharedI2cDevice<'static>>,
    pub rtc: Pcf85063<SharedI2cDevice<'static>>,
    pub power_key: PowerKeyMonitor<'d>,
    pub imu_int1: Input<'d>,
    pub imu_int2: Input<'d>,
    pub rtc_int: Input<'d>,
    _reserved_pins: ReservedPins,
}

impl Board<'static> {
    pub fn init(peri: BoardPeripherals<'static>) -> Self {
        // 1. Backlight
        let backlight = Backlight::new(peri.ledc, peri.gpio7);

        // 2. Display (SPI2)
        let display = BandDisplay::new(
            peri.spi2,
            peri.gpio11, // SCK
            peri.gpio21, // MOSI
            peri.gpio10, // CS
            peri.gpio19, // DC
            peri.gpio20, // RST
            peri.gpio15, // SD_CS (deselect)
        );

        // 3. Shared I2C0 for IMU & RTC
        let i2c_config = I2cConfig::default().with_frequency(Rate::from_khz(400));
        let i2c = I2c::new(peri.i2c0, i2c_config)
            .expect("Failed to init I2C0")
            .with_scl(peri.gpio8)
            .with_sda(peri.gpio18);

        let shared_i2c = SHARED_I2C.init(Mutex::new(RefCell::new(i2c)));
        let imu_device = CriticalSectionDevice::new(shared_i2c);
        let rtc_device = CriticalSectionDevice::new(shared_i2c);

        // 4. IMU & RTC Drivers
        let imu = Qmi8658::new(imu_device).expect("QMI8658 init failed");
        let rtc = Pcf85063::new(rtc_device);

        // 5. Power rail ADC & BOOT key
        let power_key = PowerKeyMonitor::new(peri.adc1, peri.gpio0, peri.gpio23, peri.gpio9);

        // 6. Interrupt Inputs
        let imu_int1 = Input::new(peri.gpio4, InputConfig::default().with_pull(Pull::Down));
        let imu_int2 = Input::new(peri.gpio5, InputConfig::default().with_pull(Pull::Down));
        let rtc_int = Input::new(peri.gpio6, InputConfig::default().with_pull(Pull::Up));

        let reserved_pins = ReservedPins::seal(
            peri.gpio24,
            peri.gpio25,
            peri.gpio26,
            peri.gpio27,
            peri.gpio28,
            peri.gpio29,
            peri.gpio30,
        );

        Self {
            display,
            backlight,
            imu,
            rtc,
            power_key,
            imu_int1,
            imu_int2,
            rtc_int,
            _reserved_pins: reserved_pins,
        }
    }
}
