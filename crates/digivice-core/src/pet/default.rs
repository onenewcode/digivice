//! Autonomous Digimon Pet Behavior Engine.
//!
//! Manages Digimon actions, 3-phase action transitions (Enter -> Loop -> Exit),
//! music rhythm synchronization, Zero-FPU IMU attitude balance, and circadian metabolism.

use super::nurture::NurtureTimers;
use super::sleep::SleepPolicy;
use super::species::{
    ActionPhase, AttentionState, FoodType, HealthStatus, LookDirection, MusicDanceStyle, PetAction,
    PetSpecies, QueuedAction, SleepState, StepLeg,
};
use super::transition::{InertialTransform, SPRING_SQUASH_LUT, SquashStretch};
use super::{PetAnimationState, PetBehavior, PetEvent, PetFrame, PetMood, PupilShape};
use crate::domain::{
    CalendarDay, ChargeState, DeviceOrientation, MusicMood, MusicScene, MusicSection, PetArchive,
    PetArchiveData,
};
use embassy_time::{Duration, Instant};
use heapless::Deque;
use log::info;

/// 16-step natural blink easing LUT (0 = closed, 100 = open)
pub const BLINK_LUT: [u8; 16] = [
    100, 75, 40, 10, 0, 0, 15, 50, 85, 105, 102, 100, 100, 100, 100, 100,
];

/// Cumulative swing reversals required to complete a Pendulum Training set and spark evolution.
pub const PENDULUM_SHAKE_EVOLUTION_QUOTA: u8 = 10;

pub struct DefaultPet {
    animation: PetAnimationState,
    inertial_transform: InertialTransform,
    blink_tick: u16,
    blink_frame: Option<usize>,
    squash_step: Option<usize>,

    // Three-phase action lifecycle management
    action_phase: ActionPhase,
    action_tick: u16,
    action_duration_ticks: u16,
    action_queue: Deque<QueuedAction, 4>,

    // Sensor & reaction timers
    reaction_timer: Option<Instant>,
    shake_count: u8,
    shake_timer: Option<Instant>,

    // Music scene tracking
    last_music_scene: Option<MusicScene>,

    nurture_timers: NurtureTimers,

    // Autonomous wandering state (Tamagotchi waddle walk & screen peek)
    wander_target_x: i8,
    wander_pause_ticks: u16,
    wander_step_phase: u8,
    wander_peek_timer: u16,

    // Canonical Digivice nurture timers
    last_calendar_day: Option<CalendarDay>,
    sleep_policy: SleepPolicy,
}

impl Default for DefaultPet {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultPet {
    pub fn new() -> Self {
        Self::with_stats(4, 4)
    }

    /// Restore durable care state; never carry a previous boot's Instant or live sleep sources.
    pub fn from_archive(archive: PetArchive, now: Instant) -> Self {
        let data = archive.data();
        let mut pet = Self::with_full_stats(
            data.hunger,
            data.strength,
            PetSpecies::from_stage(data.stage),
            data.weight_g,
            data.care_mistakes,
            data.training_count,
        );
        pet.animation.age_days = data.age_days;
        pet.animation.health = data.health;
        pet.animation.poop_count = data.poop_count;
        pet.animation.energy = data.energy;
        pet.animation.affinity = data.affinity;
        pet.last_calendar_day = data.calendar_day;
        pet.nurture_timers = NurtureTimers::restore(data.progress, now);
        pet.nurture_timers.advance_to(now, &mut pet.animation);
        if pet.animation.is_sick() {
            pet.animation.mood = PetMood::Dizzy;
            pet.animation.pupil_shape = PupilShape::Dizzy;
        }
        pet
    }

    /// Initialize Koromon with persistent hunger and strength stats.
    pub fn with_stats(hunger: u8, strength: u8) -> Self {
        Self::with_stats_and_species(hunger, strength, PetSpecies::Koromon)
    }

