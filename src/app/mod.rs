//! Application coordinator and device-level state machine.
//!
//! Product focus rhythm lives in `app::focus`, domain models in `domain`, mascot behavior in `pet`,
//! and transport details in `radio`. This module translates events between those layers and
//! publishes a UI snapshot, keeping each subsystem replaceable.

mod config;
pub mod focus;

pub use config::ConfigState;
pub use focus::FocusSession;

use crate::board::{ButtonPressState, DateTime, ImuAttitudeSnapshot, PowerStatus};
use crate::domain::{DeviceOrientation, FocusEvent, HidAction, MusicMood, MusicScene, ViewMode};
use crate::gesture::GestureEvent;
use crate::ipc::HID_ACTION_CHANNEL;
use crate::pet::{DefaultPet, PetBehavior, PetEvent};
use crate::radio::{BleTargetCommand, PresenceState, RadioReport, TargetBinding};
use crate::storage::{BlackboxSnapshot, DeviceConfig, TriggerReason};
use crate::ui::UiState;
use embassy_time::Instant;
use esp_println::println;

const DIMMED_BACKLIGHT_PERCENT: u8 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BacklightPolicy {
    Off,
    Dimmed,
    Normal,
}

impl BacklightPolicy {
    const fn percent(self, normal: u8) -> u8 {
        match self {
            Self::Off => 0,
            Self::Dimmed => DIMMED_BACKLIGHT_PERCENT,
            Self::Normal => normal,
        }
    }
}

/// Central application coordinator and finite state machine.
pub struct AppStateMachine<P: PetBehavior = DefaultPet> {
    state: UiState,
    config: ConfigState,
    pet: P,
    focus: FocusSession,
    dirty: bool,
}

impl AppStateMachine<DefaultPet> {
    /// Create new coordinator initialized from device configuration.
    pub fn new(config: DeviceConfig) -> Self {
        Self::with_pet(config, DefaultPet::new())
    }
}

