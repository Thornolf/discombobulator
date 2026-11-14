use std::path::PathBuf;

use jiff::Timestamp;
use jiff::tz::TimeZone;
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
    pub has_decision: bool,
}

impl Photo {
    pub fn effective_decision(&self) -> Decision {
        if self.has_decision {
            return self.decision;
        }
        Decision {
            rating: self.camera_rating.unwrap_or(0),
            ..self.decision
        }
    }

    pub fn time_ms(&self) -> i64 {
        self.capture_ms
            .unwrap_or_else(|| local_wall_clock_ms(self.modified_ms))
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

pub fn shift_by_offset(utc_ms: i64, offset_seconds: i32) -> i64 {
    utc_ms.saturating_add(i64::from(offset_seconds) * 1000)
}

pub fn local_wall_clock_ms(utc_ms: i64) -> i64 {
    let offset_seconds = Timestamp::from_millisecond(utc_ms)
        .map(|timestamp| TimeZone::system().to_offset(timestamp).seconds())
        .unwrap_or(0);
    shift_by_offset(utc_ms, offset_seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::parse_exif_datetime;

    #[test]
    fn modified_time_is_shifted_into_local_wall_clock() {
        let paris_summer = 2 * 3600;
        let utc_ms = 1_791_787_330_000;
        let shifted = shift_by_offset(utc_ms, paris_summer);
        assert_eq!(
            Some(shifted / 1000),
            parse_exif_datetime("2026:10:12 08:42:10")
        );
        assert_eq!(shift_by_offset(utc_ms, -5 * 3600), utc_ms - 18_000_000);
        assert_eq!(shift_by_offset(i64::MAX, 3600), i64::MAX);
    }

    #[test]
    fn system_offset_shift_stays_within_a_day() {
        let utc_ms = 1_791_787_330_000;
        assert!((local_wall_clock_ms(utc_ms) - utc_ms).abs() <= 26 * 3_600_000);
    }
}
