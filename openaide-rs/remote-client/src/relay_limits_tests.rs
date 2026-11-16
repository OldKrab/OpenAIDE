use super::ThrottleCount;

#[test]
fn only_growth_in_the_throttled_count_is_reported() {
    let mut count = ThrottleCount::default();

    assert_eq!(count.observe(0), None);
    assert_eq!(count.observe(2), Some(2));
    assert_eq!(count.observe(2), None);
    assert_eq!(count.observe(3), Some(1));
}
