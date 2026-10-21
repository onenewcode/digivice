//! Application Coordinator and Central State Machine
//!
//! Provides `AppStateMachine`, an event-driven coordinator that encapsulates
//! state transitions, UI model dirty tracking, host connection watchdog,
//! and action notification lifecycles.

pub mod pet_engine;

pub use self::pet_engine::{FaceDownTransition, PetEngine};
use crate::board::{DateTime, ImuAttitudeSnapshot, PowerStatus};
use crate::gesture::GestureEvent;
use crate::proto::{
    ActionNotification, DeviceEvent, HostPacket, PresenceState, TapContext, TimeSync,
};
use crate::radio::{BleTargetCommand, RadioReport, TargetBinding};
use crate::storage::{BlackboxSnapshot, DeviceConfig, TriggerReason};
use crate::ui::{DeviceOrientation, HostConnectionState, UiState, ViewMode};
use embassy_time::{Duration, Instant};

/// Central application coordinator and finite state machine.
pub struct AppStateMachine {
    state: UiState,
    config: ConfigState,
    pet: PetEngine,
    boot_instant: Instant,
    last_heartbeat: Instant,
    last_host_packet: Instant,
    action_timer: Option<Instant>,
    dirty: bool,
}

pub struct HostPacketOutcome {
    pub time_sync: Option<TimeSync>,
    pub target: Option<BleTargetCommand>,
    pub dump_logs: bool,
}

/// Device configuration plus the BLE binding that is currently active in RAM.
///
/// `ble_target_hash` follows the radio's accepted state instead of a queued host command.
/// This prevents the coordinator from advertising a hash that has not finished persisting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigState {
    config: DeviceConfig,
    active_target: Option<TargetBinding>,
}

impl ConfigState {
    pub fn from_persisted(config: DeviceConfig) -> Self {
        let active_target = if config.ble_target_hash == 0 {
            None
        } else {
            config.ble_target_hash.try_into().ok()
        };
        Self {
            config,
            active_target,
        }
    }

    pub fn config(&self) -> DeviceConfig {
        self.config
    }

    pub fn active_target(&self) -> Option<TargetBinding> {
        self.active_target
    }

    pub fn set_active_target(&mut self, target: Option<TargetBinding>) {
        self.config.ble_target_hash = target.map(TargetBinding::hash).unwrap_or(0) as u32;
        self.active_target = target;
    }
}

impl AppStateMachine {
    /// Create new coordinator initialized from device configuration.
    pub fn new(config: DeviceConfig) -> Self {
        let now = Instant::now();
        let mut state = UiState {
            mode: config.default_mode,
            ..Default::default()
        };
        // These are live observations, not startup assumptions. Keeping them false until
        // the first host/radio report makes the first NEAR/connected edge observable.
        state.presence = PresenceState::Away;
        state.ble_rssi = -85;
        state.host_connection = HostConnectionState::Disconnected;
        let mut coordinator = Self {
            state,
            config: ConfigState::from_persisted(config),
            pet: PetEngine::new(),
            boot_instant: now,
            last_heartbeat: now,
            last_host_packet: now,
            action_timer: None,
            dirty: true,
        };
        coordinator.update_backlight();
        coordinator
    }

    /// Read-only reference to current UI state.
    pub fn state(&self) -> &UiState {
        &self.state
    }

    /// Check if UI state has changed since last publish.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark UI state as clean and return a cloned snapshot for publishing.
    pub fn take_ui_snapshot(&mut self) -> Option<UiState> {
        if self.dirty {
            self.dirty = false;
            Some(self.state.clone())
        } else {
            None
        }
    }

    /// Update target backlight baseline according to power & presence state.
    fn update_backlight(&mut self) {
        let old = self.state.backlight_pct;
        if self.state.is_facedown() {
            self.state.backlight_pct = 0;
        } else if self.config.active_target().is_some() && !self.state.is_ble_near() {
            self.state.backlight_pct = 12;
        } else {
            self.state.backlight_pct = self.config.config().brightness_normal;
        }
        if self.state.backlight_pct != old {
            self.dirty = true;
        }
    }

