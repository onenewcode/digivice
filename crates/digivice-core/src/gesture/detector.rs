//! Tap, Tilt, FaceDown, and Button Gesture Detection Engine

use super::GestureEvent;
use super::filter::Attitude;
use crate::domain::ImuSample;
use embassy_time::{Duration, Instant};
use libm::fabsf;

#[derive(Debug, Clone, Copy, PartialEq)]
enum TapState {
    Idle,
    FirstTapDetected { time: Instant },
    Cooldown { time: Instant },
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TiltState {
    Centered,
    TiltedLeft { since: Instant },
    TiltedRight { since: Instant },
    LockedLeft,
    LockedRight,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum FaceDownState {
    FaceUp,
    Entering { since: Instant },
    FaceDown,
    Exiting { since: Instant },
}

/// Principal spatial axis for shake detection. Eliminates primitive obsession.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShakeAxis {
    X,
    Y,
    Z,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShakeState {
    Idle,
    PositivePeak { axis: ShakeAxis, since: Instant },
    NegativePeak { axis: ShakeAxis, since: Instant },
}

const SHAKE_PEAK_THRESHOLD_MG: i32 = 260;
const SHAKE_REVERSAL_THRESHOLD_MG: i32 = 180;
const SHAKE_MIN_REVERSAL_MS: u64 = 40;
const SHAKE_MAX_REVERSAL_MS: u64 = 550;
const SHAKE_SUPPRESS_TAP_MS: u64 = 500;
const SHAKE_MOTION_SUPPRESS_TAP_MG: i32 = 160;

pub struct GestureDetector {
    prev_sample: ImuSample,
    tap_state: TapState,
    tilt_state: TiltState,
    facedown_state: FaceDownState,
    shake_state: ShakeState,
    base_ax_fp: i32,
    base_ay_fp: i32,
    base_az_fp: i32,
    baseline_init: bool,
    last_motion_time: Option<Instant>,
    last_heading_emitted: u16,
    last_heading_time: Instant,
}

impl Default for GestureDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureDetector {
    pub fn new() -> Self {
        Self::new_at(Instant::now())
    }

    /// Start a replay or a live detector at an explicit monotonic timestamp.
    pub fn new_at(now: Instant) -> Self {
        Self {
            prev_sample: ImuSample::default(),
            tap_state: TapState::Idle,
            tilt_state: TiltState::Centered,
            facedown_state: FaceDownState::FaceUp,
            shake_state: ShakeState::Idle,
            base_ax_fp: 0,
            base_ay_fp: 0,
            base_az_fp: 0,
            baseline_init: false,
            last_motion_time: None,
            last_heading_emitted: 0,
            last_heading_time: now,
        }
    }

    #[inline(never)]
    fn process_tap(&mut self, sample: &ImuSample, now: Instant) -> Option<GestureEvent> {
        let d_az = (sample.az_mg - self.prev_sample.az_mg).abs();
        let d_ax = (sample.ax_mg - self.prev_sample.ax_mg).abs();
        let d_ay = (sample.ay_mg - self.prev_sample.ay_mg).abs();
        let jerk = d_az + (d_ax + d_ay) / 2;
        self.prev_sample = *sample;

        let is_tap_peak = jerk > 320;
        match self.tap_state {
            TapState::Idle => {
                if is_tap_peak {
                    self.tap_state = TapState::FirstTapDetected { time: now };
                }
                None
            }
            TapState::FirstTapDetected { time } => {
                let elapsed = now.duration_since(time);
                if is_tap_peak
                    && elapsed >= Duration::from_millis(60)
                    && elapsed <= Duration::from_millis(420)
                {
                    self.tap_state = TapState::Cooldown { time: now };
                    Some(GestureEvent::DoubleTap)
                } else if elapsed > Duration::from_millis(420) {
                    self.tap_state = TapState::Cooldown { time: now };
                    Some(GestureEvent::SingleTap)
                } else {
                    None
                }
            }
            TapState::Cooldown { time } => {
                if now.duration_since(time) > Duration::from_millis(180) {
                    self.tap_state = TapState::Idle;
                }
                None
            }
        }
    }

    #[inline(never)]
    fn process_facedown(&mut self, gz_filt_mg: f32, now: Instant) -> Option<GestureEvent> {
        match self.facedown_state {
            FaceDownState::FaceUp => {
                if gz_filt_mg < -650.0 {
                    self.facedown_state = FaceDownState::Entering { since: now };
                }
                None
            }
            FaceDownState::Entering { since } => {
                if gz_filt_mg < -650.0 {
                    if now.duration_since(since) >= Duration::from_millis(200) {
                        self.facedown_state = FaceDownState::FaceDown;
                        Some(GestureEvent::FaceDownEnter)
                    } else {
                        None
                    }
                } else {
                    self.facedown_state = FaceDownState::FaceUp;
                    None
                }
            }
            FaceDownState::FaceDown => {
                if gz_filt_mg > 400.0 {
                    self.facedown_state = FaceDownState::Exiting { since: now };
                }
                None
            }
            FaceDownState::Exiting { since } => {
                if gz_filt_mg > 400.0 {
                    if now.duration_since(since) >= Duration::from_millis(100) {
                        self.facedown_state = FaceDownState::FaceUp;
                        Some(GestureEvent::FaceDownExit)
                    } else {
                        None
                    }
                } else {
                    self.facedown_state = FaceDownState::FaceDown;
                    None
                }
            }
        }
    }

    #[inline(never)]
    fn process_tilt(&mut self, attitude: &Attitude, now: Instant) -> Option<GestureEvent> {
        if fabsf(attitude.pitch_deg) >= 50.0 || attitude.gz_filt_mg <= 400.0 {
            return None;
        }

        match self.tilt_state {
            TiltState::Centered => {
                if attitude.roll_deg < -18.0 {
                    self.tilt_state = TiltState::TiltedLeft { since: now };
                } else if attitude.roll_deg > 18.0 {
                    self.tilt_state = TiltState::TiltedRight { since: now };
                }
                None
            }
            TiltState::TiltedLeft { since } => {
                if attitude.roll_deg < -18.0 {
                    if now.duration_since(since) >= Duration::from_millis(60) {
                        self.tilt_state = TiltState::LockedLeft;
                        Some(GestureEvent::TiltLeft)
                    } else {
                        None
                    }
                } else {
                    self.tilt_state = TiltState::Centered;
                    None
                }
            }
            TiltState::TiltedRight { since } => {
                if attitude.roll_deg > 18.0 {
                    if now.duration_since(since) >= Duration::from_millis(60) {
                        self.tilt_state = TiltState::LockedRight;
                        Some(GestureEvent::TiltRight)
                    } else {
                        None
                    }
                } else {
                    self.tilt_state = TiltState::Centered;
                    None
                }
            }
            TiltState::LockedLeft => {
                if attitude.roll_deg > -8.0 {
                    self.tilt_state = TiltState::Centered;
                }
                None
            }
            TiltState::LockedRight => {
                if attitude.roll_deg < 8.0 {
                    self.tilt_state = TiltState::Centered;
                }
                None
            }
        }
    }

    #[inline(never)]
    fn process_heading(&mut self, heading_deg: f32, now: Instant) -> Option<GestureEvent> {
        let current_heading = heading_deg as u16;
        if current_heading != self.last_heading_emitted
            && now.duration_since(self.last_heading_time) >= Duration::from_millis(50)
        {
            let diff = shortest_heading_delta(current_heading, self.last_heading_emitted);
            if (1..=180).contains(&diff) {
                self.last_heading_emitted = current_heading;
                self.last_heading_time = now;
                return Some(GestureEvent::HeadingUpdate(current_heading));
            }
        }
        None
    }

    #[inline(never)]
    fn process_shake(&mut self, sample: &ImuSample, now: Instant) -> Option<GestureEvent> {
        let ax_fp = sample.ax_mg << 8;
        let ay_fp = sample.ay_mg << 8;
        let az_fp = sample.az_mg << 8;

        if !self.baseline_init {
            self.base_ax_fp = ax_fp;
            self.base_ay_fp = ay_fp;
            self.base_az_fp = az_fp;
            self.baseline_init = true;
        } else {
            // First-order IIR baseline filter (alpha = 1/128) tracking slow static gravity
            // without attenuating the 2..5 Hz hand shake dynamics
            self.base_ax_fp += (ax_fp - self.base_ax_fp) / 128;
            self.base_ay_fp += (ay_fp - self.base_ay_fp) / 128;
            self.base_az_fp += (az_fp - self.base_az_fp) / 128;
        }

        let dyn_ax = (ax_fp - self.base_ax_fp) >> 8;
        let dyn_ay = (ay_fp - self.base_ay_fp) >> 8;
        let dyn_az = (az_fp - self.base_az_fp) >> 8;

        match self.shake_state {
            ShakeState::Idle => {
                let abs_x = dyn_ax.abs();
                let abs_y = dyn_ay.abs();
                let abs_z = dyn_az.abs();

                let (dominant_axis, val) = if abs_x >= abs_y && abs_x >= abs_z {
                    (ShakeAxis::X, dyn_ax)
                } else if abs_y >= abs_x && abs_y >= abs_z {
                    (ShakeAxis::Y, dyn_ay)
                } else {
                    (ShakeAxis::Z, dyn_az)
                };

                if val >= SHAKE_PEAK_THRESHOLD_MG {
                    self.shake_state = ShakeState::PositivePeak {
                        axis: dominant_axis,
                        since: now,
                    };
                } else if val <= -SHAKE_PEAK_THRESHOLD_MG {
                    self.shake_state = ShakeState::NegativePeak {
                        axis: dominant_axis,
                        since: now,
                    };
                }
                None
            }
            ShakeState::PositivePeak { axis, since } => {
                if now.duration_since(since) > Duration::from_millis(SHAKE_MAX_REVERSAL_MS) {
                    self.shake_state = ShakeState::Idle;
                    None
                } else {
                    let val = match axis {
                        ShakeAxis::X => dyn_ax,
                        ShakeAxis::Y => dyn_ay,
                        ShakeAxis::Z => dyn_az,
                    };
                    if let Some(new_axis) = opposite_axis(axis, val, dyn_ax, dyn_ay, dyn_az, false)
                    {
                        let elapsed = now.duration_since(since);
                        if elapsed >= Duration::from_millis(SHAKE_MIN_REVERSAL_MS) {
                            self.shake_state = ShakeState::NegativePeak {
                                axis: new_axis,
                                since: now,
                            };
                            self.last_motion_time = Some(now);
                            Some(GestureEvent::Shake)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
            }
            ShakeState::NegativePeak { axis, since } => {
                if now.duration_since(since) > Duration::from_millis(SHAKE_MAX_REVERSAL_MS) {
                    self.shake_state = ShakeState::Idle;
                    None
                } else {
                    let val = match axis {
                        ShakeAxis::X => dyn_ax,
                        ShakeAxis::Y => dyn_ay,
                        ShakeAxis::Z => dyn_az,
                    };
                    if let Some(new_axis) = opposite_axis(axis, val, dyn_ax, dyn_ay, dyn_az, true) {
                        let elapsed = now.duration_since(since);
                        if elapsed >= Duration::from_millis(SHAKE_MIN_REVERSAL_MS) {
                            self.shake_state = ShakeState::PositivePeak {
                                axis: new_axis,
                                since: now,
                            };
                            self.last_motion_time = Some(now);
                            Some(GestureEvent::Shake)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
            }
        }
    }

    /// Process one sample, prioritizing safety transitions over optional interactions.
    pub fn process_imu(
        &mut self,
        sample: &ImuSample,
        attitude: &Attitude,
        now: Instant,
    ) -> Option<GestureEvent> {
        if !self.baseline_init {
            self.prev_sample = *sample;
        }
        let safety_event = self.process_facedown(attitude.gz_filt_mg, now);
        let shake_event = self.process_shake(sample, now);

        let dyn_ax = ((sample.ax_mg << 8) - self.base_ax_fp) >> 8;
        let dyn_ay = ((sample.ay_mg << 8) - self.base_ay_fp) >> 8;
        let dyn_az = ((sample.az_mg << 8) - self.base_az_fp) >> 8;
        let motion_active = dyn_ax.abs() > SHAKE_MOTION_SUPPRESS_TAP_MG
            || dyn_ay.abs() > SHAKE_MOTION_SUPPRESS_TAP_MG
            || (self.shake_state != ShakeState::Idle
                && dyn_az.abs() > SHAKE_MOTION_SUPPRESS_TAP_MG);
        if motion_active {
            self.last_motion_time = Some(now);
        }

        let suppress_interactions = self.shake_state != ShakeState::Idle
            || motion_active
            || self.last_motion_time.is_some_and(|t| {
                now.duration_since(t) < Duration::from_millis(SHAKE_SUPPRESS_TAP_MS)
            });
        if safety_event.is_some() || suppress_interactions {
            self.tap_state = TapState::Idle;
            self.tilt_state = TiltState::Centered;
            // Suppressed samples still establish the next jerk baseline. Do not rearm
            // tap/tilt on this sample or defer a reversal shock into a later SingleTap.
            self.prev_sample = *sample;
            return safety_event.or(shake_event);
        }

        if let Some(evt) = self.process_tap(sample, now) {
            return Some(evt);
        }
        if let Some(evt) = self.process_tilt(attitude, now) {
            return Some(evt);
        }
        self.process_heading(attitude.heading_deg, now)
    }
}

fn shortest_heading_delta(current: u16, previous: u16) -> u16 {
    let forward = (i32::from(current) - i32::from(previous)).rem_euclid(360) as u16;
    forward.min(360 - forward)
}

/// Select the strongest opposite half-wave, retaining the current principal axis when possible.
fn opposite_axis(
    axis: ShakeAxis,
    current_value: i32,
    ax: i32,
    ay: i32,
    az: i32,
    positive: bool,
) -> Option<ShakeAxis> {
    let threshold = SHAKE_REVERSAL_THRESHOLD_MG;
    let current_reversed = if positive {
        current_value >= threshold
    } else {
        current_value <= -threshold
    };
    if current_reversed {
        return Some(axis);
    }

    let mut best: Option<(ShakeAxis, i32)> = None;
    for (candidate, value) in [(ShakeAxis::X, ax), (ShakeAxis::Y, ay), (ShakeAxis::Z, az)] {
        let reversed = if positive {
            value >= threshold
        } else {
            value <= -threshold
        };
        if reversed && best.is_none_or(|(_, previous)| value.abs() > previous.abs()) {
            best = Some((candidate, value));
        }
    }
    best.map(|(candidate, _)| candidate)
}
