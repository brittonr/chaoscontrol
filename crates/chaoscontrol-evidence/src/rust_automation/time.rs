//! Deterministic UTC rendering for supplied Unix seconds.

const SECONDS_PER_MINUTE: i64 = 60;
const MINUTES_PER_HOUR: i64 = 60;
const HOURS_PER_DAY: i64 = 24;
const SECONDS_PER_DAY: i64 = SECONDS_PER_MINUTE * MINUTES_PER_HOUR * HOURS_PER_DAY;
const CIVIL_EPOCH_OFFSET_DAYS: i64 = 719_468;
const DAYS_PER_ERA: i64 = 146_097;
const DAYS_PER_FOUR_YEARS: i64 = 1_460;
const DAYS_PER_HUNDRED_YEARS: i64 = 36_524;
const DAYS_PER_FOUR_HUNDRED_YEARS: i64 = 146_096;
const YEARS_PER_ERA: i64 = 400;
const DAYS_PER_YEAR: i64 = 365;
const YEARS_PER_LEAP_CYCLE: i64 = 4;
const YEARS_PER_CENTURY: i64 = 100;
const MONTH_PROJECTION_FACTOR: i64 = 5;
const MONTH_PROJECTION_OFFSET: i64 = 2;
const MONTH_PROJECTION_DIVISOR: i64 = 153;
const MONTH_OFFSET: i64 = 2;
const MARCH_BASE_MONTH: i64 = 3;
const JANUARY_SHIFT: i64 = -9;
const YEAR_MONTH_CUTOFF: i64 = 10;

pub fn rfc3339_utc(unix_seconds: u64) -> Result<String, String> {
    let seconds = i64::try_from(unix_seconds)
        .map_err(|_| String::from("Unix seconds exceed UTC formatter range"))?;
    let days = seconds.div_euclid(SECONDS_PER_DAY);
    let within_day = seconds.rem_euclid(SECONDS_PER_DAY);
    let hour = within_day / (SECONDS_PER_MINUTE * MINUTES_PER_HOUR);
    let minute = (within_day / SECONDS_PER_MINUTE) % MINUTES_PER_HOUR;
    let second = within_day % SECONDS_PER_MINUTE;
    let (year, month, day) = civil_from_days(days);
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
    ))
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + CIVIL_EPOCH_OFFSET_DAYS;
    let era = shifted.div_euclid(DAYS_PER_ERA);
    let day_of_era = shifted - era * DAYS_PER_ERA;
    let year_of_era = (day_of_era - day_of_era / DAYS_PER_FOUR_YEARS
        + day_of_era / DAYS_PER_HUNDRED_YEARS
        - day_of_era / DAYS_PER_FOUR_HUNDRED_YEARS)
        / DAYS_PER_YEAR;
    let mut year = year_of_era + era * YEARS_PER_ERA;
    let day_of_year = day_of_era
        - (DAYS_PER_YEAR * year_of_era + year_of_era / YEARS_PER_LEAP_CYCLE
            - year_of_era / YEARS_PER_CENTURY);
    let month_prime = (MONTH_PROJECTION_FACTOR * day_of_year + MONTH_PROJECTION_OFFSET)
        / MONTH_PROJECTION_DIVISOR;
    let day = day_of_year
        - (MONTH_PROJECTION_DIVISOR * month_prime + MONTH_PROJECTION_OFFSET)
            / MONTH_PROJECTION_FACTOR
        + 1;
    let month = month_prime
        + if month_prime < YEAR_MONTH_CUTOFF {
            MARCH_BASE_MONTH
        } else {
            JANUARY_SHIFT
        };
    year += if month <= MONTH_OFFSET { 1 } else { 0 };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::rfc3339_utc;

    #[test]
    fn epoch_and_known_timestamp_render_in_utc() {
        assert_eq!(rfc3339_utc(0).expect("epoch"), "1970-01-01T00:00:00Z");
        assert_eq!(
            rfc3339_utc(1_700_000_000).expect("known"),
            "2023-11-14T22:13:20Z"
        );
    }
}
