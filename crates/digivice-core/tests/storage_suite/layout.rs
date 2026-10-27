use digivice_core::storage::{
    BLACKBOX_FLASH_OFFSET, BLACKBOX_SNAPSHOT_CAPACITY, CONFIG_FLASH_PRIMARY_OFFSET,
    CONFIG_FLASH_SECONDARY_OFFSET, FLASH_SECTOR_SIZE,
};

const FLASH_TOTAL_SIZE_16MB: u32 = 16 * 1024 * 1024; // 0x0100_0000
const FACTORY_APP_MAX_OFFSET: u32 = 0x0040_0000; // 4MB mark

#[test]
fn flash_storage_constants_are_sector_aligned() {
    assert_eq!(FLASH_SECTOR_SIZE, 4096);
    assert_eq!(
        CONFIG_FLASH_PRIMARY_OFFSET % (FLASH_SECTOR_SIZE as u32),
        0,
        "Primary sector must be 4KB aligned"
    );
    assert_eq!(
        CONFIG_FLASH_SECONDARY_OFFSET % (FLASH_SECTOR_SIZE as u32),
        0,
        "Secondary sector must be 4KB aligned"
    );
    assert_eq!(
        BLACKBOX_FLASH_OFFSET % (FLASH_SECTOR_SIZE as u32),
        0,
        "Blackbox offset must be 4KB aligned"
    );
}

#[test]
fn flash_storage_boundary_protects_application_image_area() {
    // Factory firmware binary is placed in 0x0001_0000..0x0040_0000
    const {
        assert!(
            CONFIG_FLASH_PRIMARY_OFFSET >= FACTORY_APP_MAX_OFFSET,
            "Storage must start at or above 4MB to never collide with firmware code"
        );
    }
    assert_eq!(CONFIG_FLASH_PRIMARY_OFFSET, 0x0040_0000);
}

#[test]
fn flash_sectors_are_disjoint_and_fit_in_chip_capacity() {
    assert_eq!(
        CONFIG_FLASH_SECONDARY_OFFSET,
        CONFIG_FLASH_PRIMARY_OFFSET + FLASH_SECTOR_SIZE as u32,
        "Secondary sector must directly follow primary sector without overlap"
    );
    assert!(
        BLACKBOX_FLASH_OFFSET >= CONFIG_FLASH_SECONDARY_OFFSET + FLASH_SECTOR_SIZE as u32,
        "Blackbox must not overlap with ping-pong configuration sectors"
    );

    let blackbox_end = BLACKBOX_FLASH_OFFSET + BLACKBOX_SNAPSHOT_CAPACITY * 64;
    assert!(
        blackbox_end <= FLASH_TOTAL_SIZE_16MB,
        "Blackbox snapshot capacity must fit within 16MB flash capacity"
    );
}
