use std::ffi::c_int;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use image::imageops::FilterType;
use image::{Rgb, RgbImage};
use ort::device::Device as GpuDevice;
use ort::environment::Environment;
use ort::ep::{self, ArenaExtendStrategy};
use ort::session::{Session, SessionOutputs};
use ort::value::Tensor;

use crate::gpu::{self, Uuid};
use crate::plane::Plane;
use crate::raster::Point;
use skwd_wall_core::infrastructure::drawing::pack::{DepthModel, Device, Pack};

pub const THREADS: usize = 2;
const LINEART_THREADS: usize = 4;
const CHANNELS: usize = 3;
const LINEART_STRIDE: usize = 4;
const DEPTH_SIDE: f32 = 518.0;
const DEPTH3_SIDE: f32 = 504.0;
const DEPTH_MULTIPLE: f32 = 14.0;
const SKY_CUTOFF: f32 = 0.3;
const SKY_HORIZON: f32 = 99.0;
const UNKNOWN_CAMERA: f32 = -1.0;
const EXTRINSIC_SIDE: usize = 4;
const INTRINSIC_SIDE: usize = 3;
const DEPTH_MEAN: [f32; CHANNELS] = [0.485, 0.456, 0.406];
const DEPTH_STD: [f32; CHANNELS] = [0.229, 0.224, 0.225];
const DEPTH_RANGE: [f32; 2] = [8.0, 92.0];
const SUBJECT_SIDE: u32 = 640;
const BOX_VALUES: usize = 4;

struct MaskSpec {
    side: u32,
    salient_shift: f32,
}

const MASKS: MaskSpec = MaskSpec { side: 1024, salient_shift: 0.5 };

struct DetectorSpec {
    side: u32,
    fill: u8,
    confidence: f32,
    overlap: f32,
}

const DETECTOR: DetectorSpec =
    DetectorSpec { side: 640, fill: 114, confidence: 0.35, overlap: 0.5 };

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Cpu,
    Cuda,
    Webgpu,
}

impl From<Backend> for Device {
    fn from(backend: Backend) -> Self {
        match backend {
            Backend::Cpu => Self::Cpu,
            Backend::Cuda => Self::Cuda,
            Backend::Webgpu => Self::Webgpu,
        }
    }
}

impl Backend {
    pub fn label(self) -> &'static str {
        Device::from(self).label()
    }
}

struct WebGpuSpec {
    library: &'static str,
    provider: &'static str,
    options: &'static [(&'static str, &'static str)],
    cpu_nodes: &'static str,
    adapter: &'static str,
    dawn_backend: &'static str,
    vulkan: &'static str,
}

const WEBGPU: WebGpuSpec = WebGpuSpec {
    library: "webgpu",
    provider: "WebGpuExecutionProvider",
    options: &[
        ("preferredLayout", "NCHW"),
        ("storageBufferCacheMode", "simple"),
        ("powerPreference", "high-performance"),
    ],
    cpu_nodes: "forceCpuNodeNames",
    adapter: "adapterIndex",
    dawn_backend: "dawnBackendType",
    vulkan: "Vulkan",
};

#[derive(Clone, Copy)]
struct Target {
    backend: Backend,
    gpu: Option<c_int>,
}

const CPU_NODE_SEPARATOR: &str = "\n";

#[derive(Clone, Copy)]
struct Tuning<'a> {
    threads: usize,
    cpu_nodes: &'a [String],
}

const PLAIN: Tuning = Tuning { threads: THREADS, cpu_nodes: &[] };

struct Contract {
    inputs: &'static [&'static str],
    outputs: &'static [&'static str],
}

const LINEART: Contract = Contract { inputs: &["image"], outputs: &["lines"] };
const DEPTH: Contract = Contract { inputs: &["pixel_values"], outputs: &["predicted_depth"] };
const DEPTH3: Contract =
    Contract { inputs: &["image", "extrinsics_in", "intrinsics_in"], outputs: &["depth", "sky"] };
const SUBJECT: Contract =
    Contract { inputs: &["pixel_values"], outputs: &["logits", "pred_boxes"] };
