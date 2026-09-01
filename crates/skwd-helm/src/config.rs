use std::path::PathBuf;

use serde_json::Value;

pub struct Config {
    data: Value,
    path: PathBuf,
}

impl Config {
    pub fn load() -> Self {
        let path = skwd_config::config_path();
        secure_file(&path);
        let data = std::fs::read_to_string(&path).map_or_else(
            |_| Value::Object(serde_json::Map::new()),
            |text| {
                serde_json::from_str(&text).unwrap_or_else(|_| {
                    let backup = path.with_extension("json.corrupt");
                    let _ = std::fs::copy(&path, &backup);
                    secure_file(&backup);
                    Value::Object(serde_json::Map::new())
                })
            },
        );
        Self { data, path }
    }

    #[cfg(test)]
    pub fn from_data(data: Value) -> Self {
        Self { data, path: std::env::temp_dir().join("skwd_helm_test.json") }
    }

    pub fn wallpaper_dir(&self) -> String {
        skwd_config::wallpaper_dir(&self.data)
    }

    pub fn video_dir(&self) -> String {
        skwd_config::video_dir(&self.data)
    }

    pub fn cache_dir(&self) -> String {
        skwd_config::cache_dir_of(&self.data)
    }

    pub fn root(&self) -> &Value {
        &self.data
    }

    #[cfg(test)]
    pub fn str_path(&self, path: &str) -> String {
        if path == skwd_config::keys::we_render::ENGINE {
            return String::from("native");
        }
        if let Some(value) = skwd_config::get(&self.data, path) {
            if let Some(text) = value.as_str() {
                return text.to_string();
            }
            if let Some(number) = value.as_f64() {
                return format_number(number);
            }
        }
        skwd_config::schema::text_default(path).map_or_else(
            || skwd_config::schema::number_default(path).map_or_else(String::new, format_number),
            str::to_string,
        )
    }

    pub fn set_key(&mut self, path: &str, value: Value) {
        let parts: Vec<&str> = path.split('.').filter(|part| !part.is_empty()).collect();
        let Some((last, parents)) = parts.split_last() else { return };
        if !self.data.is_object() {
            self.data = Value::Object(serde_json::Map::new());
        }
        let mut current = &mut self.data;
        for part in parents {
            let Some(next) = descend(current, part) else { return };
            current = next;
        }
        put(current, last, value);
    }

    pub fn remove_key(&mut self, path: &str) {
        let parts: Vec<&str> = path.split('.').filter(|part| !part.is_empty()).collect();
        let Some((last, parents)) = parts.split_last() else { return };
        let mut current = &mut self.data;
        for part in parents {
            let Some(next) = current.as_object_mut().and_then(|object| object.get_mut(*part))
            else {
                return;
            };
            current = next;
        }
        if let Some(object) = current.as_object_mut() {
            object.remove(*last);
        }
    }

    pub fn persist(&self) -> std::io::Result<()> {
        let mut data = self.data.clone();
        skwd_config::canonicalize_paper_engine(&mut data);
        skwd_config::canonicalize_we_renderer(&mut data);
        let text = serde_json::to_string_pretty(&data).map_err(std::io::Error::other)?;
        if let Some(directory) = self.path.parent() {
            std::fs::create_dir_all(directory)?;
        }
        skwd_config::atomic_write_mode(&self.path, format!("{text}\n").as_bytes(), Some(0o600))
    }
}

fn descend<'a>(current: &'a mut Value, part: &str) -> Option<&'a mut Value> {
    if current.is_array() {
        return current.as_array_mut()?.get_mut(part.parse::<usize>().ok()?);
    }
    if !current.is_object() {
        *current = Value::Object(serde_json::Map::new());
    }
    Some(
        current
            .as_object_mut()?
            .entry(part.to_string())
            .or_insert_with(|| Value::Object(serde_json::Map::new())),
    )
}

fn put(current: &mut Value, last: &str, value: Value) {
    if current.is_array() {
        if let Some(slot) = last
            .parse::<usize>()
            .ok()
            .and_then(|index| current.as_array_mut().and_then(|array| array.get_mut(index)))
        {
            *slot = value;
        }
        return;
    }
    if !current.is_object() {
        *current = Value::Object(serde_json::Map::new());
    }
    if let Some(object) = current.as_object_mut() {
        object.insert(last.to_string(), value);
    }
}

#[cfg(test)]
fn format_number(value: f64) -> String {
    if value.fract() == 0.0 { format!("{value:.0}") } else { value.to_string() }
}

#[cfg(unix)]
fn secure_file(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let Ok(metadata) = std::fs::symlink_metadata(path) else { return };
    if metadata.file_type().is_file() && metadata.permissions().mode() & 0o7777 != 0o600 {
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
}

#[cfg(not(unix))]
fn secure_file(_path: &std::path::Path) {}