    /// Initialize Digimon with full canonical nurture stats.
    pub fn with_full_stats(
        hunger: u8,
        strength: u8,
        species: PetSpecies,
        weight_g: u8,
        care_mistakes: u8,
        training_count: u16,
    ) -> Self {
        let actual_weight = if weight_g == 0 {
            species.base_weight_g()
        } else {
            weight_g
        };
        Self {
            animation: PetAnimationState {
                species,
                action: PetAction::IdleBreathe,
                action_phase: ActionPhase::Loop,
                squash: SquashStretch::default(),
                wander_x: 0,
                hunger: hunger.min(4),
                strength: strength.min(4),
                poop_count: 0,
                sleep_state: SleepState::Awake,
                charge_state: ChargeState::StandbyOrDone,
                music_beat_pulse: 0,
                mood: PetMood::Normal,
                pupil_shape: PupilShape::Round,
                gaze_x: 0,
                gaze_y: 0,
                tilt_deg: 0,
                blink_pct: 100,
                anim_tick: 0,
                energy: 100,
                affinity: 0,
                training_shakes: 0,
                weight_g: actual_weight,
                health: HealthStatus::Healthy,
                attention: AttentionState::Idle,
                care_mistakes,
                training_count,
                age_days: 1,
            },
            inertial_transform: InertialTransform::new(),
            blink_tick: 0,
            blink_frame: None,
            squash_step: None,

            action_phase: ActionPhase::Loop,
            action_tick: 0,
            action_duration_ticks: 30, // Default loop duration ~1 sec
            action_queue: Deque::new(),

            reaction_timer: None,
            shake_count: 0,
            shake_timer: None,

            last_music_scene: None,
            nurture_timers: NurtureTimers::default(),

            wander_target_x: 0,
            wander_pause_ticks: 90,
            wander_step_phase: 0,
            wander_peek_timer: 0,

            last_calendar_day: None,
            sleep_policy: SleepPolicy::default(),
        }
    }

    /// Initialize Digimon with persistent hunger, strength stats, and species evolution stage.
    pub fn with_stats_and_species(hunger: u8, strength: u8, species: PetSpecies) -> Self {
        Self::with_full_stats(hunger, strength, species, species.base_weight_g(), 0, 0)
    }

    #[inline]
    pub fn hunger(&self) -> u8 {
        self.animation.hunger
    }

    #[inline]
    pub fn strength(&self) -> u8 {
        self.animation.strength
    }

    #[inline]
    pub fn weight_g(&self) -> u8 {
        self.animation.weight_g
    }

    #[inline]
    pub fn is_sick(&self) -> bool {
        self.animation.is_sick()
    }

    #[inline]
    pub fn care_mistakes(&self) -> u8 {
        self.animation.care_mistakes
    }

    #[inline]
    pub fn training_count(&self) -> u16 {
        self.animation.training_count
    }

    #[inline]
    pub fn species(&self) -> PetSpecies {
        self.animation.species
    }

    #[inline]
    pub fn set_species(&mut self, species: PetSpecies) {
        self.animation.species = species;
    }

    /// Trigger the evolution sequence towards the next species in the line.
    pub fn trigger_evolution(&mut self) {
        let target = self.animation.species.next_evolution();
        self.animation.action = PetAction::Evolving {
            progress_pct: 0,
            target_species: target,
        };
        self.action_phase = ActionPhase::Loop;
        self.action_tick = 0;
        self.action_duration_ticks = 40; // ~1.2s evolution sequence
        self.animation.mood = PetMood::Focused;
        self.animation.pupil_shape = PupilShape::Star;
        self.squash_step = Some(0);
    }

    /// Process USB battery rail charge status transitions.
    pub fn on_charge_state(&mut self, state: ChargeState) {
        let was_charging = self.animation.charge_state.is_charging();
        self.animation.charge_state = state;
        if state.is_charging() && !self.animation.sleep_state.is_sleeping() {
            self.animation.mood = PetMood::Happy;
            self.animation.pupil_shape = PupilShape::Star;
        } else if was_charging && !state.is_charging() && self.animation.mood == PetMood::Happy {
            self.animation.mood = PetMood::Normal;
            self.animation.pupil_shape = PupilShape::Round;
        }
    }

    /// Process RTC circadian rhythm (Koromon sleeps at night between 22:00 and 07:00).
    pub fn on_circadian_hour(&mut self, hour: u8, now: Instant) {
        self.sleep_policy.set_hour(hour);
        self.reconcile_sleep(now);
    }

    fn on_calendar_day(&mut self, day: CalendarDay) {
        if let Some(previous) = self.last_calendar_day {
            let Some(days) = day.checked_days_since(previous) else {
                return; // Clock rollback cannot repeat already-counted days.
            };
            self.animation.age_days = self
                .animation
                .age_days
                .saturating_add(days.min(u32::from(u16::MAX)) as u16);
        }
        self.last_calendar_day = Some(day);
    }

    /// Check if current action is uninterruptible (e.g. eating, attacking, cleaning, or evolving sequence)
    #[inline]
    fn is_current_uninterruptible(&self) -> bool {
        self.animation.action.is_uninterruptible()
    }