const CHARACTER: Contract = Contract { inputs: &["img"], outputs: &["mask"] };
const SALIENT: Contract = Contract { inputs: &["input_image"], outputs: &["output_image"] };
const LOCATOR: Contract = Contract { inputs: &["images"], outputs: &["output0"] };

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Bounds {
    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    pub fn height(&self) -> f32 {
        self.bottom - self.top
    }

    pub fn middle(&self) -> Point {
        [f32::midpoint(self.left, self.right), f32::midpoint(self.top, self.bottom)]
    }

    pub fn contains(&self, point: Point) -> bool {
        (self.left..=self.right).contains(&point[0]) && (self.top..=self.bottom).contains(&point[1])
    }

    pub fn overlap(&self, other: &Bounds) -> f32 {
        let width = (self.right.min(other.right) - self.left.max(other.left)).max(0.0);
        let height = (self.bottom.min(other.bottom) - self.top.max(other.top)).max(0.0);
        let shared = width * height;
        shared / (self.width() * self.height() + other.width() * other.height() - shared)
    }
}

#[derive(Clone, Copy)]
pub enum Locator {
    Faces,
    Heads,
}

struct Letterbox {
    image: RgbImage,
    scale: f32,
    left: u32,
    top: u32,
    width: u32,
    height: u32,
}

impl Letterbox {
    fn unframe(
        &self,
        [cx, cy, width, height]: [f32; BOX_VALUES],
        back: f32,
        size: [usize; 2],
    ) -> Bounds {
        let x = |at: f32| {
            ((at - self.left as f32) / self.scale * back).clamp(0.0, size[0] as f32 - 1.0)
        };
        let y =
            |at: f32| ((at - self.top as f32) / self.scale * back).clamp(0.0, size[1] as f32 - 1.0);
        Bounds {
            left: x(cx - width / 2.0),
            top: y(cy - height / 2.0),
            right: x(cx + width / 2.0),
            bottom: y(cy + height / 2.0),
        }
    }
}

fn letterbox(image: &RgbImage, side: u32, fill: u8) -> Letterbox {
    let scale = side as f32 / image.width().max(image.height()) as f32;
    let fit = |length: u32| ((length as f32 * scale).round() as u32).clamp(1, side);
    let (width, height) = (fit(image.width()), fit(image.height()));
    let resized = image::imageops::resize(image, width, height, FilterType::Triangle);
    let mut framed = RgbImage::from_pixel(side, side, Rgb([fill; CHANNELS]));
    let (left, top) = ((side - width) / 2, (side - height) / 2);
    image::imageops::replace(&mut framed, &resized, left.into(), top.into());
    Letterbox { image: framed, scale, left, top, width, height }
}

#[derive(Clone, Debug)]
pub struct Detection {
    pub confidence: f32,
    pub bounds: Bounds,
}

pub struct Models<'pack> {
    environment: Environment,
    pack: &'pack Pack,
    target: Target,
}

impl<'pack> Models<'pack> {
    pub fn load(pack: &'pack Pack, device: Device) -> Result<Self> {
        let environment = ort::init_from(&pack.runtime)
            .with_context(|| format!("load ONNX Runtime {}", pack.runtime.display()))?
            .with_telemetry(false)
            .build()?;
        let order = order(device, pack.webgpu.is_some());
        let tuning = lineart_tuning(pack);
        let chosen = gpu::configured();
        let (_, target) = first_working(order, |backend| {
            let gpu = match backend {
                Backend::Cpu => None,
                Backend::Cuda => place(device, backend, chosen.as_ref(), gpu::cuda_ordinal)?,
                Backend::Webgpu => {
                    let library =
                        pack.webgpu.as_ref().context("drawing pack has no webgpu plugin")?;
                    environment
                        .register_ep_library(WEBGPU.library, library)
                        .with_context(|| format!("load webgpu plugin {}", library.display()))?;
                    place(device, backend, chosen.as_ref(), gpu::webgpu_adapter)?
                }
            };
            let target = Target { backend, gpu };
            open(&environment, target, &pack.lineart, tuning)?;
            Ok(target)
        })?;
        eprintln!("drawing: using {}", target.backend.label());
        Ok(Self { environment, pack, target })
    }

    pub fn backend(&self) -> Backend {
        self.target.backend
    }

