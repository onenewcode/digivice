//! Validated calendar values and clock-health states, independent of hardware drivers.

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

    /// Clamp internal arithmetic results; external RTC registers must use `new` instead.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtcStatus {
    /// A running, supported hardware clock supplied a validated calendar.
    HardwarePresent,
    /// Extrapolating a previously validated or explicitly set calendar.
    FallbackSoftware(RtcError),
    /// Only a placeholder calendar is available; do not use it for circadian policy.
    NeedsSetting(RtcError),
}

impl RtcStatus {
    pub const fn is_trusted(self) -> bool {
        !matches!(self, Self::NeedsSetting(_))
    }
}

/// Calendar and provenance travel together across task boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockSnapshot {
    pub datetime: DateTime,
    pub status: RtcStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircadianTime {
    Unavailable,
    Hour(u8),
}

impl ClockSnapshot {
    pub fn trusted_calendar_day(self) -> Option<CalendarDay> {
        self.status
            .is_trusted()
            .then(|| CalendarDay::from_datetime(&self.datetime))
    }

    /// Include the first trusted reading even if its hour matches the placeholder.
    pub fn circadian_change(self, previous: Self) -> Option<CircadianTime> {
        if !self.status.is_trusted() {
            return previous
                .status
                .is_trusted()
                .then_some(CircadianTime::Unavailable);
        }
        if !previous.status.is_trusted() || self.datetime.hour() != previous.datetime.hour() {
            Some(CircadianTime::Hour(self.datetime.hour()))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtcError {
    I2cError,
    HardwareNotPresent,
    NotValidated,
    ClockStopped,
    OscillatorStopped,
    UnsupportedMode,
    InvalidBcd,
    InvalidWeekday,
    InvalidCalendar(DateTimeError),
}

impl core::fmt::Display for RtcError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::I2cError => write!(f, "PCF85063 I2C bus transaction error"),
            Self::HardwareNotPresent => write!(f, "PCF85063 hardware RTC not detected"),
            Self::NotValidated => write!(f, "RTC calendar has not been validated"),
            Self::ClockStopped => write!(f, "RTC divider is stopped (STOP)"),
            Self::OscillatorStopped => write!(f, "RTC time integrity lost (OS)"),
            Self::UnsupportedMode => write!(f, "RTC requires normal 24-hour mode"),
            Self::InvalidBcd => write!(f, "RTC register contains invalid BCD"),
            Self::InvalidWeekday => write!(f, "RTC weekday is outside 0..=6"),
            Self::InvalidCalendar(error) => write!(f, "RTC calendar is invalid: {error}"),
        }
    }
}

impl core::error::Error for RtcError {}

pub fn datetime_to_epoch(dt: &DateTime) -> u64 {
    // Supported Gregorian calendar range: 2020..=2099.
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

/// A validated device-calendar day; no timezone or DST conversion is performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CalendarDay(u32);

impl CalendarDay {
    pub fn from_datetime(datetime: &DateTime) -> Self {
        Self((datetime_to_epoch(datetime) / 86_400) as u32)
    }

    pub const fn from_day_count(days: u32) -> Option<Self> {
        if days >= 18_262 && days <= (MAX_SUPPORTED_EPOCH / 86_400) as u32 {
            Some(Self(days))
        } else {
            None
        }
    }

    pub const fn day_count(self) -> u32 {
        self.0
    }

    pub const fn checked_days_since(self, previous: Self) -> Option<u32> {
        self.0.checked_sub(previous.0)
    }
}

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
