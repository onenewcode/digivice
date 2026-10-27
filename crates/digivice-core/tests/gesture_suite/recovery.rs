use std::cell::RefCell;
use std::rc::Rc;

use digivice_core::domain::{ImuError, ImuSample, ImuStatus};
use digivice_core::imu::{
    IMU_RETRY_INTERVAL, ImuAttitudeSnapshot, QMI8658_ADDR, Qmi8658, REG_AX_L, REG_CTRL1, REG_CTRL2,
    REG_CTRL3, REG_CTRL5, REG_CTRL7, REG_STATUS0, REG_WHO_AM_I, WHO_AM_I_VALUE,
};
use embassy_time::Instant;
use embedded_hal::i2c::{ErrorKind, ErrorType, I2c, Operation};

#[derive(Clone)]
struct FakeBus(Rc<RefCell<Device>>);

struct Device {
    disconnected: bool,
    id: u8,
    failing_write: Option<u8>,
    failing_read: Option<u8>,
    failing_write_number: Option<usize>,
    ready: u8,
    reads: Vec<u8>,
    writes: Vec<[u8; 2]>,
}

impl Default for FakeBus {
    fn default() -> Self {
        Self(Rc::new(RefCell::new(Device {
            disconnected: false,
            id: WHO_AM_I_VALUE,
            failing_write: None,
            failing_read: None,
            failing_write_number: None,
            ready: 3,
            reads: Vec::new(),
            writes: Vec::new(),
        })))
    }
}

impl ErrorType for FakeBus {
    type Error = ErrorKind;
}

impl I2c for FakeBus {
    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Self::Error> {
        assert_eq!(address, QMI8658_ADDR);
        let mut device = self.0.borrow_mut();
        match operations {
            [Operation::Write(register), Operation::Read(output)] if register.len() == 1 => {
                let register = register[0];
                device.reads.push(register);
                if device.disconnected || device.failing_read == Some(register) {
                    return Err(ErrorKind::Other);
                }
                match register {
                    REG_WHO_AM_I => output.copy_from_slice(&[device.id]),
                    REG_STATUS0 => output.copy_from_slice(&[device.ready]),
                    REG_AX_L => {
                        let mut bytes = [0; 12];
                        bytes[4..6].copy_from_slice(&(-8192_i16).to_le_bytes());
                        output.copy_from_slice(&bytes);
                    }
                    _ => panic!("unexpected IMU register {register:#x}"),
                }
                Ok(())
            }
            [Operation::Write(bytes)] if bytes.len() == 2 => {
                device.writes.push([bytes[0], bytes[1]]);
                if device.disconnected
                    || device.failing_write == Some(bytes[0])
                    || device.failing_write_number == Some(device.writes.len())
                {
                    Err(ErrorKind::Other)
                } else {
                    Ok(())
                }
            }
            _ => panic!("unexpected IMU transaction"),
        }
    }
}

fn at(millis: u64) -> Instant {
    Instant::from_millis(millis)
}

#[test]
fn a_runtime_bus_failure_immediately_changes_driver_health() {
    let bus = FakeBus::default();
    let mut imu = Qmi8658::new_at(bus.clone(), at(0));
    bus.0.borrow_mut().failing_read = Some(REG_AX_L);
    assert_eq!(imu.read_sample_at(at(0)), Err(ImuError::BusError));
    assert_eq!(imu.status(), ImuStatus::Unavailable(ImuError::BusError));
}

#[test]
fn a_device_missing_at_boot_recovers_after_the_retry_interval() {
    let bus = FakeBus::default();
    bus.0.borrow_mut().disconnected = true;
    let mut imu = Qmi8658::new_at(bus.clone(), at(0));
    assert_eq!(imu.status(), ImuStatus::Unavailable(ImuError::BusError));
    bus.0.borrow_mut().disconnected = false;
    assert!(imu.read_sample_at(at(1000)).is_ok());
    assert_eq!(imu.status(), ImuStatus::HardwarePresent);
}

#[test]
fn missing_devices_are_not_probed_at_the_two_hundred_hertz_poll_rate() {
    assert_eq!(IMU_RETRY_INTERVAL.as_millis(), 1000);
    let bus = FakeBus::default();
    bus.0.borrow_mut().disconnected = true;
    let mut imu = Qmi8658::new_at(bus.clone(), at(0));
    for millis in (0..3000).step_by(5) {
        assert_eq!(imu.read_sample_at(at(millis)), Err(ImuError::BusError));
    }
    assert_eq!(bus.0.borrow().reads, [REG_WHO_AM_I; 3]);
    assert!(bus.0.borrow().writes.is_empty());
}

