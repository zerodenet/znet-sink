use znet_client_core::scheduling::retry_delay_seconds;
#[test]
fn retries_are_bounded_even_for_overflowing_counts() {
    assert_eq!(retry_delay_seconds(1, 60, 900), 60);
    assert_eq!(retry_delay_seconds(3, 60, 900), 240);
    assert_eq!(retry_delay_seconds(u32::MAX, 60, 900), 900);
    assert_eq!(retry_delay_seconds(0, 60, 900), 60);
    assert_eq!(retry_delay_seconds(3, u64::MAX, 900), 900);
}
