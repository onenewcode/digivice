//! PCF85063ATL Real-Time Clock Driver (I2C address 0x51)
//! Supports automatic fallback if RTC hardware is not soldered on board.

use embassy_time::Instant;
use embedded_hal::i2c::I2c;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateTimeError {
    InvalidYear,
    InvalidMonth,
    InvalidDay,
    InvalidHour,
    InvalidMinute,
    InvalidSecond,
}

impl core::fmt::Display for DateTimeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidYear => write!(f, "Year out of supported range (2020..=2099)"),
            Self::InvalidMonth => write!(f, "Month out of range (1..=12)"),
            Self::InvalidDay => write!(f, "Day out of range for month"),
            Self::InvalidHour => write!(f, "Hour out of range (0..=23)"),
            Self::InvalidMinute => write!(f, "Minute out of range (0..=59)"),
            Self::InvalidSecond => write!(f, "Second out of range (0..=59)"),
        }
    }
}

impl core::error::Error for DateTimeError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    year: u16,  // 2020..=2099
    month: u8,  // 1..=12
    day: u8,    // 1..=31
    hour: u8,   // 0..=23
    minute: u8, // 0..=59
    second: u8, // 0..=59
}

impl DateTime {
    pub fn new(
        year: u16,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
    ) -> Result<Self, DateTimeError> {
        if !(2020..=2099).contains(&year) {
            return Err(DateTimeError::InvalidYear);
        }
        if !(1..=12).contains(&month) {
            return Err(DateTimeError::InvalidMonth);
        }
        let max_d = days_in_month(year, month);
        if day == 0 || day > max_d {
            return Err(DateTimeError::InvalidDay);
        }
        if hour > 23 {
            return Err(DateTimeError::InvalidHour);
        }
        if minute > 59 {
            return Err(DateTimeError::InvalidMinute);
        }
        if second > 59 {
            return Err(DateTimeError::InvalidSecond);
        }
        Ok(Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
        })
    }

    /// Construct a clamped DateTime from raw hardware inputs (Defensive Type Constructor)
    pub(crate) fn new_clamped(
        year: u16,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
    ) -> Self {
        let yr = year.clamp(2020, 2099);
        let mo = month.clamp(1, 12);
        let max_d = days_in_month(yr, mo);
        let d = day.clamp(1, max_d);
        let h = hour.min(23);
        let m = minute.min(59);
        let s = second.min(59);
        Self {
            year: yr,
            month: mo,
            day: d,
            hour: h,
            minute: m,
            second: s,
        }
    }

    pub const fn year(&self) -> u16 {
        self.year
    }

    pub const fn month(&self) -> u8 {
        self.month
    }

    pub const fn day(&self) -> u8 {
        self.day
    }

    pub const fn hour(&self) -> u8 {
        self.hour
    }

    pub const fn minute(&self) -> u8 {
        self.minute
    }

    pub const fn second(&self) -> u8 {
        self.second
    }

    /// Sanitize an existing instance ensuring all fields satisfy invariant rules
    pub fn sanitize(self) -> Self {
        Self::new_clamped(
            self.year,
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second,
        )
    }
}

impl Default for DateTime {
    fn default() -> Self {
        Self {
            year: 2026,
            month: 10,
            day: 20,
            hour: 12,
            minute: 0,
            second: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RtcStatus {
    HardwarePresent,
    FallbackSoftware,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtcError {
    I2cError,
    HardwareNotPresent,
}

impl core::fmt::Display for RtcError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::I2cError => write!(f, "PCF85063 I2C bus transaction error"),
            Self::HardwareNotPresent => write!(f, "PCF85063 hardware RTC not detected"),
        }
    }
}

impl core::error::Error for RtcError {}

pub struct Pcf85063<I2C> {
    i2c: I2C,
    status: RtcStatus,
    soft_epoch_offset_sec: u64,
    soft_start_instant: Instant,
}

impl<I2C: I2c> Pcf85063<I2C> {
    pub fn new(mut i2c: I2C) -> Self {
        let mut ctrl = [0u8; 1];
        let present = i2c
            .write_read(PCF85063_ADDR, &[REG_CONTROL_1], &mut ctrl)
            .is_ok();

        let status = if present {
            // Clear STOP bit (bit 3) if set
            let _ = i2c.write(PCF85063_ADDR, &[REG_CONTROL_1, ctrl[0] & !0x20]);
            RtcStatus::HardwarePresent
        } else {
            RtcStatus::FallbackSoftware
        };

        Self {
            i2c,
            status,
            soft_epoch_offset_sec: 1792488000, // Default 2026-10-20 base
            soft_start_instant: Instant::now(),
        }
    }

    pub fn status(&self) -> RtcStatus {
        self.status
    }

    pub fn is_hardware_present(&self) -> bool {
        self.status == RtcStatus::HardwarePresent
    }

