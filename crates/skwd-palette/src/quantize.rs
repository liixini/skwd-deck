use crate::{Rgb, to_hsl};

const MAX_SAMPLES: usize = 4096;

pub fn quantize(rgba: &[u8], count: usize) -> Vec<Rgb> {
    if count == 0 || rgba.len() < 4 {
        return Vec::new();
    }
    let pixel_count = rgba.len() / 4;
    let step = (pixel_count / MAX_SAMPLES).max(1);
    let mut samples: Vec<Rgb> = Vec::with_capacity(MAX_SAMPLES.min(pixel_count));
    let mut index = 0;
    while index < pixel_count {
        let offset = index * 4;
        if rgba[offset + 3] >= 128 {
            samples.push(Rgb(rgba[offset], rgba[offset + 1], rgba[offset + 2]));
        }
        index += step;
    }
    if samples.is_empty() {
        return Vec::new();
    }

    let mut boxes: Vec<Vec<Rgb>> = vec![samples];
    while boxes.len() < count {
        let Some(index) = boxes
            .iter()
            .enumerate()
            .filter(|(_, colors)| colors.len() > 1)
            .max_by_key(|(_, colors)| box_range(colors))
            .map(|(index, _)| index)
        else {
            break;
        };
        if box_range(&boxes[index]) == 0 {
            break;
        }
        let mut colors = boxes.swap_remove(index);
        let channel = longest_channel(&colors);
        colors.sort_by_key(|color| channel_value(*color, channel));
        let upper = colors.split_off(colors.len() / 2);
        boxes.push(colors);
        boxes.push(upper);
    }

    let mut result: Vec<Rgb> = boxes.iter().map(|colors| average(colors)).collect();
    result.sort_by(|a, b| population_weight(*b).total_cmp(&population_weight(*a)));
    result
}

fn channel_value(color: Rgb, channel: u8) -> u8 {
    match channel {
        0 => color.0,
        1 => color.1,
        _ => color.2,
    }
}

fn box_range(colors: &[Rgb]) -> u32 {
    let mut low = [255u8; 3];
    let mut high = [0u8; 3];
    for color in colors {
        for (index, value) in [color.0, color.1, color.2].into_iter().enumerate() {
            low[index] = low[index].min(value);
            high[index] = high[index].max(value);
        }
    }
    (0..3).map(|index| (high[index] - low[index]) as u32).max().unwrap_or(0)
}

fn longest_channel(colors: &[Rgb]) -> u8 {
    let mut low = [255u8; 3];
    let mut high = [0u8; 3];
    for color in colors {
        for (index, value) in [color.0, color.1, color.2].into_iter().enumerate() {
            low[index] = low[index].min(value);
            high[index] = high[index].max(value);
        }
    }
    let red = high[0] - low[0];
    let green = high[1] - low[1];
    let blue = high[2] - low[2];
    if red >= green && red >= blue {
        0
    } else if green >= blue {
        1
    } else {
        2
    }
}

fn average(colors: &[Rgb]) -> Rgb {
    if colors.is_empty() {
        return Rgb(0, 0, 0);
    }
    let (mut red, mut green, mut blue) = (0u64, 0u64, 0u64);
    for color in colors {
        red += color.0 as u64;
        green += color.1 as u64;
        blue += color.2 as u64;
    }
    let count = colors.len() as u64;
    Rgb((red / count) as u8, (green / count) as u8, (blue / count) as u8)
}

fn population_weight(color: Rgb) -> f32 {
    let (_, saturation, light) = to_hsl(color);
    saturation * (1.0 - (light - 0.5).abs() * 1.4)
}

#[cfg(test)]
#[path = "quantize_tests.rs"]
mod tests;
