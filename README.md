# Akshat

Akshat is a personal Linux desktop development project for searching screenshots by their recognized text. It indexes image folders selected by the user, stores OCR text locally, and presents matching screenshots in a keyboard-driven search window. It does not capture screenshots.

## Project status

This repository is maintained as a personal development project. The performance figures and compatibility notes in `docs/` and `notes.md` were inherited from the source project and have not been independently verified in this checkout.

## Development setup

The workspace uses Rust 1.95. Install the system build dependencies required by GPUI, Wayland/X11, and fontconfig for your Linux distribution, then run:

```sh
cargo run -p akshat-app -- --once
cargo run -p akshat -- ocr path/to/screenshot.png
```

`--once` exits when the window closes instead of leaving a resident process. The OCR command reads one image. On first use, the OCR model and ONNX Runtime are downloaded and checksum-verified.

Useful development commands:

```sh
cargo fmt --all
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

The app can install a user background service during onboarding. Use `akshat watch` to run the indexer manually.

## Architecture

- `crates/core`: configuration, SQLite index/search, shared image geometry, and reversible system-trash operations.
- `crates/ocr`: screenshot-oriented text detection and recognition using PP-OCRv6 and ONNX Runtime.
- `crates/cli`: command-line interface and background folder watcher/indexer.
- `crates/app`: GPUI search window, onboarding, settings, image loading, and interaction logic.
- `docs/`: usage, compatibility, performance, troubleshooting, and architecture notes.

See [architecture](docs/architecture.md), [usage](docs/usage.md), and [troubleshooting](docs/troubleshooting.md) for details.

## Data and privacy

Screenshots stay in their original folders. The application stores its configuration, OCR index, downloaded models, runtime, and thumbnails in the user's standard config, data, and cache directories under `akshat`. OCR and search run locally. Network access is used to download the OCR models and ONNX Runtime on first use.

The application data namespace changed from the source project's name to `akshat`; existing index/config/cache directories are not migrated automatically. Screenshots are unaffected, and the index can be rebuilt.

## Third-party components

This project uses PaddleOCR PP-OCRv6 models, ONNX Runtime, SQLite FTS5, and GPUI. Their names and licenses remain credited here because they identify upstream software and models used by the project.

## License

Licensed under the GNU General Public License v3.0 or later. See [LICENSE](LICENSE).
