use digivice_core::storage::{CONFIG_MAGIC, DeviceConfig, config::deserialize_config_record};

#[test]
fn no_single_bit_corruption_of_a_current_record_is_accepted_as_legacy() {
    let original = DeviceConfig::default().serialize();
    for index in 0..original.len() {
        for bit in 0..8 {
            let mut corrupted = original;
            corrupted[index] ^= 1 << bit;
            assert!(
                deserialize_config_record(&corrupted).is_err(),
                "accepted corrupted byte {index}, bit {bit} as a valid record"
            );
        }
    }
}

#[test]
fn an_uncommitted_prefix_never_becomes_a_legacy_record() {
    let original = DeviceConfig::default().serialize();
    for written in 0..original.len() {
        let mut torn = [0xff; DeviceConfig::SERIALIZED_SIZE];
        torn[..written].copy_from_slice(&original[..written]);
        assert!(
            deserialize_config_record(&torn).is_err(),
            "accepted {written} byte prefix"
        );
    }
}

#[test]
fn a_legacy_record_with_an_erased_tail_still_decodes_without_writing_it_back() {
    let mut legacy = [0; 44];
    legacy[..4].copy_from_slice(&CONFIG_MAGIC.to_le_bytes());
    legacy[28] = (-78_i8) as u8;
    legacy[29] = 80;
    assert_eq!(
        deserialize_config_record(&legacy),
        Ok(DeviceConfig::default())
    );

    let mut sector_record = [0xff; DeviceConfig::SERIALIZED_SIZE];
    sector_record[..44].copy_from_slice(&legacy);
    assert_eq!(
        deserialize_config_record(&sector_record),
        Ok(DeviceConfig::default())
    );

    sector_record[48] = 0;
    assert!(deserialize_config_record(&sector_record).is_err());
}
