use digivice_core::pet::{
    ActionPhase, DefaultPet, FoodType, PetAction, PetBehavior, PetEvent, PetSpecies, SleepState,
};
use embassy_time::Instant;

fn at(millis: u64) -> Instant {
    Instant::from_millis(millis)
}

#[test]
fn uninterruptible_feeding_queues_patting_flush_and_attack_with_predictable_phases_and_durations() {
    let mut pet = DefaultPet::with_stats_and_species(0, 0, PetSpecies::Koromon);
    let mut t = 0u64;

    // Start with food: double tap initiates feeding (hunger < 4)
    pet.handle(PetEvent::DoubleTap, at(t));
    let frame = pet.tick(at(t));
    assert_eq!(frame.action, PetAction::NoticeFood(FoodType::Meat));
    assert_eq!(frame.action_phase, ActionPhase::Enter);

    // During uninterruptible eating, send Tap (patting: HappyHop, 24), Flush (FlushClean, 20), Attack (AttackSpecial, 36)
    t += 25;
    pet.handle(PetEvent::Tap, at(t));
    t += 25;
    pet.trigger_action(PetAction::FlushClean { tick: 0 }, 20);
    t += 25;
    pet.handle(PetEvent::Attack, at(t));

    // Helper to step ticks and collect action progression
    let mut history = heapless::Vec::<(PetAction, ActionPhase), 256>::new();
    for _ in 0..180 {
        t += 25;
        let f = pet.tick(at(t));
        if history.last().copied() != Some((f.action, f.action_phase)) {
            let _ = history.push((f.action, f.action_phase));
        }
    }

    // Sequence must progress through:
    // NoticeFood (Enter -> Loop -> Exit)
    // -> EatingChew (Enter -> Loop -> Exit)
    // -> SatisfiedGulp (Enter -> Loop -> Exit)
    // -> Queued HappyHop (Enter -> Loop -> Exit)
    // -> Queued FlushClean (Enter -> Loop -> Exit)
    // -> Celebration HappyHop (Enter -> Loop -> Exit)
    // -> Queued AttackSpecial (Enter -> Loop -> Exit)
    // -> IdleBreathe
    let actions: heapless::Vec<PetAction, 16> = history
        .iter()
        .filter(|(_, phase)| *phase == ActionPhase::Enter)
        .map(|(act, _)| *act)
        .collect();

    assert!(matches!(
        actions.as_slice(),
        &[
            PetAction::EatingChew { .. },
            PetAction::SatisfiedGulp,
            PetAction::HappyHop,
            PetAction::FlushClean { .. },
            PetAction::HappyHop,
            PetAction::AttackSpecial { .. },
        ]
    ));
}

#[test]
fn safe_sleep_immediately_interrupts_any_active_action_and_clears_queue() {
    let mut pet = DefaultPet::with_stats_and_species(0, 0, PetSpecies::Koromon);
    let mut t = 0u64;

    // Start feeding (NoticeFood) and queue actions
    pet.handle(PetEvent::DoubleTap, at(t));
    pet.handle(PetEvent::Tap, at(t));
    pet.handle(PetEvent::Attack, at(t));

    let frame = pet.tick(at(t));
    assert_eq!(frame.action, PetAction::NoticeFood(FoodType::Meat));

    // Physical FaceDown occurs: must immediately interrupt to SleepingZzz
    t += 25;
    pet.handle(PetEvent::FaceDown, at(t));
    let frame = pet.tick(at(t));
    assert_eq!(frame.sleep_state, SleepState::Asleep);
    assert!(matches!(frame.action, PetAction::SleepingZzz { .. }));

    // Interactions during sleep cannot wake or queue actions
    t += 25;
    pet.handle(PetEvent::Tap, at(t));
    pet.handle(PetEvent::Attack, at(t));
    let frame = pet.tick(at(t));
    assert_eq!(frame.sleep_state, SleepState::Asleep);
    assert!(matches!(frame.action, PetAction::SleepingZzz { .. }));
}

#[test]
fn interruptible_action_gracefully_winds_down_to_exit_when_interrupted_by_higher_priority() {
    let mut pet = DefaultPet::new();
    let mut t = 0u64;

    // Start HappyHop (Normal priority, duration 24)
    pet.trigger_action(PetAction::HappyHop, 24);
    let f1 = pet.tick(at(t));
    assert_eq!(f1.action, PetAction::HappyHop);
    assert_eq!(f1.action_phase, ActionPhase::Enter);

    // Enter phase completes in 2 ticks
    t += 25;
    let _ = pet.tick(at(t));
    t += 25;
    let f3 = pet.tick(at(t));
    assert_eq!(f3.action, PetAction::HappyHop);
    assert_eq!(f3.action_phase, ActionPhase::Loop);

    // Higher priority action arrives (AttackSpecial, High priority)
    pet.handle(PetEvent::Attack, at(t));

    // Next tick must be Exit phase of HappyHop
    t += 25;
    let f4 = pet.tick(at(t));
    assert_eq!(f4.action, PetAction::HappyHop);
    assert_eq!(f4.action_phase, ActionPhase::Exit);

    // Exit runs for 2 ticks, then transitions to AttackSpecial Enter
    t += 25;
    let _ = pet.tick(at(t));
    t += 25;
    let f6 = pet.tick(at(t));
    assert_eq!(f6.action, PetAction::AttackSpecial { tick: 0 });
    assert_eq!(f6.action_phase, ActionPhase::Enter);
}

#[test]
fn queue_priority_eviction_drops_lowest_priority_when_full() {
    let mut pet = DefaultPet::new();

    // Pet in an uninterruptible action
    pet.trigger_action(PetAction::NoticeFood(FoodType::Meat), 10);
    let _ = pet.tick(at(0));

    // Fill queue (capacity 4) with Normal priority actions
    pet.trigger_action(PetAction::HappyHop, 10);
    pet.trigger_action(PetAction::HappyHop, 11);
    pet.trigger_action(PetAction::HappyHop, 12);
    pet.trigger_action(PetAction::HappyHop, 13);

    // A Low priority action should be rejected when full of Normal actions
    pet.trigger_action(
        PetAction::MusicDance {
            style: digivice_core::pet::MusicDanceStyle::ChillSway,
            beat_tick: 0,
        },
        10,
    );

    // A High priority action (AttackSpecial) should evict one of the Normal items
    pet.trigger_action(PetAction::AttackSpecial { tick: 0 }, 36);

    // When NoticeFood finishes its sequence, queued actions emerge
    let mut t = 0u64;
    let mut executed_special = false;
    for _ in 0..200 {
        t += 25;
        let f = pet.tick(at(t));
        if matches!(f.action, PetAction::AttackSpecial { .. }) {
            executed_special = true;
            break;
        }
    }
    assert!(executed_special);
}