    fn enqueue_action(&mut self, queued: QueuedAction) {
        if self.action_queue.len() < self.action_queue.capacity() {
            let _ = self.action_queue.push_back(queued);
        } else if let Some((min_idx, min_item)) = self
            .action_queue
            .iter()
            .enumerate()
            .min_by_key(|(_, item)| item.priority)
            && queued.priority > min_item.priority
        {
            let mut temp: Deque<QueuedAction, 4> = Deque::new();
            for (i, item) in self.action_queue.iter().enumerate() {
                if i != min_idx {
                    let _ = temp.push_back(*item);
                }
            }
            let _ = temp.push_back(queued);
            self.action_queue = temp;
        }
    }

    fn start_action(&mut self, action: PetAction, duration_ticks: u16) {
        self.animation.action = action;
        self.action_phase = ActionPhase::Enter;
        self.action_tick = 0;
        self.action_duration_ticks = duration_ticks;
        self.squash_step = Some(0); // Trigger elastic anticipation
    }

    fn start_idle(&mut self) {
        self.animation.action = PetAction::IdleBreathe;
        self.action_phase = ActionPhase::Loop;
        self.action_tick = 0;
        self.action_duration_ticks = 30;
    }

    /// Set a new action with smooth 3-phase transition.
    /// If currently in an uninterruptible action or active action with >= priority,
    /// queues the action in the bounded queue.
    pub fn trigger_action(&mut self, action: PetAction, duration_ticks: u16) {
        if self.animation.sleep_state.is_sleeping() {
            return;
        }

        let queued = QueuedAction::new(action, duration_ticks);

        if self.is_current_uninterruptible() {
            self.enqueue_action(queued);
            return;
        }

        if matches!(self.animation.action, PetAction::IdleBreathe) {
            self.start_action(action, duration_ticks);
            return;
        }

        // Interruptible active action:
        // Higher or equal priority winds down current action via graceful Exit
        if queued.priority >= self.animation.action.default_priority() {
            self.enqueue_action(queued);
            if self.action_phase != ActionPhase::Exit {
                self.action_phase = ActionPhase::Exit;
                self.action_tick = 0;
            }
        } else {
            self.enqueue_action(queued);
        }
    }

    /// Cure pet sickness via affectionate care / first aid
    pub fn on_heal(&mut self, now: Instant) {
        if self.animation.sleep_state.is_sleeping() {
            return;
        }

        self.animation.health = HealthStatus::Healthy;
        self.nurture_timers.reset_starvation();
        self.nurture_timers.reset_uncleaned();
        self.animation.mood = PetMood::Happy;
        self.animation.pupil_shape = PupilShape::Heart;
        self.animation.affinity = self.animation.affinity.saturating_add(15);
        self.reaction_timer = Some(now);

        info!(
            "Healed sickness for {}! Health restored.",
            self.animation.species.name()
        );
        self.trigger_action(PetAction::HealCure { tick: 0 }, 24);
    }

    /// Flush and clean all accumulated poop from desk with water wave
    pub fn on_flush(&mut self, now: Instant) {
        if self.animation.sleep_state.is_sleeping() {
            return;
        }

        if self.animation.poop_count == 0 {
            return;
        }

        self.animation.poop_count = 0;
        self.nurture_timers.reset_uncleaned();
        self.animation.mood = PetMood::Happy;
        self.animation.pupil_shape = PupilShape::Heart;
        self.animation.affinity = self.animation.affinity.saturating_add(10);
        self.reaction_timer = Some(now);

        info!(
            "Toilet Flush: Desk cleaned for {}!",
            self.animation.species.name()
        );
        self.trigger_action(PetAction::FlushClean { tick: 0 }, 20);
    }

    /// Handle single tap (patting / affectionate greeting or curing sickness)
    pub fn on_tap(&mut self, now: Instant) {
        let was_sleeping = self.animation.sleep_state.is_sleeping();
        self.sleep_policy.request_temporary_wake(now);
        self.reconcile_sleep(now);
        if was_sleeping || self.animation.sleep_state.is_sleeping() {
            return;
        }

        if self.animation.is_sick() {
            self.on_heal(now);
            return;
        }

        self.animation.mood = PetMood::Happy;
        self.animation.pupil_shape = PupilShape::Heart;
        self.animation.affinity = self.animation.affinity.saturating_add(5);
        self.reaction_timer = Some(now);

        self.trigger_action(PetAction::HappyHop, 24);
    }

