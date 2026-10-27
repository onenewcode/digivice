//! PCF85063ATL Real-Time Clock Driver (I2C address 0x51)
//! Supports automatic fallback if RTC hardware is not soldered on board.

use embassy_time::Instant;
use embedded_hal::i2c::I2c;

use crate::domain::ClockSnapshot;
pub use crate::domain::clock::{
    DateTime, DateTimeError, MAX_SUPPORTED_EPOCH, RtcError, RtcStatus, datetime_to_epoch,
    days_in_month, epoch_to_datetime, is_leap_year,
};

pub const PCF85063_ADDR: u8 = 0x51;

pub const REG_CONTROL_1: u8 = 0x00;
pub const REG_CONTROL_2: u8 = 0x01;
pub const REG_OFFSET: u8 = 0x02;
pub const REG_RAM_BYTE: u8 = 0x03;
pub const REG_SECONDS: u8 = 0x04;
pub const REG_MINUTES: u8 = 0x05;
pub const REG_HOURS: u8 = 0x06;
pub const REG_DAYS: u8 = 0x07;
pub const REG_WEEKDAYS: u8 = 0x08;
pub const REG_MONTHS: u8 = 0x09;
pub const REG_YEARS: u8 = 0x0A;
pub const REG_SECOND_ALARM: u8 = 0x0B;
pub const REG_MINUTE_ALARM: u8 = 0x0C;
pub const REG_HOUR_ALARM: u8 = 0x0D;
pub const REG_TIMER_VALUE: u8 = 0x0F;
pub const REG_TIMER_MODE: u8 = 0x10;

const CONTROL_STOP: u8 = 1 << 5;
const CONTROL_12_HOUR: u8 = 1 << 1;
const CONTROL_EXTERNAL_TEST: u8 = 1 << 7;
const SECONDS_OS: u8 = 1 << 7;

#[derive(Clone, Copy, PartialEq, Eq)]
enum HardwarePresence {
    Missing,
    Detected,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TimeTrust {
    Unset,
    Trusted,
}

pub struct Pcf85063<I2C> {
    i2c: I2C,
    status: RtcStatus,
    presence: HardwarePresence,
    trust: TimeTrust,
    soft_epoch_offset_sec: u64,
    soft_start_instant: Instant,
}

impl<I2C: I2c> Pcf85063<I2C> {
    pub fn new(i2c: I2C) -> Self {
        Self::new_at(i2c, Instant::now())
    }

    pub fn new_at(i2c: I2C, now: Instant) -> Self {
        let mut rtc = Self {
            i2c,
            status: RtcStatus::NeedsSetting(RtcError::NotValidated),
            presence: HardwarePresence::Missing,
            trust: TimeTrust::Unset,
            soft_epoch_offset_sec: datetime_to_epoch(&DateTime::default()),
            soft_start_instant: now,
        };
        // Probing is read-only: clearing STOP cannot recover time lost while stopped.
        if let Err(error) = rtc.read_control() {
            rtc.status = RtcStatus::NeedsSetting(error);
        }
        rtc
    }

    pub fn status(&self) -> RtcStatus {
        self.status
    }

    pub fn is_hardware_present(&self) -> bool {
        self.presence == HardwarePresence::Detected
    }

    pub fn get_datetime(&mut self) -> DateTime {
        self.get_datetime_at(Instant::now())
    }

    pub fn snapshot(&mut self) -> ClockSnapshot {
        self.snapshot_at(Instant::now())
    }

    pub fn snapshot_at(&mut self, now: Instant) -> ClockSnapshot {
        ClockSnapshot {
            datetime: self.get_datetime_at(now),
            status: self.status,
        }
    }

    pub fn get_datetime_at(&mut self, now: Instant) -> DateTime {
        // Always retry, including when the part was missing at boot.
        match self.read_hardware_datetime() {
            Ok(datetime) => {
                self.soft_epoch_offset_sec = datetime_to_epoch(&datetime);
                self.soft_start_instant = now;
                self.trust = TimeTrust::Trusted;
                self.status = RtcStatus::HardwarePresent;
                return datetime;
            }
            Err(error) => self.degrade(error),
        }
        self.get_software_datetime_at(now)
    }

    fn read_control(&mut self) -> Result<u8, RtcError> {
        let mut control = [0];
        if self
            .i2c
            .write_read(PCF85063_ADDR, &[REG_CONTROL_1], &mut control)
            .is_err()
        {
            self.presence = HardwarePresence::Missing;
            return Err(RtcError::I2cError);
        }
        self.presence = HardwarePresence::Detected;
        if control[0] & CONTROL_STOP != 0 {
            return Err(RtcError::ClockStopped);
        }
        if control[0] & (CONTROL_12_HOUR | CONTROL_EXTERNAL_TEST) != 0 {
            return Err(RtcError::UnsupportedMode);
        }
        Ok(control[0])
    }

