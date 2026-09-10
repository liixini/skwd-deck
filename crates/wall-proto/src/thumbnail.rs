use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ThumbnailEncodeRequest {
    pub we_id: String,
    pub image: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ThumbnailEncodeResponse {
    pub error: Option<String>,
}
