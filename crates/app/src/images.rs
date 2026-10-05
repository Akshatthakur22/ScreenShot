//! Decoded images, least recently used first out.
//!
//! This used to go through gpui's own image loader behind an LRU cache, and
//! memory climbed about 9 MB per page of scrolling without ever coming back,
//! even with nothing painted and every cached image provably released. Doing
//! the decode here means every reference to a picture is ours, and dropping
//! one out of the cache actually frees it. notes.md has the measurements.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{App, RenderImage, Window};
use image::{Frame, imageops::FilterType};
use smallvec::smallvec;

pub enum Lookup {
    Ready(Arc<RenderImage>),
    Pending,
    /// Not seen before, decode it at no more than this many pixels a side.
    Start(u32),
}

enum Slot {
    Loading,
    Ready(Arc<RenderImage>),
    Failed,
}

pub struct Images {
    slots: HashMap<PathBuf, Slot>,
    /// Least recently used at the front.
    order: VecDeque<PathBuf>,
    capacity: usize,
    /// Anything bigger gets scaled down on decode. A 28 megapixel scroll
    /// capture would otherwise be a 112 MB texture.
    max_side: u32,
}

impl Images {
    pub fn new(capacity: usize, max_side: u32) -> Self {
        Self {
            slots: HashMap::new(),
            order: VecDeque::new(),
            capacity,
            max_side,
        }
    }

    /// The image if it's ready. `Start` means the caller should kick off a
    /// decode for it, which happens once per path.
    pub fn get(&mut self, path: &Path, window: &mut Window, cx: &mut App) -> Lookup {
        if let Some(slot) = self.slots.get(path) {
            let found = match slot {
                Slot::Ready(image) => Lookup::Ready(image.clone()),
                Slot::Loading | Slot::Failed => Lookup::Pending,
            };
            self.touch(path);
            return found;
        }

        self.slots.insert(path.to_owned(), Slot::Loading);
        self.order.push_back(path.to_owned());
        while self.order.len() > self.capacity {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(Slot::Ready(image)) = self.slots.remove(&oldest) {
                // Frees the texture on the gpu, the pixels go with the Arc.
                cx.drop_image(image, Some(window));
            }
        }
        Lookup::Start(self.max_side)
    }

    /// Called when a decode finishes. If the slot was evicted in the meantime
    /// the image is just dropped.
    pub fn finish(&mut self, path: &Path, image: Option<Arc<RenderImage>>) {
        if let Some(slot) = self.slots.get_mut(path) {
            *slot = match image {
                Some(image) => Slot::Ready(image),
                None => Slot::Failed,
            };
        }
    }

    /// Drops everything but the `keep` most recently used.
    pub fn shrink_to(&mut self, keep: usize, cx: &mut App) {
        while self.order.len() > keep {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(Slot::Ready(image)) = self.slots.remove(&oldest) {
                cx.drop_image(image, None);
            }
        }
    }

    fn touch(&mut self, path: &Path) {
        if self.order.back().is_some_and(|p| p == path) {
            return;
        }
        if let Some(at) = self.order.iter().position(|p| p == path) {
            let p = self
                .order
                .remove(at)
                .expect("position came from the same deque");
            self.order.push_back(p);
        }
    }
}

/// Same cap as the indexer: nothing that needs more than 256 MB to decode,
/// so one enormous image can't take a small laptop down with it. Sniffs the
/// format from the contents, not the extension.
fn open_image(path: &Path) -> Option<image::DynamicImage> {
    let mut reader = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().ok()
}

/// Redraws a grid thumbnail from its screenshot, cut the same way the
/// indexer cuts them (`akshat_core::tile_crop`, 480 wide, jpeg). Failing
/// just means the tile stays blank, so errors are dropped.
pub fn write_thumbnail(original: &Path, dest: &Path) {
    let Some(img) = open_image(original) else {
        return;
    };
    let (iw, ih) = (img.width(), img.height());
    let crop = akshat_core::tile_crop(iw, ih);
    let (cx, cy) = ((crop.x * iw as f32) as u32, (crop.y * ih as f32) as u32);
    let (cw, ch) = ((crop.w * iw as f32) as u32, (crop.h * ih as f32) as u32);
    let w = 480.min(cw).max(1);
    let h = ((w as f32 * ch as f32 / cw.max(1) as f32).round() as u32).max(1);
    let thumb = img
        .crop_imm(cx, cy, cw, ch)
        .resize_exact(w, h, FilterType::Triangle)
        .into_rgb8();

    let Some(dir) = dest.parent() else { return };
    let _ = std::fs::create_dir_all(dir);
    let tmp = dest.with_extension("tmp");
    let Ok(file) = std::fs::File::create(&tmp) else {
        return;
    };
    let mut out = std::io::BufWriter::new(file);
    let encoded =
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 78).encode_image(&thumb);
    drop(out);
    if encoded.is_ok() {
        let _ = std::fs::rename(&tmp, dest);
    }
}

/// Runs on a background thread.
pub fn decode(path: &Path, max_side: u32) -> Option<Arc<RenderImage>> {
    let mut img = open_image(path)?;
    if img.width().max(img.height()) > max_side {
        img = img.resize(max_side, max_side, FilterType::Triangle);
    }
    let mut pixels = img.into_rgba8();
    // gpui wants BGRA.
    for px in pixels.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(smallvec![Frame::new(pixels)])))
}