    fn read_hardware_datetime(&mut self) -> Result<DateTime, RtcError> {
        self.read_control()?;
        let mut buf = [0; 7];
        self.i2c
            .write_read(PCF85063_ADDR, &[REG_SECONDS], &mut buf)
            .map_err(|_| RtcError::I2cError)?;
        if buf[0] & SECONDS_OS != 0 {
            return Err(RtcError::OscillatorStopped);
        }
        if buf[4] > 6 {
            return Err(RtcError::InvalidWeekday);
        }
        DateTime::new(
            2000 + u16::from(bcd_to_dec(buf[6])?),
            bcd_to_dec(buf[5])?,
            bcd_to_dec(buf[3])?,
            bcd_to_dec(buf[2])?,
            bcd_to_dec(buf[1])?,
            bcd_to_dec(buf[0])?,
        )
        .map_err(RtcError::InvalidCalendar)
    }

    fn degrade(&mut self, error: RtcError) {
        self.status = match self.trust {
            TimeTrust::Trusted => RtcStatus::FallbackSoftware(error),
            TimeTrust::Unset => RtcStatus::NeedsSetting(error),
        };
    }

    pub fn set_datetime(&mut self, dt: &DateTime) -> Result<(), RtcError> {
        let yr = if dt.year() >= 2000 {
            (dt.year() - 2000) as u8
        } else {
            0
        };
        let buf = [
            REG_SECONDS,
            dec_to_bcd(dt.second()),
            dec_to_bcd(dt.minute()),
            dec_to_bcd(dt.hour()),
            dec_to_bcd(dt.day()),
            0x01, // weekday
            dec_to_bcd(dt.month()),
            dec_to_bcd(yr),
        ];

        if self.is_hardware_present() {
            let write_result = self.read_control().and_then(|_| {
                self.i2c
                    .write(PCF85063_ADDR, &buf)
                    .map_err(|_| RtcError::I2cError)
            });
            if let Err(error) = write_result {
                self.degrade(error);
                return Err(error);
            }
        }

        // Also update software clock base
        self.soft_epoch_offset_sec = datetime_to_epoch(dt);
        self.soft_start_instant = Instant::now();
        self.trust = TimeTrust::Trusted;
        self.status = if self.is_hardware_present() {
            RtcStatus::HardwarePresent
        } else {
            RtcStatus::FallbackSoftware(RtcError::HardwareNotPresent)
        };
        Ok(())
    }

    /// Set countdown timer for sedentary reminder (GPIO6 interrupt)
    pub fn configure_timer_alarm(&mut self, seconds: u8) -> Result<(), RtcError> {
        if !self.is_hardware_present() {
            return Err(RtcError::HardwareNotPresent);
        }
        // Enable timer interrupt in Control_2 (bit 6 = TIE)
        self.i2c
            .write(PCF85063_ADDR, &[REG_CONTROL_2, 0b0100_0000])
            .map_err(|_| RtcError::I2cError)?;
        // Set Timer value
        self.i2c
            .write(PCF85063_ADDR, &[REG_TIMER_VALUE, seconds])
            .map_err(|_| RtcError::I2cError)?;
        // Timer mode: 1Hz clock source, Timer enabled (TE=1, TI_TP=0, TD=10)
        self.i2c
            .write(PCF85063_ADDR, &[REG_TIMER_MODE, 0b0001_1010])
            .map_err(|_| RtcError::I2cError)?;
        Ok(())
    }

    /// Clear alarm / timer interrupt flag in Control_2
    pub fn clear_interrupt_flag(&mut self) -> Result<(), RtcError> {
        if self.is_hardware_present() {
            self.i2c
                .write(PCF85063_ADDR, &[REG_CONTROL_2, 0x00])
                .map_err(|_| RtcError::I2cError)?;
        }
        Ok(())
    }

    fn get_software_datetime_at(&self, now: Instant) -> DateTime {
        let elapsed_secs = now
            .checked_duration_since(self.soft_start_instant)
            .map_or(0, |elapsed| elapsed.as_secs());
        let total_secs = self.soft_epoch_offset_sec.saturating_add(elapsed_secs);
        epoch_to_datetime(total_secs)
    }

    pub fn i2c_mut(&mut self) -> &mut I2C {
        &mut self.i2c
    }
}

#[inline]
fn bcd_to_dec(val: u8) -> Result<u8, RtcError> {
    if val & 0x0F > 9 || val >> 4 > 9 {
        return Err(RtcError::InvalidBcd);
    }
    Ok((val & 0x0F) + ((val >> 4) * 10))
}

#[inline]
fn dec_to_bcd(val: u8) -> u8 {
    ((val / 10) << 4) | (val % 10)
}
