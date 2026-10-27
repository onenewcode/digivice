use std::collections::VecDeque;

use digivice_core::rtc::{
    DateTime, PCF85063_ADDR, Pcf85063, REG_CONTROL_1, REG_SECONDS, RtcError, RtcStatus,
};
use embassy_time::Instant;
use embedded_hal::i2c::{ErrorKind, ErrorType, I2c, Operation};

const VALID: [u8; 7] = [0x00, 0x00, 0x12, 0x01, 0x00, 0x01, 0x35];

struct ClockBus {
    control: u8,
    control_error: Option<ErrorKind>,
    samples: VecDeque<Result<[u8; 7], ErrorKind>>,
    writes: usize,
}

impl ClockBus {
    fn with_samples(samples: impl IntoIterator<Item = Result<[u8; 7], ErrorKind>>) -> Self {
        Self {
            control: 0,
            control_error: None,
            samples: samples.into_iter().collect(),
            writes: 0,
        }
    }
}

impl ErrorType for ClockBus {
    type Error = ErrorKind;
}

impl I2c for ClockBus {
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
                if let Some(error) = self.control_error {
                    return Err(error);
                }
                output[0] = self.control;
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
            [Operation::Write(_)] => {
                self.writes += 1;
                Err(ErrorKind::Other)
            }
            _ => panic!("unexpected RTC transaction"),
        }
    }
}

fn at(seconds: u64) -> Instant {
    Instant::from_secs(seconds)
}

#[test]
fn invalid_calendar_reads_never_replace_the_last_valid_anchor() {
    let mut invalid = Vec::new();
    let mut os_flag = VALID;
    os_flag[0] = 0x80;
    invalid.push(os_flag);
    for (index, value) in [
        (0, 0x0a),
        (1, 0x1f),
        (2, 0x24),
        (3, 0x00),
        (4, 0x07),
        (5, 0x13),
        (6, 0xa0),
        (6, 0x19),
    ] {
        let mut sample = VALID;
        sample[index] = value;
        invalid.push(sample);
    }
    let mut impossible_date = VALID;
    impossible_date[3] = 0x29;
    impossible_date[5] = 0x02;
    invalid.push(impossible_date);

    for sample in invalid {
        let bus = ClockBus::with_samples([Ok(VALID), Ok(sample), Ok(VALID)]);
        let mut rtc = Pcf85063::new_at(bus, at(0));
        let initial = rtc.get_datetime_at(at(0));
        assert_eq!(rtc.status(), RtcStatus::HardwarePresent);
        let fallback = rtc.get_datetime_at(at(1));
        assert_eq!(
            fallback.year(),
            initial.year(),
            "bad register data: {sample:x?}"
        );
        assert_eq!(fallback.month(), initial.month());
        assert_eq!(fallback.day(), initial.day());
        assert_eq!(fallback.second(), 1, "bad register data: {sample:x?}");
        assert!(matches!(rtc.status(), RtcStatus::FallbackSoftware(_)));
        assert!(rtc.status().is_trusted());
        assert_eq!(rtc.get_datetime_at(at(2)), initial);
        assert_eq!(rtc.status(), RtcStatus::HardwarePresent);
    }
}

#[test]
fn stopped_or_unsupported_clock_is_not_a_hardware_time_source() {
    for (control, reason) in [
        (0x20, RtcError::ClockStopped),
        (0x02, RtcError::UnsupportedMode),
        (0x80, RtcError::UnsupportedMode),
    ] {
        let mut bus = ClockBus::with_samples([Ok(VALID)]);
        bus.control = control;
        let mut rtc = Pcf85063::new_at(bus, at(0));
        assert_eq!(rtc.get_datetime_at(at(0)), DateTime::default());
        assert_eq!(rtc.status(), RtcStatus::NeedsSetting(reason));
        assert!(rtc.is_hardware_present());
        assert!(!rtc.status().is_trusted());
        assert_eq!(
            rtc.i2c_mut().writes,
            0,
            "probing must not alter an untrusted clock"
        );
    }
}

