use std::collections::VecDeque;

use digivice_core::domain::{ImuError, ImuStatus};
use digivice_core::imu::{
    QMI8658_ADDR, Qmi8658, REG_AX_L, REG_STATUS0, REG_TEMP_L, REG_WHO_AM_I, WHO_AM_I_VALUE,
};
use embedded_hal::i2c::{ErrorKind, ErrorType, I2c, Operation};

struct ScriptedBus {
    samples: VecDeque<Result<[u8; 12], ErrorKind>>,
    temperatures: VecDeque<Result<[u8; 2], ErrorKind>>,
    inertial_reads: usize,
    temperature_reads: usize,
}

impl ScriptedBus {
    fn samples(count: usize) -> Self {
        Self {
            samples: std::iter::repeat_n(Ok(inertial_bytes()), count).collect(),
            temperatures: std::iter::repeat_n(Err(ErrorKind::Other), count).collect(),
            inertial_reads: 0,
            temperature_reads: 0,
        }
    }
}

impl ErrorType for ScriptedBus {
    type Error = ErrorKind;
}

impl I2c for ScriptedBus {
    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Self::Error> {
        assert_eq!(address, QMI8658_ADDR);
        match operations {
            [Operation::Write(register), Operation::Read(output)]
                if *register == [REG_WHO_AM_I] =>
            {
                output.copy_from_slice(&[WHO_AM_I_VALUE]);
                Ok(())
            }
            [Operation::Write(register), Operation::Read(output)] if *register == [REG_STATUS0] => {
                output.copy_from_slice(&[3]);
                Ok(())
            }
            [Operation::Write(register), Operation::Read(output)] if *register == [REG_AX_L] => {
                self.inertial_reads += 1;
                match self.samples.pop_front() {
                    Some(Ok(bytes)) => {
                        output.copy_from_slice(&bytes);
                        Ok(())
                    }
                    Some(Err(error)) => Err(error),
                    None => panic!("inertial script exhausted"),
                }
            }
            [Operation::Write(register), Operation::Read(output)] if *register == [REG_TEMP_L] => {
                self.temperature_reads += 1;
                match self.temperatures.pop_front() {
                    Some(Ok(bytes)) => {
                        output.copy_from_slice(&bytes);
                        Ok(())
                    }
                    Some(Err(error)) => Err(error),
                    None => panic!("temperature script exhausted"),
                }
            }
            [Operation::Write(_)] => Ok(()),
            _ => panic!("unexpected IMU transaction"),
        }
    }
}

fn inertial_bytes() -> [u8; 12] {
    let mut bytes = [0; 12];
    for (pair, raw) in bytes
        .as_chunks_mut::<2>()
        .0
        .iter_mut()
        .zip([0_i16, 0, -8192, 64, -128, 192])
    {
        pair.copy_from_slice(&raw.to_le_bytes());
    }
    bytes
}

#[test]
fn a_successful_inertial_burst_is_not_discarded_when_temperature_would_fail() {
    let mut imu = Qmi8658::new(ScriptedBus::samples(1));
    let sample = match imu.read_sample() {
        Ok(sample) => sample,
        Err(error) => panic!("valid inertial burst was discarded: {error}"),
    };
    assert_eq!((sample.ax_mg, sample.ay_mg, sample.az_mg), (0, 0, 1000));
    assert_eq!(
        (sample.gx_mdps, sample.gy_mdps, sample.gz_mdps),
        (1000, -2000, -3000)
    );
    assert_eq!(imu.i2c_mut().inertial_reads, 1);
    assert_eq!(imu.i2c_mut().temperature_reads, 0);
    assert_eq!(imu.status(), ImuStatus::HardwarePresent);
}

#[test]
fn two_hundred_high_rate_reads_use_only_two_hundred_inertial_transactions() {
    let mut imu = Qmi8658::new(ScriptedBus::samples(200));
    let successful = (0..200).filter(|_| imu.read_sample().is_ok()).count();
    assert_eq!(successful, 200);
    assert_eq!(imu.i2c_mut().inertial_reads, 200);
    assert_eq!(imu.i2c_mut().temperature_reads, 0);
}

#[test]
fn an_inertial_bus_error_remains_an_error_not_a_successful_zero_sample() {
    let mut bus = ScriptedBus::samples(0);
    bus.samples.push_back(Err(ErrorKind::Other));
    let mut imu = Qmi8658::new(bus);
    assert_eq!(imu.read_sample(), Err(ImuError::BusError));
}

#[test]
fn an_explicit_temperature_failure_cannot_disable_inertial_sampling() {
    let mut imu = Qmi8658::new(ScriptedBus::samples(1));
    assert_eq!(imu.read_temperature(), Err(ImuError::BusError));
    assert_eq!(imu.status(), ImuStatus::HardwarePresent);
    assert!(imu.read_sample().is_ok());
    assert_eq!(imu.i2c_mut().temperature_reads, 1);
    assert_eq!(imu.i2c_mut().inertial_reads, 1);
}

#[test]
fn explicit_temperature_uses_signed_q8_8_without_an_extra_twenty_five_degree_offset() {
    let mut bus = ScriptedBus::samples(1);
    let values = [
        (0_i16, 0),
        (26 * 256 + 128, 2650),
        (-7 * 256 - 128, -750),
        (i16::MIN, -12800),
        (i16::MAX, 12799),
    ];
    bus.temperatures = values
        .iter()
        .map(|(raw, _)| Ok(raw.to_le_bytes()))
        .collect();
    let mut imu = Qmi8658::new(bus);
    for (_, expected) in values {
        let measured = imu.read_temperature().map(|value| value.centi_celsius());
        assert_eq!(measured, Ok(expected));
    }
    assert_eq!(imu.i2c_mut().inertial_reads, 0);
    assert_eq!(imu.i2c_mut().temperature_reads, values.len());
}

#[test]
fn low_rate_temperature_adds_only_one_transaction_to_two_hundred_inertial_reads() {
    let mut bus = ScriptedBus::samples(200);
    bus.temperatures = VecDeque::from([Ok((25_i16 * 256).to_le_bytes())]);
    let mut imu = Qmi8658::new(bus);
    for _ in 0..200 {
        assert!(imu.read_sample().is_ok());
    }
    assert_eq!(
        imu.read_temperature().map(|value| value.centi_celsius()),
        Ok(2500)
    );
    assert_eq!(imu.i2c_mut().inertial_reads, 200);
    assert_eq!(imu.i2c_mut().temperature_reads, 1);
}
