use std::{cell::RefCell, rc::Rc};

use digivice_core::storage::{
    BLACKBOX_FLASH_OFFSET, BLACKBOX_SNAPSHOT_CAPACITY, BlackboxSnapshot, FLASH_SECTOR_SIZE,
    StorageManager, TriggerReason,
};
use embedded_storage::nor_flash::{ErrorType, NorFlash, NorFlashErrorKind, ReadNorFlash};

const TOTAL_FLASH_SIZE: usize = 0x100_0000; // 16MB

struct MemFlashInner {
    bytes: Vec<u8>,
    erase_count: usize,
    write_count: usize,
}

#[derive(Clone)]
struct MemFlash(Rc<RefCell<MemFlashInner>>);

impl MemFlash {
    fn new() -> Self {
        Self(Rc::new(RefCell::new(MemFlashInner {
            bytes: vec![0xFF; TOTAL_FLASH_SIZE],
            erase_count: 0,
            write_count: 0,
        })))
    }

    fn corrupt_byte(&self, offset: u32) {
        let mut inner = self.0.borrow_mut();
        inner.bytes[offset as usize] ^= 0xAA;
    }
}

impl ErrorType for MemFlash {
    type Error = NorFlashErrorKind;
}

impl ReadNorFlash for MemFlash {
    const READ_SIZE: usize = 1;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        let inner = self.0.borrow();
        let start = offset as usize;
        let end = start + bytes.len();
        if end > inner.bytes.len() {
            return Err(NorFlashErrorKind::OutOfBounds);
        }
        bytes.copy_from_slice(&inner.bytes[start..end]);
        Ok(())
    }

    fn capacity(&self) -> usize {
        TOTAL_FLASH_SIZE
    }
}

impl NorFlash for MemFlash {
    const WRITE_SIZE: usize = 1;
    const ERASE_SIZE: usize = FLASH_SECTOR_SIZE;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        let mut inner = self.0.borrow_mut();
        inner.erase_count += 1;
        let start = from as usize;
        let end = to as usize;
        if end > inner.bytes.len()
            || !start.is_multiple_of(FLASH_SECTOR_SIZE)
            || !end.is_multiple_of(FLASH_SECTOR_SIZE)
        {
            return Err(NorFlashErrorKind::OutOfBounds);
        }
        for b in &mut inner.bytes[start..end] {
            *b = 0xFF;
        }
        Ok(())
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        let mut inner = self.0.borrow_mut();
        inner.write_count += 1;
        let start = offset as usize;
        let end = start + bytes.len();
        if end > inner.bytes.len() {
            return Err(NorFlashErrorKind::OutOfBounds);
        }
        for (i, &byte) in bytes.iter().enumerate() {
            let target = &mut inner.bytes[start + i];
            // Emulate NOR flash bit clearing: can only change 1 -> 0
            if (*target & byte) != byte {
                // Illegal write without erase
                return Err(NorFlashErrorKind::Other);
            }
            *target &= byte;
        }
        Ok(())
    }
}

fn sample_snapshot(timestamp_sec: u64) -> BlackboxSnapshot {
    BlackboxSnapshot {
        timestamp_sec,
        v_rail_mv: 4100,
        wifi_congestion: [0; 13],
        trigger_reason: TriggerReason::PowerDip,
        ap_count: 2,
        ble_count: 1,
        seq: 0,
        _reserved: [0; 34],
    }
}

#[test]
fn single_corrupted_slot_is_isolated_without_erasing_sector() {
    let flash = MemFlash::new();
    let mut manager = StorageManager::new(flash.clone());

    // 1. Write 20 valid snapshots into Sector 0
    for i in 0..20 {
        manager.record_snapshot(&sample_snapshot(1000 + i)).unwrap();
    }

    // 2. Corrupt slot 7 (in the middle of sector 0)
    let slot_7_offset = BLACKBOX_FLASH_OFFSET + 7 * 64;
    flash.corrupt_byte(slot_7_offset + 10);

    // 3. Reboot: create new manager instance
    let mut rebooted = StorageManager::new(flash.clone());

    // 4. Verify that slot 7 fails deserialization, but slots 0..6 and 8..19 remain 100% intact!
    assert!(
        rebooted.read_snapshot(7).is_err(),
        "Slot 7 must be detected as corrupted"
    );
    for i in 0..20 {
        if i == 7 {
            continue;
        }
        let snap = rebooted.read_snapshot(i).unwrap();
        assert_eq!(snap.timestamp_sec, 1000 + i as u64);
        assert_eq!(snap.seq, i + 1);
    }

    // 5. Verify the cursor resumes at slot 20 with sequence 21
    let next_slot = rebooted.record_snapshot(&sample_snapshot(2000)).unwrap();
    assert_eq!(next_slot, 20);
    let snap_20 = rebooted.read_snapshot(20).unwrap();
    assert_eq!(snap_20.seq, 21);
}

#[test]
fn record_snapshots_across_reboots_at_sector_and_ring_boundaries() {
    let flash = MemFlash::new();
    let mut manager = StorageManager::new(flash.clone());

    // Write up to sector boundary (slot 63)
    for i in 0..64 {
        let slot = manager.record_snapshot(&sample_snapshot(i)).unwrap();
        assert_eq!(slot, i as u32);
    }

    // Reboot right at sector boundary (next slot is 64, start of Sector 1)
    let mut reboot_sector = StorageManager::new(flash.clone());
    let slot_64 = reboot_sector.record_snapshot(&sample_snapshot(64)).unwrap();
    assert_eq!(slot_64, 64);
    assert_eq!(reboot_sector.read_snapshot(64).unwrap().seq, 65);

    // Fill to near end of ring buffer (slots 65..1000)
    for i in 65..BLACKBOX_SNAPSHOT_CAPACITY {
        let slot = reboot_sector
            .record_snapshot(&sample_snapshot(i as u64))
            .unwrap();
        assert_eq!(slot, i);
    }

    // Reboot right at ring boundary (slot 999 filled, next is slot 0 wrapping around)
    let mut reboot_wrap = StorageManager::new(flash.clone());
    let slot_0_round2 = reboot_wrap.record_snapshot(&sample_snapshot(1000)).unwrap();
    assert_eq!(slot_0_round2, 0, "Must wrap around to slot 0");
    let snap_0_round2 = reboot_wrap.read_snapshot(0).unwrap();
    assert_eq!(
        snap_0_round2.seq, 1001,
        "Sequence must be monotonically 1001"
    );

    // Write another 10 snapshots into round 2
    for i in 1..10 {
        let slot = reboot_wrap
            .record_snapshot(&sample_snapshot(1000 + i as u64))
            .unwrap();
        assert_eq!(slot, i);
    }

    // Reboot again in the middle of round 2
    let mut reboot_round2 = StorageManager::new(flash.clone());
    let slot_10_round2 = reboot_round2
        .record_snapshot(&sample_snapshot(1010))
        .unwrap();
    assert_eq!(slot_10_round2, 10);
    assert_eq!(reboot_round2.read_snapshot(10).unwrap().seq, 1011);
}
