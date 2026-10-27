//! Flight recorder telemetry snapshot structures and serialization.

use crate::storage::StorageError;
use crate::storage::crc::crc32;

const BLACKBOX_CRC_OFFSET: usize = 56;
const BLACKBOX_COMMIT_OFFSET: usize = 60;
const BLACKBOX_COMMIT_MARKER: u32 = 0x4242_4F58;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum TriggerReason {
    #[default]
    ManualDoubleTap = 0,
    RadioError = 1,
    PowerDip = 2,
    Unknown(u8),
}

impl From<u8> for TriggerReason {
    fn from(val: u8) -> Self {
        match val {
            0 => Self::ManualDoubleTap,
            1 => Self::RadioError,
            2 => Self::PowerDip,
            other => Self::Unknown(other),
        }
    }
}

impl From<TriggerReason> for u8 {
    fn from(reason: TriggerReason) -> Self {
        match reason {
            TriggerReason::ManualDoubleTap => 0,
            TriggerReason::RadioError => 1,
            TriggerReason::PowerDip => 2,
            TriggerReason::Unknown(code) => code,
        }
    }
}

#[inline]
pub const fn is_seq_newer(a: u32, b: u32) -> bool {
    (a != b) && ((a.wrapping_sub(b) as i32) > 0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlackboxSnapshot {
    pub timestamp_sec: u64,
    pub v_rail_mv: u16,
    pub wifi_congestion: [u8; 13],
    pub trigger_reason: TriggerReason,
    pub ap_count: u8,
    pub ble_count: u8,
    pub seq: u32,
    pub _reserved: [u8; 34], // Pad to exact 64 bytes
}

impl BlackboxSnapshot {
    pub const SERIALIZED_SIZE: usize = 64;

    /// Serialize snapshot into a fixed 64-byte payload.
    pub fn serialize(&self) -> [u8; Self::SERIALIZED_SIZE] {
        let mut buf = [0u8; Self::SERIALIZED_SIZE];
        buf[0..8].copy_from_slice(&self.timestamp_sec.to_le_bytes());
        buf[8..10].copy_from_slice(&self.v_rail_mv.to_le_bytes());
        buf[10..23].copy_from_slice(&self.wifi_congestion);
        buf[23] = u8::from(self.trigger_reason);
        buf[24] = self.ap_count;
        buf[25] = self.ble_count;
        buf[26..30].copy_from_slice(&self.seq.to_le_bytes());
        buf[30..BLACKBOX_CRC_OFFSET].copy_from_slice(&self._reserved[..26]);
        let crc = crc32(&buf[..BLACKBOX_CRC_OFFSET]);
        buf[BLACKBOX_CRC_OFFSET..BLACKBOX_COMMIT_OFFSET].copy_from_slice(&crc.to_le_bytes());
        buf[BLACKBOX_COMMIT_OFFSET..Self::SERIALIZED_SIZE]
            .copy_from_slice(&BLACKBOX_COMMIT_MARKER.to_le_bytes());
        buf
    }

    /// Deserialize 64-byte record from Flash using direct slice indexing.
    pub fn deserialize(buf: &[u8]) -> Result<Self, StorageError> {
        if buf.len() < Self::SERIALIZED_SIZE {
            return Err(StorageError::BufferTooSmall);
        }

        let (payload, metadata) = buf.split_at(BLACKBOX_CRC_OFFSET);
        let expected_crc = u32::from_le_bytes([metadata[0], metadata[1], metadata[2], metadata[3]]);
        let commit = u32::from_le_bytes([metadata[4], metadata[5], metadata[6], metadata[7]]);
        if commit != BLACKBOX_COMMIT_MARKER || crc32(payload) != expected_crc {
            return Err(StorageError::CrcMismatch);
        }

        let timestamp_sec = u64::from_le_bytes([
            payload[0], payload[1], payload[2], payload[3], payload[4], payload[5], payload[6],
            payload[7],
        ]);
        let v_rail_mv = u16::from_le_bytes([payload[8], payload[9]]);
        let mut wifi_congestion = [0u8; 13];
        wifi_congestion.copy_from_slice(&payload[10..23]);
        let trigger_reason = TriggerReason::from(payload[23]);
        let ap_count = payload[24];
        let ble_count = payload[25];
        let seq = u32::from_le_bytes([payload[26], payload[27], payload[28], payload[29]]);
        let mut reserved = [0u8; 34];
        reserved[..26].copy_from_slice(&payload[30..56]);

        Ok(Self {
            timestamp_sec,
            v_rail_mv,
            wifi_congestion,
            trigger_reason,
            ap_count,
            ble_count,
            seq,
            _reserved: reserved,
        })
    }
}