    pub fn check(&self) -> Result<()> {
        for (path, tuning, contract) in [
            (&self.pack.lineart, lineart_tuning(self.pack), &LINEART),
            (&self.pack.depth, PLAIN, self.depth_contract()),
            (&self.pack.subject, PLAIN, &SUBJECT),
            (&self.pack.character, PLAIN, &CHARACTER),
            (&self.pack.salient, PLAIN, &SALIENT),
            (&self.pack.faces, PLAIN, &LOCATOR),
            (&self.pack.heads, PLAIN, &LOCATOR),
        ] {
            let session = self.session(path, tuning)?;
            let has_inputs = contract
                .inputs
                .iter()
                .all(|name| session.inputs().iter().any(|outlet| outlet.name() == *name));
            let has_outputs = contract
                .outputs
                .iter()
                .all(|name| session.outputs().iter().any(|outlet| outlet.name() == *name));
            ensure!(
                has_inputs && has_outputs,
                "{} does not match the drawing model contract",
                path.display()
            );
        }
        let blank = RgbImage::new(SUBJECT_SIDE, SUBJECT_SIDE);
        self.subjects(&blank, [SUBJECT_SIDE as usize; 2])?;
        Ok(())
    }

    fn session(&self, path: &Path, tuning: Tuning) -> Result<Session> {
        open(&self.environment, self.target, path, tuning)
    }

    pub fn lineart(&self, image: &RgbImage) -> Result<Plane> {
        let (width, height) = (image.width() as usize, image.height() as usize);
        let padded =
            [width.next_multiple_of(LINEART_STRIDE), height.next_multiple_of(LINEART_STRIDE)];
        let pixels = planar(image, padded, |_, value| value);
        let tensor = Tensor::from_array(([1, CHANNELS, padded[1], padded[0]], pixels))?;
        let (shape, lines) =
            self.infer(&self.pack.lineart, lineart_tuning(self.pack), &LINEART, tensor)?;
        ensure!(
            shape == [1, 1, padded[1] as i64, padded[0] as i64],
            "lineart output has an unexpected shape"
        );
        let mut strength = Plane::filled(width, height, 0.0);
        for y in 0..height {
            for x in 0..width {
                strength.data[y * width + x] = (1.0 - lines[y * padded[0] + x]).clamp(0.0, 1.0);
            }
        }
        Ok(strength)
    }

