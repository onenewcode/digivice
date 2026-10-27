//! GC9A01 Display Driver and LEDC Backlight Controller for Waveshare ESP32-C6-LCD-1.28

use display_interface_spi::SPIInterface;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use embedded_hal::delay::DelayNs;
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
use esp_println::println;
use gc9a01::{Gc9a01, display::DisplayResolution240x240, mode::BasicMode, prelude::*};
use static_cell::StaticCell;

/// Send one GC9A01 command and its optional parameter payload.
///
/// The generic `gc9a01-rs` 0.4.x init sequence omits the panel's vendor
/// unlock/power tables.  The Waveshare panel needs the complete sequence used
/// by its reference Arduino driver before accepting frame-memory writes.
fn send_lcd_command(
    interface: &mut impl display_interface::WriteOnlyDataCommand,
    command: u8,
    data: &[u8],
) -> Result<(), display_interface::DisplayError> {
    interface.send_commands(display_interface::DataFormat::U8(&[command]))?;
    if !data.is_empty() {
        interface.send_data(display_interface::DataFormat::U8(data))?;
    }
    Ok(())
}

fn init_waveshare_gc9a01(
    interface: &mut impl display_interface::WriteOnlyDataCommand,
    delay: &mut impl embedded_hal::delay::DelayNs,
) -> Result<(), display_interface::DisplayError> {
    // Vendor unlock and power/gamma setup from Waveshare's GC9A01 reference.
    const COMMANDS: &[(u8, &[u8])] = &[
        (0xEF, &[]),
        (0xEB, &[0x14]),
        (0xFE, &[]),
        (0xEF, &[]),
        (0xEB, &[0x14]),
        (0x84, &[0x40]),
        (0x85, &[0xFF]),
        (0x86, &[0xFF]),
        (0x87, &[0xFF]),
        (0x88, &[0x0A]),
        (0x89, &[0x21]),
        (0x8A, &[0x00]),
        (0x8B, &[0x80]),
        (0x8C, &[0x01]),
        (0x8D, &[0x01]),
        (0x8E, &[0xFF]),
        (0x8F, &[0xFF]),
        (0xB6, &[0x00, 0x20]),
        (0x36, &[0x08]), // BGR color order; final orientation is set below
        (0x3A, &[0x05]),
        (0x90, &[0x08, 0x08, 0x08, 0x08]),
        (0xBD, &[0x06]),
        (0xBC, &[0x00]),
        (0xFF, &[0x60, 0x01, 0x04]),
        (0xC3, &[0x13]),
        (0xC4, &[0x13]),
        (0xC9, &[0x22]),
        (0xBE, &[0x11]),
        (0xE1, &[0x10, 0x0E]),
        (0xDF, &[0x21, 0x0C, 0x02]),
        (0xF0, &[0x45, 0x09, 0x08, 0x08, 0x26, 0x2A]),
        (0xF1, &[0x43, 0x70, 0x72, 0x36, 0x37, 0x6F]),
        (0xF2, &[0x45, 0x09, 0x08, 0x08, 0x26, 0x2A]),
        (0xF3, &[0x43, 0x70, 0x72, 0x36, 0x37, 0x6F]),
        (0xED, &[0x1B, 0x0B]),
        (0xAE, &[0x77]),
        (0xCD, &[0x63]),
        (
            0x70,
            &[0x07, 0x07, 0x04, 0x0E, 0x0F, 0x09, 0x07, 0x08, 0x03],
        ),
        (0xE8, &[0x34]),
        (
            0x62,
            &[
                0x18, 0x0D, 0x71, 0xED, 0x70, 0x70, 0x18, 0x0F, 0x71, 0xEF, 0x70, 0x70,
            ],
        ),
        (
            0x63,
            &[
                0x18, 0x11, 0x71, 0xF1, 0x70, 0x70, 0x18, 0x13, 0x71, 0xF3, 0x70, 0x70,
            ],
        ),
        (0x64, &[0x28, 0x29, 0xF1, 0x01, 0xF1, 0x00, 0x07]),
        (
            0x66,
            &[0x3C, 0x00, 0xCD, 0x67, 0x45, 0x45, 0x10, 0x00, 0x00, 0x00],
        ),
        (
            0x67,
            &[0x00, 0x3C, 0x00, 0x00, 0x00, 0x01, 0x54, 0x10, 0x32, 0x98],
        ),
        (0x74, &[0x10, 0x85, 0x80, 0x00, 0x00, 0x4E, 0x00]),
        (0x98, &[0x3E, 0x07]),
        (0x35, &[]),
        (0x21, &[]), // panel-specific inversion required by this module
    ];

    for &(command, data) in COMMANDS {
        send_lcd_command(interface, command, data)?;
    }
    send_lcd_command(interface, 0x11, &[])?; // Sleep out
    delay.delay_ms(120);
    send_lcd_command(interface, 0x29, &[])?; // Display on
    delay.delay_ms(20);
    send_lcd_command(interface, 0x51, &[0x5F])?; // Normal panel brightness
    send_lcd_command(interface, 0x53, &[0x2C])?; // Brightness control block on
    Ok(())
}

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
        let mut di = SPIInterface::new(spi_device, dc_pin);

        let mut delay = esp_hal::delay::Delay::new();
        // Reset, then run the complete Waveshare panel sequence instead of
        // the abbreviated generic init from gc9a01-rs.
        rst_pin.set_high();
        delay.delay_ms(50);
        rst_pin.set_low();
        delay.delay_ms(50);
        rst_pin.set_high();
        delay.delay_ms(50);
        if let Err(err) = init_waveshare_gc9a01(&mut di, &mut delay) {
            println!("WARN: Failed to init GC9A01 panel: {:?}", err);
        }
        let mut driver = Gc9a01::new(di, DisplayResolution240x240, DisplayRotation::Rotate180);
        if let Err(err) = driver.set_display_rotation(DisplayRotation::Rotate180) {
            println!("WARN: Failed to set display rotation: {:?}", err);
        }

        println!("LCD init complete (SPI2 40MHz, CS=10, DC=19, RST=20)");

        Self {
            driver,
            band_buffer,
            _sd_cs: sd_cs_pin,
        }
    }

    /// Clear entire 240x240 screen to a solid color
    pub fn clear_screen(&mut self, color: Rgb565) {
        let raw = color.into_storage();
        self.band_buffer.fill(raw);
        for band in 0..NUM_BANDS {
            let y_start = (band * BAND_HEIGHT) as u16;
            if let Err(err) = self.flush_band(y_start, BAND_HEIGHT as u16) {
                println!("LCD clear band {} failed: {:?}", band, err);
            }
        }
    }

    /// Flush the band buffer to GC9A01 at specified Y range
    pub fn flush_band(
        &mut self,
        y_start: u16,
        height: u16,
    ) -> Result<(), display_interface::DisplayError> {
        let Some(y_end_exclusive) = y_start.checked_add(height) else {
            return Err(display_interface::DisplayError::OutOfBoundsError);
        };
        if height == 0
            || height > BAND_HEIGHT as u16
            || y_start >= DISPLAY_HEIGHT
            || y_end_exclusive > DISPLAY_HEIGHT
        {
            return Err(display_interface::DisplayError::OutOfBoundsError);
        }
        let y_end = y_end_exclusive - 1;
        self.driver
            .set_draw_area((0, y_start), (DISPLAY_WIDTH - 1, y_end))?;
        self.driver.set_write_mode()?;
        let num_pixels = (DISPLAY_WIDTH as usize) * (height as usize);
        self.driver.draw_buffer(&self.band_buffer[..num_pixels])?;
        Ok(())
    }

    /// Render a full frame band by band
    pub fn render_full_frame<F>(
        &mut self,
        bg_color: Rgb565,
        mut render_fn: F,
    ) -> Result<(), display_interface::DisplayError>
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
            self.flush_band(y_offset as u16, BAND_HEIGHT as u16)?;
        }
        Ok(())
    }

    /// Render only the bands intersecting the given Y range [y_min, y_max]
    pub fn render_partial<F>(
        &mut self,
        y_min: u16,
        y_max: u16,
        bg_color: Rgb565,
        mut render_fn: F,
    ) -> Result<(), display_interface::DisplayError>
    where
        F: FnMut(&mut BandTarget),
    {
        if y_min > y_max || y_max >= DISPLAY_HEIGHT {
            return Err(display_interface::DisplayError::OutOfBoundsError);
        }
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
            self.flush_band(y_offset as u16, BAND_HEIGHT as u16)?;
        }
        Ok(())
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
