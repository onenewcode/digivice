//! 2.4GHz Wi-Fi Spectrum Congestion Analyzer & Radar Mapper

use crate::ui::WifiApBlip;
use esp_radio::wifi::ap::AccessPointInfo;
use heapless::Vec;

pub struct WifiRadarAnalyzer {
    pub congestion: [u8; 13],
    pub ap_blips: Vec<WifiApBlip, 8>,
}

impl Default for WifiRadarAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl WifiRadarAnalyzer {
    pub fn new() -> Self {
        Self {
            congestion: [0u8; 13],
            ap_blips: Vec::new(),
        }
    }

    /// Process a list of discovered APs from esp_radio scan
    pub fn process_scan_results(&mut self, aps: &[AccessPointInfo]) {
        let mut raw_scores = [0u32; 13];
        self.ap_blips.clear();

        for ap in aps {
            let ch = ap.channel;
            let rssi = ap.signal_strength;

            // Keep top APs for radar display
            if (1..=13).contains(&ch) && !self.ap_blips.is_full() {
                let _ = self.ap_blips.push(WifiApBlip { channel: ch, rssi });
            }

            if (1..=13).contains(&ch) {
                let ch_idx = (ch - 1) as usize;
                // AP contribution score based on RSSI: (-100 dBm -> 0, -30 dBm -> 70)
                let base_score = (rssi.max(-100) + 100) as u32;

                // Center channel gets full score
                raw_scores[ch_idx] += base_score;

                // ±1 channel gets 70% spillover
                if ch_idx > 0 {
                    raw_scores[ch_idx - 1] += (base_score * 7) / 10;
                }
                if ch_idx + 1 < 13 {
                    raw_scores[ch_idx + 1] += (base_score * 7) / 10;
                }

                // ±2 channels get 30% spillover
                if ch_idx >= 2 {
                    raw_scores[ch_idx - 2] += (base_score * 3) / 10;
                }
                if ch_idx + 2 < 13 {
                    raw_scores[ch_idx + 2] += (base_score * 3) / 10;
                }
            }
        }

        // Normalize raw scores into 0..100 scale
        let max_score = raw_scores.iter().copied().max().unwrap_or(1).max(1);
        for (ch, score) in raw_scores.iter().enumerate().take(13) {
            let norm = (*score * 100) / max_score;
            self.congestion[ch] = norm.min(100) as u8;
        }
    }
}