    fn depth_contract(&self) -> &'static Contract {
        match self.pack.depth_model {
            DepthModel::AnythingV2 => &DEPTH,
            DepthModel::Anything3 => &DEPTH3,
        }
    }

    pub fn depth(&self, image: &RgbImage, size: [usize; 2]) -> Result<Plane> {
        let (width, height) = image.dimensions();
        let input = match (self.pack.depth_model, self.pack.depth_side) {
            (_, Some(side)) => longest_input(width, height, side.get() as f32),
            (DepthModel::AnythingV2, None) => depth_input(width, height),
            (DepthModel::Anything3, None) => longest_input(width, height, DEPTH3_SIDE),
        };
        match self.pack.depth_model {
            DepthModel::AnythingV2 => self.disparity(image, size, input),
            DepthModel::Anything3 => self.distance(image, size, input),
        }
    }

    fn disparity(&self, image: &RgbImage, size: [usize; 2], input: [usize; 2]) -> Result<Plane> {
        let tensor =
            Tensor::from_array(([1, CHANNELS, input[1], input[0]], depth_pixels(image, input)))?;
        let (shape, values) = self.infer(&self.pack.depth, PLAIN, &DEPTH, tensor)?;
        let [_, rows, columns] = shape[..] else {
            anyhow::bail!("depth output has an unexpected shape");
        };
        let farther = values.iter().map(|value| -value).collect();
        Ok(spread(Plane { width: columns as usize, height: rows as usize, data: farther }.resize(
            size[0],
            size[1],
            FilterType::Triangle,
        )))
    }

    fn distance(&self, image: &RgbImage, size: [usize; 2], input: [usize; 2]) -> Result<Plane> {
        let camera = |side: usize| {
            Tensor::from_array(([1, 1, side, side], vec![UNKNOWN_CAMERA; side * side]))
        };
        let mut session = self.session(&self.pack.depth, PLAIN)?;
        let outputs = session.run(ort::inputs![
            DEPTH3.inputs[0] => Tensor::from_array(
                ([1, 1, CHANNELS, input[1], input[0]], depth_pixels(image, input)),
            )?,
            DEPTH3.inputs[1] => camera(EXTRINSIC_SIDE)?,
            DEPTH3.inputs[2] => camera(INTRINSIC_SIDE)?,
        ])?;
        let (shape, distance) = tensor_output(&outputs, DEPTH3.outputs[0])?;
        let [_, _, rows, columns] = shape[..] else {
            anyhow::bail!("depth output has an unexpected shape");
        };
        let (_, sky) = tensor_output(&outputs, DEPTH3.outputs[1])?;
        far_from_distance(distance, sky, [columns as usize, rows as usize], size)
    }

    fn infer(
        &self,
        path: &Path,
        tuning: Tuning,
        contract: &Contract,
        tensor: Tensor<f32>,
    ) -> Result<(Vec<i64>, Vec<f32>)> {
        let mut session = self.session(path, tuning)?;
        let outputs = session.run(ort::inputs![contract.inputs[0] => tensor])?;
        let (shape, values) = tensor_output(&outputs, contract.outputs[0])?;
        Ok((shape.to_vec(), values.to_vec()))
    }

    fn mask(&self, model: &Path, contract: &Contract, pixels: Vec<f32>) -> Result<Plane> {
        let side = MASKS.side as usize;
        let tensor = Tensor::from_array(([1, CHANNELS, side, side], pixels))?;
        let (shape, data) = self.infer(model, PLAIN, contract, tensor)?;
        ensure!(
            shape == [1, 1, side as i64, side as i64],
            "{} mask has an unexpected shape",
            model.display()
        );
        Ok(Plane { width: side, height: side, data })
    }

    pub fn character(&self, image: &RgbImage, size: [usize; 2]) -> Result<Plane> {
        let framed = letterbox(image, MASKS.side, 0);
        let side = MASKS.side as usize;
        let pixels = planar(&framed.image, [side, side], |_, value| value);
        let mask = self.mask(&self.pack.character, &CHARACTER, pixels)?;
        Ok(mask
            .crop(
                framed.left as usize,
                framed.top as usize,
                framed.width as usize,
                framed.height as usize,
            )
            .resize(size[0], size[1], FilterType::Triangle))
    }

    pub fn salient(&self, image: &RgbImage, size: [usize; 2]) -> Result<Plane> {
        let side = MASKS.side as usize;
        let resized = image::imageops::resize(image, MASKS.side, MASKS.side, FilterType::Triangle);
        let pixels = planar(&resized, [side, side], |_, value| value - MASKS.salient_shift);
        let mask = self.mask(&self.pack.salient, &SALIENT, pixels)?;
        let (unit, _, _) = mask.unit();
        Ok(unit.resize(size[0], size[1], FilterType::Triangle))
    }

    pub fn locate(
        &self,
        locator: Locator,
        image: &RgbImage,
        size: [usize; 2],
    ) -> Result<Vec<Bounds>> {
        let model = match locator {
            Locator::Faces => &self.pack.faces,
            Locator::Heads => &self.pack.heads,
        };
        let framed = letterbox(image, DETECTOR.side, DETECTOR.fill);
        let side = DETECTOR.side as usize;
        let pixels = planar(&framed.image, [side, side], |_, value| value);
        let tensor = Tensor::from_array(([1, CHANNELS, side, side], pixels))?;
        let (shape, values) = self.infer(model, PLAIN, &LOCATOR, tensor)?;
        let [_, rows, columns] = shape[..] else {
            anyhow::bail!("{} output has an unexpected shape", model.display());
        };
        let shape = [rows as usize, columns as usize];
        ensure!(
            shape[0].min(shape[1]) == BOX_VALUES + 1 && shape[0] != shape[1],
            "{} output is not a single class box detector",
            model.display()
        );
        let back = size[0] as f32 / image.width() as f32;
        Ok(suppress(candidates(&values, shape), |values| framed.unframe(values, back, size)))
    }

    pub fn subjects(&self, image: &RgbImage, size: [usize; 2]) -> Result<Vec<Detection>> {
        let resized =
            image::imageops::resize(image, SUBJECT_SIDE, SUBJECT_SIDE, FilterType::Triangle);
        let side = SUBJECT_SIDE as usize;
        let pixels = planar(&resized, [side, side], |_, value| value);
        let tensor = Tensor::from_array(([1, CHANNELS, side, side], pixels))?;
        let mut session = self.session(&self.pack.subject, PLAIN)?;
        let outputs = session.run(ort::inputs![SUBJECT.inputs[0] => tensor])?;
        let (shape, logits) = tensor_output(&outputs, SUBJECT.outputs[0])?;
        let (_, boxes) = tensor_output(&outputs, SUBJECT.outputs[1])?;
        let Some(&classes) = shape.last() else {
            anyhow::bail!("subject outputs have unexpected shapes");
        };
        let classes = classes as usize;
        ensure!(
            classes > 0 && boxes.len() == logits.len() / classes * BOX_VALUES,
            "subject outputs have unexpected shapes"
        );
        ensure!(
            self.pack.subject_classes.iter().all(|&class| class < classes),
            "subject classes exceed the model's {classes} classes"
        );
        let mut found = Vec::new();
        for (scores, values) in logits.chunks_exact(classes).zip(boxes.chunks_exact(BOX_VALUES)) {
            let confidence = self
                .pack
                .subject_classes
                .iter()
                .map(|&class| sigmoid(scores[class]))
                .fold(0.0, f32::max);
            if confidence < self.pack.subject_confidence {
                continue;
            }
            if let Some(bounds) = decode_box(values, size) {
                found.push(Detection { confidence, bounds });
            }
        }
        Ok(found)
    }
}

