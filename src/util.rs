/// Rounds an f64 to the given number of decimal places
pub fn round(num: f64, decimal_places: u32) -> f64 {
    let mult = 10_u64.pow(decimal_places) as f64;
    (num * mult).round() / mult
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use super::round;

    #[test]
    fn test_round() {
        let tests = vec![
            (PI, 0, 3.0),
            (PI, 1, 3.1),
            (PI, 2, 3.14),
            (PI, 3, 3.142),
            (PI, 4, 3.1416),
            (PI, 5, 3.14159),
            (PI, 6, 3.141593),
            (-PI, 0, -3.0),
            (-PI, 1, -3.1),
            (-PI, 2, -3.14),
            (-PI, 3, -3.142),
            (-PI, 4, -3.1416),
            (-PI, 5, -3.14159),
            (-PI, 6, -3.141593),
            (23.49999, 2, 23.50),
            (23.49999, 3, 23.50),
            (23.49999, 4, 23.50),
            (23.49999, 5, 23.49999),
            (-94231.14, 1, -94231.1),
            (-94231.15, 1, -94231.2),
        ];

        for (value, decimals, expected) in tests {
            assert_eq!(round(value, decimals), expected);
        }
    }
}
