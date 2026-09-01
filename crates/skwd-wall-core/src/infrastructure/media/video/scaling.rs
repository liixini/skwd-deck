use ffmpeg_the_third as ff;

use super::super::cover_dims;

pub(super) struct CoverScaler {
    target_width: u32,
    target_height: u32,
    destination_format: ff::format::Pixel,
    bytes_per_pixel: usize,
    flags: ff::software::scaling::Flags,
    state: Option<(ff::software::scaling::Context, ff::format::Pixel, u32, u32, u32, u32)>,
    scaled: ff::frame::Video,
}

impl CoverScaler {
    pub(super) fn new(
        target_width: u32,
        target_height: u32,
        destination_format: ff::format::Pixel,
        bytes_per_pixel: usize,
    ) -> Self {
        Self::new_with_flags(
            target_width,
            target_height,
            destination_format,
            bytes_per_pixel,
            ff::software::scaling::Flags::BILINEAR,
        )
    }

    pub(super) fn new_with_flags(
        target_width: u32,
        target_height: u32,
        destination_format: ff::format::Pixel,
        bytes_per_pixel: usize,
        flags: ff::software::scaling::Flags,
    ) -> Self {
        Self {
            target_width,
            target_height,
            destination_format,
            bytes_per_pixel,
            flags,
            state: None,
            scaled: ff::frame::Video::empty(),
        }
    }

    pub(super) fn cover_into(
        &mut self,
        frame: &ff::frame::Video,
        buffer: &mut Vec<u8>,
    ) -> anyhow::Result<()> {
        use ff::software::scaling::Context as Scaler;
        let (source_width, source_height, format) = (frame.width(), frame.height(), frame.format());
        if source_width == 0 || source_height == 0 {
            anyhow::bail!("empty frame");
        }
        let stale = self.state.as_ref().is_none_or(|(_, previous, width, height, _, _)| {
            *previous != format || *width != source_width || *height != source_height
        });
        if stale {
            let (cover_width, cover_height) =
                cover_dims(source_width, source_height, self.target_width, self.target_height);
            let flags = ff::software::scaling::Flags::from_bits_truncate(self.flags.bits());
            let scaler = Scaler::get(
                format,
                source_width,
                source_height,
                self.destination_format,
                cover_width,
                cover_height,
                flags,
            )?;
            self.state =
                Some((scaler, format, source_width, source_height, cover_width, cover_height));
            self.scaled = ff::frame::Video::empty();
        }
        let Some((scaler, _, _, _, cover_width, cover_height)) = self.state.as_mut() else {
            anyhow::bail!("scaler init failed");
        };
        let (cover_width, cover_height) = (*cover_width, *cover_height);
        scaler.run(frame, &mut self.scaled)?;
        let stride = self.scaled.stride(0);
        let data = self.scaled.data(0);
        let offset_x = ((cover_width - self.target_width) / 2) as usize * self.bytes_per_pixel;
        let offset_y = ((cover_height - self.target_height) / 2) as usize;
        let row_bytes = self.target_width as usize * self.bytes_per_pixel;
        buffer.clear();
        buffer.reserve(row_bytes * self.target_height as usize);
        for row in 0..self.target_height as usize {
            let start = (offset_y + row) * stride + offset_x;
            buffer.extend_from_slice(&data[start..start + row_bytes]);
        }
        Ok(())
    }

    pub(super) fn cover_rgb(
        &mut self,
        frame: &ff::frame::Video,
    ) -> anyhow::Result<image::RgbImage> {
        let mut buffer = Vec::new();
        self.cover_into(frame, &mut buffer)?;
        image::RgbImage::from_raw(self.target_width, self.target_height, buffer)
            .ok_or_else(|| anyhow::anyhow!("cover buffer size mismatch"))
    }
}