    /// Trigger the Digimon's signature special attack.
    pub fn on_attack(&mut self, now: Instant) {
        if self.animation.sleep_state.is_sleeping() {
            return;
        }

        self.animation.mood = PetMood::Focused;
        self.animation.pupil_shape = PupilShape::CatSlit;
        self.animation.affinity = self.animation.affinity.saturating_add(15);
        self.reaction_timer = Some(now);

        info!(
            "Special Attack unleashed by {}!",
            self.animation.species.name()
        );
        self.trigger_action(PetAction::AttackSpecial { tick: 0 }, 36);
    }

    /// Handle double tap:
    /// - If sick: Heal sickness!
    /// - If hungry (hunger < 4): Feed Roast Meat (🍖), Hunger +1, Weight +1g, Energy +20.
    /// - If hunger == 4 && strength < 4: Feed Super Vitamin Protein Pill (💊), Strength +1, Weight +2g, Energy +30.
    /// - If both full (4/4): Unleash Signature Special Attack!
    pub fn on_double_tap(&mut self, now: Instant) {
        let was_sleeping = self.animation.sleep_state.is_sleeping();
        self.sleep_policy.request_temporary_wake(now);
        self.reconcile_sleep(now);
        if was_sleeping || self.animation.sleep_state.is_sleeping() {
            return;
        }

        if self.animation.is_sick() {
            self.on_heal(now);
            return;
        }

        if self.animation.hunger < 4 {
            self.animation.mood = PetMood::Happy;
            self.animation.pupil_shape = PupilShape::Star;
            self.animation.hunger = self.animation.hunger.saturating_add(1).min(4);
            self.animation.weight_g = self.animation.weight_g.saturating_add(1);
            self.animation.energy = self.animation.energy.saturating_add(20).min(100);
            self.animation.affinity = self.animation.affinity.saturating_add(10);
            self.reaction_timer = Some(now);

            info!(
                "Feeding Roast Meat to {}! Hunger: {}/4, Weight: {}g",
                self.animation.species.name(),
                self.animation.hunger,
                self.animation.weight_g
            );
            self.trigger_action(PetAction::NoticeFood(FoodType::Meat), 10);
        } else if self.animation.strength < 4 {
            self.animation.mood = PetMood::Focused;
            self.animation.pupil_shape = PupilShape::Star;
            self.animation.strength = self.animation.strength.saturating_add(1).min(4);
            self.animation.weight_g = self.animation.weight_g.saturating_add(2);
            self.animation.energy = self.animation.energy.saturating_add(30).min(100);
            self.animation.affinity = self.animation.affinity.saturating_add(15);
            self.reaction_timer = Some(now);

            info!(
                "Feeding Super Vitamin Pill to {}! Power: {}/4, Weight: {}g",
                self.animation.species.name(),
                self.animation.strength,
                self.animation.weight_g
            );
            self.trigger_action(PetAction::NoticeFood(FoodType::Pill), 10);
        } else {
            self.on_attack(now);
        }
    }

    /// Handle shake / Pendulum Shake training
    pub fn on_shake(&mut self, now: Instant) {
        if self.animation.sleep_state.is_sleeping() || self.animation.is_sick() {
            return;
        }

        if let Some(prev) = self.shake_timer
            && now.duration_since(prev) > Duration::from_secs(3)
        {
            self.shake_count = 0;
        }
        self.shake_timer = Some(now);
        self.shake_count = self.shake_count.saturating_add(1);
        self.animation.training_shakes = self.shake_count.min(PENDULUM_SHAKE_EVOLUTION_QUOTA);
        self.reaction_timer = Some(now);

        if self.shake_count >= PENDULUM_SHAKE_EVOLUTION_QUOTA {
            self.shake_count = 0;
            self.animation.training_shakes = 0;
            self.animation.training_count = self.animation.training_count.saturating_add(1);
            self.animation.mood = PetMood::Happy;
            self.animation.strength = 4;
            self.animation.energy = 100;
            // Training burns 1g weight, but cannot drop below species base weight
            let base_wt = self.animation.species.base_weight_g();
            self.animation.weight_g = self.animation.weight_g.saturating_sub(1).max(base_wt);

            info!(
                "Pendulum Training Set #{} complete! Strength: 4/4, Weight: {}g",
                self.animation.training_count, self.animation.weight_g
            );

            if self.animation.species.has_next_evolution() {
                info!(
                    "Pendulum Shake: Evolution triggered! {} -> {}",
                    self.animation.species.name(),
                    self.animation.species.next_evolution().name()
                );
                self.trigger_evolution();
            } else {
                // Ultra stage (Omegamon): unleash Supreme Cannon victory blast!
                self.trigger_action(PetAction::AttackSpecial { tick: 0 }, 36);
            }
        } else {
            let charge_pct = ((self.shake_count as u16 * 100)
                / PENDULUM_SHAKE_EVOLUTION_QUOTA as u16)
                .min(100) as u8;
            info!(
                "Pendulum Shake: swing #{}/{} (charge {}%)",
                self.shake_count, PENDULUM_SHAKE_EVOLUTION_QUOTA, charge_pct
            );
            self.trigger_action(PetAction::ShakeChargeSprint { charge_pct }, 16);
        }
    }

