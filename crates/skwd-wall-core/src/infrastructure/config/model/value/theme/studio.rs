use serde_json::Value;

use super::ThemeConfig;

impl ThemeConfig<'_> {
    pub fn studio_area(&self) -> Option<wall_proto::StudioArea> {
        let edges: Vec<f32> = self
            .config
            .get(skwd_config::keys::theme::STUDIO_AREA)
            .and_then(Value::as_array)?
            .iter()
            .filter_map(Value::as_f64)
            .map(|edge| edge as f32)
            .collect();
        wall_proto::StudioArea::from_edges(&edges)
    }

    pub fn studio_shades(&self) -> Vec<i64> {
        self.config
            .get(skwd_config::keys::theme::STUDIO_SHADES)
            .and_then(Value::as_array)
            .map(|steps| steps.iter().map(|step| step.as_i64().unwrap_or(0)).collect())
            .unwrap_or_default()
    }
}
