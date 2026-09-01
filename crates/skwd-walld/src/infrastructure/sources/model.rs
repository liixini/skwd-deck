#[derive(Debug, Clone, PartialEq)]
pub struct Attribution {
    pub text: String,
    pub link: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceResult {
    pub id: String,
    pub full_url: String,
    pub thumb_url: String,
    pub resolution: String,
    pub file_size: u64,
    pub title: String,
    pub track_url: String,
    pub attribution: Option<Attribution>,
    pub duration_secs: u64,
}

impl SourceResult {
    pub fn new(id: impl Into<String>, full_url: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            full_url: full_url.into(),
            thumb_url: String::new(),
            resolution: String::new(),
            file_size: 0,
            title: String::new(),
            track_url: String::new(),
            attribution: None,
            duration_secs: 0,
        }
    }
}

pub struct SourcePage {
    pub results: Vec<SourceResult>,
    pub last_page: u32,
    pub current_page: u32,
    pub next_cursor: Option<String>,
}