impl<P: PetBehavior> AppStateMachine<P> {
    /// Create a coordinator with a custom pet implementation.
    pub fn with_pet(config: DeviceConfig, pet: P) -> Self {
        let mut state = UiState {
            mode: config.default_mode,
            ..Default::default()
        };
        // Presence is a live observation; start conservatively until the scanner reports Near.
        state.presence = PresenceState::Away;
        state.ble_rssi = -85;
        let mut coordinator = Self {
            state,
            config: ConfigState::from_persisted(config),
            pet,
            focus: FocusSession::default(),
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
        let policy = match (
            self.state.orientation,
            self.config.active_target(),
            self.state.presence,
        ) {
            (DeviceOrientation::FaceDown, _, _) => BacklightPolicy::Off,
            (_, Some(_), PresenceState::Away) => BacklightPolicy::Dimmed,
            _ => BacklightPolicy::Normal,
        };
        self.state.backlight_pct = policy.percent(self.config.config().brightness_normal);
        if self.state.backlight_pct != old {
            self.dirty = true;
        }
    }

    /// Handle incoming gesture event with state guards.
    pub fn handle_gesture(&mut self, gesture: GestureEvent) {
        self.state.last_gesture = Some(gesture);
        self.dirty = true;

        match gesture {
            GestureEvent::FaceDownEnter => {
                let now = Instant::now();
                self.state.orientation = DeviceOrientation::FaceDown;
                self.pet.handle(PetEvent::FaceDown, now);
                self.focus.apply(FocusEvent::FaceDown, now);
                self.sync_focus_state();
                self.update_backlight();
            }
            GestureEvent::FaceDownExit => {
                let now = Instant::now();
                self.state.orientation = DeviceOrientation::FaceUp;
                self.pet.handle(PetEvent::FaceUp, now);
                self.focus.apply(FocusEvent::FaceUp, now);
                self.sync_focus_state();
                self.update_backlight();
            }
            // If device is face down, guard against accidental mode changes.
            _ if self.state.is_facedown() => {}

            GestureEvent::SingleTap => {
                let now = Instant::now();
                self.pet.handle(PetEvent::Tap, now);
                self.focus.toggle(now);
                self.sync_focus_state();
                println!("Gesture SingleTap -> Toggle Play/Pause");
                if HID_ACTION_CHANNEL.try_send(HidAction::PlayPause).is_err() {
                    println!("WARN: HID_ACTION_CHANNEL full; dropping media action");
                }
            }
            GestureEvent::DoubleTap => {
                let now = Instant::now();
                self.pet.handle(PetEvent::DoubleTap, now);
                println!("Gesture DoubleTap -> Next Track");
                let _ = HID_ACTION_CHANNEL.try_send(HidAction::NextTrack);
                self.dirty = true;
            }
            GestureEvent::TiltLeft => {
                let now = Instant::now();
                self.pet.handle(PetEvent::Tap, now);
                println!("Gesture TiltLeft -> Previous Track");
                let _ = HID_ACTION_CHANNEL.try_send(HidAction::PreviousTrack);
                self.dirty = true;
            }
            GestureEvent::TiltRight => {
                let now = Instant::now();
                self.pet.handle(PetEvent::DoubleTap, now);
                println!("Gesture TiltRight -> Next Track");
                let _ = HID_ACTION_CHANNEL.try_send(HidAction::NextTrack);
                self.dirty = true;
            }
            GestureEvent::HeadingUpdate(hdg) => {
                self.state.heading_deg = hdg as f32;
            }
        }
    }

    /// Apply a binding only after the radio has accepted the persisted value.
    pub fn apply_target_binding(&mut self, command: BleTargetCommand) {
        self.config.set_active_target(command.into());
        self.dirty = true;
    }

    /// Build a local blackbox snapshot for a future local export command.
    pub fn blackbox_snapshot(&self) -> BlackboxSnapshot {
        BlackboxSnapshot {
            timestamp_sec: crate::board::datetime_to_epoch(&self.state.datetime),
            v_rail_mv: self.state.v_rail_mv,
            wifi_congestion: [0; 13],
            trigger_reason: TriggerReason::ManualDoubleTap,
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

    /// Accept a newer music scene without making music connectivity a runtime dependency.
    pub fn handle_music_scene(&mut self, scene: MusicScene) {
        let now = Instant::now();
        if self.state.music.mood == MusicMood::Unknown || scene.is_newer_than(&self.state.music) {
            self.state.music = scene;
            self.pet.handle(PetEvent::MusicScene(scene), now);
            self.dirty = true;
        } else if !self.state.music.is_active(now) {
            self.state.music = MusicScene::inactive(now);
            self.pet.handle(PetEvent::MusicScene(self.state.music), now);
            self.dirty = true;
        }
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
        let button_changed = self.state.boot_press_state != status.boot_press_state;
        if v_diff >= 20 || self.state.charge_state != status.charge_state || button_changed {
            if button_changed && status.boot_press_state == ButtonPressState::Pressed {
                println!("BOOT button pressed -> Toggle ViewMode");
                self.state.mode = self.state.mode.next();
                self.update_backlight();
            }
            self.state.v_rail_mv = status.v_rail_mv;
            self.state.charge_state = status.charge_state;
            self.state.boot_press_state = status.boot_press_state;
            self.dirty = true;
        }
    }

    /// Handle IMU sample update via strongly-typed ImuAttitudeSnapshot (only marks dirty if in SelfTest view).
    pub fn handle_imu(&mut self, snapshot: ImuAttitudeSnapshot) {
        self.pet.handle(
            PetEvent::Attitude {
                ax_mg: snapshot.sample.ax_mg,
                ay_mg: snapshot.sample.ay_mg,
            },
            Instant::now(),
        );
        self.state.imu_sample = snapshot.sample;
        self.state.heading_deg = snapshot.heading_deg;
        self.state.imu_status = snapshot.imu_status;
        if self.state.mode == ViewMode::SelfTest || self.state.mode == ViewMode::Pet {
            self.dirty = true;
        }
    }

    fn sync_focus_state(&mut self) {
        let phase = self.focus.phase();
        if self.state.focus_phase != phase {
            self.state.focus_phase = phase;
            self.pet.handle(PetEvent::FocusPhase(phase), Instant::now());
            self.dirty = true;
        }
    }

    /// Periodic local state tick; never depends on USB or a host heartbeat.
    pub fn tick(&mut self, now: Instant) {
        // 1. Advance local focus rhythm independently of connectivity.
        let previous_focus = self.focus.phase();
        let focus_phase = self.focus.tick(now);
        if focus_phase != previous_focus {
            self.sync_focus_state();
        }

        // 2. Expire music input without making the radio a source of truth.
        if !self.state.music.is_active(now) && self.state.music.mood != MusicMood::Unknown {
            self.state.music = MusicScene::inactive(now);
            self.pet.handle(PetEvent::MusicScene(self.state.music), now);
            self.dirty = true;
        }

        // 3. Update Pet Behavior Engine
        let new_pet = self.pet.tick(now);
        if self.state.pet != new_pet {
            self.state.pet = new_pet;
            if self.state.mode == ViewMode::Pet {
                self.dirty = true;
            }
        }
    }
}
