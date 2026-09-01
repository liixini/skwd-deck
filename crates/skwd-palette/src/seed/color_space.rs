use crate::Rgb;

fn linearize(channel: u8) -> f32 {
    let value = f32::from(channel) / 255.0;
    if value <= 0.040_45 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
}

fn pivot(value: f32) -> f32 {
    if value > 0.008_856 { value.cbrt() } else { 7.787 * value + 16.0 / 116.0 }
}

pub fn to_lab(color: Rgb) -> (f32, f32, f32) {
    let (red, green, blue) = (linearize(color.0), linearize(color.1), linearize(color.2));
    let x = pivot((red * 0.4124 + green * 0.3576 + blue * 0.1805) / 0.950_47);
    let y = pivot(red * 0.2126 + green * 0.7152 + blue * 0.0722);
    let z = pivot((red * 0.0193 + green * 0.1192 + blue * 0.9505) / 1.088_83);
    (116.0 * y - 16.0, 500.0 * (x - y), 200.0 * (y - z))
}

pub fn chroma_of(color: Rgb) -> f32 {
    let (_, a, b) = to_lab(color);
    a.hypot(b)
}

pub(super) fn distance_squared(a: (f32, f32, f32), b: (f32, f32, f32)) -> f32 {
    (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2) + (a.2 - b.2).powi(2)
}
