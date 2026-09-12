//! Runtime-neutral timing rules shared by client scheduling adapters.
/// Retry numbering starts at one. Arithmetic saturates before applying the cap.
pub fn retry_delay_seconds(retry: u32, base: u64, maximum: u64) -> u64 {
    let multiplier = 1_u64
        .checked_shl(retry.saturating_sub(1))
        .unwrap_or(u64::MAX);
    base.saturating_mul(multiplier).min(maximum)
}