fn lineart_tuning(pack: &Pack) -> Tuning<'_> {
    Tuning { threads: LINEART_THREADS, cpu_nodes: &pack.lineart_cpu_nodes }
}

fn open(environment: &Environment, target: Target, path: &Path, tuning: Tuning) -> Result<Session> {
    let backend = target.backend;
    let device = (backend == Backend::Webgpu).then(|| webgpu_device(environment)).transpose()?;
    let build = || -> ort::Result<Session> {
        let builder = Session::builder(environment)?
            .with_intra_threads(tuning.threads)?
            .with_memory_pattern(false)?;
        let unpooled = || ep::CPU::default().with_arena_allocator(false).build();
        let mut builder = match backend {
            Backend::Cpu => builder.with_execution_providers([unpooled()])?,
            Backend::Cuda => {
                let cuda = ep::CUDA::default()
                    .with_arena_extend_strategy(ArenaExtendStrategy::SameAsRequested)
                    .with_conv_max_workspace(false);
                let cuda = match target.gpu {
                    Some(ordinal) => cuda.with_device_id(ordinal),
                    None => cuda,
                };
                builder.with_execution_providers([cuda.build().error_on_failure()])?
            }
            Backend::Webgpu => builder
                .with_execution_providers([unpooled()])?
                .with_devices(device, Some(&webgpu_options(tuning.cpu_nodes, target.gpu)))?,
        };
        builder.commit_from_file(path)
    };
    build().with_context(|| format!("load drawing model {} on {}", path.display(), backend.label()))
}

pub fn falls_back(device: Device, backend: Backend, error: &anyhow::Error) -> bool {
    device == Device::Auto && backend != Backend::Cpu && from_runtime(error)
}

fn from_runtime(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| cause.downcast_ref::<ort::Error>().is_some())
}

fn place(
    device: Device,
    backend: Backend,
    chosen: Option<&Uuid>,
    find: impl FnOnce(&Uuid) -> Result<Option<c_int>>,
) -> Result<Option<c_int>> {
    let Some(chosen) = chosen else {
        return Ok(None);
    };
    let label = backend.label();
    let found = find(chosen);
    if let Ok(Some(index)) = found {
        eprintln!("drawing: configured gpu is {label} device {index}");
        return Ok(Some(index));
    }
    let reason = found.err().map(|error| format!(": {error:#}")).unwrap_or_default();
    if device == Device::Auto && backend == Backend::Cuda {
        bail!("configured gpu has no {label} device{reason}");
    }
    eprintln!("drawing: configured gpu has no {label} device{reason}, using the default");
    Ok(None)
}

