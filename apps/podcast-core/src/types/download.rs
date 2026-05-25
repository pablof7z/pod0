use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DownloadState {
    NotDownloaded,
    Queued,
    Downloading {
        progress: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        bytes_written: Option<i64>,
    },
    Downloaded {
        local_file_url: Url,
        byte_count: i64,
    },
    Failed {
        message: String,
    },
}

impl Default for DownloadState {
    fn default() -> Self {
        Self::NotDownloaded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_downloaded_round_trip() {
        let value = DownloadState::NotDownloaded;
        let json = serde_json::to_string(&value).unwrap();
        let back: DownloadState = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }

    #[test]
    fn downloading_round_trip() {
        let value = DownloadState::Downloading {
            progress: 0.33,
            bytes_written: Some(1024),
        };
        let json = serde_json::to_string(&value).unwrap();
        let back: DownloadState = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }

    #[test]
    fn downloaded_round_trip() {
        let value = DownloadState::Downloaded {
            local_file_url: Url::parse("file:///tmp/ep.mp3").unwrap(),
            byte_count: 4096,
        };
        let json = serde_json::to_string(&value).unwrap();
        let back: DownloadState = serde_json::from_str(&json).unwrap();
        assert_eq!(value, back);
    }
}