#[test]
fn a_device_missing_at_boot_is_probed_again() {
    let mut bus = ClockBus::with_samples([Ok(VALID)]);
    bus.control_error = Some(ErrorKind::Other);
    let mut rtc = Pcf85063::new_at(bus, at(0));
    assert_eq!(rtc.status(), RtcStatus::NeedsSetting(RtcError::I2cError));
    assert!(!rtc.is_hardware_present());
    rtc.i2c_mut().control_error = None;
    assert_eq!(rtc.get_datetime_at(at(1)).year(), 2035);
    assert_eq!(rtc.status(), RtcStatus::HardwarePresent);
    assert!(rtc.is_hardware_present());
}

#[test]
fn an_uninitialized_calendar_never_becomes_trusted_by_clamping() {
    let bus = ClockBus::with_samples([Ok([0; 7])]);
    let mut rtc = Pcf85063::new_at(bus, at(0));
    assert!(!rtc.status().is_trusted());
    assert_eq!(rtc.get_datetime_at(at(0)), DateTime::default());
    assert!(matches!(rtc.status(), RtcStatus::NeedsSetting(_)));
}

#[test]
fn bus_failures_and_recovery_update_health_without_disabling_retries() {
    let bus = ClockBus::with_samples([Ok(VALID), Err(ErrorKind::Other), Ok(VALID)]);
    let mut rtc = Pcf85063::new_at(bus, at(0));
    rtc.get_datetime_at(at(0));
    rtc.get_datetime_at(at(1));
    assert_eq!(
        rtc.status(),
        RtcStatus::FallbackSoftware(RtcError::I2cError)
    );
    rtc.i2c_mut().control_error = Some(ErrorKind::Other);
    assert_eq!(rtc.get_datetime_at(at(2)).second(), 2);
    assert!(!rtc.is_hardware_present());
    assert_eq!(
        rtc.status(),
        RtcStatus::FallbackSoftware(RtcError::I2cError)
    );
    rtc.i2c_mut().control_error = None;
    assert_eq!(rtc.get_datetime_at(at(3)).year(), 2035);
    assert_eq!(rtc.status(), RtcStatus::HardwarePresent);
}

#[test]
fn a_failed_calendar_write_does_not_report_success_or_replace_the_anchor() {
    let bus = ClockBus::with_samples([Ok(VALID), Err(ErrorKind::Other)]);
    let mut rtc = Pcf85063::new_at(bus, at(0));
    rtc.get_datetime_at(at(0));
    assert_eq!(
        rtc.set_datetime(&DateTime::default()),
        Err(RtcError::I2cError)
    );
    assert_eq!(
        rtc.status(),
        RtcStatus::FallbackSoftware(RtcError::I2cError)
    );
    assert_eq!(rtc.get_datetime_at(at(1)).year(), 2035);
}

#[test]
fn snapshots_pair_each_calendar_with_the_health_of_that_read() {
    let bus = ClockBus::with_samples([Err(ErrorKind::Other), Ok(VALID), Err(ErrorKind::Other)]);
    let mut rtc = Pcf85063::new_at(bus, at(0));
    let initial = rtc.snapshot_at(at(0));
    assert_eq!(initial.status, RtcStatus::NeedsSetting(RtcError::I2cError));
    assert_eq!(initial.datetime, DateTime::default());
    let hardware = rtc.snapshot_at(at(1));
    assert_eq!(hardware.status, RtcStatus::HardwarePresent);
    assert_eq!(hardware.datetime.year(), 2035);
    let fallback = rtc.snapshot_at(at(2));
    assert_eq!(
        fallback.status,
        RtcStatus::FallbackSoftware(RtcError::I2cError)
    );
    assert_eq!(fallback.datetime.year(), 2035);
    assert_eq!(fallback.datetime.second(), 1);
}
