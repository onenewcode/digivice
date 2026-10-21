//! GC9A01 Display Driver and LEDC Backlight Controller for Waveshare ESP32-C6-LCD-1.28

use display_interface_spi::SPIInterface;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use embedded_hal_bus::spi::{ExclusiveDevice, NoDelay};
use esp_hal::Blocking;
use esp_hal::gpio::{DriveMode, Level, Output, OutputConfig};
use esp_hal::ledc::{
    LSGlobalClkSource, Ledc, LowSpeed,
    channel::{self, ChannelIFace},
    timer::{self, TimerIFace},
};
use esp_hal::peripherals::{GPIO7, GPIO10, GPIO11, GPIO15, GPIO19, GPIO20, GPIO21, LEDC, SPI2};
use esp_hal::spi::Mode;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::time::Rate;
use gc9a01::{Gc9a01, display::DisplayResolution240x240, mode::BasicMode, prelude::*};
use static_cell::StaticCell;

pub const DISPLAY_WIDTH: u16 = 240;
pub const DISPLAY_HEIGHT: u16 = 240;
pub const BAND_HEIGHT: usize = 20;
pub const NUM_BANDS: usize = 12; // 240 / 20 = 12 bands
pub const BAND_BUFFER_SIZE: usize = (DISPLAY_WIDTH as usize) * BAND_HEIGHT;

pub type DisplayInterface<'d> =
    SPIInterface<ExclusiveDevice<Spi<'d, Blocking>, Output<'d>, NoDelay>, Output<'d>>;

static LEDC_TIMER: StaticCell<timer::Timer<'static, LowSpeed>> = StaticCell::new();

/// Backlight controller using LEDC PWM (GPIO7)
pub struct Backlight<'d> {
    channel: channel::Channel<'d, LowSpeed>,
    current_brightness: u8,
}

impl<'d> Backlight<'d> {
    pub fn new(ledc_peri: LEDC<'static>, gpio7: GPIO7<'d>) -> Self {
        let mut ledc = Ledc::new(ledc_peri);
        ledc.set_global_slow_clock(LSGlobalClkSource::APBClk);

        let mut timer = ledc.timer::<LowSpeed>(timer::Number::Timer0);
        timer
            .configure(timer::config::Config {
                duty: timer::config::Duty::Duty8Bit,
                clock_source: timer::LSClockSource::APBClk,
                frequency: Rate::from_khz(20),
            })
            .expect("Failed to configure LEDC timer");

        let timer_ref: &'static timer::Timer<'static, LowSpeed> = LEDC_TIMER.init(timer);

        let mut channel = ledc.channel(channel::Number::Channel0, gpio7);
        channel
            .configure(channel::config::Config {
                timer: timer_ref,
                duty_pct: 80,
                drive_mode: DriveMode::PushPull,
            })
            .expect("Failed to configure LEDC channel");

        Self {
            channel,
            current_brightness: 80,
        }
    }

    /// Set backlight brightness percentage (0..=100)
    pub fn set_brightness(&mut self, percent: u8) {
        let p = percent.min(100);
        self.current_brightness = p;
        let _ = self.channel.set_duty(p);
    }

    pub fn get_brightness(&self) -> u8 {
        self.current_brightness
    }
}

pub struct DisplayPins<'d> {
    pub sck: GPIO11<'d>,
    pub mosi: GPIO21<'d>,
    pub cs: GPIO10<'d>,
    pub dc: GPIO19<'d>,
    pub rst: GPIO20<'d>,
    pub sd_cs: GPIO15<'d>,
}

/// GC9A01 Display Wrapper using statically allocated band buffer reference.
/// Keeping the 9.6KB buffer out of the struct prevents large stack frame moves.
pub struct BandDisplay<'d> {
    pub driver: Gc9a01<DisplayInterface<'d>, DisplayResolution240x240, BasicMode>,
    pub band_buffer: &'d mut [u16; BAND_BUFFER_SIZE],
    _sd_cs: Output<'d>,
}

impl<'d> BandDisplay<'d> {
    pub fn new(
        spi2: SPI2<'d>,
        pins: DisplayPins<'d>,
        band_buffer: &'d mut [u16; BAND_BUFFER_SIZE],
    ) -> Self {
        // Deselect TF card SPI CS to prevent bus contention
        let mut sd_cs_pin = Output::new(pins.sd_cs, Level::High, OutputConfig::default());
        sd_cs_pin.set_high();

        let cs_pin = Output::new(pins.cs, Level::High, OutputConfig::default());
        let dc_pin = Output::new(pins.dc, Level::High, OutputConfig::default());
        let mut rst_pin = Output::new(pins.rst, Level::High, OutputConfig::default());

        let spi = Spi::new(
            spi2,
            SpiConfig::default()
                .with_frequency(Rate::from_mhz(40))
                .with_mode(Mode::_0),
        )
        .expect("Failed to init SPI")
        .with_sck(pins.sck)
        .with_mosi(pins.mosi);

        let spi_device =
            ExclusiveDevice::new_no_delay(spi, cs_pin).expect("Failed to create SpiDevice");
        let di = SPIInterface::new(spi_device, dc_pin);

        let mut driver = Gc9a01::new(di, DisplayResolution240x240, DisplayRotation::Rotate180);

        let mut delay = esp_hal::delay::Delay::new();
        driver
            .reset(&mut rst_pin, &mut delay)
            .expect("Failed to reset display");
        driver.init(&mut delay).expect("Failed to init display");
        driver
            .set_display_rotation(DisplayRotation::Rotate180)
            .expect("Failed to set display rotation");

        let mut display = Self {
            driver,
            band_buffer,
            _sd_cs: sd_cs_pin,
        };

        display.clear_screen(Rgb565::BLACK);
        display
    }

