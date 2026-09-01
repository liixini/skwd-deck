pub use super::tags::matches_tag_spec;

pub struct Item<'a> {
    pub key: &'a str,
    pub tags: &'a [String],
    pub kind: &'a str,
    pub hue: i64,
    pub width: i64,
    pub height: i64,
}

pub fn folder_of(key: &str) -> String {
    let rel = key.split_once(':').map_or(key, |(_, rest)| rest);
    match rel.rsplit_once('/') {
        Some((dir, _)) => dir.to_string(),
        None => String::new(),
    }
}

fn matches_type(kind: &str, spec: &str) -> bool {
    let wanted = match spec.trim().to_lowercase().as_str() {
        "image" | "picture" | "pic" | "static" | "img" => "static",
        "video" | "vid" | "mp4" => "video",
        "we" | "scene" | "wallpaperengine" | "wallpaper-engine" => "we",
        other => return kind == other,
    };
    kind == wanted
}

fn color_bucket(name: &str) -> Option<i64> {
    if let Ok(num) = name.parse::<i64>() {
        return Some(num);
    }
    Some(match name {
        "red" => 0,
        "orange" => 1,
        "yellow" => 2,
        "lime" | "chartreuse" => 3,
        "green" => 4,
        "spring" | "mint" | "emerald" => 5,
        "cyan" | "teal" | "aqua" => 6,
        "azure" | "sky" => 7,
        "blue" => 8,
        "violet" | "purple" | "indigo" => 9,
        "magenta" | "fuchsia" => 10,
        "pink" | "rose" => 11,
        "gray" | "grey" | "mono" | "monochrome" | "grayscale" | "greyscale" | "black" | "white" => {
            99
        }
        _ => return None,
    })
}

#[derive(Clone, Copy)]
enum Comparison {
    Ge,
    Le,
    Gt,
    Lt,
    Eq,
}

fn parse_comparison(spec: &str) -> (Comparison, &str) {
    if let Some(rest) = spec.strip_prefix(">=") {
        (Comparison::Ge, rest)
    } else if let Some(rest) = spec.strip_prefix("<=") {
        (Comparison::Le, rest)
    } else if let Some(rest) = spec.strip_prefix('>') {
        (Comparison::Gt, rest)
    } else if let Some(rest) = spec.strip_prefix('<') {
        (Comparison::Lt, rest)
    } else if let Some(rest) = spec.strip_prefix('=') {
        (Comparison::Eq, rest)
    } else {
        (Comparison::Ge, spec)
    }
}

fn comparison_matches(comparison: Comparison, lhs: f64, rhs: f64) -> bool {
    match comparison {
        Comparison::Ge => lhs >= rhs,
        Comparison::Le => lhs <= rhs,
        Comparison::Gt => lhs > rhs,
        Comparison::Lt => lhs < rhs,
        Comparison::Eq => (lhs - rhs).abs() < 0.5,
    }
}

fn matches_dimension(field: &str, spec: &str, width: i64, height: i64) -> bool {
    let (comparison, rest) = parse_comparison(spec.trim());
    match field {
        "width" => rest
            .trim()
            .parse::<f64>()
            .is_ok_and(|num| comparison_matches(comparison, width as f64, num)),
        "height" => rest
            .trim()
            .parse::<f64>()
            .is_ok_and(|num| comparison_matches(comparison, height as f64, num)),
        "res" => {
            let Some((width_spec, height_spec)) = rest.split_once(['x', 'X']) else {
                return false;
            };
            match (width_spec.trim().parse::<f64>(), height_spec.trim().parse::<f64>()) {
                (Ok(wanted_width), Ok(wanted_height)) => {
                    comparison_matches(comparison, width as f64, wanted_width)
                        && comparison_matches(comparison, height as f64, wanted_height)
                }
                _ => false,
            }
        }
        "ratio" => {
            if height == 0 {
                return false;
            }
            let ratio = width as f64 / height as f64;
            match rest.trim() {
                "landscape" => ratio > 1.0,
                "portrait" => ratio < 1.0,
                "square" => (ratio - 1.0).abs() < 0.05,
                other => {
                    other.parse::<f64>().is_ok_and(|num| comparison_matches(comparison, ratio, num))
                }
            }
        }
        _ => false,
    }
}

fn matches_clause(clause: &str, item: &Item<'_>) -> bool {
    match clause {
        "all" | "favourites" => true,
        clause if clause.starts_with("folder:") => folder_of(item.key) == clause["folder:".len()..],
        clause if clause.starts_with("tag:") => {
            matches_tag_spec(item.tags, &clause["tag:".len()..])
        }
        clause if clause.starts_with("type:") => matches_type(item.kind, &clause["type:".len()..]),
        clause if clause.starts_with("color:") || clause.starts_with("colour:") => {
            let spec = clause.split_once(':').map_or("", |(_, rest)| rest);
            spec.split(',')
                .filter_map(|name| color_bucket(&name.trim().to_lowercase()))
                .any(|bucket| bucket == item.hue)
        }
        clause if clause.starts_with("width:") => {
            matches_dimension("width", &clause["width:".len()..], item.width, item.height)
        }
        clause if clause.starts_with("height:") => {
            matches_dimension("height", &clause["height:".len()..], item.width, item.height)
        }
        clause if clause.starts_with("res:") => {
            matches_dimension("res", &clause["res:".len()..], item.width, item.height)
        }
        clause if clause.starts_with("ratio:") => {
            matches_dimension("ratio", &clause["ratio:".len()..], item.width, item.height)
        }
        _ => false,
    }
}

pub fn matches(item: &Item<'_>, source: &str) -> bool {
    let mut any = false;
    for clause in source.split_whitespace() {
        if !matches_clause(clause, item) {
            return false;
        }
        any = true;
    }
    any
}

pub fn source_wants_favourites(source: &str) -> bool {
    source.split_whitespace().any(|clause| clause == "favourites")
}

#[cfg(test)]
#[path = "filter_tests.rs"]
mod tests;
