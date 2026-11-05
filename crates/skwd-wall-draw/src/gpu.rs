use std::ffi::c_int;

use anyhow::{Result, bail, ensure};
use ash::vk;
use libloading::os::unix::{Library, Symbol};
use skwd_config::GPU_UUID_BYTES;

pub type Uuid = [u8; GPU_UUID_BYTES];

const CUDA_LIBRARY: &str = "libcuda.so.1";
const CU_INIT: &str = "cuInit";
const CU_COUNT: &str = "cuDeviceGetCount";
const CU_DEVICE: &str = "cuDeviceGet";
const CU_UUID: &str = "cuDeviceGetUuid";
const CUDA_SUCCESS: c_int = 0;
const DAWN_ORDER: [vk::PhysicalDeviceType; 3] = [
    vk::PhysicalDeviceType::DISCRETE_GPU,
    vk::PhysicalDeviceType::INTEGRATED_GPU,
    vk::PhysicalDeviceType::CPU,
];

type CuInit = unsafe extern "C" fn(u32) -> c_int;
type CuCount = unsafe extern "C" fn(*mut c_int) -> c_int;
type CuDevice = unsafe extern "C" fn(*mut c_int, c_int) -> c_int;
type CuUuid = unsafe extern "C" fn(*mut Uuid, c_int) -> c_int;

pub fn configured() -> Option<Uuid> {
    skwd_config::gpu_uuid(&std::env::var(skwd_config::GPU_DEVICE_ENV).ok()?)
}

pub fn cuda_ordinal(uuid: &Uuid) -> Result<Option<c_int>> {
    let position = cuda_uuids()?.iter().position(|listed| listed == uuid);
    Ok(position.map(c_int::try_from).transpose()?)
}

pub fn webgpu_adapter(uuid: &Uuid) -> Result<Option<c_int>> {
    dawn_index(&vulkan_devices()?, uuid)
}

fn cuda_uuids() -> Result<Vec<Uuid>> {
    let library = unsafe {
        Library::open(Some(CUDA_LIBRARY), libc::RTLD_NOW | libc::RTLD_LOCAL | libc::RTLD_NODELETE)
    }?;
    let check = |code: c_int, call: &str| {
        if code != CUDA_SUCCESS {
            bail!("{call} returned {code}");
        }
        Ok(())
    };
    unsafe {
        let init: Symbol<CuInit> = library.get(CU_INIT.as_bytes())?;
        let count: Symbol<CuCount> = library.get(CU_COUNT.as_bytes())?;
        let device: Symbol<CuDevice> = library.get(CU_DEVICE.as_bytes())?;
        let uuid: Symbol<CuUuid> = library.get(CU_UUID.as_bytes())?;
        check(init(0), CU_INIT)?;
        let mut total = 0;
        check(count(&raw mut total), CU_COUNT)?;
        (0..total)
            .map(|ordinal| {
                let (mut handle, mut id) = (0, [0; GPU_UUID_BYTES]);
                check(device(&raw mut handle, ordinal), CU_DEVICE)?;
                check(uuid(&raw mut id, handle), CU_UUID)?;
                Ok(id)
            })
            .collect()
    }
}

fn vulkan_devices() -> Result<Vec<(Uuid, vk::PhysicalDeviceType)>> {
    let entry = unsafe { ash::Entry::load() }?;
    let app = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_1);
    let info = vk::InstanceCreateInfo::default().application_info(&app);
    let instance = unsafe { entry.create_instance(&info, None) }?;
    let listed = unsafe { instance.enumerate_physical_devices() }.map(|devices| {
        devices
            .into_iter()
            .filter(|&device| {
                unsafe { instance.get_physical_device_properties(device) }.api_version
                    >= vk::API_VERSION_1_1
            })
            .map(|device| {
                let mut ids = vk::PhysicalDeviceIDProperties::default();
                let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut ids);
                unsafe { instance.get_physical_device_properties2(device, &mut properties) };
                let kind = properties.properties.device_type;
                (ids.device_uuid, kind)
            })
            .collect()
    });
    unsafe { instance.destroy_instance(None) };
    Ok(listed?)
}

fn dawn_index(devices: &[(Uuid, vk::PhysicalDeviceType)], uuid: &Uuid) -> Result<Option<c_int>> {
    let rank = |kind: &vk::PhysicalDeviceType| {
        DAWN_ORDER.iter().position(|each| each == kind).unwrap_or(DAWN_ORDER.len())
    };
    let Some((_, kind)) = devices.iter().find(|(listed, _)| listed == uuid) else {
        return Ok(None);
    };
    let class = rank(kind);
    let shared = devices.iter().filter(|(_, other)| rank(other) == class).count();
    ensure!(shared == 1, "another vulkan gpu shares its type, so it cannot be matched reliably");
    let before = devices.iter().filter(|(_, other)| rank(other) < class).count();
    Ok(Some(c_int::try_from(before)?))
}

#[cfg(test)]
#[path = "gpu_tests.rs"]
mod tests;
