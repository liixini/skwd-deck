use std::{sync::mpsc, time::Duration};

use anyhow::{Context, bail, ensure};
use block_compression::{BC7Settings, CompressionVariant, GpuBlockCompressor};
use skwd_wall_core::blocks::{
    BLOCK_EDGE, Mip, NEAR_BLOCK_BYTES, RGBA_CHANNELS, mip_shapes, near_size,
};
use wgpu::util::{DeviceExt, TextureDataOrder};
use wgpu::{
    Adapter, Backends, Buffer, BufferDescriptor, BufferUsages, CommandEncoderDescriptor,
    ComputePassDescriptor, Device, DeviceDescriptor, DeviceType, ExperimentalFeatures, Extent3d,
    Features, Instance, InstanceDescriptor, Limits, MapMode, MemoryHints, PollType,
    PowerPreference, Queue, RequestAdapterOptions, TextureDescriptor, TextureDimension,
    TextureFormat, TextureUsages, TextureViewDescriptor, Trace,
};

const READBACK_TIMEOUT: Duration = Duration::from_secs(30);
const RGB_CHANNELS: usize = 3;
const RGB_TOLERANCE: u8 = 4;
const ALPHA_FLOOR: u8 = 254;
const PROBE_COLORS: [[u8; RGB_CHANNELS]; 3] = [[24, 80, 160], [200, 32, 96], [16, 224, 112]];

pub(super) struct Encoder {
    device: Device,
    queue: Queue,
    compressor: GpuBlockCompressor,
}

