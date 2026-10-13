use super::Config;

impl Config {
    pub fn excluded_folders(&self) -> Vec<String> {
        skwd_config::excluded_folders(self.root())
    }

    pub fn library_policy(&self) -> skwd_config::LibraryPolicy {
        skwd_config::LibraryPolicy::new(self.root())
    }

    pub fn library_profile(&self) -> Option<skwd_config::LibraryProfile> {
        skwd_config::active_library_profile(self.root())
    }
}
