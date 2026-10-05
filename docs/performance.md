# Performance

The figures on this page are measurements reported by the source project. They have not been independently reproduced in this checkout.

- [Test environment](#test-environment)
- [Search window](#search-window)
- [Indexing](#indexing)
- [Search](#search)
- [Disk usage](#disk-usage)
- [Measuring on your system](#measuring-on-your-system)

## Test environment

| Component | Specification |
|---|---|
| CPU | Intel Core i7-14700HX, 28 threads |
| GPU | NVIDIA GeForce RTX 4060 Laptop GPU |
| OS | Fedora 43 |

"No GPU" configurations hide all GPU drivers so that rendering uses Mesa's software renderer, as on a machine without a GPU. "4 cores" and "2 cores" configurations pin the process to that many cores with `taskset`.

## Search window

Each configuration ran one scripted session: open the window, type a word, open and close a screenshot, then press Page Down 15 times.

| Metric | GPU | No GPU, 28 threads | No GPU, 4 cores | No GPU, 2 cores |
|---|---|---|---|---|
| First launch to visible window | 0.5 to 0.7 s | 0.4 s | 0.4 s | 0.4 s |
| Subsequent launches | ~120 ms | | | |
| Frame time while scrolling, median | 7 ms (144 fps) | 41 to 45 ms | 78 ms | |
| Frame time while scrolling, 95th percentile | 31 to 39 ms | 74 to 80 ms | 98 ms | 98 ms |
| Frame build time (application code only) | 0.04 ms | 0.05 ms | 0.04 ms | 0.03 ms |
| CPU time for the session | 3.1 s | 28 to 31 s | 17 to 20 s | 12 to 14 s |
| Private memory after the session | 123 MB | 145 MB | 142 to 160 MB | 140 to 164 MB |
| Memory including shared libraries | 245 MB | 405 MB | 410 MB | 400 MB |
| Memory while hidden | 37 MB | | | |
| Escape to window closed | | | 124 ms | 122 ms |

Empty cells were not measured.

The window renders only in response to changes, so an idle open window consumes no CPU in any configuration. After the first launch, the process remains resident while hidden. Most of a cold start is GPU driver initialization, which subsequent launches avoid.

## Indexing

Indexing 25 real screenshots with the process pinned to a given number of cores:

| Cores | 1 | 2 | 4 |
|---|---|---|---|
| Time per screenshot | 1.32 s | 1.21 s | 0.67 s |
| Peak memory | 278 MB | 311 MB | 289 MB |

| Metric | Result |
|---|---|
| Screenshot saved to searchable | 0.77 s |
| Indexer memory between screenshots | ~60 MB |
| Full index of 5,159 screenshots, 6 threads | 43 min, 0 failures |
| Recall against PaddleOCR's default configuration | 99.5% of words |

Recall is measured on 25 randomly selected screenshots against the larger detection model at PaddleOCR's default settings. The tuned configuration uses a detector 4.6x faster. See [Architecture](architecture.md#text-recognition) for details.

## Search

1 to 10 ms per keystroke across more than 5,000 screenshots, including fetching the matched lines for the visible results.

## Disk usage

| Item | Size |
|---|---|
| Text index | ~4.6 MB per 1,000 screenshots |
| Thumbnails | ~23 MB per 1,000 screenshots (24 KB each; regenerated on demand) |
| OCR models | 22 MB, downloaded once |
| ONNX Runtime | 24 MB, downloaded once |
| Binaries | 8 MB (`akshat`) and 34 MB (`akshat-app`) |

## Measuring on your system

- `AKSHAT_FRAME_STATS=1 akshat-app --once` prints frame timing statistics when the window closes.
- `akshat ocr <image>` prints model load, decode and OCR times for a single image.

Results from other hardware, particularly integrated GPUs and low-power laptops, should be recorded with the hardware and software configuration used.
