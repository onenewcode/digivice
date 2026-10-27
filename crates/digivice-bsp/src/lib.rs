//! Board Support Package (BSP) for Waveshare ESP32-C6-LCD-1.28

#![no_std]
#![forbid(unsafe_code)]
#![deny(warnings)]
#![deny(clippy::large_stack_frames)]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is prohibited in embedded systems with HAL-managed resources"
)]

pub mod display;
pub mod power_key;

pub use digivice_core::drivers::imu;
pub use digivice_core::drivers::rtc;

use core::cell::RefCell;
use critical_section::Mutex;
use embedded_hal_bus::i2c::CriticalSectionDevice;
use esp_hal::Blocking;
use esp_hal::gpio::{Input, InputConfig, Pull};
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use esp_hal::peripherals::{
    ADC1, FLASH, GPIO0, GPIO4, GPIO5, GPIO6, GPIO7, GPIO8, GPIO9, GPIO10, GPIO11, GPIO15, GPIO18,
    GPIO19, GPIO20, GPIO21, GPIO23, I2C0, LEDC, SPI2,
};
use esp_hal::time::Rate;
use static_cell::{ConstStaticCell, StaticCell};

pub type StorageManager<'d> = digivice_core::storage::StorageManager<esp_storage::FlashStorage<'d>>;
pub use digivice_core::domain::{ButtonDebouncer, ButtonPressState};
pub use display::{
    BAND_BUFFER_SIZE, Backlight, BandDisplay, BandTarget, DISPLAY_HEIGHT, DISPLAY_WIDTH,
    DisplayPins,
};
pub use imu::{ImuAttitudeSnapshot, ImuError, ImuSample, ImuStatus, Qmi8658};
pub use power_key::{ChargeState, PowerKeyMonitor, PowerStatus, VoltageValidity};
pub use rtc::{DateTime, Pcf85063, RtcError, RtcStatus, datetime_to_epoch, epoch_to_datetime};

pub type SharedI2c = Mutex<RefCell<I2c<'static, Blocking>>>;
pub type SharedI2cDevice<'a> = CriticalSectionDevice<'a, I2c<'static, Blocking>>;

static SHARED_I2C: StaticCell<SharedI2c> = StaticCell::new();
static DISPLAY_BAND_BUFFER: ConstStaticCell<[u16; BAND_BUFFER_SIZE]> =
    ConstStaticCell::new([0u16; BAND_BUFFER_SIZE]);

pub struct BoardPeripherals<'d> {
    pub flash: FLASH<'d>,
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
}

pub struct Board<'d> {
    pub display: BandDisplay<'d>,
    pub backlight: Backlight<'d>,
    pub imu: Qmi8658<SharedI2cDevice<'static>>,
    pub rtc: Pcf85063<SharedI2cDevice<'static>>,
    pub power_key: PowerKeyMonitor<'d>,
    pub storage: StorageManager<'d>,
    pub imu_int1: Input<'d>,
    pub imu_int2: Input<'d>,
    pub rtc_int: Input<'d>,
}

impl Board<'static> {
    pub fn init(peri: BoardPeripherals<'static>) -> Self {
        // 1. Backlight
        let backlight = Backlight::new(peri.ledc, peri.gpio7);

        // 2. Display (SPI2) with static band buffer allocation
        let band_buffer = DISPLAY_BAND_BUFFER.take();
        let display = BandDisplay::new(
            peri.spi2,
            DisplayPins {
                sck: peri.gpio11,
                mosi: peri.gpio21,
                cs: peri.gpio10,
                dc: peri.gpio19,
                rst: peri.gpio20,
                sd_cs: peri.gpio15,
            },
            band_buffer,
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

        // 4. IMU & RTC Drivers (Fault-tolerant initialization with graceful degradation)
        let imu = Qmi8658::new(imu_device);
        let rtc = Pcf85063::new(rtc_device);

        // 5. Power rail ADC & BOOT key
        let power_key = PowerKeyMonitor::new(peri.adc1, peri.gpio0, peri.gpio23, peri.gpio9);

        // 6. Flash Storage Manager
        let storage = StorageManager::new(esp_storage::FlashStorage::new(peri.flash));

        // 7. Interrupt Inputs
        let imu_int1 = Input::new(peri.gpio5, InputConfig::default().with_pull(Pull::Down));
        let imu_int2 = Input::new(peri.gpio4, InputConfig::default().with_pull(Pull::Down));
        let rtc_int = Input::new(peri.gpio6, InputConfig::default().with_pull(Pull::Up));

        Self {
            display,
            backlight,
            imu,
            rtc,
            power_key,
            storage,
            imu_int1,
            imu_int2,
            rtc_int,
        }
    }
}
