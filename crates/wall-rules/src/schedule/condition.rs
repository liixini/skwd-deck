use super::date::{DateRange, parse_date};
use super::time::{At, parse_at};

#[derive(Clone, Copy)]
pub enum Cmp {
    Ge,
    Le,
    Gt,
    Lt,
    Eq,
}

pub(super) fn parse_cmp(spec: &str) -> (Cmp, &str) {
    if let Some(rest) = spec.strip_prefix(">=") {
        (Cmp::Ge, rest)
    } else if let Some(rest) = spec.strip_prefix("<=") {
        (Cmp::Le, rest)
    } else if let Some(rest) = spec.strip_prefix('>') {
        (Cmp::Gt, rest)
    } else if let Some(rest) = spec.strip_prefix('<') {
        (Cmp::Lt, rest)
    } else if let Some(rest) = spec.strip_prefix('=') {
        (Cmp::Eq, rest)
    } else {
        (Cmp::Eq, spec)
    }
}

pub(super) fn cmp_ord<T: Ord + Copy>(comparison: Cmp, lhs: T, rhs: T) -> bool {
    match comparison {
        Cmp::Ge => lhs >= rhs,
        Cmp::Le => lhs <= rhs,
        Cmp::Gt => lhs > rhs,
        Cmp::Lt => lhs < rhs,
        Cmp::Eq => lhs == rhs,
    }
}

fn weekday_num(text: &str) -> Option<u32> {
    match text.trim().to_lowercase().as_str() {
        "sun" | "sunday" => Some(0),
        "mon" | "monday" => Some(1),
        "tue" | "tuesday" => Some(2),
        "wed" | "wednesday" => Some(3),
        "thu" | "thursday" => Some(4),
        "fri" | "friday" => Some(5),
        "sat" | "saturday" => Some(6),
        other => other.parse::<u32>().ok().filter(|&day| day < 7),
    }
}

pub enum Clause {
    TimeWindow(At, At),
    Time(Cmp, At),
    Weekday(Vec<u32>),
    Date(DateRange),
    Year(Cmp, i64),
    Weather(Vec<String>),
    Power(bool),
    Battery(Cmp, u8),
    Output(String),
    OutputCount(Cmp, u32),
    Never,
}

fn offset_solar(at: At) -> bool {
    matches!(at, At::Sunrise(offset) | At::Sunset(offset) if offset != 0)
}

fn parse_time_clause(value: &str, version: u64) -> Clause {
    if let Some((from, until)) = value.split_once("..") {
        match (parse_at(from.trim()), parse_at(until.trim())) {
            (Some(from), Some(until))
                if version >= 2 || !(offset_solar(from) || offset_solar(until)) =>
            {
                Clause::TimeWindow(from, until)
            }
            _ => Clause::Never,
        }
    } else {
        let (comparison, rest) = parse_cmp(value);
        parse_at(rest.trim()).map_or(Clause::Never, |at| {
            if version >= 2 || !offset_solar(at) {
                Clause::Time(comparison, at)
            } else {
                Clause::Never
            }
        })
    }
}

pub(super) fn parse_clause(token: &str, version: u64) -> Clause {
    let Some((factor, value)) = token.split_once(':') else {
        return Clause::Never;
    };
    let value = value.trim();
    match factor.trim() {
        "time" => parse_time_clause(value, version),
        "weekday" | "day" => {
            let days: Vec<u32> = value.split(',').filter_map(weekday_num).collect();
            if days.is_empty() { Clause::Never } else { Clause::Weekday(days) }
        }
        "date" => {
            if let Some((from, until)) = value.split_once("..") {
                match (parse_date(from.trim()), parse_date(until.trim())) {
                    (Some(start), Some(end)) => Clause::Date(DateRange { start, end }),
                    _ => Clause::Never,
                }
            } else {
                parse_date(value).map_or(Clause::Never, |date| {
                    Clause::Date(DateRange { start: date, end: date })
                })
            }
        }
        "year" => {
            let (comparison, rest) = parse_cmp(value);
            rest.trim().parse::<i64>().map_or(Clause::Never, |year| Clause::Year(comparison, year))
        }
        "weather" => {
            let set: Vec<String> = value
                .split(',')
                .map(|tag| tag.trim().to_lowercase())
                .filter(|tag| !tag.is_empty())
                .collect();
            if set.is_empty() { Clause::Never } else { Clause::Weather(set) }
        }
        "power" if version >= 2 => match value.to_lowercase().as_str() {
            "battery" => Clause::Power(true),
            "external" => Clause::Power(false),
            _ => Clause::Never,
        },
        "battery" if version >= 2 => {
            let (comparison, rest) = parse_cmp(value);
            rest.trim()
                .parse::<u8>()
                .ok()
                .filter(|percent| *percent <= 100)
                .map_or(Clause::Never, |percent| Clause::Battery(comparison, percent))
        }
        "output" if version >= 2 => {
            let name = value.trim();
            if name.is_empty() || name.len() > 128 {
                Clause::Never
            } else {
                Clause::Output(name.to_string())
            }
        }
        "outputs" if version >= 2 => {
            let (comparison, rest) = parse_cmp(value);
            rest.trim()
                .parse::<u32>()
                .ok()
                .filter(|count| *count <= 64)
                .map_or(Clause::Never, |count| Clause::OutputCount(comparison, count))
        }
        _ => Clause::Never,
    }
}

#[cfg(test)]
#[path = "condition_tests.rs"]
mod tests;
