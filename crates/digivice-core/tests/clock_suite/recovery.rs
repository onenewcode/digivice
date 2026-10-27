use std::collections::VecDeque;

use digivice_core::rtc::{DateTime, PCF85063_ADDR, Pcf85063, REG_CONTROL_1, REG_SECONDS};
use embassy_time::Instant;
use embedded_hal::i2c::{ErrorKind, ErrorType, I2c, Operation};

struct ScriptedI2c {
    samples: VecDeque<Result<[u8; 7], ErrorKind>>,
}

impl ErrorType for ScriptedI2c {
    type Error = ErrorKind;
}

impl I2c for ScriptedI2c {
    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), ErrorKind> {
        assert_eq!(address, PCF85063_ADDR);
        match operations {
            [Operation::Write(register), Operation::Read(output)]
                if *register == [REG_CONTROL_1] =>
            {
                output[0] = 0;
                Ok(())
            }
            [Operation::Write(register), Operation::Read(output)] if *register == [REG_SECONDS] => {
                match self.samples.pop_front() {
                    Some(Ok(sample)) => {
                        output.copy_from_slice(&sample);
                        Ok(())
                    }
                    Some(Err(error)) => Err(error),
                    None => panic!("calendar script exhausted"),
                }
            }
            [Operation::Write(_)] => Ok(()),
            _ => panic!("unexpected I2C transaction"),
        }
    }
}

fn datetime(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> DateTime {
    match DateTime::new(year, month, day, hour, minute, second) {
        Ok(value) => value,
        Err(error) => panic!("invalid test date: {error}"),
    }
}

#[test]
fn transient_failures_extrapolate_the_last_hardware_read_across_midnight() {
    let bus = ScriptedI2c {
        samples: VecDeque::from([
            Ok([0x58, 0x59, 0x23, 0x28, 0x00, 0x02, 0x35]),
            Err(ErrorKind::Other),
            Err(ErrorKind::Other),
            Ok([0x03, 0x00, 0x00, 0x01, 0x00, 0x03, 0x35]),
        ]),
    };
    let mut rtc = Pcf85063::new_at(bus, Instant::from_millis(0));
    for (millis, expected) in [
        (1000, datetime(2035, 2, 28, 23, 59, 58)),
        (2000, datetime(2035, 2, 28, 23, 59, 59)),
        (5000, datetime(2035, 3, 1, 0, 0, 2)),
        (6000, datetime(2035, 3, 1, 0, 0, 3)),
    ] {
        assert_eq!(rtc.get_datetime_at(Instant::from_millis(millis)), expected);
    }
}

#[test]
fn every_successful_read_refreshes_the_software_anchor() {
    let bus = ScriptedI2c {
        samples: VecDeque::from([
            Ok([0x00, 0x00, 0x12, 0x01, 0x00, 0x01, 0x35]),
            Ok([0x00, 0x00, 0x18, 0x01, 0x00, 0x02, 0x40]),
            Err(ErrorKind::Other),
        ]),
    };
    let mut rtc = Pcf85063::new_at(bus, Instant::from_millis(0));
    rtc.get_datetime_at(Instant::from_millis(1000));
    assert_eq!(
        rtc.get_datetime_at(Instant::from_millis(10_000)),
        datetime(2040, 2, 1, 18, 0, 0)
    );
    assert_eq!(
        rtc.get_datetime_at(Instant::from_millis(11_000)),
        datetime(2040, 2, 1, 18, 0, 1)
    );
}

#[test]
fn repeated_failures_cross_a_leap_day_without_resetting_the_anchor() {
    let bus = ScriptedI2c {
        samples: VecDeque::from([
            Ok([0x59, 0x59, 0x23, 0x28, 0x00, 0x02, 0x36]),
            Err(ErrorKind::Other),
            Err(ErrorKind::Other),
        ]),
    };
    let mut rtc = Pcf85063::new_at(bus, Instant::from_millis(0));
    rtc.get_datetime_at(Instant::from_millis(10));
    assert_eq!(
        rtc.get_datetime_at(Instant::from_millis(2010)),
        datetime(2036, 2, 29, 0, 0, 1)
    );
    assert_eq!(
        rtc.get_datetime_at(Instant::from_millis(86_401_010)),
        datetime(2036, 3, 1, 0, 0, 0)
    );
}
