//! Tap, Tilt, FaceDown, and Button Gesture Detection Engine

use super::GestureEvent;
use super::filter::Attitude;
use crate::board::ImuSample;
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

pub struct GestureDetector {
    prev_sample: ImuSample,
    tap_state: TapState,
    tilt_state: TiltState,
    facedown_state: FaceDownState,
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
        let now = Instant::now();
        Self {
            prev_sample: ImuSample::default(),
            tap_state: TapState::Idle,
            tilt_state: TiltState::Centered,
            facedown_state: FaceDownState::FaceUp,
            last_heading_emitted: 0,
            last_heading_time: now,
        }
    }

    /// Process a new IMU sample and return any detected gesture events (up to 2 per frame)
    pub fn process_imu(
        &mut self,
        sample: &ImuSample,
        attitude: &Attitude,
        now: Instant,
    ) -> Option<GestureEvent> {
        // 1. Jerk calculation for Tap detection
        let d_az = (sample.az_mg - self.prev_sample.az_mg).abs();
        let d_ax = (sample.ax_mg - self.prev_sample.ax_mg).abs();
        let d_ay = (sample.ay_mg - self.prev_sample.ay_mg).abs();
        let jerk = d_az + (d_ax + d_ay) / 2;
        self.prev_sample = *sample;

        // Lower threshold from 650 to 320 for responsive light table/chassis tap recognition
        let is_tap_peak = jerk > 320;

        // Tap State Machine
        let mut tap_event = None;
        match self.tap_state {
            TapState::Idle => {
                if is_tap_peak {
                    self.tap_state = TapState::FirstTapDetected { time: now };
                }
            }
            TapState::FirstTapDetected { time } => {
                let elapsed = now.duration_since(time);
                // Natural human double-tap interval: 60ms..=420ms
                if is_tap_peak
                    && elapsed >= Duration::from_millis(60)
                    && elapsed <= Duration::from_millis(420)
                {
                    tap_event = Some(GestureEvent::DoubleTap);
                    self.tap_state = TapState::Cooldown { time: now };
                } else if elapsed > Duration::from_millis(420) {
                    tap_event = Some(GestureEvent::SingleTap);
                    self.tap_state = TapState::Cooldown { time: now };
                }
            }
            TapState::Cooldown { time } => {
                if now.duration_since(time) > Duration::from_millis(180) {
                    self.tap_state = TapState::Idle;
                }
            }
        }

        if let Some(evt) = tap_event {
            return Some(evt);
        }

        // 2. FaceDown State Machine (Z gravity < -650mg, responsive 200ms entry / 100ms exit)
        match self.facedown_state {
            FaceDownState::FaceUp => {
                if attitude.gz_filt_mg < -650.0 {
                    self.facedown_state = FaceDownState::Entering { since: now };
                }
            }
            FaceDownState::Entering { since } => {
                if attitude.gz_filt_mg < -650.0 {
                    if now.duration_since(since) >= Duration::from_millis(200) {
                        self.facedown_state = FaceDownState::FaceDown;
                        return Some(GestureEvent::FaceDownEnter);
                    }
                } else {
                    self.facedown_state = FaceDownState::FaceUp;
                }
            }
            FaceDownState::FaceDown => {
                if attitude.gz_filt_mg > 400.0 {
                    self.facedown_state = FaceDownState::Exiting { since: now };
                }
            }
            FaceDownState::Exiting { since } => {
                if attitude.gz_filt_mg > 400.0 {
                    if now.duration_since(since) >= Duration::from_millis(100) {
                        self.facedown_state = FaceDownState::FaceUp;
                        return Some(GestureEvent::FaceDownExit);
                    }
                } else {
                    self.facedown_state = FaceDownState::FaceDown;
                }
            }
        }

        // 3. Tilt State Machine: Ergonomic 18 deg threshold, 60ms dwell, broader pitch tolerance
        if fabsf(attitude.pitch_deg) < 50.0 && attitude.gz_filt_mg > 400.0 {
            match self.tilt_state {
                TiltState::Centered => {
                    if attitude.roll_deg < -18.0 {
                        self.tilt_state = TiltState::TiltedLeft { since: now };
                    } else if attitude.roll_deg > 18.0 {
                        self.tilt_state = TiltState::TiltedRight { since: now };
                    }
                }
                TiltState::TiltedLeft { since } => {
                    if attitude.roll_deg < -18.0 {
                        if now.duration_since(since) >= Duration::from_millis(60) {
                            self.tilt_state = TiltState::LockedLeft;
                            return Some(GestureEvent::TiltLeft);
                        }
                    } else {
                        self.tilt_state = TiltState::Centered;
                    }
                }
                TiltState::TiltedRight { since } => {
                    if attitude.roll_deg > 18.0 {
                        if now.duration_since(since) >= Duration::from_millis(60) {
                            self.tilt_state = TiltState::LockedRight;
                            return Some(GestureEvent::TiltRight);
                        }
                    } else {
                        self.tilt_state = TiltState::Centered;
                    }
                }
                TiltState::LockedLeft => {
                    if attitude.roll_deg > -8.0 {
                        self.tilt_state = TiltState::Centered;
                    }
                }
                TiltState::LockedRight => {
                    if attitude.roll_deg < 8.0 {
                        self.tilt_state = TiltState::Centered;
                    }
                }
            }
        }

        // 4. Heading Update (rate limited to max 20Hz, threshold >= 1 degree)
        let current_heading = attitude.heading_deg as u16;
        if current_heading != self.last_heading_emitted
            && now.duration_since(self.last_heading_time) >= Duration::from_millis(50)
        {
            let diff = ((current_heading as i32) - (self.last_heading_emitted as i32)).abs();
            if (1..=350).contains(&diff) {
                self.last_heading_emitted = current_heading;
                self.last_heading_time = now;
                return Some(GestureEvent::HeadingUpdate(current_heading));
            }
        }

        None
    }
}