#[test]
fn an_unexpected_identity_is_never_configured_and_can_be_replaced_after_backoff() {
    let bus = FakeBus::default();
    bus.0.borrow_mut().id = 0xFF;
    let mut imu = Qmi8658::new_at(bus.clone(), at(0));
    assert_eq!(imu.who_am_i(), 0xFF);
    assert_eq!(
        imu.status(),
        ImuStatus::Unavailable(ImuError::DeviceNotFound)
    );
    assert!(bus.0.borrow().writes.is_empty());
    bus.0.borrow_mut().id = WHO_AM_I_VALUE;
    assert_eq!(imu.read_sample_at(at(999)), Err(ImuError::DeviceNotFound));
    assert!(imu.read_sample_at(at(1000)).is_ok());
    assert_eq!(imu.who_am_i(), WHO_AM_I_VALUE);
}

#[test]
fn each_configuration_write_failure_stops_initialization_and_retries_the_whole_sequence() {
    for failed_write in 1..=6 {
        let bus = FakeBus::default();
        bus.0.borrow_mut().failing_write_number = Some(failed_write);
        let mut imu = Qmi8658::new_at(bus.clone(), at(0));
        assert_eq!(
            imu.status(),
            ImuStatus::Unavailable(ImuError::ConfigurationFailed)
        );
        assert_eq!(bus.0.borrow().writes.len(), failed_write);
        assert_eq!(
            imu.read_sample_at(at(999)),
            Err(ImuError::ConfigurationFailed)
        );
        assert_eq!(bus.0.borrow().writes.len(), failed_write);
        bus.0.borrow_mut().failing_write_number = None;
        assert!(imu.read_sample_at(at(1000)).is_ok());
        let device = bus.0.borrow();
        assert_eq!(device.writes.len(), failed_write + 6);
        assert_eq!(device.writes[failed_write], [REG_CTRL7, 0]);
        assert_eq!(device.writes[failed_write + 5], [REG_CTRL7, 3]);
    }
}

#[test]
fn a_runtime_disconnect_reinitializes_only_when_due_and_then_resumes_sampling() {
    let bus = FakeBus::default();
    let mut imu = Qmi8658::new_at(bus.clone(), at(0));
    assert!(imu.read_sample_at(at(0)).is_ok());
    bus.0.borrow_mut().disconnected = true;
    assert_eq!(imu.read_sample_at(at(10)), Err(ImuError::BusError));
    let reads_at_fault = bus.0.borrow().reads.len();
    bus.0.borrow_mut().disconnected = false;
    for millis in 11..1010 {
        assert_eq!(imu.read_sample_at(at(millis)), Err(ImuError::BusError));
    }
    assert_eq!(bus.0.borrow().reads.len(), reads_at_fault);
    assert!(imu.read_sample_at(at(1010)).is_ok());
    assert_eq!(imu.status(), ImuStatus::HardwarePresent);
    assert_eq!(bus.0.borrow().writes.len(), 12);
}

#[test]
fn the_first_burst_after_configuration_requires_both_data_ready_bits() {
    let bus = FakeBus::default();
    bus.0.borrow_mut().ready = 0;
    let mut imu = Qmi8658::new_at(bus.clone(), at(0));
    for (millis, bits) in [(0, 0), (10, 1), (20, 2)] {
        bus.0.borrow_mut().ready = bits;
        assert_eq!(imu.read_sample_at(at(millis)), Err(ImuError::DataNotReady));
        assert_eq!(imu.status(), ImuStatus::HardwarePresent);
    }
    assert_eq!(
        bus.0
            .borrow()
            .reads
            .iter()
            .filter(|&&register| register == REG_AX_L)
            .count(),
        0
    );
    bus.0.borrow_mut().ready = 3;
    assert_eq!(
        imu.read_sample_at(at(30)).map(|sample| sample.az_mg),
        Ok(1000)
    );
    assert_eq!(imu.status(), ImuStatus::HardwarePresent);
}

#[test]
fn a_stuck_first_data_ready_gets_a_bounded_reinitialization_attempt() {
    let bus = FakeBus::default();
    bus.0.borrow_mut().ready = 0;
    let mut imu = Qmi8658::new_at(bus.clone(), at(0));
    assert_eq!(imu.read_sample_at(at(999)), Err(ImuError::DataNotReady));
    assert_eq!(imu.read_sample_at(at(1000)), Err(ImuError::DataNotReady));
    let reads_at_timeout = bus.0.borrow().reads.len();
    assert_eq!(imu.read_sample_at(at(1999)), Err(ImuError::DataNotReady));
    assert_eq!(bus.0.borrow().reads.len(), reads_at_timeout);
    bus.0.borrow_mut().ready = 3;
    assert!(imu.read_sample_at(at(2000)).is_ok());
    assert_eq!(bus.0.borrow().writes.len(), 12);
}

