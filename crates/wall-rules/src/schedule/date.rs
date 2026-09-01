#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DateSpec {
    pub year: Option<i32>,
    pub month: u32,
    pub day: u32,
}

pub fn parse_date(text: &str) -> Option<DateSpec> {
    let valid = |month: u32, day: u32| (1..=12).contains(&month) && (1..=31).contains(&day);
    let parts: Vec<&str> = text.trim().split('-').map(str::trim).collect();
    match parts.as_slice() {
        [month, day] => {
            let (month, day) = (month.parse().ok()?, day.parse().ok()?);
            valid(month, day).then_some(DateSpec { year: None, month, day })
        }
        [year, month, day] => {
            let (year, month, day) = (year.parse().ok()?, month.parse().ok()?, day.parse().ok()?);
            valid(month, day).then_some(DateSpec { year: Some(year), month, day })
        }
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DateRange {
    pub start: DateSpec,
    pub end: DateSpec,
}

pub fn date_in_range(range: &DateRange, year: i32, month: u32, day: u32) -> bool {
    let (start, end) = (range.start, range.end);
    if let (Some(start_year), Some(end_year)) = (start.year, end.year) {
        let today = (year, month, day);
        (start_year, start.month, start.day) <= today && today <= (end_year, end.month, end.day)
    } else {
        let (month_day, start_month_day, end_month_day) =
            ((month, day), (start.month, start.day), (end.month, end.day));
        if start_month_day <= end_month_day {
            start_month_day <= month_day && month_day <= end_month_day
        } else {
            month_day >= start_month_day || month_day <= end_month_day
        }
    }
}

#[cfg(test)]
#[path = "date_tests.rs"]
mod tests;