    fn reconcile_sleep(&mut self, now: Instant) {
        let desired = self.sleep_policy.state(now);
        if desired != self.animation.sleep_state {
            match desired {
                SleepState::Asleep => self.enter_sleep(),
                SleepState::Awake => self.leave_sleep(),
            }
        }
    }

    fn enter_sleep(&mut self) {
        self.animation.sleep_state = SleepState::Asleep;
        self.animation.mood = PetMood::Sleeping;
        self.animation.pupil_shape = PupilShape::Round;
        self.animation.action = PetAction::SleepingZzz { bubble_pct: 0 };
        self.animation.music_beat_pulse = 0;
        self.action_phase = ActionPhase::Loop;
        self.action_tick = 0;
        self.blink_frame = None;
        self.reaction_timer = None;
        self.squash_step = None;
        self.action_queue.clear();
        self.inertial_transform.reset(0, 0, 0);
        self.animation.gaze_x = 0;
        self.animation.gaze_y = 0;
        self.animation.tilt_deg = 0;
    }

    fn leave_sleep(&mut self) {
        self.animation.sleep_state = SleepState::Awake;
        self.animation.mood = PetMood::Normal;
        self.animation.pupil_shape = PupilShape::Round;
        self.animation.action = PetAction::StretchAndYawn;
        self.action_phase = ActionPhase::Enter;
        self.action_tick = 0;
        self.action_duration_ticks = 20;
        self.blink_frame = Some(6);
        self.squash_step = Some(0);
    }

    /// Update gaze and tilt orientation from filtered gravity vector with Zero-FPU EMA smoothing.
    pub fn on_imu_attitude(&mut self, gx_filt_mg: i32, gy_filt_mg: i32, gz_filt_mg: i32) {
        if self.animation.sleep_state.is_sleeping() {
            self.animation.gaze_x = 0;
            self.animation.gaze_y = 0;
            self.animation.tilt_deg = 0;
            return;
        }

        // Downward gravity component in the Y-Z plane from filtered gravity
        let down_sq =
            (gy_filt_mg as i64 * gy_filt_mg as i64 + gz_filt_mg as i64 * gz_filt_mg as i64) as u64;
        let g_down = (down_sq.isqrt() as f32).max(100.0);

        // Lateral tilt angle: purely governed by gx_filt_mg against g_down.
        // Invariant: Lifting up ("抬起", changing pitch / gy & gz) leaves gx ≈ 0,
        // so tilt angle remains strictly 0 (no rotation on lift: pitch-invariant).
        // Tilting towards either side (gx < 0 for left, gx > 0 for right)
        // tilts directly towards that side without high-frequency translation jitter.
        let (target_x, target_tilt) = if gx_filt_mg.abs() <= 60 {
            // Deadzone for slight vibrations / flat holding
            (0i16, 0i16)
        } else {
            let tilt_angle = crate::gesture::fast_atan2_deg(gx_filt_mg as f32, g_down) as i16;
            let tx = (((gx_filt_mg * 20) / 1000).clamp(-20, 20)) as i16;
            (tx, tilt_angle.clamp(-90, 90))
        };

        self.inertial_transform.update(target_x, 0, target_tilt);

        self.animation.gaze_x = self.inertial_transform.current_x as i8;
        self.animation.gaze_y = 0;
        self.animation.tilt_deg = self.inertial_transform.current_tilt_deg;

        // Dynamic tilt posture tracking
        if self.animation.tilt_deg.abs() > 8 {
            if matches!(self.animation.action, PetAction::IdleBreathe) {
                self.animation.action = PetAction::TiltBalance {
                    angle_deg: self.animation.tilt_deg,
                };
            } else if let PetAction::TiltBalance { angle_deg } = &mut self.animation.action {
                *angle_deg = self.animation.tilt_deg;
            }
        } else if self.animation.tilt_deg.abs() <= 4
            && matches!(self.animation.action, PetAction::TiltBalance { .. })
        {
            self.animation.action = PetAction::IdleBreathe;
        }
    }

