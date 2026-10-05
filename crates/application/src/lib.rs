use std::fs;

use akshat_core::{Hit, Index};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Serialize;
use thiserror::Error;

const MAX_LIMIT: usize = 100;
const MAX_QUERY_BYTES: usize = 512;
const MAX_IMAGE_BYTES: u64 = 25 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("core initialization failed")]
    Initialization(#[source] anyhow::Error),
    #[error("search query is too long")]
    QueryTooLong,
    #[error("shot ID must be positive")]
    InvalidShotId,
    #[error("shot was not found in the index")]
    ShotNotFound,
    #[error("indexed screenshot is unavailable")]
    ImageUnavailable,
    #[error("indexed screenshot exceeds the display size limit")]
    ImageTooLarge,
    #[error("database operation failed")]
    Core(#[source] anyhow::Error),
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShotDto {
    pub id: i64,
    pub mtime: i64,
    pub width: u32,
    pub height: u32,
    pub ocr_snippet: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShotDetailDto {
    pub shot: ShotDto,
    pub ocr_lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StatsDto {
    pub screenshot_count: usize,
    pub line_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShotImageDto {
    pub mime_type: String,
    pub data_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    pub code: &'static str,
    pub message: &'static str,
}

impl From<&ApplicationError> for IpcError {
    fn from(error: &ApplicationError) -> Self {
        let (code, message) = match error {
            ApplicationError::Initialization(_) => (
                "core_initialization",
                "Could not initialize the local index.",
            ),
            ApplicationError::QueryTooLong => ("invalid_query", "Search query is too long."),
            ApplicationError::InvalidShotId => ("invalid_shot_id", "Shot ID must be positive."),
            ApplicationError::ShotNotFound => {
                ("shot_not_found", "The indexed screenshot was not found.")
            }
            ApplicationError::ImageUnavailable => (
                "image_unavailable",
                "The indexed screenshot file is unavailable.",
            ),
            ApplicationError::ImageTooLarge => {
                ("image_too_large", "The screenshot is too large to display.")
            }
            ApplicationError::Core(_) => ("database_error", "The local index operation failed."),
        };
        Self { code, message }
    }
}

pub struct Application {
    index: Index,
}

impl Application {
    pub fn open_default() -> Result<Self, ApplicationError> {
        Index::open_default()
            .map(|index| Self { index })
            .map_err(ApplicationError::Initialization)
    }

    pub fn from_index(index: Index) -> Self {
        Self { index }
    }

    pub fn stats(&self) -> Result<StatsDto, ApplicationError> {
        Ok(StatsDto {
            screenshot_count: self.index.visible_len().map_err(ApplicationError::Core)?,
            line_count: self.index.line_len().map_err(ApplicationError::Core)?,
        })
    }

    pub fn recent_shots(&self, limit: usize) -> Result<Vec<ShotDto>, ApplicationError> {
        self.index
            .find("", limit.clamp(1, MAX_LIMIT))
            .map_err(ApplicationError::Core)?
            .into_iter()
            .map(|hit| {
                let lines = self.index.lines(hit.id).map_err(ApplicationError::Core)?;
                Ok(shot_dto(
                    hit,
                    lines
                        .into_iter()
                        .take(2)
                        .map(|line| line.text)
                        .collect::<Vec<_>>()
                        .join(" · "),
                ))
            })
            .collect()
    }

    pub fn search_shots(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ShotDto>, ApplicationError> {
        if query.len() > MAX_QUERY_BYTES {
            return Err(ApplicationError::QueryTooLong);
        }
        self.index
            .search(query, limit.clamp(1, MAX_LIMIT))
            .map_err(ApplicationError::Core)
            .map(|hits| {
                hits.into_iter()
                    .map(|hit| {
                        let snippet = hit
                            .lines
                            .iter()
                            .map(|line| line.text.as_str())
                            .collect::<Vec<_>>()
                            .join(" · ");
                        shot_dto(hit, snippet)
                    })
                    .collect()
            })
    }

    pub fn shot(&self, id: i64) -> Result<ShotDto, ApplicationError> {
        if id <= 0 {
            return Err(ApplicationError::InvalidShotId);
        }
        self.index
            .get(id)
            .map_err(ApplicationError::Core)?
            .map(|hit| shot_dto(hit, String::new()))
            .ok_or(ApplicationError::ShotNotFound)
    }

    pub fn shot_detail(&self, id: i64) -> Result<ShotDetailDto, ApplicationError> {
        let mut shot = self.shot(id)?;
        let lines = self.index.lines(id).map_err(ApplicationError::Core)?;
        shot.ocr_snippet = lines
            .iter()
            .take(2)
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join(" · ");
        Ok(ShotDetailDto {
            shot,
            ocr_lines: lines.into_iter().map(|line| line.text).collect(),
        })
    }

    pub fn shot_thumbnail(&self, id: i64) -> Result<ShotImageDto, ApplicationError> {
        if id <= 0 {
            return Err(ApplicationError::InvalidShotId);
        }
        let path = self
            .index
            .get(id)
            .map_err(ApplicationError::Core)?
            .ok_or(ApplicationError::ShotNotFound)?
            .path;
        let thumbnail = akshat_core::thumb_path(&path).map_err(ApplicationError::Core)?;
        image_data(&thumbnail)
    }

    pub fn shot_image(&self, id: i64) -> Result<ShotImageDto, ApplicationError> {
        if id <= 0 {
            return Err(ApplicationError::InvalidShotId);
        }
        let path = self
            .index
            .get(id)
            .map_err(ApplicationError::Core)?
            .ok_or(ApplicationError::ShotNotFound)?
            .path;
        image_data(&path)
    }
}

fn image_data(path: &std::path::Path) -> Result<ShotImageDto, ApplicationError> {
    let metadata = fs::metadata(path).map_err(|_| ApplicationError::ImageUnavailable)?;
    if metadata.len() > MAX_IMAGE_BYTES {
        return Err(ApplicationError::ImageTooLarge);
    }
    let bytes = fs::read(&path).map_err(|_| ApplicationError::ImageUnavailable)?;
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err(ApplicationError::ImageTooLarge);
    }
    let mime_type = match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("jpg" | "jpeg") => "image/jpeg",
        _ => return Err(ApplicationError::ImageUnavailable),
    };
    Ok(ShotImageDto {
        mime_type: mime_type.to_owned(),
        data_url: format!("data:{mime_type};base64,{}", STANDARD.encode(bytes)),
    })
}

fn shot_dto(hit: Hit, ocr_snippet: String) -> ShotDto {
    ShotDto {
        id: hit.id,
        mtime: hit.mtime,
        width: hit.width,
        height: hit.height,
        ocr_snippet,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akshat_core::{Line, Rect, Shot};

    fn app() -> Application {
        let mut index = Index::open_in_memory().unwrap();
        index
            .insert(
                &Shot {
                    path: "/tmp/example.png".into(),
                    mtime: 42,
                    width: 120,
                    height: 80,
                },
                &[Line {
                    text: "phase two screenshot".into(),
                    rect: Rect {
                        x: 0.0,
                        y: 0.0,
                        w: 1.0,
                        h: 0.2,
                    },
                    score: 0.9,
                }],
            )
            .unwrap();
        Application::from_index(index)
    }

    #[test]
    fn empty_index_returns_empty_library_data() {
        let app = Application::from_index(Index::open_in_memory().unwrap());
        assert_eq!(app.stats().unwrap().screenshot_count, 0);
        assert!(app.recent_shots(10).unwrap().is_empty());
        assert!(app.search_shots("not indexed", 10).unwrap().is_empty());
    }

    #[test]
    fn core_initializes_and_stats_use_index_data() {
        let app = app();
        assert_eq!(
            app.stats().unwrap(),
            StatsDto {
                screenshot_count: 1,
                line_count: 1
            }
        );
    }

    #[test]
    fn recent_and_search_return_indexed_shots() {
        let app = app();
        let recent = app.recent_shots(10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].ocr_snippet, "phase two screenshot");
        let results = app.search_shots("phase two", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].ocr_snippet, "phase two screenshot");
        assert!(app.search_shots("missing", 10).unwrap().is_empty());
    }

    #[test]
    fn detail_returns_original_ocr_lines() {
        let detail = app().shot_detail(1).unwrap();
        assert_eq!(detail.shot.id, 1);
        assert_eq!(detail.ocr_lines, ["phase two screenshot"]);
    }

    #[test]
    fn invalid_or_missing_shot_returns_error() {
        let app = app();
        assert!(matches!(app.shot(0), Err(ApplicationError::InvalidShotId)));
        assert!(matches!(app.shot(100), Err(ApplicationError::ShotNotFound)));
    }

    #[test]
    fn dto_serialization_is_camel_case() {
        let json = serde_json::to_string(&app().stats().unwrap()).unwrap();
        assert!(json.contains("screenshotCount"));
        assert!(json.contains("lineCount"));
    }

    #[test]
    fn query_size_is_bounded() {
        assert!(matches!(
            app().search_shots(&"x".repeat(MAX_QUERY_BYTES + 1), 1),
            Err(ApplicationError::QueryTooLong)
        ));
    }
}
