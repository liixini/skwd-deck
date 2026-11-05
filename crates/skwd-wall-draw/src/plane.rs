use image::imageops::FilterType;
use image::{ImageBuffer, Luma};

const PERCENT: f32 = 100.0;

#[derive(Clone, Debug)]
pub struct Plane {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
}

impl Plane {
    pub fn filled(width: usize, height: usize, value: f32) -> Self {
        Self { width, height, data: vec![value; width * height] }
    }

    pub fn at(&self, x: usize, y: usize) -> f32 {
        self.data[y * self.width + x]
    }

    pub fn sample(&self, point: [f32; 2]) -> f32 {
        let x = (point[0].round().max(0.0) as usize).min(self.width - 1);
        let y = (point[1].round().max(0.0) as usize).min(self.height - 1);
        self.at(x, y)
    }

    pub fn unit(&self) -> (Self, f32, f32) {
        let low = self.data.iter().copied().fold(f32::INFINITY, f32::min);
        let high = self.data.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let span = (high - low).max(f32::EPSILON);
        let data = self.data.iter().map(|value| (value - low) / span).collect();
        (Self { width: self.width, height: self.height, data }, low, span)
    }

    pub fn resize(&self, width: usize, height: usize, filter: FilterType) -> Self {
        let (unit, low, span) = self.unit();
        let buffer: ImageBuffer<Luma<f32>, Vec<f32>> =
            ImageBuffer::from_raw(self.width as u32, self.height as u32, unit.data)
                .expect("plane size");
        let resized = image::imageops::resize(&buffer, width as u32, height as u32, filter);
        let data = resized.into_raw().into_iter().map(|value| value * span + low).collect();
        Self { width, height, data }
    }

    pub fn crop(&self, left: usize, top: usize, width: usize, height: usize) -> Self {
        let buffer: ImageBuffer<Luma<f32>, Vec<f32>> =
            ImageBuffer::from_raw(self.width as u32, self.height as u32, self.data.clone())
                .expect("plane size");
        let window = image::imageops::crop_imm(
            &buffer,
            left as u32,
            top as u32,
            width as u32,
            height as u32,
        );
        Self { width, height, data: window.to_image().into_raw() }
    }

    pub fn map(&self, change: impl Fn(f32) -> f32) -> Self {
        Self {
            width: self.width,
            height: self.height,
            data: self.data.iter().map(|&value| change(value)).collect(),
        }
    }

    pub fn above(&self, cutoff: f32) -> Self {
        self.map(|value| f32::from(u8::from(value > cutoff)))
    }

    pub fn zip(&self, other: &Self, change: impl Fn(f32, f32) -> f32) -> Self {
        assert_eq!((self.width, self.height), (other.width, other.height), "plane sizes");
        let data =
            self.data.iter().zip(&other.data).map(|(&own, &theirs)| change(own, theirs)).collect();
        Self { width: self.width, height: self.height, data }
    }

    pub fn mean(&self, radius: usize) -> Self {
        let (width, height) = (self.width, self.height);
        let stride = width + 1;
        let mut sums = vec![0.0f64; stride * (height + 1)];
        for y in 0..height {
            let mut row = 0.0;
            for x in 0..width {
                row += f64::from(self.at(x, y));
                sums[(y + 1) * stride + x + 1] = sums[y * stride + x + 1] + row;
            }
        }
        let mut mean = Self::filled(width, height, 0.0);
        for y in 0..height {
            for x in 0..width {
                let (left, top) = (x.saturating_sub(radius), y.saturating_sub(radius));
                let (right, bottom) = ((x + radius + 1).min(width), (y + radius + 1).min(height));
                let total = sums[bottom * stride + right]
                    - sums[top * stride + right]
                    - sums[bottom * stride + left]
                    + sums[top * stride + left];
                mean.data[y * width + x] =
                    (total / ((right - left) * (bottom - top)) as f64) as f32;
            }
        }
        mean
    }

    pub fn guided(&self, guide: &Self, radius: usize, epsilon: f32) -> Self {
        let guide_mean = guide.mean(radius);
        let own_mean = self.mean(radius);
        let square_mean = guide.map(|value| value * value).mean(radius);
        let product_mean = guide.zip(self, |light, level| light * level).mean(radius);
        let variance = square_mean.zip(&guide_mean, |square, mean| square - mean * mean);
        let joint = guide_mean.zip(&own_mean, |light, level| light * level);
        let covariance = product_mean.zip(&joint, |product, shared| product - shared);
        let slope =
            covariance.zip(&variance, |covariance, variance| covariance / (variance + epsilon));
        let offset = own_mean
            .zip(&slope.zip(&guide_mean, |slope, mean| slope * mean), |mean, shift| mean - shift);
        let lift = guide.zip(&slope.mean(radius), |light, slope| light * slope);
        lift.zip(&offset.mean(radius), |scaled, offset| (scaled + offset).clamp(0.0, 1.0))
    }

    pub fn percentile(&self, percent: f32) -> f32 {
        let mut sorted = self.data.clone();
        sorted.sort_by(f32::total_cmp);
        let rank = percent / PERCENT * (sorted.len() - 1) as f32;
        let low = rank.floor() as usize;
        let high = (low + 1).min(sorted.len() - 1);
        sorted[low] + (sorted[high] - sorted[low]) * (rank - low as f32)
    }

    pub fn ink(&self, cutoff: f32) -> Vec<u8> {
        self.data
            .iter()
            .map(
                |&value| {
                    if value >= cutoff { (value * f32::from(u8::MAX)).round() as u8 } else { 0 }
                },
            )
            .collect()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.data
            .iter()
            .map(|value| (value.clamp(0.0, 1.0) * f32::from(u8::MAX)).round() as u8)
            .collect()
    }
}

#[cfg(test)]
#[path = "plane_tests.rs"]
mod tests;
