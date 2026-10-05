use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

use akshat_core::{Index, Shot};
use akshat_ocr::Ocr;
use anyhow::{Context, Result};
use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer};
use image::RgbImage;
use image::codecs::jpeg::JpegEncoder;

// Grid thumbnails. 480 wide is sharp in a ~240 px cell on a 2x screen, and at
// this quality one lands around 20 to 40 KB, so a 5000 shot library is a
// couple hundred MB of cache rather than a gigabyte.
const THUMB_WIDTH: u32 = 480;
const THUMB_QUALITY: u8 = 78;

// Below this it's an icon, a 1x2 test png, or a slip of the mouse.
const MIN_SIDE: u32 = 16;

const EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

pub enum Outcome {
    Unchanged,
    Thumbnail,
    Indexed { lines: usize, took: Duration },
    Hidden(String),
}

pub struct Indexer {
    pub index: Index,
    ocr: Ocr,
}

impl Indexer {
    pub fn new(threads: usize) -> Result<Self> {
        Ok(Self {
            index: Index::open_default()?,
            ocr: Ocr::new(threads)?,
        })
    }

    pub fn set_threads(&mut self, threads: usize) -> Result<()> {
        self.ocr = Ocr::new(threads)?;
        Ok(())
    }

    pub fn index_file(&mut self, path: &Path) -> Result<Outcome> {
        let mtime = mtime(path)?;
        if self.index.is_current(path, mtime)? {
            // The thumbnail cache can be wiped (or its format change) without
            // the text going stale, and redrawing one is ~50 ms against ~600
            // for OCR, so only the thumbnail gets redone.
            let thumb = akshat_core::thumb_path(path)?;
            if !thumb.exists() && self.index.is_visible(path)? {
                write_thumbnail(&akshat_ocr::load_image(path)?, &thumb)?;
                release_memory();
                return Ok(Outcome::Thumbnail);
            }
            return Ok(Outcome::Unchanged);
        }

        let t = Instant::now();
        let img = match akshat_ocr::load_image(path) {
            Ok(img) => img,
            Err(e) => return self.hide(path, mtime, format!("{e:#}")),
        };
        if img.width() < MIN_SIDE || img.height() < MIN_SIDE {
            return self.hide(
                path,
                mtime,
                format!("{}x{} is too small", img.width(), img.height()),
            );
        }

        let lines = self.ocr.read(&img)?;
        write_thumbnail(&img, &akshat_core::thumb_path(path)?)?;
        let shot = Shot {
            path: path.to_owned(),
            mtime,
            width: img.width(),
            height: img.height(),
        };
        self.index.insert(&shot, &lines)?;

        drop(img);
        release_memory();
        Ok(Outcome::Indexed {
            lines: lines.len(),
            took: t.elapsed(),
        })
    }

    /// Recorded as 0x0, so it's skipped next time but never shown.
    fn hide(&mut self, path: &Path, mtime: i64, why: String) -> Result<Outcome> {
        let shot = Shot {
            path: path.to_owned(),
            mtime,
            width: 0,
            height: 0,
        };
        self.index.insert(&shot, &[])?;
        Ok(Outcome::Hidden(why))
    }

    pub fn rename(&mut self, from: &Path, to: &Path) -> Result<bool> {
        if !self.index.rename(from, to)? {
            return Ok(false);
        }
        let (old, new) = (akshat_core::thumb_path(from)?, akshat_core::thumb_path(to)?);
        let _ = fs::rename(old, new);
        Ok(true)
    }

    pub fn forget(&mut self, path: &Path) -> Result<bool> {
        if let Ok(thumb) = akshat_core::thumb_path(path) {
            let _ = fs::remove_file(thumb);
        }
        self.index.remove(path)
    }
}

pub fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Every image under the given folders, newest first. Newest first because
/// the backfill of an existing library takes a while and the shots from this
/// week are the ones most likely to be searched for during it.
pub fn scan(dirs: &[PathBuf]) -> Vec<PathBuf> {
    // A set, since overlapping folders (~/Pictures and ~/Pictures/Screenshots)
    // would otherwise list the same file twice.
    let mut found = HashSet::new();
    let mut stack: Vec<PathBuf> = dirs.to_vec();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let hidden = entry.file_name().to_string_lossy().starts_with('.');
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            // Symlinked files are followed, symlinked folders aren't, one
            // pointing back up the tree would never finish.
            let file = kind.is_file() || (kind.is_symlink() && path.is_file());
            if kind.is_dir() && !hidden {
                stack.push(path);
            } else if file && is_image(&path) {
                found.insert(path);
            }
        }
    }
    let mut found: Vec<(i64, PathBuf)> = found
        .into_iter()
        .map(|p| (mtime(&p).unwrap_or(0), p))
        .collect();
    found.sort_by(|a, b| b.cmp(a));
    found.into_iter().map(|(_, p)| p).collect()
}

fn mtime(path: &Path) -> Result<i64> {
    let modified = fs::metadata(path)?.modified()?;
    Ok(modified
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0))
}

/// Cut to exactly what the grid tile shows, see `akshat_core::tile_crop`.
fn write_thumbnail(img: &RgbImage, dest: &Path) -> Result<()> {
    let (iw, ih) = (img.width(), img.height());
    let crop = akshat_core::tile_crop(iw, ih);
    let (cw, ch) = (crop.w * iw as f32, crop.h * ih as f32);
    let w = THUMB_WIDTH.min(cw as u32).max(1);
    let h = ((w as f32 * ch / cw).round() as u32).max(1);

    let mut thumb = RgbImage::new(w, h);
    let opts = ResizeOptions::new()
        .resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3))
        .crop(
            (crop.x * iw as f32) as f64,
            (crop.y * ih as f32) as f64,
            cw as f64,
            ch as f64,
        );
    Resizer::new().resize(img, &mut thumb, &opts)?;

    let dir = dest.parent().context("thumbnail path has no parent")?;
    fs::create_dir_all(dir)?;
    let tmp = dest.with_extension("tmp");
    let mut out = std::io::BufWriter::new(fs::File::create(&tmp)?);
    JpegEncoder::new_with_quality(&mut out, THUMB_QUALITY).encode_image(&thumb)?;
    drop(out);
    fs::rename(&tmp, dest)?;
    Ok(())
}

/// glibc keeps freed memory around for reuse. Right after OCR that's a couple
/// hundred MB of activations and decoded pixels nobody needs until the next
/// screenshot, which for the watcher can be hours away.
fn release_memory() {
    #[cfg(target_env = "gnu")]
    unsafe {
        libc::malloc_trim(0);
    }
}

/// Drops the process to idle CPU and IO priority, so indexing only ever runs
/// on time nothing else wants. Failing is fine, it just runs at normal priority.
pub fn become_idle() {
    #[cfg(target_os = "linux")]
    unsafe {
        let param = libc::sched_param { sched_priority: 0 };
        libc::sched_setscheduler(0, libc::SCHED_IDLE, &param);
        // ioprio_set(IOPRIO_WHO_PROCESS, self, IOPRIO_CLASS_IDLE << 13). libc
        // has no wrapper for it.
        libc::syscall(libc::SYS_ioprio_set, 1, 0, 3 << 13);
    }
}
