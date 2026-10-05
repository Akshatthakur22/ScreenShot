//! Text detection. The model paints a probability map, "how likely is this
//! pixel part of a text line", and everything after that is turning blobs on
//! that map into boxes.

use anyhow::Result;
use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer};
use image::RgbImage;
use ort::{session::Session, value::Tensor};

// Same numbers PaddleOCR ships with for its DB post processing.
const THRESH: f32 = 0.3;
const BOX_THRESH: f32 = 0.5;
const UNCLIP_RATIO: f32 = 1.6;
const MIN_SIZE: f32 = 3.0;

const MAX_PIXELS: u32 = 1280 * 800;

/// A detected line in source image pixels, x0 y0 inclusive, x1 y1 exclusive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Region {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

impl Region {
    pub fn width(&self) -> u32 {
        self.x1 - self.x0
    }
    pub fn height(&self) -> u32 {
        self.y1 - self.y0
    }
}

pub fn detect(session: &mut Session, img: &RgbImage) -> Result<Vec<Region>> {
    let (w, h) = input_size(img.width(), img.height());
    let mut resized = RgbImage::new(w, h);
    Resizer::new().resize(
        img,
        &mut resized,
        &ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear)),
    )?;

    let input = Tensor::from_array(([1usize, 3, h as usize, w as usize], to_chw(&resized)))?;
    let outputs = session.run(ort::inputs!["x" => input])?;
    let (_, prob) = outputs[0].try_extract_tensor::<f32>()?;

    let map = ProbMap {
        w: w as usize,
        h: h as usize,
        prob,
    };
    let sx = img.width() as f32 / w as f32;
    let sy = img.height() as f32 / h as f32;
    Ok(regions(&map, sx, sy, img.width(), img.height()))
}

/// Picks the size the detector sees, both sides a multiple of 32 as the
/// network requires.
///
/// Paddle does the opposite of this. It upscales until the short side is 736
/// px and never scales down, which suits photos of documents. Screenshot text
/// is already rendered at a size meant to be read, so upscaling a 586x134 crop
/// 5.5x bought nothing but a 330 MB peak, and a 4K shot at full size was most
/// of the per shot time. Only scaling down, to at most 1080p worth of pixels,
/// read every word the paddle sizing did on my test set. See notes.md.
fn input_size(w: u32, h: u32) -> (u32, u32) {
    let (w, h) = (w as f32, h as f32);
    let scale = (MAX_PIXELS as f32 / (w * h)).sqrt().min(1.0);
    let round32 = |v: f32| (((v * scale) / 32.0).round() as u32).max(1) * 32;
    (round32(w), round32(h))
}

/// HWC u8 to CHW f32, scaled to [-1, 1].
pub(crate) fn to_chw(img: &RgbImage) -> Vec<f32> {
    let plane = (img.width() * img.height()) as usize;
    let mut out = vec![0f32; plane * 3];
    for (i, px) in img.pixels().enumerate() {
        for c in 0..3 {
            out[c * plane + i] = px[c] as f32 / 127.5 - 1.0;
        }
    }
    out
}

struct ProbMap<'a> {
    w: usize,
    h: usize,
    prob: &'a [f32],
}

#[derive(Clone, Copy)]
struct Blob {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

/// Paddle traces each blob's contour and fits a rotated rectangle to it. Text
/// in screenshots is upright nearly always, so plain connected components with
/// an axis aligned box get the same answer without any of the geometry.
fn regions(map: &ProbMap, sx: f32, sy: f32, src_w: u32, src_h: u32) -> Vec<Region> {
    let bitmap = dilate(map);
    let mut out = Vec::new();

    for b in blobs(&bitmap, map.w, map.h) {
        // Box sizes follow opencv's minAreaRect, which measures between pixel
        // centres, hence no +1.
        let (bw, bh) = ((b.x1 - b.x0) as f32, (b.y1 - b.y0) as f32);
        if bw.min(bh) < MIN_SIZE {
            continue;
        }
        if mean_prob(map, &b) < BOX_THRESH {
            continue;
        }

        // The model is trained on shrunk text regions, so the blob is smaller
        // than the actual text. DB grows it back by area * ratio / perimeter.
        let d = bw * bh * UNCLIP_RATIO / (2.0 * (bw + bh));
        let (ew, eh) = (bw + 2.0 * d, bh + 2.0 * d);
        if ew.min(eh) < MIN_SIZE + 2.0 {
            continue;
        }

        let x0 = ((b.x0 as f32 - d) * sx).round().clamp(0.0, src_w as f32) as u32;
        let y0 = ((b.y0 as f32 - d) * sy).round().clamp(0.0, src_h as f32) as u32;
        let x1 = ((b.x1 as f32 + d) * sx).round().clamp(0.0, src_w as f32) as u32;
        let y1 = ((b.y1 as f32 + d) * sy).round().clamp(0.0, src_h as f32) as u32;
        if x1 > x0 && y1 > y0 {
            out.push(Region { x0, y0, x1, y1 });
        }
    }

    sort_reading_order(&mut out);
    out
}

/// Threshold plus a 2x2 dilation, which closes the hairline gaps the model
/// leaves inside a line so one line doesn't come out as three boxes.
fn dilate(map: &ProbMap) -> Vec<bool> {
    let (w, h) = (map.w, map.h);
    let on = |x: usize, y: usize| map.prob[y * w + x] > THRESH;
    let mut out = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            out[y * w + x] = on(x, y)
                || (x > 0 && on(x - 1, y))
                || (y > 0 && on(x, y - 1))
                || (x > 0 && y > 0 && on(x - 1, y - 1));
        }
    }
    out
}