fn order(device: Device, webgpu: bool) -> &'static [Backend] {
    match device {
        Device::Cpu => &[Backend::Cpu],
        Device::Cuda => &[Backend::Cuda],
        Device::Webgpu => &[Backend::Webgpu],
        Device::Auto if webgpu => &[Backend::Cuda, Backend::Webgpu, Backend::Cpu],
        Device::Auto => &[Backend::Cuda, Backend::Cpu],
    }
}

fn first_working<T>(
    order: &[Backend],
    mut open: impl FnMut(Backend) -> Result<T>,
) -> Result<(Backend, T)> {
    let (&last, fallbacks) = order.split_last().expect("device order");
    for &backend in fallbacks {
        match open(backend) {
            Ok(value) => return Ok((backend, value)),
            Err(error) => {
                eprintln!("drawing: {} unavailable, falling back: {error:#}", backend.label());
            }
        }
    }
    Ok((last, open(last)?))
}

fn webgpu_device(environment: &Environment) -> Result<GpuDevice<'_>> {
    environment
        .devices()
        .find(|device| device.ep().is_ok_and(|name| name == WEBGPU.provider))
        .context("no WebGPU device is listed")
}

fn webgpu_options(cpu_nodes: &[String], adapter: Option<c_int>) -> Vec<(String, String)> {
    let nodes =
        (!cpu_nodes.is_empty()).then(|| (WEBGPU.cpu_nodes, cpu_nodes.join(CPU_NODE_SEPARATOR)));
    let adapter = adapter.into_iter().flat_map(|index| {
        [(WEBGPU.adapter, index.to_string()), (WEBGPU.dawn_backend, WEBGPU.vulkan.to_owned())]
    });
    WEBGPU
        .options
        .iter()
        .map(|&(key, value)| (key, value.to_owned()))
        .chain(nodes)
        .chain(adapter)
        .map(|(key, value)| (format!("{}.{key}", WEBGPU.provider), value))
        .collect()
}

fn tensor_output<'a>(outputs: &'a SessionOutputs, name: &str) -> Result<(&'a [i64], &'a [f32])> {
    let output = outputs.get(name).with_context(|| format!("model output {name} is missing"))?;
    let (shape, values) = output.try_extract_tensor::<f32>()?;
    ensure!(values.iter().all(|value| value.is_finite()), "model output {name} is not finite");
    Ok((&shape[..], values))
}

fn depth_input(width: u32, height: u32) -> [usize; 2] {
    let scale_width = DEPTH_SIDE / width as f32;
    let scale_height = DEPTH_SIDE / height as f32;
    let scale = if (1.0 - scale_width).abs() < (1.0 - scale_height).abs() {
        scale_width
    } else {
        scale_height
    };
    patches(width, height, scale)
}

fn longest_input(width: u32, height: u32, target: f32) -> [usize; 2] {
    patches(width, height, target / width.max(height) as f32)
}

fn patches(width: u32, height: u32, scale: f32) -> [usize; 2] {
    [width, height].map(|side| {
        ((side as f32 * scale / DEPTH_MULTIPLE).round().max(1.0) * DEPTH_MULTIPLE) as usize
    })
}

fn depth_pixels(image: &RgbImage, input: [usize; 2]) -> Vec<f32> {
    let resized =
        image::imageops::resize(image, input[0] as u32, input[1] as u32, FilterType::CatmullRom);
    planar(&resized, input, |channel, value| (value - DEPTH_MEAN[channel]) / DEPTH_STD[channel])
}