#[test]
fn streaming_reads_skip_repeated_samples_when_status_has_no_new_accel_or_gyro_data() {
    let bus = FakeBus::default();
    let mut imu = Qmi8658::new_at(bus.clone(), at(0));
    assert!(imu.read_sample_at(at(0)).is_ok());
    let reads_after_first = bus.0.borrow().reads.len();
    bus.0.borrow_mut().ready = 0;
    assert_eq!(imu.read_sample_at(at(5)), Err(ImuError::DataNotReady));
    assert_eq!(bus.0.borrow().reads.len(), reads_after_first + 1);
    assert_eq!(
        bus.0
            .borrow()
            .reads
            .iter()
            .filter(|&&register| register == REG_AX_L)
            .count(),
        1
    );
    bus.0.borrow_mut().ready = 1;
    assert_eq!(imu.read_sample_at(at(10)), Err(ImuError::DataNotReady));
    bus.0.borrow_mut().ready = 3;
    assert!(imu.read_sample_at(at(15)).is_ok());
    assert_eq!(
        bus.0
            .borrow()
            .reads
            .iter()
            .filter(|&&register| register == REG_AX_L)
            .count(),
        2
    );
}

#[test]
fn data_ready_bus_errors_and_retry_overflow_never_become_healthy_samples() {
    let bus = FakeBus::default();
    bus.0.borrow_mut().failing_read = Some(REG_STATUS0);
    let mut imu = Qmi8658::new_at(bus.clone(), at(0));
    assert_eq!(
        imu.read_sample_at(Instant::from_ticks(u64::MAX)),
        Err(ImuError::BusError)
    );
    assert_eq!(imu.status(), ImuStatus::Unavailable(ImuError::BusError));
    let reads_at_fault = bus.0.borrow().reads.len();
    bus.0.borrow_mut().failing_read = None;
    for _ in 0..200 {
        assert_eq!(
            imu.read_sample_at(Instant::from_ticks(u64::MAX)),
            Err(ImuError::BusError)
        );
    }
    assert_eq!(bus.0.borrow().reads.len(), reads_at_fault);

    let bus = FakeBus::default();
    let mut imu = Qmi8658::new_at(bus.clone(), Instant::from_ticks(u64::MAX));
    assert_eq!(imu.status(), ImuStatus::Unavailable(ImuError::NotValidated));
    let reads_at_overflow = bus.0.borrow().reads.len();
    for _ in 0..200 {
        assert_eq!(
            imu.read_sample_at(Instant::from_ticks(u64::MAX)),
            Err(ImuError::NotValidated)
        );
    }
    assert_eq!(bus.0.borrow().reads.len(), reads_at_overflow);
}

#[test]
fn only_healthy_snapshots_expose_samples_and_default_is_not_validated() {
    assert_eq!(ImuAttitudeSnapshot::default().valid_sample(), None);
    let sample = ImuSample {
        ax_mg: 123,
        az_mg: 1000,
        ..Default::default()
    };
    for error in [
        ImuError::BusError,
        ImuError::DeviceNotFound,
        ImuError::ConfigurationFailed,
        ImuError::DataNotReady,
        ImuError::NotValidated,
    ] {
        let snapshot = ImuAttitudeSnapshot {
            sample,
            imu_status: ImuStatus::Unavailable(error),
            ..Default::default()
        };
        assert_eq!(snapshot.valid_sample(), None);
    }
    let snapshot = ImuAttitudeSnapshot {
        sample,
        imu_status: ImuStatus::HardwarePresent,
        ..Default::default()
    };
    assert_eq!(snapshot.valid_sample(), Some(sample));
}

#[test]
fn six_dof_configuration_uses_two_hundred_twenty_four_hertz_not_fifty_six_hertz() {
    let bus = FakeBus::default();
    let mut imu = Qmi8658::new_at(bus.clone(), at(0));
    let expected = [
        [REG_CTRL7, 0],
        [REG_CTRL1, 0x60],
        [REG_CTRL2, 0x15], // +/-4g, 6DOF ODR code 0x5: 224.2 Hz.
        [REG_CTRL3, 0x55], // +/-512 dps, the same ODR.
        [REG_CTRL5, 0x55],
        [REG_CTRL7, 3],
    ];
    assert_eq!(bus.0.borrow().writes.as_slice(), expected);
    bus.0.borrow_mut().failing_read = Some(REG_AX_L);
    assert_eq!(imu.read_sample_at(at(0)), Err(ImuError::BusError));
    bus.0.borrow_mut().failing_read = None;
    assert!(imu.read_sample_at(at(1000)).is_ok());
    assert_eq!(&bus.0.borrow().writes[expected.len()..], expected);
}