/// 8-connected components by flood fill, keeping only each one's bounds.
fn blobs(bitmap: &[bool], w: usize, h: usize) -> Vec<Blob> {
    let mut seen = vec![false; w * h];
    let mut stack = Vec::new();
    let mut out = Vec::new();

    for start in 0..w * h {
        if !bitmap[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let mut b = Blob {
            x0: usize::MAX,
            y0: usize::MAX,
            x1: 0,
            y1: 0,
        };

        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            b.x0 = b.x0.min(x);
            b.y0 = b.y0.min(y);
            b.x1 = b.x1.max(x);
            b.y1 = b.y1.max(y);

            for (dx, dy) in [
                (-1, -1),
                (0, -1),
                (1, -1),
                (-1, 0),
                (1, 0),
                (-1, 1),
                (0, 1),
                (1, 1),
            ] {
                let (nx, ny) = (x as isize + dx, y as isize + dy);
                if nx < 0 || ny < 0 || nx >= w as isize || ny >= h as isize {
                    continue;
                }
                let j = ny as usize * w + nx as usize;
                if bitmap[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        out.push(b);
    }
    out
}

/// Paddle's "fast" score: the average probability over the box, not just over
/// the blob, so a box that is mostly background scores low.
fn mean_prob(map: &ProbMap, b: &Blob) -> f32 {
    let mut sum = 0.0;
    for y in b.y0..=b.y1 {
        sum += map.prob[y * map.w + b.x0..=y * map.w + b.x1]
            .iter()
            .sum::<f32>();
    }
    sum / ((b.x1 - b.x0 + 1) * (b.y1 - b.y0 + 1)) as f32
}

/// Top to bottom, and left to right for boxes sitting on the same line.
fn sort_reading_order(regions: &mut [Region]) {
    regions.sort_by_key(|r| (r.y0, r.x0));
    // Boxes on one visual line rarely share an exact top edge, so a plain sort
    // can put the right half of a line before its left half. One bubble pass
    // over near neighbours fixes that, same as paddle does.
    for i in 0..regions.len() {
        for j in (1..=i).rev() {
            let (a, b) = (regions[j - 1], regions[j]);
            let same_line = b.y0.abs_diff(a.y0) < 10;
            if same_line && b.x0 < a.x0 {
                regions.swap(j - 1, j);
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_is_a_multiple_of_32() {
        for (w, h) in [
            (1151, 477),
            (2568, 1428),
            (1, 2),
            (3840, 2160),
            (736, 736),
            (40, 9000),
        ] {
            let (iw, ih) = input_size(w, h);
            assert!(
                iw % 32 == 0 && ih % 32 == 0 && iw > 0 && ih > 0,
                "{w}x{h} -> {iw}x{ih}"
            );
            // rounding to 32 can push a little past the cap, never far
            assert!(
                iw * ih <= MAX_PIXELS / 10 * 11,
                "{w}x{h} -> {iw}x{ih} too big"
            );
        }
    }

    #[test]
    fn small_images_stay_put_big_ones_are_capped() {
        assert_eq!(input_size(1151, 477), (1152, 480));
        assert_eq!(input_size(1280, 800), (1280, 800));
        assert_eq!(input_size(1920, 1080), (1344, 768));
        assert_eq!(input_size(3840, 2160), (1344, 768));
    }

    fn map_with_rect(w: usize, h: usize, x0: usize, y0: usize, x1: usize, y1: usize) -> Vec<f32> {
        let mut prob = vec![0.0; w * h];
        for y in y0..=y1 {
            for x in x0..=x1 {
                prob[y * w + x] = 0.9;
            }
        }
        prob
    }

    #[test]
    fn one_blob_becomes_one_grown_box() {
        let prob = map_with_rect(100, 40, 10, 10, 60, 20);
        let map = ProbMap {
            w: 100,
            h: 40,
            prob: &prob,
        };
        let r = regions(&map, 1.0, 1.0, 100, 40);
        assert_eq!(r.len(), 1);
        // grown by 51*11*1.6/(2*62) ~ 7.2 px on every side (dilation adds 1 to the far edges)
        assert_eq!(
            r[0],
            Region {
                x0: 3,
                y0: 3,
                x1: 68,
                y1: 28
            }
        );
    }

    #[test]
    fn specks_and_faint_blobs_are_dropped() {
        let mut prob = map_with_rect(100, 40, 10, 10, 11, 11);
        for y in 25..35 {
            for x in 30..80 {
                prob[y * 100 + x] = 0.35;
            }
        }
        let map = ProbMap {
            w: 100,
            h: 40,
            prob: &prob,
        };
        assert!(regions(&map, 1.0, 1.0, 100, 40).is_empty());
    }

    #[test]
    fn boxes_scale_back_to_the_source_image() {
        let prob = map_with_rect(100, 40, 10, 10, 60, 20);
        let map = ProbMap {
            w: 100,
            h: 40,
            prob: &prob,
        };
        let r = regions(&map, 2.0, 2.0, 200, 80);
        assert_eq!(
            r[0],
            Region {
                x0: 6,
                y0: 6,
                x1: 136,
                y1: 56
            }
        );
    }

    #[test]
    fn same_line_reads_left_to_right() {
        let mut r = vec![
            Region {
                x0: 300,
                y0: 100,
                x1: 400,
                y1: 120,
            },
            Region {
                x0: 10,
                y0: 104,
                x1: 200,
                y1: 124,
            },
            Region {
                x0: 10,
                y0: 10,
                x1: 200,
                y1: 30,
            },
        ];
        sort_reading_order(&mut r);
        assert_eq!(r.iter().map(|r| r.x0).collect::<Vec<_>>(), [10, 10, 300]);
        assert_eq!(r[0].y0, 10);
    }
}
