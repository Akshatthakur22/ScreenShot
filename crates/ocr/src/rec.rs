//! Text recognition. Each detected line is cut out, squashed to 48 px tall,
//! and the model returns, for every thin vertical slice of it, a probability
//! for each of its ~18k characters. CTC decoding turns that into a string.

use anyhow::{Context, Result};
use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer};
use image::RgbImage;
use ort::{session::Session, value::Tensor};

use crate::det::Region;

const HEIGHT: u32 = 48;
// Paddle never makes a batch narrower than 320 px, short words get padded.
const MIN_WIDTH: u32 = 320;

// The output is batch * steps * 18710 floats, one step per 8 px of width. That
// vocabulary is what makes it big: 32 long lines at once is a quarter gigabyte
// just for the scores. So batches are capped by total width, which keeps that
// tensor around 15 MB, instead of by a line count. Going from 6400 down to
// this was no slower on my test set.
const BATCH_WIDTH: u32 = 1600;
const MAX_BATCH: usize = 32;

/// The model's own alphabet, stored in the onnx metadata. Index 0 is the CTC
/// blank and the space character is appended at the end, same as paddle.
pub fn alphabet(session: &Session) -> Result<Vec<String>> {
    let meta = session.metadata()?;
    let chars = meta
        .custom("character")
        .context("rec model has no character list")?;
    let mut out = vec![String::new()];
    out.extend(chars.lines().map(str::to_owned));
    out.push(" ".into());
    Ok(out)
}

pub fn recognize(
    session: &mut Session,
    alphabet: &[String],
    img: &RgbImage,
    regions: &[Region],
) -> Result<Vec<(String, f32)>> {
    let mut results = vec![(String::new(), 0.0); regions.len()];

    // Similar widths go in the same batch so little of it is padding.
    let ratio = |r: &Region| r.width() as f32 / r.height() as f32;
    let mut order: Vec<usize> = (0..regions.len()).collect();
    order.sort_by(|&a, &b| ratio(&regions[a]).total_cmp(&ratio(&regions[b])));

    let mut resizer = Resizer::new();
    let mut start = 0;
    while start < order.len() {
        // Sorted ascending, so the last line in a batch sets its width.
        let mut end = start + 1;
        while end < order.len()
            && end - start < MAX_BATCH
            && (end - start + 1) as u32 * batch_width(ratio(&regions[order[end]])) <= BATCH_WIDTH
        {
            end += 1;
        }
        let batch = &order[start..end];
        let width = batch_width(ratio(&regions[batch[batch.len() - 1]]));

        let input = batch_tensor(&mut resizer, img, regions, batch, width)?;
        let outputs = session.run(ort::inputs![input])?;
        let (shape, probs) = outputs[0].try_extract_tensor::<f32>()?;
        let (steps, classes) = (shape[1] as usize, shape[2] as usize);

        for (n, &i) in batch.iter().enumerate() {
            let slice = &probs[n * steps * classes..(n + 1) * steps * classes];
            results[i] = ctc_decode(slice, classes, alphabet);
        }
        start = end;
    }
    Ok(results)
}

fn batch_width(max_ratio: f32) -> u32 {
    ((HEIGHT as f32 * max_ratio).ceil() as u32).max(MIN_WIDTH)
}

fn batch_tensor(
    resizer: &mut Resizer,
    img: &RgbImage,
    regions: &[Region],
    batch: &[usize],
    width: u32,
) -> Result<Tensor<f32>> {
    let plane = (HEIGHT * width) as usize;
    // Padding stays 0.0, which is mid grey after normalisation, like paddle.
    let mut data = vec![0f32; batch.len() * 3 * plane];

    for (n, &i) in batch.iter().enumerate() {
        let r = regions[i];
        let w =
            ((HEIGHT as f32 * r.width() as f32 / r.height() as f32).ceil() as u32).clamp(1, width);
        let mut line = RgbImage::new(w, HEIGHT);
        let opts = ResizeOptions::new()
            .resize_alg(ResizeAlg::Convolution(FilterType::Bilinear))
            .crop(
                r.x0 as f64,
                r.y0 as f64,
                r.width() as f64,
                r.height() as f64,
            );
        resizer.resize(img, &mut line, &opts)?;

        let base = n * 3 * plane;
        for (x, y, px) in line.enumerate_pixels() {
            let at = (y * width + x) as usize;
            for c in 0..3 {
                data[base + c * plane + at] = px[c] as f32 / 127.5 - 1.0;
            }
        }
    }
    Ok(Tensor::from_array((
        [batch.len(), 3, HEIGHT as usize, width as usize],
        data,
    ))?)
}

/// Greedy CTC: take the likeliest class at every step, collapse runs of the
/// same class, drop blanks. "hh-e-ll-ll-o" becomes "hello", and the blank
/// between the two "ll" runs is what keeps the double l.
fn ctc_decode(probs: &[f32], classes: usize, alphabet: &[String]) -> (String, f32) {
    let mut text = String::new();
    let (mut sum, mut kept) = (0.0, 0);
    let mut prev = 0;

    for step in probs.chunks_exact(classes) {
        let (best, p) = step
            .iter()
            .copied()
            .enumerate()
            .fold(
                (0, f32::MIN),
                |acc, (i, p)| if p > acc.1 { (i, p) } else { acc },
            );
        if best != 0
            && best != prev
            && let Some(ch) = alphabet.get(best)
        {
            text.push_str(ch);
            sum += p;
            kept += 1;
        }
        prev = best;
    }

    let score = if kept == 0 { 0.0 } else { sum / kept as f32 };
    (text, score)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alphabet() -> Vec<String> {
        ["", "h", "e", "l", "o", " "]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    // one row per step, a 1.0 on the winning class
    fn steps(winners: &[usize]) -> Vec<f32> {
        let mut out = vec![0.0; winners.len() * 6];
        for (t, &w) in winners.iter().enumerate() {
            out[t * 6 + w] = 1.0;
        }
        out
    }

    #[test]
    fn collapses_runs_and_keeps_doubled_letters() {
        let probs = steps(&[1, 1, 0, 2, 0, 3, 3, 0, 3, 0, 4, 4]);
        assert_eq!(ctc_decode(&probs, 6, &alphabet()), ("hello".into(), 1.0));
    }

    #[test]
    fn all_blank_is_empty_with_zero_score() {
        assert_eq!(
            ctc_decode(&steps(&[0, 0, 0]), 6, &alphabet()),
            (String::new(), 0.0)
        );
    }

    #[test]
    fn score_is_the_mean_of_kept_steps() {
        let mut probs = steps(&[1, 2]);
        probs[1] = 0.5;
        probs[6 + 2] = 0.7;
        let (text, score) = ctc_decode(&probs, 6, &alphabet());
        assert_eq!(text, "he");
        assert!((score - 0.6).abs() < 1e-6);
    }

    #[test]
    fn batch_width_never_goes_under_paddles_minimum() {
        assert_eq!(batch_width(0.5), 320);
        assert_eq!(batch_width(10.0), 480);
    }
}