    /// Handle incoming gesture event with state guards.
    pub fn handle_gesture(&mut self, gesture: GestureEvent) -> Option<DeviceEvent> {
        self.state.last_gesture = Some(gesture);
        self.dirty = true;

        match gesture {
            GestureEvent::FaceDownEnter => {
                self.state.orientation = DeviceOrientation::FaceDown;
                self.pet.on_facedown(FaceDownTransition::Entering);
                self.update_backlight();
                Some(DeviceEvent::FaceDown {
                    transition: FaceDownTransition::Entering,
                })
            }
            GestureEvent::FaceDownExit => {
                self.state.orientation = DeviceOrientation::FaceUp;
                self.pet.on_facedown(FaceDownTransition::Exiting);
                self.update_backlight();
                Some(DeviceEvent::FaceDown {
                    transition: FaceDownTransition::Exiting,
                })
            }
            // If device is face down, guard against accidental mode or action changes
            _ if self.state.is_facedown() => None,

            GestureEvent::SingleTap => {
                let now = Instant::now();
                self.pet.on_tap(now);
                if self.state.action != ActionNotification::Idle {
                    self.state.action = ActionNotification::Idle;
                    self.action_timer = None;
                }
                Some(DeviceEvent::SingleTap {
                    context: TapContext::NotifyClear,
                })
            }
            GestureEvent::DoubleTap => {
                let now = Instant::now();
                self.pet.on_double_tap(now);
                if let ActionNotification::WaitConfirm {
                    ref source,
                    ref title,
                } = self.state.action
                {
                    self.state.action = ActionNotification::Success {
                        source: source.clone(),
                        title: title.clone(),
                    };
                    // Auto-collapse success notice after 3 seconds
                    self.action_timer = Some(Instant::now());
                    Some(DeviceEvent::DoubleTap {
                        context: TapContext::WaitConfirm,
                    })
                } else {
                    None
                }
            }
            GestureEvent::TiltLeft => {
                // Guarded mode transition to the left: Pet -> Island -> SelfTest
                let new_mode = self.state.mode.prev();
                if new_mode != self.state.mode {
                    self.state.mode = new_mode;
                    self.update_backlight();
                }
                None
            }
            GestureEvent::TiltRight => {
                // Guarded mode transition to the right: SelfTest -> Island -> Pet
                let new_mode = self.state.mode.next();
                if new_mode != self.state.mode {
                    self.state.mode = new_mode;
                    self.update_backlight();
                }
                None
            }
            GestureEvent::HeadingUpdate(hdg) => {
                self.state.heading_deg = hdg as f32;
                None
            }
        }
    }

    /// Handle incoming host packet from USB.
    pub fn handle_host_packet(&mut self, pkt: HostPacket) -> HostPacketOutcome {
        self.last_host_packet = Instant::now();
        if !self.state.is_host_connected() {
            self.state.host_connection = HostConnectionState::Connected;
            self.dirty = true;
        }

        match pkt {
            HostPacket::Telemetry(telem) => {
                self.pet.on_telemetry(telem.cpu_pct, telem.cpu_temp);
                self.state.telemetry = telem;
                self.dirty = true;
                HostPacketOutcome {
                    time_sync: None,
                    target: None,
                    dump_logs: false,
                }
            }
            HostPacket::Action(act) => {
                self.state.action = act;
                self.action_timer = Some(Instant::now());
                self.dirty = true;
                HostPacketOutcome {
                    time_sync: None,
                    target: None,
                    dump_logs: false,
                }
            }
            HostPacket::Sync(sync) => {
                self.dirty = true;
                HostPacketOutcome {
                    time_sync: Some(sync),
                    target: None,
                    dump_logs: false,
                }
            }
            HostPacket::BindBleTarget { binding } => HostPacketOutcome {
                time_sync: None,
                target: Some(BleTargetCommand::Bind(binding)),
                dump_logs: false,
            },
            HostPacket::ClearBleTarget => HostPacketOutcome {
                time_sync: None,
                target: Some(BleTargetCommand::Clear),
                dump_logs: false,
            },
            HostPacket::DumpLogs => HostPacketOutcome {
                time_sync: None,
                target: None,
                dump_logs: true,
            },
        }
    }