    pub fn get_datetime(&mut self) -> DateTime {
        if self.status == RtcStatus::HardwarePresent {
            let mut buf = [0u8; 7];
            if self
                .i2c
                .write_read(PCF85063_ADDR, &[REG_SECONDS], &mut buf)
                .is_ok()
            {
                let second = bcd_to_dec(buf[0] & 0x7F);
                let minute = bcd_to_dec(buf[1] & 0x7F);
                let hour = bcd_to_dec(buf[2] & 0x3F);
                let day = bcd_to_dec(buf[3] & 0x3F);
                let month = bcd_to_dec(buf[5] & 0x1F);
                let year = 2000 + (bcd_to_dec(buf[6]) as u16);

                return DateTime::new_clamped(year, month, day, hour, minute, second);
            }
        }

        // Software Fallback Clock
        self.get_software_datetime()
    }

    pub fn set_datetime(&mut self, dt: &DateTime) -> Result<(), RtcError> {
        let yr = if dt.year >= 2000 {
            (dt.year - 2000) as u8
        } else {
            0
        };
        let buf = [
            REG_SECONDS,
            dec_to_bcd(dt.second),
            dec_to_bcd(dt.minute),
            dec_to_bcd(dt.hour),
            dec_to_bcd(dt.day),
            0x01, // weekday
            dec_to_bcd(dt.month),
            dec_to_bcd(yr),
        ];

        if self.status == RtcStatus::HardwarePresent {
            self.i2c
                .write(PCF85063_ADDR, &buf)
                .map_err(|_| RtcError::I2cError)?;
        }

        // Also update software clock base
        self.soft_epoch_offset_sec = datetime_to_epoch(dt);
        self.soft_start_instant = Instant::now();
        Ok(())
    }

    /// Set countdown timer for sedentary reminder (GPIO6 interrupt)
    pub fn configure_timer_alarm(&mut self, seconds: u8) -> Result<(), RtcError> {
        if self.status != RtcStatus::HardwarePresent {
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
        if self.status == RtcStatus::HardwarePresent {
            self.i2c
                .write(PCF85063_ADDR, &[REG_CONTROL_2, 0x00])
                .map_err(|_| RtcError::I2cError)?;
        }
        Ok(())
    }

    fn get_software_datetime(&self) -> DateTime {
        let elapsed_secs = self.soft_start_instant.elapsed().as_secs();
        let total_secs = self.soft_epoch_offset_sec + elapsed_secs;
        epoch_to_datetime(total_secs)
    }

    pub fn i2c_mut(&mut self) -> &mut I2C {
        &mut self.i2c
    }
}

#[inline]
fn bcd_to_dec(val: u8) -> u8 {
    (val & 0x0F) + ((val >> 4) * 10)
}

#[inline]
fn dec_to_bcd(val: u8) -> u8 {
    ((val / 10) << 4) | (val % 10)
}

pub fn datetime_to_epoch(dt: &DateTime) -> u64 {
    // Simplified epoch calculation (2020..2040)
    let mut days: u64 = 0;
    for y in 2020..dt.year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }
    let month_days = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for (m, &m_days) in month_days
        .iter()
        .enumerate()
        .take(dt.month as usize)
        .skip(1)
    {
        if m == 2 && is_leap_year(dt.year) {
            days += 29;
        } else {
            days += m_days as u64;
        }
    }
    days += (dt.day as u64).saturating_sub(1);
    (days + 18262) * 86400 // 18262 days from 1970 to 2020
        + (dt.hour as u64) * 3600
        + (dt.minute as u64) * 60
        + (dt.second as u64)
}

pub const MAX_SUPPORTED_EPOCH: u64 = 4_102_444_799; // 2099-12-31 23:59:59 UTC

pub fn epoch_to_datetime(total_secs: u64) -> DateTime {
    let mut total_secs = total_secs.min(MAX_SUPPORTED_EPOCH);
    let second = (total_secs % 60) as u8;
    total_secs /= 60;
    let minute = (total_secs % 60) as u8;
    total_secs /= 60;
    let hour = (total_secs % 24) as u8;
    let mut days = total_secs / 24;

    // Days since Jan 1 1970 (Fast-forward past 1970..2020 base 18262 days to avoid redundant iterations)
    let mut year: u16 = 1970;
    if days >= 18262 {
        days -= 18262;
        year = 2020;
    }
    while year < 2099 {
        let leap = is_leap_year(year);
        let days_in_year = if leap { 366 } else { 365 };
        if days >= days_in_year {
            days -= days_in_year;
            year += 1;
        } else {
            break;
        }
    }

    let leap = is_leap_year(year);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];

    let mut month: u8 = 1;
    for &m_days in &month_days {
        if days >= m_days as u64 {
            days -= m_days as u64;
            month += 1;
        } else {
            break;
        }
    }
    let month = month.min(12);
    let day = ((days + 1) as u8).min(31);

    DateTime::new_clamped(year, month, day, hour, minute, second)
}

#[inline]
pub fn is_leap_year(year: u16) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

#[inline]
pub fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}
