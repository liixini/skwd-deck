#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum At {
    Sunrise(i32),
    Sunset(i32),
    Clock(u32),
}

fn solar_offset(text: &str, name: &str, make: fn(i32) -> At) -> Option<At> {
    let rest = text.strip_prefix(name)?;
    if rest.is_empty() {
        return Some(make(0));
    }
    let offset = rest.parse::<i32>().ok()?;
    (-720..=720).contains(&offset).then_some(make(offset))
}

pub fn parse_at(text: &str) -> Option<At> {
    let value = text.trim().to_lowercase();
    solar_offset(&value, "sunrise", At::Sunrise)
        .or_else(|| solar_offset(&value, "sunset", At::Sunset))
        .or_else(|| {
            let (hour, minute) = value.split_once(':')?;
            let hour: u32 = hour.trim().parse().ok()?;
            let minute: u32 = minute.trim().parse().ok()?;
            (hour < 24 && minute < 60).then_some(At::Clock(hour * 60 + minute))
        })
}

pub fn fire_minute(at: At, sunrise: u32, sunset: u32) -> u32 {
    match at {
        At::Sunrise(offset) => (sunrise as i32 + offset).rem_euclid(1440) as u32,
        At::Sunset(offset) => (sunset as i32 + offset).rem_euclid(1440) as u32,
        At::Clock(minute) => minute,
    }
}

#[cfg(test)]
#[path = "time_tests.rs"]
mod tests;