    /// Apply a binding only after the radio has accepted the persisted value.
    pub fn apply_target_binding(&mut self, command: BleTargetCommand) {
        self.config.set_active_target(command.into());
        self.dirty = true;
    }

    /// Build a snapshot for a host-requested blackbox export.
    pub fn blackbox_snapshot(&self) -> BlackboxSnapshot {
        BlackboxSnapshot {
            timestamp_sec: crate::board::datetime_to_epoch(&self.state.datetime),
            v_rail_mv: self.state.v_rail_mv,
            wifi_congestion: [0; 13],
            trigger_reason: TriggerReason::HostDump,
            ap_count: 0,
            ble_count: match self.state.presence {
                PresenceState::Near => 1,
                PresenceState::Away => 0,
            },
            _reserved: [0; 38],
        }
    }

    /// Handle periodic BLE radio presence report.
    pub fn handle_radio_report(&mut self, report: RadioReport) {
        if self.state.presence != report.presence || self.state.ble_rssi != report.ble_rssi {
            self.state.presence = report.presence;
            self.state.ble_rssi = report.ble_rssi;
            self.update_backlight();
            self.dirty = true;
        }
    }

    /// Convert a presence report into an edge-triggered host event.
    pub fn presence_event(&self, report: &RadioReport) -> Option<DeviceEvent> {
        if !report.is_target_bound() || !report.is_target_seen() {
            return None;
        }
        let state = report.presence;
        let changed = match state {
            PresenceState::Near => !self.state.is_ble_near(),
            PresenceState::Away => self.state.is_ble_near(),
        };
        changed.then_some(DeviceEvent::Presence {
            state,
            rssi: report.ble_rssi,
        })
    }

    pub fn bound_target(&self) -> Option<TargetBinding> {
        self.config.active_target()
    }

    /// Return the current configuration and its active BLE binding.
    pub fn config_state(&self) -> ConfigState {
        self.config
    }

    /// Candidate configuration for a pending target command.
    ///
    /// The active binding is intentionally unchanged until the radio acknowledges the
    /// successfully persisted value.
    pub fn config_with_target(&self, target: BleTargetCommand) -> DeviceConfig {
        let mut config = self.config.config();
        config.ble_target_hash = match target {
            BleTargetCommand::Bind(binding) => binding.hash() as u32,
            BleTargetCommand::Clear => 0,
        };
        config
    }

    /// Handle RTC time tick.
    pub fn handle_time(&mut self, dt: DateTime) {
        self.state.datetime = dt;
        self.dirty = true;
    }

    /// Handle power rail and key voltage sampling via strongly-typed PowerStatus.
    pub fn handle_power(&mut self, status: PowerStatus) {
        let v_diff = (self.state.v_rail_mv as i32 - status.v_rail_mv as i32).abs();
        if v_diff >= 20
            || self.state.charge_state != status.charge_state
            || self.state.boot_press_state != status.boot_press_state
        {
            self.state.v_rail_mv = status.v_rail_mv;
            self.state.charge_state = status.charge_state;
            self.state.boot_press_state = status.boot_press_state;
            self.dirty = true;
        }
    }

    /// Handle IMU sample update via strongly-typed ImuAttitudeSnapshot (only marks dirty if in SelfTest view).
    pub fn handle_imu(&mut self, snapshot: ImuAttitudeSnapshot) {
        self.pet
            .on_imu_attitude(snapshot.sample.ax_mg, snapshot.sample.ay_mg);
        self.state.imu_sample = snapshot.sample;
        self.state.heading_deg = snapshot.heading_deg;
        self.state.imu_status = snapshot.imu_status;
        if self.state.mode == ViewMode::SelfTest || self.state.mode == ViewMode::Pet {
            self.dirty = true;
        }
    }

