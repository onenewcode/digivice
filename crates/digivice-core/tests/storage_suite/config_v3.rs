use crate::support;

use digivice_core::domain::{CalendarDay, DateTime, HealthStatus, NurtureProgress, ViewMode};
use digivice_core::storage::{
    CONFIG_COMMIT_OFFSET, CONFIG_PAYLOAD_OFFSET, CONFIG_VERSION, DeviceConfig, StorageError,
    config::deserialize_config_record, crc32,
};

fn date(year: u16, month: u8, day: u8) -> DateTime {
    match DateTime::new(year, month, day, 12, 0, 0) {
        Ok(date) => date,
        Err(error) => panic!("invalid test date: {error}"),
    }
}

#[test]
fn v3_round_trips_every_durable_field_and_full_width_training() {
    for training in [0, 255, 256, 1024, u16::MAX] {
        let mut config = DeviceConfig::default();
        config.seq = u32::MAX;
        config.pet_training_count = training;
        config.pet_age_days = 400;
        config.pet_health = HealthStatus::Sick;
        config.pet_poop_count = 4;
        config.pet_energy = 37;
        config.pet_affinity = u16::MAX;
        config.pet_nurture_progress = NurtureProgress::new_clamped(29_999, 60_000, 40_000, 179_999);
        config.pet_calendar_day = Some(CalendarDay::from_datetime(&date(2035, 12, 31)));
        assert_eq!(config.validate(), Ok(()));
        let bytes = config.serialize();
        assert_eq!(bytes.len(), 84);
        assert_eq!(CONFIG_COMMIT_OFFSET % 4, 0);
        assert_eq!(deserialize_config_record(&bytes), Ok(config));
        assert_eq!(DeviceConfig::deserialize(&bytes), Ok(config));
        assert_eq!(bytes[43], 0);
        assert_eq!(bytes[45], 0);
    }
}

#[test]
fn v2_migration_preserves_existing_values_and_explicitly_defaults_absent_state() {
    let migrated = match deserialize_config_record(&support::v2_record(100, 255)) {
        Ok(config) => config,
        Err(error) => panic!("V2 migration failed: {error}"),
    };
    assert_eq!(migrated.version(), CONFIG_VERSION);
    assert_eq!(migrated.seq, 100);
    assert_eq!(migrated.gyro_bias_mdps, [123, 0, 0]);
    assert_eq!(migrated.accel_bias_mg, [-17, 0, 0]);
    assert_eq!(migrated.ble_target_hash, 0x1234);
    assert_eq!(migrated.rssi_away_threshold, -82);
    assert_eq!(migrated.brightness_normal, 45);
    assert_eq!(migrated.default_mode, ViewMode::Status);
    assert_eq!(migrated.pet_hunger, 2);
    assert_eq!(migrated.pet_strength, 1);
    assert_eq!(migrated.pet_weight_g, 42);
    assert_eq!(migrated.pet_care_mistakes, 7);
    assert_eq!(migrated.pet_evolution_stage, 4);
    assert_eq!(migrated.pet_training_count, 255);
    assert_eq!(migrated.pet_age_days, 1);
    assert_eq!(migrated.pet_health, HealthStatus::Healthy);
    assert_eq!(migrated.pet_poop_count, 0);
    assert_eq!(migrated.pet_energy, 100);
    assert_eq!(migrated.pet_affinity, 0);
    assert_eq!(migrated.pet_nurture_progress, NurtureProgress::default());
    assert_eq!(migrated.pet_calendar_day, None);
    assert_eq!(
        DeviceConfig::deserialize(&migrated.serialize()),
        Ok(migrated)
    );
}

#[test]
fn damaged_v2_records_cannot_bypass_validation_via_v1_or_v3() {
    let original = support::v2_record(7, 255);
    for index in 0..original.len() {
        for bit in 0..8 {
            let mut corrupted = original;
            corrupted[index] ^= 1 << bit;
            assert!(
                deserialize_config_record(&corrupted).is_err(),
                "byte={index}, bit={bit}"
            );
        }
    }
    for written in 0..original.len() {
        let mut torn = [0xff; DeviceConfig::SERIALIZED_SIZE];
        torn[..written].copy_from_slice(&original[..written]);
        assert!(
            deserialize_config_record(&torn).is_err(),
            "V2 prefix {written}"
        );
    }
}

#[test]
fn invalid_health_and_calendar_anchors_are_not_silently_trusted() {
    let original = DeviceConfig::default().serialize();
    for invalid in [0, 1] {
        let mut bytes = original;
        if invalid == 0 {
            bytes[52] = 2;
        } else {
            bytes[76..80].copy_from_slice(&0_u32.to_le_bytes());
        }
        let crc = crc32(&bytes[CONFIG_PAYLOAD_OFFSET..CONFIG_COMMIT_OFFSET]);
        bytes[8..12].copy_from_slice(&crc.to_le_bytes());
        assert_eq!(
            deserialize_config_record(&bytes),
            Err(StorageError::InvalidPetState)
        );
    }
}

#[test]
fn persisted_elapsed_time_is_bounded_below_periods_or_at_condition_limits() {
    let progress = NurtureProgress::new_clamped(u32::MAX, u32::MAX, u32::MAX, u32::MAX);
    assert_eq!(progress.digestion_ms(), 29_999);
    assert_eq!(progress.starvation_ms(), 60_000);
    assert_eq!(progress.uncleaned_ms(), 40_000);
    assert_eq!(progress.unattended_ms(), 179_999);
    assert!(CalendarDay::from_day_count(18_261).is_none());
    assert!(CalendarDay::from_day_count(u32::MAX).is_none());
    for (year, month, day) in [(2020, 1, 1), (2099, 12, 31)] {
        let day = CalendarDay::from_datetime(&date(year, month, day));
        assert_eq!(CalendarDay::from_day_count(day.day_count()), Some(day));
    }
}