    /// Process Bluetooth GATT MusicScene telemetry.
    pub fn on_music_scene(&mut self, scene: MusicScene, now: Instant) {
        self.last_music_scene = Some(scene);

        if !scene.is_active(now) {
            self.animation.music_beat_pulse = 0;
            if matches!(self.animation.action, PetAction::MusicDance { .. }) {
                self.start_idle();
            }
            return;
        }

        let current_phase = scene.current_beat_phase(now);
        self.animation.music_beat_pulse = current_phase;

        if self.animation.sleep_state.is_sleeping() || self.reaction_timer.is_some() {
            return;
        }

        if self.animation.hunger == 0 || self.animation.poop_count >= 3 {
            self.animation.action = PetAction::DrowsyNod;
            self.action_phase = ActionPhase::Loop;
            self.action_tick = 0;
            return;
        }

        let style = if scene.section == MusicSection::Chorus {
            MusicDanceStyle::ChorusFever
        } else if scene.energy > 70 || scene.mood == MusicMood::Excited {
            MusicDanceStyle::RockHeadbang
        } else if scene.energy >= 35 || scene.mood == MusicMood::Happy {
            MusicDanceStyle::PopBounce
        } else {
            MusicDanceStyle::ChillSway
        };

        if !self.is_current_uninterruptible() {
            let was_dancing = matches!(self.animation.action, PetAction::MusicDance { .. });
            self.animation.action = PetAction::MusicDance {
                style,
                beat_tick: current_phase,
            };
            if !was_dancing {
                self.action_phase = ActionPhase::Enter;
                self.action_tick = 0;
                self.action_duration_ticks = 30;
                self.squash_step = Some(0);
            }
        }
    }

    fn advance_nurture(&mut self, now: Instant) {
        let old_health = self.animation.health;
        let old_mistakes = self.animation.care_mistakes;
        if !self.animation.sleep_state.is_sleeping()
            && let Some(deadline) = self.sleep_policy.temporary_wake_deadline()
            && now >= deadline
        {
            self.nurture_timers
                .advance_to(deadline, &mut self.animation);
            self.reconcile_sleep(now);
        }
        self.nurture_timers.advance_to(now, &mut self.animation);
        if old_health != self.animation.health {
            info!("Pet fell sick due to neglect/mess! Sickness triggered.");
        }
        if old_mistakes != self.animation.care_mistakes {
            info!(
                "Unattended call: Care Mistakes total: {}",
                self.animation.care_mistakes
            );
        }
    }

    #[inline(never)]
    fn update_reaction_and_music_timers(&mut self, now: Instant) {
        // Clear temporary user reaction timers (after 3 seconds)
        if self
            .reaction_timer
            .is_some_and(|t| now.duration_since(t) >= Duration::from_secs(3))
        {
            self.reaction_timer = None;
            self.animation.training_shakes = 0;
            if !self.animation.sleep_state.is_sleeping() {
                if self.animation.hunger == 0 || self.animation.poop_count >= 3 {
                    self.animation.mood = PetMood::Dizzy;
                    self.animation.pupil_shape = PupilShape::Dizzy;
                } else {
                    self.animation.mood = PetMood::Normal;
                    self.animation.pupil_shape = PupilShape::Round;
                }
            }
        }

        if self
            .shake_timer
            .is_some_and(|t| now.duration_since(t) >= Duration::from_secs(3))
        {
            self.shake_count = 0;
            self.animation.training_shakes = 0;
        }

        // Continuous beat phase extrapolation for Bluetooth music telemetry
        if let Some(scene) = self.last_music_scene {
            if scene.is_active(now) && !self.animation.sleep_state.is_sleeping() {
                let current_phase = scene.current_beat_phase(now);
                self.animation.music_beat_pulse = current_phase;
                if let PetAction::MusicDance { beat_tick, .. } = &mut self.animation.action {
                    *beat_tick = current_phase;
                }
            } else {
                self.animation.music_beat_pulse = 0;
                if matches!(self.animation.action, PetAction::MusicDance { .. }) {
                    self.start_idle();
                }
                if !scene.is_active(now) {
                    self.last_music_scene = None;
                }
            }
        }
    }