    /// Periodic state machine tick: checks host watchdog, action timeouts, and heartbeats.
    pub fn tick(&mut self, now: Instant) -> Option<DeviceEvent> {
        // 1. Host Watchdog: 10-second timeout disconnects host status
        if self.state.is_host_connected()
            && now.duration_since(self.last_host_packet) >= Duration::from_secs(10)
        {
            self.state.host_connection = HostConnectionState::Disconnected;
            self.dirty = true;
        }

        // 2. Action Lifecycle Management
        match self.state.action {
            ActionNotification::WaitConfirm {
                ref source,
                ref title,
            } => {
                if let Some(t) = self.action_timer {
                    // Auto-timeout after 15 seconds if unconfirmed
                    if now.duration_since(t) >= Duration::from_secs(15) {
                        self.state.action = ActionNotification::Error {
                            source: source.clone(),
                            title: title.clone(),
                        };
                        self.action_timer = Some(now); // Keep error card for 3 seconds
                        self.dirty = true;
                    }
                }
            }
            ActionNotification::Success { .. } | ActionNotification::Error { .. } => {
                if let Some(t) = self.action_timer {
                    // Auto-dismiss completed action card after 3 seconds
                    if now.duration_since(t) >= Duration::from_secs(3) {
                        self.state.action = ActionNotification::Idle;
                        self.action_timer = None;
                        self.dirty = true;
                    }
                }
            }
            _ => {}
        }

        // 3. Update Pet Behavior Engine
        let new_pet = self.pet.tick(now);
        if self.state.pet != new_pet {
            self.state.pet = new_pet;
            if self.state.mode == ViewMode::Pet {
                self.dirty = true;
            }
        }

        // 4. Periodic Heartbeat (every 5 seconds, monotonic drift-free calculation)
        if now.duration_since(self.last_heartbeat) >= Duration::from_secs(5) {
            self.last_heartbeat = now;
            let uptime_sec = now.duration_since(self.boot_instant).as_secs() as u32;
            Some(DeviceEvent::Heartbeat {
                v_rail: self.state.v_rail_mv,
                uptime_sec,
            })
        } else {
            None
        }
    }
}

#[cfg(all(test, not(target_os = "none")))]
mod tests {
    use super::*;
    use crate::radio::{TargetBindingState, TargetVisibility};
    use heapless::Vec;

    fn report(near: bool, rssi: i8) -> RadioReport {
        RadioReport {
            target_bound: TargetBindingState::Bound,
            target_seen: TargetVisibility::Visible,
            presence: if near {
                PresenceState::Near
            } else {
                PresenceState::Away
            },
            ble_rssi: rssi,
            ble_blips: Vec::new(),
        }
    }

    #[test]
    fn clear_binding_emits_clear_command() {
        let mut app = AppStateMachine::new(DeviceConfig::default());
        let outcome = app.handle_host_packet(HostPacket::ClearBleTarget);
        assert_eq!(outcome.target, Some(BleTargetCommand::Clear));
        app.apply_target_binding(BleTargetCommand::Clear);
        assert_eq!(app.bound_target(), None);
        assert_eq!(app.config_state().config().ble_target_hash, 0);
    }

    #[test]
    fn presence_event_is_edge_triggered() {
        let mut app = AppStateMachine::new(DeviceConfig::default());
        let away = report(false, -82);
        let near = report(true, -54);

        app.handle_radio_report(near.clone());
        assert_eq!(app.presence_event(&near), None);
        assert_eq!(
            app.presence_event(&away)
                .map(|event| matches!(event, DeviceEvent::Presence { rssi: -82, .. })),
            Some(true)
        );
    }

    #[test]
    fn unobserved_target_does_not_start_near() {
        let mut app = AppStateMachine::new(DeviceConfig::default());
        let report = RadioReport {
            target_bound: TargetBindingState::Bound,
            target_seen: TargetVisibility::Hidden,
            presence: PresenceState::Away,
            ble_rssi: -85,
            ble_blips: Vec::new(),
        };
        assert_eq!(app.presence_event(&report), None);
        app.handle_radio_report(report);
        assert!(!app.state().is_ble_near());
    }
}
