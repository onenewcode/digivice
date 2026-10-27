use digivice_core::domain::{ImuError, ImuSample, ImuStatus};
use digivice_core::imu::ImuAttitudeSnapshot;
use digivice_core::pet::{DefaultPet, PetBehavior, PetEvent};
use embassy_time::Instant;

fn at(millis: u64) -> Instant {
    Instant::from_millis(millis)
}

#[test]
fn snapshot_valid_filtered_gravity_requires_healthy_hardware() {
    let healthy = ImuAttitudeSnapshot {
        sample: ImuSample::default(),
        roll_deg: 0.0,
        pitch_deg: 0.0,
        heading_deg: 0.0,
        gx_filt_mg: 150,
        gy_filt_mg: 200,
        gz_filt_mg: 950,
        imu_status: ImuStatus::HardwarePresent,
    };
    assert_eq!(healthy.valid_filtered_gravity(), Some((150, 200, 950)));

    let faulted = ImuAttitudeSnapshot {
        sample: ImuSample::default(),
        roll_deg: 0.0,
        pitch_deg: 0.0,
        heading_deg: 0.0,
        gx_filt_mg: 150,
        gy_filt_mg: 200,
        gz_filt_mg: 950,
        imu_status: ImuStatus::Unavailable(ImuError::BusError),
    };
    assert_eq!(faulted.valid_filtered_gravity(), None);
}

#[test]
fn pitch_changes_remain_strictly_pitch_invariant_with_zero_lateral_tilt() {
    let mut pet = DefaultPet::new();
    let mut t = 0u64;

    // Pitch angles from 0° (flat on desk) to 90° (standing upright)
    let pitch_vectors = [
        (0, 0, 1000),  // Flat: 0°
        (0, 259, 966), // 15°
        (0, 500, 866), // 30°
        (0, 707, 707), // 45°
        (0, 866, 500), // 60°
        (0, 966, 259), // 75°
        (0, 1000, 0),  // 90°: vertical
    ];

    for (gx, gy, gz) in pitch_vectors {
        for _ in 0..10 {
            t += 25;
            pet.handle(
                PetEvent::Attitude {
                    ax_mg: gx,
                    ay_mg: gy,
                    az_mg: gz,
                },
                at(t),
            );
            let frame = pet.tick(at(t));
            assert_eq!(
                frame.tilt_deg, 0,
                "Lifting/pitching should never cause lateral tilt"
            );
            assert_eq!(
                frame.gaze_x, 0,
                "Lifting/pitching should never cause lateral gaze drift"
            );
        }
    }
}

#[test]
fn lateral_tilts_follow_accurately_without_overshoot() {
    let mut pet = DefaultPet::new();
    let mut t = 0u64;

    // Tilt 30° right: gx = 500, gy = 0, gz = 866 -> atan2(500, 866) = 30°
    for _ in 0..30 {
        t += 25;
        pet.handle(
            PetEvent::Attitude {
                ax_mg: 500,
                ay_mg: 0,
                az_mg: 866,
            },
            at(t),
        );
        let _ = pet.tick(at(t));
    }
    let frame = pet.tick(at(t));
    assert_eq!(frame.tilt_deg, 30);
    assert_eq!(frame.gaze_x, 10);

    // Tilt 30° left: gx = -500, gy = 0, gz = 866 -> atan2(-500, 866) = -30°
    for _ in 0..30 {
        t += 25;
        pet.handle(
            PetEvent::Attitude {
                ax_mg: -500,
                ay_mg: 0,
                az_mg: 866,
            },
            at(t),
        );
        let _ = pet.tick(at(t));
    }
    let frame = pet.tick(at(t));
    assert_eq!(frame.tilt_deg, -30);
    assert_eq!(frame.gaze_x, -10);
}

#[test]
fn deadzone_suppresses_micro_vibrations() {
    let mut pet = DefaultPet::new();
    let mut t = 0u64;

    // Vibrations <= 60mg lateral
    for gx in [-50, -30, 0, 30, 50] {
        for _ in 0..5 {
            t += 25;
            pet.handle(
                PetEvent::Attitude {
                    ax_mg: gx,
                    ay_mg: 0,
                    az_mg: 1000,
                },
                at(t),
            );
            let frame = pet.tick(at(t));
            assert_eq!(frame.tilt_deg, 0);
            assert_eq!(frame.gaze_x, 0);
        }
    }
}
