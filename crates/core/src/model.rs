use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mark {
    Approved,
    Delete,
    Unsure,
}

impl Mark {
    pub fn as_str(self) -> &'static str {
        match self {
            Mark::Approved => "approved",
            Mark::Delete => "delete",
            Mark::Unsure => "unsure",
        }
    }

    pub fn parse(text: &str) -> Option<Mark> {
        match text {
            "approved" => Some(Mark::Approved),
            "delete" => Some(Mark::Delete),
            "unsure" => Some(Mark::Unsure),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub mark: Option<Mark>,
    pub favorite: bool,
    pub rating: u8,
}

impl Decision {
    pub fn is_empty(&self) -> bool {
        self.mark.is_none() && !self.favorite && self.rating == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Photo {
    pub id: String,
    pub dir: PathBuf,
    pub stem: String,
    pub raf: Option<PathBuf>,
    pub jpg: Option<PathBuf>,
    pub size: u64,
    pub modified_ms: i64,
    pub fingerprint: String,
    pub processed_fingerprint: Option<String>,
    pub capture_ms: Option<i64>,
    pub orientation: u16,
    pub dhash: Option<u64>,
    pub camera_rating: Option<u8>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub error: Option<String>,
    pub decision: Decision,
}

impl Photo {
    pub fn time_ms(&self) -> i64 {
        self.capture_ms.unwrap_or(self.modified_ms)
    }

    pub fn is_processed(&self) -> bool {
        self.processed_fingerprint.as_deref() == Some(self.fingerprint.as_str())
    }

    pub fn name(&self) -> String {
        self.jpg
            .as_ref()
            .or(self.raf.as_ref())
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.stem.clone())
    }
}
