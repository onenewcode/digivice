//! 16MB Flash Storage & Wireless Situation Blackbox

use esp_hal::peripherals::FLASH;
use esp_storage::FlashStorage;

pub const CONFIG_FLASH_OFFSET: u32 = 0x0040_0000; // 4MB mark
pub const BLACKBOX_FLASH_OFFSET: u32 = 0x0041_0000;
pub const FLASH_SECTOR_SIZE: usize = 4096;

pub const CONFIG_MAGIC: u32 = 0x4953_4C44; // "ISLD"

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DeviceConfig {
    pub magic: u32,
    pub gyro_bias_mdps: [i32; 3],
    pub accel_bias_mg: [i16; 3],
    pub ble_target_hash: u32,
    pub rssi_away_threshold: i8,
    pub brightness_normal: u8,
    pub default_mode: u8,
    pub _reserved: [u8; 13],
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            magic: CONFIG_MAGIC,
            gyro_bias_mdps: [0, 0, 0],
            accel_bias_mg: [0, 0, 0],
            ble_target_hash: 0,
            rssi_away_threshold: -78,
            brightness_normal: 80,
            default_mode: 0,
            _reserved: [0u8; 13],
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BlackboxSnapshot {
    pub timestamp_sec: u64,
    pub v_rail_mv: u16,
    pub wifi_congestion: [u8; 13],
    pub trigger_reason: u8, // 0=Manual DoubleTap, 1=Host Error, 2=Power Dip
    pub ap_count: u8,
    pub ble_count: u8,
    pub _reserved: [u8; 39], // Pad to 64 bytes
}

pub struct StorageManager<'d> {
    flash: FlashStorage<'d>,
}

impl<'d> StorageManager<'d> {
    pub fn new(flash_peri: FLASH<'d>) -> Self {
        Self {
            flash: FlashStorage::new(flash_peri),
        }
    }

    /// Load device configuration from Flash; returns default if magic mismatch
    pub fn load_config(&mut self) -> DeviceConfig {
        let mut buf = [0u8; core::mem::size_of::<DeviceConfig>()];
        if self.flash.read(CONFIG_FLASH_OFFSET, &mut buf).is_ok() {
            let config: DeviceConfig =
                unsafe { core::ptr::read_unaligned(buf.as_ptr() as *const _) };
            if config.magic == CONFIG_MAGIC {
                return config;
            }
        }
        DeviceConfig::default()
    }

    /// Save device configuration to Flash (erases sector first)
    #[allow(clippy::result_unit_err)]
    pub fn save_config(&mut self, config: &DeviceConfig) -> Result<(), ()> {
        self.flash
            .erase(
                CONFIG_FLASH_OFFSET,
                CONFIG_FLASH_OFFSET + (FLASH_SECTOR_SIZE as u32),
            )
            .map_err(|_| ())?;

        let bytes: &[u8] = unsafe {
            core::slice::from_raw_parts(
                config as *const _ as *const u8,
                core::mem::size_of::<DeviceConfig>(),
            )
        };

        self.flash.write(CONFIG_FLASH_OFFSET, bytes).map_err(|_| ())
    }

    /// Record a 64-byte blackbox snapshot at the circular index
    #[allow(clippy::result_unit_err)]
    pub fn record_snapshot(&mut self, index: u32, snapshot: &BlackboxSnapshot) -> Result<(), ()> {
        let max_snapshots = 1000u32;
        let slot = index % max_snapshots;
        let offset = BLACKBOX_FLASH_OFFSET + (slot * 64);

        let bytes: &[u8] = unsafe {
            core::slice::from_raw_parts(
                snapshot as *const _ as *const u8,
                core::mem::size_of::<BlackboxSnapshot>(),
            )
        };

        self.flash.write(offset, bytes).map_err(|_| ())
    }
}
