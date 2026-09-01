use crate::backend::wallpaper::ApplyRuntime;

impl ApplyRuntime {
    pub fn persisted_uniform(
        &self,
        cache_dir: &str,
        live_outputs: &[String],
        kind: &str,
        path: &str,
        we_id: &str,
    ) -> bool {
        let recorded = super::read_state(cache_dir);
        let Some(map) = recorded.as_object() else {
            return false;
        };
        let matches = |entry: &serde_json::Value| {
            entry.get("type").and_then(serde_json::Value::as_str) == Some(kind)
                && if kind == wall_proto::kind::WE {
                    entry.get("we_id").and_then(serde_json::Value::as_str).unwrap_or("") == we_id
                } else {
                    entry.get("path").and_then(serde_json::Value::as_str).unwrap_or("") == path
                }
        };
        if live_outputs.is_empty() {
            return map.get("*").is_some_and(matches);
        }
        live_outputs
            .iter()
            .all(|output| map.get(output).or_else(|| map.get("*")).is_some_and(matches))
    }

    pub fn current_source(
        &self,
        cache_dir: &str,
        live_outputs: &[String],
        output: &str,
    ) -> Option<String> {
        self.current_source_we(cache_dir, live_outputs, output, None)
    }

    pub fn current_source_we(
        &self,
        cache_dir: &str,
        live_outputs: &[String],
        output: &str,
        we_dir: Option<&std::path::Path>,
    ) -> Option<String> {
        Self::source_we_in(&super::read_state(cache_dir), live_outputs, output, we_dir)
    }

    pub fn source_we_in(
        recorded: &serde_json::Value,
        live_outputs: &[String],
        output: &str,
        we_dir: Option<&std::path::Path>,
    ) -> Option<String> {
        let map = recorded.as_object()?;
        let key = if output == "*" {
            live_outputs.iter().find(|output| map.contains_key(output.as_str())).cloned()
        } else {
            Some(output.to_string())
        };
        let entry = key.and_then(|key| map.get(&key)).or_else(|| map.get("*"))?;
        let kind = entry.get("type").and_then(serde_json::Value::as_str)?;
        if kind == wall_proto::kind::WE {
            let we_id = entry
                .get("we_id")
                .and_then(serde_json::Value::as_str)
                .filter(|id| !id.is_empty())?;
            return crate::we::find_preview(&we_dir?.join(we_id))
                .map(|preview| preview.display().to_string());
        }
        if kind != wall_proto::kind::STATIC && kind != wall_proto::kind::VIDEO {
            return None;
        }
        let path = entry
            .get("path")
            .and_then(serde_json::Value::as_str)
            .filter(|path| !path.is_empty())?;
        std::path::Path::new(path).exists().then(|| path.to_string())
    }
}
