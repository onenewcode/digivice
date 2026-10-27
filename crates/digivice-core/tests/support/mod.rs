use digivice_core::domain::ViewMode;
use digivice_core::storage::{CONFIG_COMMIT_MARKER, CONFIG_MAGIC, crc32};

/// Fixed original V2 wire layout, independent of the current serializer.
pub fn v2_record(seq: u32, training: u8) -> [u8; 52] {
    let mut bytes = [0; 52];
    bytes[..4].copy_from_slice(&CONFIG_MAGIC.to_le_bytes());
    bytes[4..6].copy_from_slice(&2_u16.to_le_bytes());
    bytes[6..8].copy_from_slice(&36_u16.to_le_bytes());
    bytes[12..16].copy_from_slice(&seq.to_le_bytes());
    bytes[16..20].copy_from_slice(&123_i32.to_le_bytes());
    bytes[28..30].copy_from_slice(&(-17_i16).to_le_bytes());
    bytes[34..38].copy_from_slice(&0x1234_u32.to_le_bytes());
    bytes[38] = (-82_i8) as u8;
    bytes[39] = 45;
    bytes[40] = ViewMode::Status.into();
    bytes[41] = 2;
    bytes[42] = 1;
    bytes[43] = 90; // Deprecated discipline, not a live V3 property.
    bytes[44] = 7;
    bytes[45] = training;
    bytes[46] = 4;
    bytes[47] = 42;
    let crc = crc32(&bytes[12..48]);
    bytes[8..12].copy_from_slice(&crc.to_le_bytes());
    bytes[48..52].copy_from_slice(&CONFIG_COMMIT_MARKER.to_le_bytes());
    bytes
}
