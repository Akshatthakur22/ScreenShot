# Akshat desktop shell

This is the Phase 1 macOS Tauri shell. It is a separate package and does not depend on `akshat-app` or GPUI. The initial screen calls the typed Rust command `check_core_connection`, which resolves the existing core data directory without opening the index or changing files.

## Run on macOS

Install the Rust toolchain from the repository's `rust-toolchain.toml`, Node.js, and the macOS Tauri prerequisites (Xcode Command Line Tools). Then:

```sh
npm install
npm run tauri dev
```

On launch, the React view invokes the Rust command and displays whether the existing `akshat-core` crate is reachable.

This shell does not capture screenshots, index files, open SQLite, or implement memories, AI, embeddings, or actions.
