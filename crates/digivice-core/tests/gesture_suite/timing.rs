use digivice_core::gesture::filter::{MAX_FILTER_INTERVAL, bounded_dt_seconds};
use embassy_time::{Duration, Instant};

fn at(micros: u64) -> Instant {
    Instant::from_micros(micros)
}

#[test]
fn the_first_sample_has_no_invented_elapsed_time() {
    assert_eq!(bounded_dt_seconds(None, at(5_000)), 0.0);
}

#[test]
fn observed_intervals_are_converted_from_monotonic_time() {
    let dt = bounded_dt_seconds(Some(at(5_000)), at(10_000));
    assert!((dt - 0.005).abs() < 0.000_001, "dt={dt}");
    assert_eq!(MAX_FILTER_INTERVAL, Duration::from_millis(100));
}

#[test]
fn zero_backward_and_long_stall_intervals_do_not_integrate_heading() {
    let now = at(100_000);
    assert_eq!(bounded_dt_seconds(Some(now), now), 0.0);
    assert_eq!(bounded_dt_seconds(Some(at(101_000)), now), 0.0);
    assert_eq!(bounded_dt_seconds(Some(at(0)), at(100_001)), 0.0);
    let edge = bounded_dt_seconds(Some(at(0)), at(100_000));
    assert!((edge - 0.1).abs() < 0.000_001, "dt={edge}");
}