impl Encoder {
    pub(super) fn new() -> anyhow::Result<Self> {
        let instance =
            Instance::new(&InstanceDescriptor { backends: Backends::VULKAN, ..Default::default() });
        let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .context("request Vulkan adapter")?;
        reject_software(&adapter)?;
        let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor {
            label: None,
            required_features: Features::empty(),
            required_limits: Limits::default(),
            experimental_features: ExperimentalFeatures::disabled(),
            memory_hints: MemoryHints::Performance,
            trace: Trace::Off,
        }))
        .context("request GPU device")?;
        let compressor = GpuBlockCompressor::new(device.clone(), queue.clone());
        let mut encoder = Self { device, queue, compressor };
        validate_probe(&encoder.encode(&probe_mips())?)?;
        Ok(encoder)
    }

    pub(super) fn encode(&mut self, mips: &[Mip]) -> anyhow::Result<Vec<u8>> {
        let total_size = validate_mips(mips)?;
        let variant = CompressionVariant::BC7(BC7Settings::opaque_fast());
        let blocks = self.device.create_buffer(&BufferDescriptor {
            label: None,
            size: total_size as u64,
            usage: BufferUsages::COPY_SRC | BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let staging = self.device.create_buffer(&BufferDescriptor {
            label: None,
            size: total_size as u64,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut textures = Vec::with_capacity(mips.len());
        let mut offset = 0;
        for mip in mips {
            let texture = self.device.create_texture_with_data(
                &self.queue,
                &TextureDescriptor {
                    label: None,
                    size: Extent3d {
                        width: mip.width,
                        height: mip.height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: TextureFormat::Rgba8Unorm,
                    usage: TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                TextureDataOrder::LayerMajor,
                &mip.rgba,
            );
            let view = texture.create_view(&TextureViewDescriptor::default());
            self.compressor.add_compression_task(
                variant,
                &view,
                mip.width,
                mip.height,
                &blocks,
                None,
                Some(offset),
            );
            textures.push(texture);
            offset += u32::try_from(variant.blocks_byte_size(mip.width, mip.height))?;
        }
        let mut encoder =
            self.device.create_command_encoder(&CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder
                .begin_compute_pass(&ComputePassDescriptor { label: None, timestamp_writes: None });
            self.compressor.compress(&mut pass);
        }
        encoder.copy_buffer_to_buffer(&blocks, 0, &staging, 0, total_size as u64);
        self.queue.submit([encoder.finish()]);
        read_blocks(&self.device, &staging)
    }
}

fn reject_software(adapter: &Adapter) -> anyhow::Result<()> {
    if adapter.get_info().device_type == DeviceType::Cpu {
        bail!("Vulkan adapter is a software device");
    }
    Ok(())
}

fn validate_mips(mips: &[Mip]) -> anyhow::Result<usize> {
    ensure!(mips.len() == mip_shapes().count(), "unexpected BC7 mip count");
    let mut total = 0usize;
    for (mip, (width, height)) in mips.iter().zip(mip_shapes()) {
        ensure!((mip.width, mip.height) == (width, height), "unexpected BC7 mip size");
        let pixels = usize::try_from(width)? * usize::try_from(height)?;
        ensure!(mip.rgba.len() == pixels * RGBA_CHANNELS, "unexpected BC7 RGBA size");
        total += near_size(width, height);
    }
    ensure!(total == NEAR_BLOCK_BYTES, "unexpected BC7 payload size");
    Ok(total)
}

fn probe_mips() -> Vec<Mip> {
    mip_shapes()
        .enumerate()
        .map(|(level, (width, height))| {
            let mut rgba = vec![0; width as usize * height as usize * RGBA_CHANNELS];
            for block_y in 0..height / BLOCK_EDGE {
                for block_x in 0..width / BLOCK_EDGE {
                    let color = probe_color(block_x, block_y, level);
                    for y in 0..BLOCK_EDGE {
                        for x in 0..BLOCK_EDGE {
                            let pixel = ((block_y * BLOCK_EDGE + y) * width
                                + block_x * BLOCK_EDGE
                                + x) as usize
                                * RGBA_CHANNELS;
                            rgba[pixel..pixel + RGB_CHANNELS].copy_from_slice(&color);
                            rgba[pixel + RGB_CHANNELS] = u8::MAX;
                        }
                    }
                }
            }
            Mip { width, height, rgba }
        })
        .collect()
}

fn probe_color(block_x: u32, block_y: u32, level: usize) -> [u8; RGB_CHANNELS] {
    PROBE_COLORS[(block_x as usize + block_y as usize + level) % PROBE_COLORS.len()]
}

fn validate_probe(blocks: &[u8]) -> anyhow::Result<()> {
    ensure!(blocks.len() == NEAR_BLOCK_BYTES, "GPU BC7 self-test size");
    let block_bytes = near_size(BLOCK_EDGE, BLOCK_EDGE);
    let mut decoded = [0; BLOCK_EDGE as usize * BLOCK_EDGE as usize * RGBA_CHANNELS];
    let mut offset = 0;
    for (level, (width, height)) in mip_shapes().enumerate() {
        for block_y in 0..height / BLOCK_EDGE {
            for block_x in 0..width / BLOCK_EDGE {
                bcdec_rs::bc7(
                    &blocks[offset..offset + block_bytes],
                    &mut decoded,
                    BLOCK_EDGE as usize * RGBA_CHANNELS,
                );
                let color = probe_color(block_x, block_y, level);
                for pixel in decoded.chunks_exact(RGBA_CHANNELS) {
                    if pixel[..RGB_CHANNELS]
                        .iter()
                        .zip(color)
                        .any(|(&actual, expected)| actual.abs_diff(expected) > RGB_TOLERANCE)
                        || pixel[RGB_CHANNELS] < ALPHA_FLOOR
                    {
                        bail!("GPU BC7 self-test mismatch");
                    }
                }
                offset += block_bytes;
            }
        }
    }
    Ok(())
}

fn read_blocks(device: &Device, staging: &Buffer) -> anyhow::Result<Vec<u8>> {
    let slice = staging.slice(..);
    let (sender, receiver) = mpsc::channel();
    slice.map_async(MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(PollType::Wait { submission_index: None, timeout: Some(READBACK_TIMEOUT) })
        .context("wait for BC7 readback")?;
    receiver
        .recv_timeout(READBACK_TIMEOUT)
        .context("receive BC7 readback")?
        .context("map BC7 readback")?;
    let data = slice.get_mapped_range().to_vec();
    staging.unmap();
    Ok(data)
}

#[cfg(test)]
#[path = "codec_tests.rs"]
mod tests;
