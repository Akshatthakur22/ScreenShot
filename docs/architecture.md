# Architecture

- [Overview](#overview)
- [Text recognition](#text-recognition)
- [Index and search](#index-and-search)
- [Search window](#search-window)
- [Repository layout](#repository-layout)

## Overview

```
Screenshot saved to a watched folder
  1. The indexer is notified through inotify and runs at idle priority
  2. OCR extracts each line of text and its bounding box
  3. Lines are stored in SQLite, with a full-text index using the trigram tokenizer
  4. The search window queries the index on every keystroke
```

akshat does not capture screenshots. It indexes the folders that existing screenshot tools write to. An existing backlog is indexed newest first, since recent screenshots are the most likely to be searched for.

## Text recognition

akshat uses [PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR)'s PP-OCRv6 models (tiny detector, small recognizer), executed on the CPU by [ONNX Runtime](https://onnxruntime.ai) through the [`ort`](https://crates.io/crates/ort) crate. The pipeline around the models is implemented in `crates/ocr`: extracting text boxes from the detector's probability map, cropping each line, batching lines through the recognizer, and CTC decoding.

### ONNX Runtime loading

ONNX Runtime is loaded dynamically at runtime from Microsoft's official release rather than linked at build time. The prebuilt binaries commonly linked by Rust projects require a recent glibc and fail to link on Ubuntu 22.04 and Debian 12. Microsoft's build requires glibc 2.27, which covers Ubuntu 18.04 and Debian 10 onward. The download is verified against a SHA-256 checksum pinned in the source.

### Engine selection

Tesseract and [ocrs](https://github.com/robertknight/ocrs) were evaluated first on real screenshots. Both missed small interface text such as sidebar labels, buttons and tab names, which is the text users most often search for. PP-OCRv6 recognized nearly all of it. The full comparison is in the [development log](../notes.md#ocr-bakeoff-python-before-writing-any-rust).

### Tuning for screenshots

PaddleOCR's defaults target photographs of documents. akshat changes them for screen content:

| Parameter | PaddleOCR default | akshat | Rationale |
|---|---|---|---|
| Detection model | Small | Tiny | 4.6x faster, retains 99.6% of words |
| Detection input size | Upscale to 736 px short side; never downscale | Downscale only, to ~1 megapixel | Screen text is already legible at native size. Upscaling a small crop cost 330 MB of memory with no accuracy gain. |
| Box geometry | Rotated rectangles | Axis-aligned rectangles | Screenshot text is horizontal, so connected components are sufficient |
| Recognition batching | 6 lines per batch | Batched by total width | The recognizer scores ~18,000 characters per step, so the output tensor dominates memory |

Single-character lines are discarded, because interface icons are frequently recognized as a single high-confidence character.

## Index and search

The index is a single SQLite database using FTS5 with the trigram tokenizer. Trigram indexing enables substring matching: `nutsmp` matches `donutsmp.net`, and partially misrecognized words remain findable.

- **One full-text row per screenshot.** Multi-word queries match across lines. Per-line text and bounding boxes are stored in a separate table.
- **Query escaping.** Every term is quoted before it reaches `MATCH`, so operators and punctuation in user input are treated as literal text.
- **Short terms.** Terms under three characters cannot use the trigram index and fall back to `LIKE`, applied to rows already narrowed by the other terms.
- **Lazy highlighting.** Matched lines are fetched only for thumbnails currently on screen. A two-letter query can match 2,000 screenshots; loading lines for all of them took 60 to 90 ms, compared with 1 to 10 ms for the ~30 visible thumbnails.

## Search window

The window is built with [GPUI](https://gpui.rs), the UI framework behind the Zed editor, rendering through wgpu on Vulkan or OpenGL.

- **Resident process.** GPU driver initialization accounts for 300 to 800 ms of a cold start. After the first launch, the process remains resident while hidden (37 MB) and later launches signal it over a Unix socket, reducing summon time to ~120 ms. The window reopens in its previous state.
- **Search highlighting.** While a query is active, thumbnails are dimmed and matched lines are outlined in blue. The accent color is reserved for search matches; selection uses the foreground color.
- **Interruptible motion.** Opening a screenshot animates it from its thumbnail with a spring. Springs can be interrupted, so pressing Escape mid-animation reverses from the current position. Animations advance by elapsed time, not per frame, so they run at the correct speed on slow machines.
- **Software rendering.** When no GPU is available, animated transitions are replaced by short fades, because each frame rendered on the CPU has a measurable power cost.
- **Image loading.** GPUI's built-in image loader leaked roughly 9 MB per page of scrolling. Thumbnails are instead decoded on a background thread and passed to GPUI as prepared render images, which keeps memory flat.
- **Clipboard.** On Wayland, clipboard contents are served by the application that set them. akshat delegates to `wl-copy` when available, so copied content persists after the window closes.

The interface uses the system sans-serif font and a neutral palette so that the screenshots themselves carry the color. Motion occurs only in response to user input.

## Repository layout

| Path | Contents |
|---|---|
| `crates/core` | Index, configuration, search, thumbnail crop rules |
| `crates/ocr` | PP-OCRv6 on ONNX Runtime: detection, recognition, decoding, model download |
| `crates/cli` | The `akshat` binary: `index`, `watch`, `search`, `stats`, `ocr` |
| `crates/app` | The `akshat-app` binary: search window, onboarding and settings |
`core` and `ocr` have no dependency on the UI, so the command-line interface can exercise them on machines without a display. The [development notes](../notes.md) describe the measurements and bugs behind these decisions.
