use serde_json::Value;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use crate::{bool_true_unless_false, keys, str_at, u64_at};

pub const DEFAULT_BATTERY_FPS: u32 = 60;
pub const DEFAULT_BATTERY_VIDEO_IDLE_SECONDS: u32 = 120;
const POWER_SUPPLY_ROOT: &str = "/sys/class/power_supply";
static LAST_KNOWN_ON_BATTERY: AtomicBool = AtomicBool::new(false);
static POWER_SOURCE_SNAPSHOT: AtomicU8 = AtomicU8::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSourceState {
    OnBattery,
    ExternalPower,
    NoSystemBattery,
    Unknown,
}

impl PowerSourceState {
    pub const fn on_battery(self) -> Option<bool> {
        match self {
            Self::OnBattery => Some(true),
            Self::ExternalPower | Self::NoSystemBattery => Some(false),
            Self::Unknown => None,
        }
    }
}

pub fn battery_saver_enabled(root: &Value) -> bool {
    bool_true_unless_false(root, keys::performance::BATTERY_SAVER)
}

pub fn battery_fps(root: &Value) -> u32 {
    u64_at(root, keys::performance::BATTERY_FPS).unwrap_or(u64::from(DEFAULT_BATTERY_FPS)).min(360)
        as u32
}

pub fn battery_video_idle_seconds(root: &Value) -> u32 {
    u64_at(root, keys::performance::BATTERY_VIDEO_IDLE_SECONDS)
        .unwrap_or(u64::from(DEFAULT_BATTERY_VIDEO_IDLE_SECONDS))
        .min(u64::from(u32::MAX)) as u32
}

pub fn battery_wallpaper_performance(root: &Value) -> bool {
    crate::bool_false_unless_true(root, keys::performance::BATTERY_WALLPAPER_PERFORMANCE)
}

pub fn configured_gpu_preference(root: &Value) -> &'static str {
    match str_at(root, keys::performance::GPU_PREFERENCE, "auto").as_str() {
        "low" => "low",
        "high" => "high",
        "none" => "none",
        _ => "auto",
    }
}

pub fn effective_gpu_preference(root: &Value, on_battery: bool) -> &'static str {
    match configured_gpu_preference(root) {
        "auto" if battery_saver_enabled(root) && on_battery => "low",
        "auto" => "none",
        preference => preference,
    }
}

pub fn effective_picker_fps(root: &Value, on_battery: bool, configured: f32) -> f32 {
    if !battery_saver_enabled(root) || !on_battery {
        return configured;
    }
    let cap = battery_fps(root);
    if cap == 0 { configured } else { configured.min(cap as f32) }
}

pub fn effective_video_idle_seconds(root: &Value, on_battery: bool, configured: u32) -> u32 {
    if !battery_saver_enabled(root) || !on_battery {
        return configured;
    }
    let cap = battery_video_idle_seconds(root);
    match (configured, cap) {
        (_, 0) => configured,
        (0, battery) => battery,
        (normal, battery) => normal.min(battery),
    }
}

pub fn effective_wallpaper_performance(root: &Value, on_battery: bool, configured: bool) -> bool {
    configured || (battery_saver_enabled(root) && on_battery && battery_wallpaper_performance(root))
}

fn trimmed(path: &Path) -> std::io::Result<String> {
    std::fs::read_to_string(path).map(|value| value.trim().to_string())
}

pub fn power_source_state_at(root: &Path) -> PowerSourceState {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return PowerSourceState::Unknown,
    };
    let mut system_battery_present = false;
    let mut uncertain = false;

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                uncertain = true;
                continue;
            }
        };
        let path = entry.path();
        let supply_type = match trimmed(&path.join("type")) {
            Ok(supply_type) => supply_type,
            Err(_) => {
                uncertain = true;
                continue;
            }
        };
        if !supply_type.eq_ignore_ascii_case("battery") {
            continue;
        }

        match trimmed(&path.join("scope")) {
            Ok(scope) if scope.eq_ignore_ascii_case("device") => continue,
            Ok(scope) if scope.eq_ignore_ascii_case("system") => {}
            // Older kernel drivers omit scope; a missing scope is the system battery.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) | Err(_) => {
                uncertain = true;
                continue;
            }
        }

        system_battery_present = true;
        let status = match trimmed(&path.join("status")) {
            Ok(status) => status,
            Err(_) => {
                uncertain = true;
                continue;
            }
        };
        if status.eq_ignore_ascii_case("discharging") {
            return PowerSourceState::OnBattery;
        }
        if !status.eq_ignore_ascii_case("charging")
            && !status.eq_ignore_ascii_case("full")
            && !status.eq_ignore_ascii_case("not charging")
        {
            uncertain = true;
        }
    }

    if uncertain {
        PowerSourceState::Unknown
    } else if system_battery_present {
        PowerSourceState::ExternalPower
    } else {
        PowerSourceState::NoSystemBattery
    }
}

pub fn battery_percent_at(root: &Path) -> Option<u8> {
    let entries = std::fs::read_dir(root).ok()?;
    let mut total = 0_u32;
    let mut count = 0_u32;
    for entry in entries.flatten() {
        let path = entry.path();
        if !trimmed(&path.join("type")).is_ok_and(|value| value.eq_ignore_ascii_case("battery")) {
            continue;
        }
        match trimmed(&path.join("scope")) {
            Ok(scope) if scope.eq_ignore_ascii_case("device") => continue,
            Ok(scope) if scope.eq_ignore_ascii_case("system") => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) | Err(_) => continue,
        }
        let Some(percent) =
            trimmed(&path.join("capacity")).ok().and_then(|value| value.parse::<u8>().ok())
        else {
            continue;
        };
        if percent <= 100 {
            total += u32::from(percent);
            count += 1;
        }
    }
    if count == 0 { None } else { Some(((total + count / 2) / count) as u8) }
}

pub fn set_power_source_snapshot(source: PowerSourceState) {
    if let Some(on_battery) = source.on_battery() {
        LAST_KNOWN_ON_BATTERY.store(on_battery, Ordering::Relaxed);
        POWER_SOURCE_SNAPSHOT.store(if on_battery { 2 } else { 1 }, Ordering::Release);
    }
}

#[cfg(target_os = "linux")]
pub fn power_source_state() -> PowerSourceState {
    power_source_state_at(Path::new(POWER_SUPPLY_ROOT))
}

#[cfg(target_os = "linux")]
pub fn battery_percent() -> Option<u8> {
    battery_percent_at(Path::new(POWER_SUPPLY_ROOT))
}

#[cfg(not(target_os = "linux"))]
pub const fn battery_percent() -> Option<u8> {
    None
}

#[cfg(not(target_os = "linux"))]
pub const fn power_source_state() -> PowerSourceState {
    PowerSourceState::NoSystemBattery
}

fn on_battery_with_memory(source: PowerSourceState, last_known: &AtomicBool) -> bool {
    match source.on_battery() {
        Some(on_battery) => {
            last_known.store(on_battery, Ordering::Relaxed);
            on_battery
        }
        None => last_known.load(Ordering::Relaxed),
    }
}

pub fn on_battery_power() -> bool {
    match POWER_SOURCE_SNAPSHOT.load(Ordering::Acquire) {
        1 => return false,
        2 => return true,
        _ => {}
    }
    on_battery_with_memory(power_source_state(), &LAST_KNOWN_ON_BATTERY)
}

#[cfg(test)]
#[path = "power_tests.rs"]
mod tests;
