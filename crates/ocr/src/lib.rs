//! OCR for screenshots: PP-OCRv6 detection and recognition models running on
//! ONNX Runtime, with the pre and post processing written for this project.

mod det;
mod models;
mod rec;

use std::path::Path;
use std::sync::OnceLock;

use akshat_core::{Line, Rect};
use anyhow::{Context, Result};
use image::RgbImage;
use ort::session::{Session, builder::GraphOptimizationLevel};

pub use models::models_dir;

// Lines the recognizer isn't at least this sure about are mostly icons read as
// letters. Paddle's default.
const MIN_SCORE: f32 = 0.5;

pub struct Ocr {
    det: Session,
    rec: Session,
    alphabet: Vec<String>,
}

impl Ocr {
    /// Loads both models, downloading them on first use. `threads` is how many
    /// cores one screenshot may use.
    pub fn new(threads: usize) -> Result<Self> {
        load_runtime()?;
        let det = session(&models::ensure(&models::DET)?, threads)?;
        let rec = session(&models::ensure(&models::REC)?, threads)?;
        let alphabet = rec::alphabet(&rec)?;
        Ok(Self { det, rec, alphabet })
    }

    pub fn read(&mut self, img: &RgbImage) -> Result<Vec<Line>> {
        // Nothing readable fits in a sliver, and the models would choke on it.
        if img.width() < 8 || img.height() < 8 {
            return Ok(Vec::new());
        }
        let regions = det::detect(&mut self.det, img)?;
        let texts = rec::recognize(&mut self.rec, &self.alphabet, img, &regions)?;

        let (w, h) = (img.width() as f32, img.height() as f32);
        Ok(regions
            .iter()
            .zip(texts)
            .filter(|(_, (text, score))| *score >= MIN_SCORE && worth_keeping(text))
            .map(|(r, (text, score))| Line {
                text: text.trim().to_owned(),
                rect: Rect {
                    x: r.x0 as f32 / w,
                    y: r.y0 as f32 / h,
                    w: r.width() as f32 / w,
                    h: r.height() as f32 / h,
                },
                score,
            })
            .collect())
    }
}

/// UI icons get detected as text and come back as one confident character: a
/// bell reads as 白, a grid as 品, a hamburger menu as 三. A lone character is
/// never something anyone searches for, so those go.
fn worth_keeping(text: &str) -> bool {
    text.trim().chars().count() > 1
}

/// ONNX Runtime is loaded at run time rather than linked in, see
/// `models::runtime` for why. Once per process.
fn load_runtime() -> Result<()> {
    static LOADED: OnceLock<()> = OnceLock::new();
    if LOADED.get().is_some() {
        return Ok(());
    }
    let lib = models::runtime()?;
    ort::init_from(&lib)
        .map_err(|e| anyhow::anyhow!("loading {}: {e}", lib.display()))?
        .commit();
    let _ = LOADED.set(());
    Ok(())
}

fn session(model: &Path, threads: usize) -> Result<Session> {
    let build = || -> ort::Result<Session> {
        // No arena and no memory pattern: every screenshot is a different size,
        // so both just hold on to the peak of the biggest one forever.
        Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(threads)?
            .with_execution_providers([ort::ep::CPU::default()
                .with_arena_allocator(false)
                .build()])?
            .with_memory_pattern(false)?
            .commit_from_file(model)
    };
    build().with_context(|| format!("loading {}", model.display()))
}

pub fn load_image(path: &Path) -> Result<RgbImage> {
    let img = open_image(path).with_context(|| format!("decoding {}", path.display()))?;
    Ok(img.into_rgb8())
}

/// Anything that would need more than this to decode is refused rather than
/// loaded. 256 MB is a 64 megapixel image, far past any screen, and a stray
/// panorama or giant scan shouldn't be able to push a small laptop into swap.
const MAX_DECODE_BYTES: u64 = 256 * 1024 * 1024;

/// Opens an image by what's in the file, not its extension (a `.png` that's
/// really a jpeg still works), with a cap on how much memory decoding it may
/// take.
pub fn open_image(path: &Path) -> Result<image::DynamicImage> {
    let mut reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    Ok(reader.decode()?)
}
