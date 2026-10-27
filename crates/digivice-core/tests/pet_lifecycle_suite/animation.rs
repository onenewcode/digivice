use digivice_core::pet::{
    ActionPhase, LookDirection, PetAction, PetAnimationState, PetFrame, PetMood, PetSpecies,
    PupilShape, SquashStretch,
};

#[test]
fn pet_animation_frame_conversion_preserves_expressive_fields() {
    let state = PetAnimationState {
        species: PetSpecies::Koromon,
        mood: PetMood::Happy,
        pupil_shape: PupilShape::Heart,
        blink_pct: 0, // Sleeping or mid-blink
        action: PetAction::SleepingZzz { bubble_pct: 75 },
        action_phase: ActionPhase::Loop,
        ..Default::default()
    };

    let frame = PetFrame::from(state);
    assert_eq!(frame.species, PetSpecies::Koromon);
    assert_eq!(frame.mood, PetMood::Happy);
    assert_eq!(frame.pupil_shape, PupilShape::Heart);
    assert_eq!(frame.blink_pct, 0);
    assert!(matches!(
        frame.action,
        PetAction::SleepingZzz { bubble_pct: 75 }
    ));
}

#[test]
fn squash_stretch_damping_lut_computes_exact_ratios_and_settlement() {
    // Step 0: Impact compression
    let step0 = SquashStretch::from_spring_step(0);
    assert_eq!(step0.offset_y_px, -3);
    assert_eq!(step0.scale_x_pct, 103);
    assert_eq!(step0.scale_y_pct, 97);

    // Step 2: Spring rebound stretch
    let step2 = SquashStretch::from_spring_step(2);
    assert_eq!(step2.offset_y_px, 2);
    assert_eq!(step2.scale_x_pct, 98);
    assert_eq!(step2.scale_y_pct, 102);

    // Settled step (>= 6)
    let settled = SquashStretch::from_spring_step(6);
    assert_eq!(settled.offset_y_px, 0);
    assert_eq!(settled.scale_x_pct, 100);
    assert_eq!(settled.scale_y_pct, 100);
}

#[test]
fn pupil_shape_and_action_variants_express_distinct_visual_states() {
    let dizzy = PetFrame {
        pupil_shape: PupilShape::Dizzy,
        action: PetAction::LookAround(LookDirection::Center),
        ..PetFrame::default()
    };
    assert_eq!(dizzy.pupil_shape, PupilShape::Dizzy);

    let star = PetFrame {
        pupil_shape: PupilShape::Star,
        ..PetFrame::default()
    };
    assert_eq!(star.pupil_shape, PupilShape::Star);

    let neutral = PetFrame::default();
    assert_eq!(neutral.pupil_shape, PupilShape::Round);
    assert_eq!(neutral.blink_pct, 100);
}
