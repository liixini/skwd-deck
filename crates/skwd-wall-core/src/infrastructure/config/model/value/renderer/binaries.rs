use super::RendererConfig;

fn colocated(name: &str) -> Option<String> {
    let executable = std::env::current_exe().ok()?;
    let candidate = executable.parent()?.join(name);
    if candidate.exists() { Some(candidate.display().to_string()) } else { None }
}

impl RendererConfig<'_> {
    fn resolved_bin(self, environment_key: &str, config_path: &str, name: &str) -> String {
        if let Some(path) = skwd_config::env(environment_key) {
            return path;
        }
        let value = self.config.str_at(config_path, "");
        if !value.is_empty() {
            return self.config.resolve(&value);
        }
        colocated(name).unwrap_or_else(|| name.to_string())
    }

    pub fn still_bin(&self) -> String {
        self.resolved_bin(
            "SKWD_WALL_PAPER_STILL",
            skwd_config::keys::paths::PAPER_STILL_BIN,
            "skwd-wall-still",
        )
    }

    pub fn paper_bin(&self) -> String {
        if let Some(path) = skwd_config::env("SKWD_PAPER_BIN") {
            return path;
        }
        let configured = self.config.str_at(skwd_config::keys::paths::PAPER_BIN, "");
        if configured.is_empty() {
            colocated("skwd-paper")
                .unwrap_or_else(|| crate::paths::paper_bin().display().to_string())
        } else {
            self.config.resolve(&configured)
        }
    }

    pub fn vk_bin(&self) -> String {
        self.resolved_bin(
            "SKWD_WALL_PAPER_VK",
            skwd_config::keys::paths::PAPER_VK_BIN,
            "skwd-wall-vk",
        )
    }
}
