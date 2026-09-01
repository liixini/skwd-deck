pub mod kind {
    pub const STATIC: &str = "static";
    pub const VIDEO: &str = "video";
    pub const WE: &str = "we";
    pub const SHADER: &str = "shader";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RendererKind {
    #[default]
    Static,
    Video,
    We,
}

impl RendererKind {
    pub const fn wire(self) -> &'static str {
        match self {
            RendererKind::Static => kind::STATIC,
            RendererKind::Video => kind::VIDEO,
            RendererKind::We => kind::WE,
        }
    }

    pub fn from_wire(raw: &str) -> Self {
        match raw {
            kind::VIDEO => RendererKind::Video,
            kind::WE => RendererKind::We,
            _ => RendererKind::Static,
        }
    }
}

#[cfg(test)]
#[path = "renderer_tests.rs"]
mod tests;