fn far_from_distance(
    distance: &[f32],
    sky: &[f32],
    grid: [usize; 2],
    size: [usize; 2],
) -> Result<Plane> {
    ensure!(sky.is_empty() || sky.len() == distance.len(), "sky output has an unexpected shape");
    let open: Vec<bool> = if sky.is_empty() {
        vec![false; distance.len()]
    } else {
        sky.iter().map(|&chance| chance >= SKY_CUTOFF).collect()
    };
    let ground: Vec<f32> =
        distance.iter().zip(&open).filter(|(_, open)| !**open).map(|(&reach, _)| reach).collect();
    if ground.is_empty() {
        return Ok(Plane::filled(size[0], size[1], 1.0));
    }
    let horizon = Plane { width: ground.len(), height: 1, data: ground }.percentile(SKY_HORIZON);
    let log = distance
        .iter()
        .zip(&open)
        .map(|(&reach, &open)| if open { horizon } else { reach }.max(f32::MIN_POSITIVE).ln())
        .collect();
    let resample = |data| {
        Plane { width: grid[0], height: grid[1], data }.resize(
            size[0],
            size[1],
            FilterType::Triangle,
        )
    };
    let mut farther = spread(resample(log));
    if !sky.is_empty() {
        for (level, chance) in farther.data.iter_mut().zip(resample(sky.to_vec()).data) {
            if chance >= SKY_CUTOFF {
                *level = 1.0;
            }
        }
    }
    Ok(farther)
}

fn spread(mut plane: Plane) -> Plane {
    let [low, high] = DEPTH_RANGE.map(|percent| plane.percentile(percent));
    let span = (high - low).max(f32::EPSILON);
    for value in &mut plane.data {
        *value = ((*value - low) / span).clamp(0.0, 1.0);
    }
    plane
}

fn candidates(values: &[f32], shape: [usize; 2]) -> Vec<(f32, [f32; BOX_VALUES])> {
    let [rows, columns] = shape;
    let (features, anchors) = if rows < columns { (rows, columns) } else { (columns, rows) };
    let value = |feature: usize, anchor: usize| {
        if rows < columns {
            values[feature * anchors + anchor]
        } else {
            values[anchor * features + feature]
        }
    };
    let mut found: Vec<(f32, [f32; BOX_VALUES])> = (0..anchors)
        .filter_map(|anchor| {
            let score =
                (BOX_VALUES..features).map(|feature| value(feature, anchor)).fold(0.0, f32::max);
            (score >= DETECTOR.confidence)
                .then(|| (score, std::array::from_fn(|feature| value(feature, anchor))))
        })
        .collect();
    found.sort_by(|left, right| right.0.total_cmp(&left.0));
    found
}

fn suppress(
    found: Vec<(f32, [f32; BOX_VALUES])>,
    unframe: impl Fn([f32; BOX_VALUES]) -> Bounds,
) -> Vec<Bounds> {
    let mut kept: Vec<Bounds> = Vec::new();
    for (_, values) in found {
        let bounds = unframe(values);
        if bounds.width() > 0.0
            && bounds.height() > 0.0
            && kept.iter().all(|other| other.overlap(&bounds) < DETECTOR.overlap)
        {
            kept.push(bounds);
        }
    }
    kept
}

fn decode_box(values: &[f32], size: [usize; 2]) -> Option<Bounds> {
    let [cx, cy, w, h] = <[f32; BOX_VALUES]>::try_from(values).ok()?;
    let (width, height) = (size[0] as f32, size[1] as f32);
    let bounds = Bounds {
        left: ((cx - w / 2.0) * width).clamp(0.0, width - 1.0),
        top: ((cy - h / 2.0) * height).clamp(0.0, height - 1.0),
        right: ((cx + w / 2.0) * width).clamp(0.0, width - 1.0),
        bottom: ((cy + h / 2.0) * height).clamp(0.0, height - 1.0),
    };
    (bounds.width() > 0.0 && bounds.height() > 0.0).then_some(bounds)
}

fn sigmoid(logit: f32) -> f32 {
    1.0 / (1.0 + (-logit).exp())
}

fn planar(image: &RgbImage, padded: [usize; 2], normalize: impl Fn(usize, f32) -> f32) -> Vec<f32> {
    let (width, height) = (image.width() as usize, image.height() as usize);
    let plane = padded[0] * padded[1];
    let mut pixels = vec![0.0; CHANNELS * plane];
    for y in 0..padded[1] {
        for x in 0..padded[0] {
            let pixel = image.get_pixel(x.min(width - 1) as u32, y.min(height - 1) as u32);
            for channel in 0..CHANNELS {
                let value = f32::from(pixel[channel]) / f32::from(u8::MAX);
                pixels[channel * plane + y * padded[0] + x] = normalize(channel, value);
            }
        }
    }
    pixels
}

#[cfg(test)]
#[path = "models_tests.rs"]
mod tests;