    #[inline(never)]
    fn advance_action_lifecycle(&mut self, now: Instant) -> Option<PetFrame> {
        self.action_tick = self.action_tick.saturating_add(1);
        match self.action_phase {
            ActionPhase::Enter => {
                if self.action_tick >= 2 {
                    self.action_phase = ActionPhase::Loop;
                    self.action_tick = 0;
                }
            }
            ActionPhase::Loop => {
                if let PetAction::EatingChew { chew_frame, .. } = &mut self.animation.action
                    && self.action_tick.is_multiple_of(4)
                {
                    *chew_frame ^= 1;
                }

                if let PetAction::AttackSpecial { tick } = &mut self.animation.action {
                    *tick = tick.saturating_add(1);
                }

                if let PetAction::FlushClean { tick } = &mut self.animation.action {
                    *tick = tick.saturating_add(1);
                }

                if let PetAction::HealCure { tick } = &mut self.animation.action {
                    *tick = tick.saturating_add(1);
                }

                // Evolution sequence progression: digital energy charging
                if let PetAction::Evolving {
                    progress_pct,
                    target_species,
                } = &mut self.animation.action
                {
                    *progress_pct = progress_pct.saturating_add(3);
                    if *progress_pct >= 100 {
                        let new_species = *target_species;
                        self.animation.species = new_species;
                        self.animation.weight_g =
                            self.animation.weight_g.max(new_species.base_weight_g());
                        self.animation.action = PetAction::HappyHop;
                        self.action_phase = ActionPhase::Enter;
                        self.action_tick = 0;
                        self.action_duration_ticks = 24;
                        self.animation.mood = PetMood::Happy;
                        self.animation.pupil_shape = PupilShape::Star;
                        self.squash_step = Some(0);
                        self.animation.action_phase = self.action_phase;
                        return Some(self.animation.into());
                    }
                }

                if self.action_tick >= self.action_duration_ticks {
                    self.action_phase = ActionPhase::Exit;
                    self.action_tick = 0;
                }
            }
            ActionPhase::Exit => {
                if self.action_tick >= 2 {
                    match self.animation.action {
                        PetAction::NoticeFood(food_type) => {
                            self.start_action(
                                PetAction::EatingChew {
                                    food_type,
                                    chew_frame: 0,
                                },
                                16,
                            );
                        }
                        PetAction::EatingChew { .. } => {
                            self.start_action(PetAction::SatisfiedGulp, 8);
                        }
                        PetAction::FlushClean { .. } | PetAction::HealCure { .. } => {
                            self.animation.mood = PetMood::Happy;
                            self.animation.pupil_shape = PupilShape::Heart;
                            self.start_action(PetAction::HappyHop, 24);
                        }
                        _ => {
                            if let Some(next) = self.action_queue.pop_front() {
                                self.start_action(next.action, next.duration_ticks);
                            } else if let Some(scene) = self.last_music_scene {
                                if scene.is_active(now) {
                                    self.on_music_scene(scene, now);
                                } else {
                                    self.start_idle();
                                }
                            } else {
                                self.start_idle();
                            }
                        }
                    }
                }
            }
        }

        self.animation.action_phase = self.action_phase;
        None
    }

    #[inline(never)]
    fn advance_wander_cycle(&mut self, now: Instant) {
        if self.reaction_timer.is_none()
            && self.action_queue.is_empty()
            && matches!(
                self.animation.action,
                PetAction::IdleBreathe | PetAction::PivotWalk { .. }
            )
            && self.last_music_scene.is_none_or(|s| !s.is_active(now))
            && !self.animation.sleep_state.is_sleeping()
            && self.animation.tilt_deg.abs() <= 8
        {
            self.wander_peek_timer = self.wander_peek_timer.wrapping_add(1);

            if self.animation.wander_x != self.wander_target_x {
                self.wander_step_phase = self.wander_step_phase.wrapping_add(1);
                let dir = if self.wander_target_x > self.animation.wander_x {
                    1
                } else {
                    -1
                };

                if self.wander_step_phase.is_multiple_of(3) {
                    self.animation.wander_x += dir;
                }

                let leg = if ((self.wander_step_phase / 6) & 1) == 0 {
                    StepLeg::Left
                } else {
                    StepLeg::Right
                };
                self.animation.action = PetAction::PivotWalk { step_leg: leg };
                self.action_phase = ActionPhase::Loop;
            } else if self.wander_pause_ticks > 0 {
                self.wander_pause_ticks -= 1;
            } else if self.wander_peek_timer >= 600 {
                self.wander_peek_timer = 0;
                if self.animation.wander_x != 0 {
                    self.wander_target_x = 0;
                } else {
                    self.trigger_action(PetAction::HappyHop, 24);
                    self.wander_pause_ticks = 80;
                }
            } else if self.animation.wander_x <= -16 {
                self.wander_target_x = 18;
                self.wander_pause_ticks = 70;
                self.trigger_action(PetAction::LookAround(LookDirection::Right), 24);
            } else if self.animation.wander_x >= 16 {
                self.wander_target_x = -18;
                self.wander_pause_ticks = 70;
                self.trigger_action(PetAction::ScratchEar, 24);
            } else {
                self.wander_target_x = if (self.animation.anim_tick & 1) == 0 {
                    -18
                } else {
                    18
                };
                self.wander_pause_ticks = 60;
            }
        }
    }

