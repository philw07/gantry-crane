/// Rounds an f64 to the given number of decimal places
pub fn round(num: f64, decimal_places: u32) -> f64 {
    let mult = 10_u64.pow(decimal_places) as f64;
    (num * mult).round() / mult
}
