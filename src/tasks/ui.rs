//! Display rendering and backlight PWM control task.

use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Instant, Timer};
use esp_println::println;

use crate::board::display::{Backlight, BandDisplay};
use crate::domain::ViewMode;
use crate::ipc::UI_STATE_SIGNAL;
use crate::ui::pet_view::{pet_dirty_y_bounds, render_pet};
use crate::ui::selftest_view::render_selftest;
use crate::ui::{COLOR_BG, UiState};

/// Task 4: UI Rendering Task (GC9A01 Display + Backlight PWM)
#[allow(
    clippy::large_stack_frames,
    reason = "UI rendering task maintains local frame snapshot and display target"
)]
#[embassy_executor::task]
pub async fn ui_render_task(
    display: &'static mut BandDisplay<'static>,
    backlight: &'static mut Backlight<'static>,
) -> ! {
    let mut ui_state = UiState::default();
    let mut breath_idx: usize = 0;
    let mut was_breathing = false;
    let mut was_facedown = false;
    let mut needs_render = true;
    let mut reported_first_render = false;
    let mut next_render_retry = Instant::now();

    let mut last_mode: Option<ViewMode> = None;
    let mut prev_dirty_y: (u16, u16) = (0, 239);

    const FRAME_INTERVAL: Duration = Duration::from_millis(30); // ~33.3 FPS
    let mut next_frame_deadline = Instant::now();

    // 1.2Hz Breathing pulse between 25% and 100% using precomputed integer LUT (Zero-FPU)
    // 28 samples * 30ms interval = 840ms period (~1.19Hz)
    const BREATHING_LUT: [u8; 28] = [
        62, 71, 79, 86, 92, 96, 99, 100, 99, 96, 92, 86, 79, 71, 62, 53, 45, 38, 32, 28, 25, 26,
        28, 32, 38, 45, 53, 58,
    ];

    loop {
        // Fetch latest state if updated
        if let Some(new_state) = UI_STATE_SIGNAL.try_take() {
            ui_state = new_state;
            needs_render = true;
        }

        let is_breathing = ui_state.focus_phase.is_active();
        if !was_breathing && is_breathing {
            breath_idx = 0;
        }
        was_breathing = is_breathing;

        let is_facedown = ui_state.is_facedown();
        let just_woke_up = was_facedown && !is_facedown;
        was_facedown = is_facedown;

        // Backlight PWM Management
        if is_facedown {
            backlight.set_brightness(0);
        } else if is_breathing {
            breath_idx = (breath_idx + 1) % BREATHING_LUT.len();
            backlight.set_brightness(BREATHING_LUT[breath_idx]);
        } else {
            backlight.set_brightness(ui_state.backlight_pct);
        }

        // Render Frame based on active ViewMode:
        // Skip heavy SPI rendering when device is face down (screen turned off onto desk)
        if needs_render && !is_facedown && Instant::now() >= next_render_retry {
            let mode = ui_state.mode;
            if !reported_first_render {
                println!(
                    "UI first render (mode={:?}, backlight={}%, face_down={})",
                    mode, ui_state.backlight_pct, is_facedown
                );
                reported_first_render = true;
            }

            let render_result = match mode {
                ViewMode::SelfTest => {
                    last_mode = Some(ViewMode::SelfTest);
                    prev_dirty_y = (0, 239);
                    display.render_full_frame(COLOR_BG, |target| {
                        render_selftest(target, &ui_state);
                    })
                }
                ViewMode::Pet => {
                    let is_first_pet_frame = last_mode != Some(ViewMode::Pet) || just_woke_up;
                    last_mode = Some(ViewMode::Pet);

                    let (cur_min, cur_max) = pet_dirty_y_bounds(&ui_state);
                    let (flush_min, flush_max) = if is_first_pet_frame {
                        // Full refresh on entering Pet mode or waking from FaceDown
                        (0, 239)
                    } else {
                        // Expand flush area with previous frame to cleanly erase outer graphics
                        (cur_min.min(prev_dirty_y.0), cur_max.max(prev_dirty_y.1))
                    };
                    prev_dirty_y = (cur_min, cur_max);

                    display.render_partial(flush_min, flush_max, COLOR_BG, |target| {
                        render_pet(target, &ui_state);
                    })
                }
            };

            match render_result {
                Ok(()) => {
                    needs_render = false;
                }
                Err(err) => {
                    println!("LCD frame render failed: {:?}; retrying in 1s", err);
                    next_render_retry = Instant::now() + Duration::from_secs(1);
                    // Force full-screen refresh upon recovery to wipe any corrupted panel VRAM
                    last_mode = None;
                    prev_dirty_y = (0, 239);
                }
            }
        }

        // Target Deadline Pacing:
        // For fluid animation (Pet mode or breathing), maintain strict isochronous frame deadlines (~33.3 FPS)
        // to completely eliminate frame interval jitter.
        // Suspend high-frequency pacing when device is FaceDown on desk to save CPU cycles and power.
        if !ui_state.is_facedown() && ui_state.requires_fluid_animation() {
            let now = Instant::now();
            if now < next_frame_deadline {
                Timer::after(next_frame_deadline - now).await;
                next_frame_deadline += FRAME_INTERVAL;
            } else {
                next_frame_deadline = now + FRAME_INTERVAL;
                // Yield to allow concurrent background tasks (BLE, IMU, App) to progress smoothly
                Timer::after(Duration::from_millis(1)).await;
            }
        } else {
            // Static or FaceDown mode: reset deadline to prevent frame bursts upon re-entering fluid animations
            next_frame_deadline = Instant::now() + FRAME_INTERVAL;
            match select(
                UI_STATE_SIGNAL.wait(),
                Timer::after(Duration::from_millis(500)),
            )
            .await
            {
                Either::First(new_state) => {
                    ui_state = new_state;
                    needs_render = true;
                }
                Either::Second(_) => {}
            }
        }
    }
}
