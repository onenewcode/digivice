//! Display rendering and backlight PWM control task.

use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Instant, Timer};
use esp_println::println;

use crate::board::display::{Backlight, BandDisplay};
use crate::domain::ViewMode;
use crate::ipc::UI_STATE_SIGNAL;
use crate::ui::pet_view::render_pet;
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
    let mut needs_render = true;
    let mut reported_first_render = false;
    let mut next_render_retry = Instant::now();

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

        // Backlight PWM Management
        if ui_state.is_facedown() {
            backlight.set_brightness(0);
        } else if is_breathing {
            breath_idx = (breath_idx + 1) % BREATHING_LUT.len();
            backlight.set_brightness(BREATHING_LUT[breath_idx]);
        } else {
            backlight.set_brightness(ui_state.backlight_pct);
        }

        // Render Frame based on active ViewMode:
        // Skip heavy SPI rendering when device is face down (screen turned off onto desk)
        if needs_render && !ui_state.is_facedown() && Instant::now() >= next_render_retry {
            let mode = ui_state.mode;
            if !reported_first_render {
                println!(
                    "UI first render (mode={:?}, backlight={}%, face_down={})",
                    mode,
                    ui_state.backlight_pct,
                    ui_state.is_facedown()
                );
                reported_first_render = true;
            }
            let render_result = display.render_full_frame(COLOR_BG, |target| match mode {
                ViewMode::SelfTest => render_selftest(target, &ui_state),
                ViewMode::Pet => render_pet(target, &ui_state),
            });
            match render_result {
                Ok(()) => {
                    needs_render = false;
                }
                Err(err) => {
                    println!("LCD frame render failed: {:?}; retrying in 1s", err);
                    next_render_retry = Instant::now() + Duration::from_secs(1);
                }
            }
        }

        // Energy-efficient dirty-gated sleep:
        // If fluid animation is required (WaitConfirm pulse or Pet mode), step at 30ms;
        // otherwise, wait reactively for new state arrivals (with 500ms safety timeout for watchdogs).
        if ui_state.requires_fluid_animation() {
            Timer::after(Duration::from_millis(30)).await;
        } else {
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
