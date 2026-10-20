//! BLE 5 Presence Detection & Beacon Tracker

use crate::ui::BleBlip;
use embassy_time::{Duration, Instant};
use heapless::Vec;

pub struct BlePresenceTracker {
    pub target_hash: Option<u16>,
    pub smoothed_target_rssi: f32,
    pub is_near: bool,
    pub last_seen_target: Instant,
    pub blips: Vec<BleBlip, 12>,
    pub rssi_near_threshold: i8,
    pub rssi_away_threshold: i8,
}

impl BlePresenceTracker {
    pub fn new(target_hash: Option<u16>) -> Self {
        Self {
            target_hash,
            smoothed_target_rssi: -85.0,
            is_near: false,
            last_seen_target: Instant::now(),
            blips: Vec::new(),
            rssi_near_threshold: -68,
            rssi_away_threshold: -78,
        }
    }

    /// Process a detected BLE advertisement report
    pub fn process_advertisement(&mut self, addr_bytes: &[u8; 6], rssi: i8, now: Instant) {
        // Hash the 6-byte address to 16-bit
        let hash = ((addr_bytes[0] as u16) ^ (addr_bytes[2] as u16) ^ (addr_bytes[4] as u16))
            | (((addr_bytes[1] as u16) ^ (addr_bytes[3] as u16) ^ (addr_bytes[5] as u16)) << 8);

        let is_target = match self.target_hash {
            Some(t) => t == hash,
            None => false,
        };

        // If target or if no target bound yet and signal is strong (e.g. phone right next to board)
        if is_target {
            self.last_seen_target = now;
            // Exponential moving average: alpha = 0.25
            self.smoothed_target_rssi = 0.25 * (rssi as f32) + 0.75 * self.smoothed_target_rssi;
            if self.smoothed_target_rssi >= (self.rssi_near_threshold as f32) {
                self.is_near = true;
            } else if self.smoothed_target_rssi < (self.rssi_away_threshold as f32) {
                self.is_near = false;
            }
        }

        // Update Radar Blip Table
        if let Some(existing) = self.blips.iter_mut().find(|b| b.hash == hash) {
            existing.rssi = rssi;
            existing.is_target = is_target;
        } else {
            if self.blips.is_full() {
                self.blips.remove(0);
            }
            let _ = self.blips.push(BleBlip {
                hash,
                rssi,
                is_target,
            });
        }
    }

    /// Check watchdog (if target not seen for > 15s -> declare AWAY)
    pub fn update_watchdog(&mut self, now: Instant) -> bool {
        if self.is_near && now.duration_since(self.last_seen_target) > Duration::from_secs(15) {
            self.is_near = false;
        }
        self.is_near
    }

    /// Bind the currently strongest device as target
    pub fn bind_strongest(&mut self) -> Option<u16> {
        if let Some(strongest) = self.blips.iter().max_by_key(|b| b.rssi) {
            self.target_hash = Some(strongest.hash);
            self.smoothed_target_rssi = strongest.rssi as f32;
            self.is_near = true;
            Some(strongest.hash)
        } else {
            None
        }
    }
}