    /// Clear entire 240x240 screen to a solid color
    pub fn clear_screen(&mut self, color: Rgb565) {
        let raw = color.into_storage();
        self.band_buffer.fill(raw);
        for band in 0..NUM_BANDS {
            let y_start = (band * BAND_HEIGHT) as u16;
            let _ = self.flush_band(y_start, BAND_HEIGHT as u16);
        }
    }

    /// Flush the band buffer to GC9A01 at specified Y range
    pub fn flush_band(
        &mut self,
        y_start: u16,
        height: u16,
    ) -> Result<(), display_interface::DisplayError> {
        let y_end = y_start + height - 1;
        self.driver
            .set_draw_area((0, y_start), (DISPLAY_WIDTH - 1, y_end))?;
        self.driver.set_write_mode()?;
        let num_pixels = (DISPLAY_WIDTH as usize) * (height as usize);
        self.driver.draw_buffer(&self.band_buffer[..num_pixels])?;
        Ok(())
    }

    /// Render a full frame band by band
    pub fn render_full_frame<F>(&mut self, bg_color: Rgb565, mut render_fn: F)
    where
        F: FnMut(&mut BandTarget),
    {
        for band in 0..NUM_BANDS {
            let y_offset = (band * BAND_HEIGHT) as i32;
            let mut target = BandTarget {
                buffer: self.band_buffer,
                y_offset,
            };
            target.clear(bg_color).ok();
            render_fn(&mut target);
            let _ = self.flush_band(y_offset as u16, BAND_HEIGHT as u16);
        }
    }

    /// Render only the bands intersecting the given Y range [y_min, y_max]
    pub fn render_partial<F>(&mut self, y_min: u16, y_max: u16, bg_color: Rgb565, mut render_fn: F)
    where
        F: FnMut(&mut BandTarget),
    {
        let start_band = (y_min as usize) / BAND_HEIGHT;
        let end_band = ((y_max as usize) / BAND_HEIGHT).min(NUM_BANDS - 1);

        for band in start_band..=end_band {
            let y_offset = (band * BAND_HEIGHT) as i32;
            let mut target = BandTarget {
                buffer: self.band_buffer,
                y_offset,
            };
            target.clear(bg_color).ok();
            render_fn(&mut target);
            let _ = self.flush_band(y_offset as u16, BAND_HEIGHT as u16);
        }
    }
}

/// Embedded-graphics DrawTarget for a single horizontal band
pub struct BandTarget<'a> {
    pub buffer: &'a mut [u16; BAND_BUFFER_SIZE],
    pub y_offset: i32,
}

impl<'a> OriginDimensions for BandTarget<'a> {
    fn size(&self) -> Size {
        Size::new(DISPLAY_WIDTH as u32, DISPLAY_HEIGHT as u32)
    }
}

impl<'a> DrawTarget for BandTarget<'a> {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(Point { x, y }, color) in pixels {
            if x >= 0
                && x < (DISPLAY_WIDTH as i32)
                && y >= self.y_offset
                && y < self.y_offset + (BAND_HEIGHT as i32)
            {
                let idx = ((y - self.y_offset) as usize) * (DISPLAY_WIDTH as usize) + (x as usize);
                self.buffer[idx] = color.into_storage();
            }
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let band_rect = Rectangle::new(
            Point::new(0, self.y_offset),
            Size::new(DISPLAY_WIDTH as u32, BAND_HEIGHT as u32),
        );
        let intersection = area.intersection(&band_rect);
        if !intersection.is_zero_sized() {
            let color_raw = color.into_storage();
            let x_start = (intersection.top_left.x.max(0) as usize).min(DISPLAY_WIDTH as usize);
            let x_end = ((intersection.top_left.x + intersection.size.width as i32).max(0)
                as usize)
                .min(DISPLAY_WIDTH as usize);
            let y_start = (intersection.top_left.y - self.y_offset).max(0) as usize;
            let y_end =
                ((intersection.top_left.y + intersection.size.height as i32 - self.y_offset).max(0)
                    as usize)
                    .min(BAND_HEIGHT);

            for row in y_start..y_end {
                let start = row * (DISPLAY_WIDTH as usize) + x_start;
                let end = row * (DISPLAY_WIDTH as usize) + x_end;
                if start < end && end <= self.buffer.len() {
                    self.buffer[start..end].fill(color_raw);
                }
            }
        }
        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.buffer.fill(color.into_storage());
        Ok(())
    }
}