    /// Advance action state machine and render frame.
    fn tick_frame(&mut self, now: Instant) -> PetFrame {
        self.advance_nurture(now);
        self.reconcile_sleep(now);
        self.animation.anim_tick = self.animation.anim_tick.wrapping_add(1);

        self.update_reaction_and_music_timers(now);

        // 3. Elastic Squash & Stretch Spring Stepper
        if let Some(step) = self.squash_step {
            self.animation.squash = SquashStretch::from_spring_step(step);
            if step + 1 < SPRING_SQUASH_LUT.len() {
                self.squash_step = Some(step + 1);
            } else {
                self.squash_step = None;
            }
        }

        // 4. Natural Blinking Animation
        let blink_pct = if self.animation.sleep_state.is_sleeping() {
            0
        } else if let Some(idx) = self.blink_frame {
            let pct = BLINK_LUT.get(idx).copied().unwrap_or(100);
            if idx + 1 < BLINK_LUT.len() {
                self.blink_frame = Some(idx + 1);
            } else {
                self.blink_frame = None;
            }
            pct
        } else {
            self.blink_tick = self.blink_tick.saturating_add(1);
            if self.blink_tick >= 120 {
                self.blink_tick = 0;
                self.blink_frame = Some(0);
            }
            100
        };
        self.animation.blink_pct = blink_pct;

        // 5. Action Lifecycle Progression (Enter -> Loop -> Exit)
        if self.animation.sleep_state.is_sleeping() {
            // Keep Koromon sleeping, pulsing breathing snot bubble
            let bubble = (((self.animation.anim_tick % 60) * 100) / 60) as u8;
            self.animation.action = PetAction::SleepingZzz { bubble_pct: bubble };
            self.animation.action_phase = ActionPhase::Loop;
            return self.animation.into();
        }

        if let Some(evolving_frame) = self.advance_action_lifecycle(now) {
            return evolving_frame;
        }

        // 6. Autonomous Life Wandering Cycle (Tamagotchi Waddle Walk & Screen Peek)
        self.advance_wander_cycle(now);

        self.animation.into()
    }
}

impl PetBehavior for DefaultPet {
    fn handle(&mut self, event: PetEvent, now: Instant) {
        self.advance_nurture(now);
        match event {
            PetEvent::Tap => self.on_tap(now),
            PetEvent::DoubleTap => self.on_double_tap(now),
            PetEvent::Shake => self.on_shake(now),
            PetEvent::Attack => self.on_attack(now),
            PetEvent::Flush => self.on_flush(now),
            PetEvent::FaceDown => {
                self.sleep_policy
                    .set_orientation(DeviceOrientation::FaceDown);
                self.reconcile_sleep(now);
            }
            PetEvent::FaceUp => {
                self.sleep_policy.set_orientation(DeviceOrientation::FaceUp);
                self.reconcile_sleep(now);
            }
            PetEvent::MusicScene(scene) => self.on_music_scene(scene, now),
            PetEvent::Attitude {
                ax_mg,
                ay_mg,
                az_mg,
            } => self.on_imu_attitude(ax_mg, ay_mg, az_mg),
            PetEvent::ChargeState(state) => self.on_charge_state(state),
            PetEvent::CircadianHour(hour) => self.on_circadian_hour(hour, now),
            PetEvent::CircadianUnavailable => {
                self.sleep_policy.clear_hour();
                self.reconcile_sleep(now);
            }
            PetEvent::CalendarDay(day) => self.on_calendar_day(day),
        }
    }

    fn tick(&mut self, now: Instant) -> PetFrame {
        self.tick_frame(now)
    }

    fn archive(&self) -> Option<PetArchive> {
        Some(PetArchive::new_clamped(PetArchiveData {
            stage: self.animation.species.stage_index(),
            hunger: self.animation.hunger,
            strength: self.animation.strength,
            weight_g: self.animation.weight_g,
            care_mistakes: self.animation.care_mistakes,
            training_count: self.animation.training_count,
            age_days: self.animation.age_days,
            health: self.animation.health,
            poop_count: self.animation.poop_count,
            energy: self.animation.energy,
            affinity: self.animation.affinity,
            progress: self.nurture_timers.snapshot(),
            calendar_day: self.last_calendar_day,
        }))
    }
}
