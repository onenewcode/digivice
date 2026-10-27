/// BLE 5 Presence Detection & Beacon Tracker
use super::{BleBlip, BleBlipRole, PresenceState, RadioStatus, TargetVisibility};
use embassy_time::{Duration, Instant};
use heapless::Vec;

pub struct BlePresenceTracker {
    pub radio_status: RadioStatus,
    pub target_hash: Option<u16>,
    pub smoothed_target_rssi: i16,
    pub presence: PresenceState,
    pub target_visibility: TargetVisibility,
    pub last_seen_target: Instant,
    pub blips: Vec<BleBlip, 12>,
    pub rssi_near_threshold: i8,
    pub rssi_away_threshold: i8,
}

/// Validated 16-bit BLE target identifier persisted in device configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetBinding(u16);

impl TargetBinding {
    pub const fn new(hash: u16) -> Option<Self> {
        if hash == 0 { None } else { Some(Self(hash)) }
    }

    /// Construct a target binding by XOR-folding a 6-byte BLE address into 16 bits.
    /// Returns None if the resulting hash evaluates to 0.
    pub const fn from_addr(addr: &[u8; 6]) -> Option<Self> {
        Self::new(Self::hash_addr(addr))
    }

    /// Pure function computing the 16-bit XOR folded hash of a 6-byte BLE address.
    #[inline]
    pub const fn hash_addr(addr: &[u8; 6]) -> u16 {
        ((addr[0] as u16) ^ (addr[2] as u16) ^ (addr[4] as u16))
            | (((addr[1] as u16) ^ (addr[3] as u16) ^ (addr[5] as u16)) << 8)
    }

    pub const fn hash(self) -> u16 {
        self.0
    }

    /// Check if a 6-byte BLE address matches this target binding hash.
    #[inline]
    pub const fn matches_addr(&self, addr: &[u8; 6]) -> bool {
        self.0 == Self::hash_addr(addr)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetBindingError {
    OutOfRange,
    Zero,
}

impl core::fmt::Display for TargetBindingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::OutOfRange => f.write_str("BLE target hash exceeds 16 bits"),
            Self::Zero => f.write_str("BLE target hash must be non-zero"),
        }
    }
}

impl core::error::Error for TargetBindingError {}

impl TryFrom<u32> for TargetBinding {
    type Error = TargetBindingError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        let hash = u16::try_from(value).map_err(|_| TargetBindingError::OutOfRange)?;
        TargetBinding::new(hash).ok_or(TargetBindingError::Zero)
    }
}

impl core::fmt::Display for TargetBinding {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl BlePresenceTracker {
    pub fn new(target: Option<TargetBinding>) -> Self {
        Self::new_with_away_threshold(target, -78)
    }

    pub fn new_with_away_threshold(target: Option<TargetBinding>, away_threshold: i8) -> Self {
        Self::new_with_away_threshold_and_status(target, away_threshold, RadioStatus::Active)
    }

    pub fn new_with_away_threshold_and_status(
        target: Option<TargetBinding>,
        away_threshold: i8,
        radio_status: RadioStatus,
    ) -> Self {
        let away_threshold = away_threshold.clamp(-120, 0);
        Self {
            radio_status,
            target_hash: target.map(TargetBinding::hash),
            smoothed_target_rssi: -85,
            presence: PresenceState::Away,
            target_visibility: TargetVisibility::Hidden,
            last_seen_target: Instant::now(),
            blips: Vec::new(),
            rssi_near_threshold: away_threshold.saturating_add(10).min(0),
            rssi_away_threshold: away_threshold,
        }
    }

    #[inline]
    pub fn is_near(&self) -> bool {
        self.radio_status.is_operational() && matches!(self.presence, PresenceState::Near)
    }

    pub fn set_radio_status(&mut self, status: RadioStatus) {
        self.radio_status = status;
        if !status.is_operational() {
            self.presence = PresenceState::Away;
            self.target_visibility = TargetVisibility::Hidden;
            self.smoothed_target_rssi = -85;
            self.blips.clear();
        }
    }

    pub fn set_target(&mut self, binding: Option<TargetBinding>) {
        self.target_hash = binding.map(TargetBinding::hash);
        self.smoothed_target_rssi = -85;
        self.presence = PresenceState::Away;
        self.target_visibility = TargetVisibility::Hidden;
        self.blips.clear();
        self.last_seen_target = Instant::now();
    }

    /// Process a detected BLE advertisement report
    pub fn process_advertisement(&mut self, addr_bytes: &[u8; 6], rssi: i8, now: Instant) {
        if !self.radio_status.is_operational() {
            return;
        }
        // Hash the 6-byte address to 16-bit
        let hash = TargetBinding::hash_addr(addr_bytes);

        let role = match self.target_hash {
            Some(t) if t == hash => BleBlipRole::Target,
            _ => BleBlipRole::Other,
        };
        let is_target = matches!(role, BleBlipRole::Target);

        // Only a bound target contributes to presence state; binding is explicit over host protocol.
        if is_target {
            self.last_seen_target = now;
            self.target_visibility = TargetVisibility::Visible;
            // Zero-FPU Integer Exponential Moving Average: new = (old * 3 + rssi) / 4
            self.smoothed_target_rssi =
                (((self.smoothed_target_rssi as i32 * 3) + (rssi as i32)) / 4) as i16;
            if self.smoothed_target_rssi >= (self.rssi_near_threshold as i16) {
                self.presence = PresenceState::Near;
            } else if self.smoothed_target_rssi < (self.rssi_away_threshold as i16) {
                self.presence = PresenceState::Away;
            }
        }

        // Update BLE Target Blip Table
        if let Some(existing) = self.blips.iter_mut().find(|b| b.hash == hash) {
            existing.rssi = rssi;
            existing.role = role;
        } else {
            if self.blips.is_full() {
                self.blips.remove(0);
            }
            let _ = self.blips.push(BleBlip { hash, rssi, role });
        }
    }

    /// Check watchdog (if target not seen for > 15s -> declare AWAY)
    pub fn update_watchdog(&mut self, now: Instant) -> PresenceState {
        if !self.radio_status.is_operational() {
            self.presence = PresenceState::Away;
            self.target_visibility = TargetVisibility::Hidden;
            self.smoothed_target_rssi = -85;
            self.blips.clear();
            return PresenceState::Away;
        }

        const BLIP_TTL: Duration = Duration::from_secs(15);
        let target_expired =
            self.target_hash.is_some() && now.duration_since(self.last_seen_target) > BLIP_TTL;
        if target_expired {
            self.presence = PresenceState::Away;
            self.target_visibility = TargetVisibility::Hidden;
            self.smoothed_target_rssi = -85;
            // Do not let bind_strongest or diagnostics reuse observations from the expired epoch.
            self.blips.clear();
        }
        self.presence
    }

    /// Bind the currently strongest device as target
    pub fn bind_strongest(&mut self) -> Option<u16> {
        if let Some(strongest) = self.blips.iter().max_by_key(|b| b.rssi) {
            let binding = TargetBinding::new(strongest.hash)?;
            self.target_hash = Some(binding.hash());
            self.smoothed_target_rssi = strongest.rssi as i16;
            self.presence = PresenceState::Near;
            self.target_visibility = TargetVisibility::Visible;
            self.last_seen_target = Instant::now();
            Some(binding.hash())
        } else {
            None
        }
    }
}
