# Troubleshooting

If a problem is not listed here, record the distribution, desktop environment, and output of `RUST_LOG=warn akshat-app --once` run from a terminal while investigating it.

- [Building](#building)
- [Launching](#launching)
- [Searching](#searching)
- [Background indexing](#background-indexing)
- [Clipboard](#clipboard)
- [Performance](#performance)

## Building

### Error mentioning `cold_path`, or requiring a newer `rustc`

The installed Rust version is older than 1.95. Install Rust with [rustup](https://rustup.rs) instead of the distribution package, and run `cargo install` from inside the cloned repository so that `rust-toolchain.toml` takes effect.

### Missing `fontconfig`, `xkbcommon`, `wayland` or `xcb`

A build dependency is missing. Re-run the command for your distribution from [step 1 of the installation](../README.md#1-install-build-dependencies).

### `linker 'cc' not found`

No C compiler is installed. It is included in step 1 (`build-essential`, `base-devel` or `gcc`).

### Bus error or "No space left on device"

The disk ran out of space during the build. About 3 GB of free space is required.

## Launching

### `akshat-app: command not found`

`~/.cargo/bin` is not on your `PATH`. Open a new terminal, or log out and back in after installing rustup.

### The keyboard shortcut does nothing

Run `RUST_LOG=warn akshat-app --once` in a terminal.

- If the window opens, the desktop environment cannot resolve the command. Use the absolute path from `which akshat-app` in the shortcut.
- If an error is printed, include it in an issue.

### The window opens as a regular window on GNOME

This is expected. GNOME does not implement the Wayland layer-shell protocol used for the overlay, so akshat falls back to a regular window.

### An old version opens after updating

The window process remains resident between uses. Stop it once with `pkill -x akshat-app`.

## Searching

### No results for anything

Run `akshat stats` to check the number of indexed screenshots. If it is zero:

1. Confirm that the folders in settings (Ctrl+,) are the folders your screenshots are saved to.
2. Confirm that background indexing is enabled and running: `systemctl --user status akshat-watch`.
3. Alternatively, index once manually with `akshat index`.

### A specific screenshot is not found

Run `akshat ocr path/to/screenshot.png` to see the text akshat extracts. If the text is missing or incorrect, open an issue with the `ocr-accuracy` label and, if it contains no private information, attach the screenshot.

### Text in Hindi, Russian, Korean, Arabic or other scripts is not found

The recognition model does not support these scripts. See [Compatibility](compatibility.md#language-support).

### Rotated or vertical text is not found

Text detection only considers horizontal lines. See [Compatibility](compatibility.md#language-support).

## Background indexing

### New screenshots are not indexed

```sh
systemctl --user status akshat-watch      # service state
journalctl --user -u akshat-watch -e      # most recent log output
```

On systems without systemd, confirm that `~/.config/autostart/akshat-watch.desktop` exists and that your desktop environment runs XDG autostart entries. The indexer can also be started manually with `akshat watch`.

### Indexing failed on first run without network access

The OCR models and ONNX Runtime could not be downloaded. Connect to the network; the systemd service retries every 30 seconds.

### Unsupported CPU architecture or musl-based distribution

Microsoft does not publish ONNX Runtime builds for these platforms. See [Compatibility](compatibility.md#cpu-architectures).

## Clipboard

### "couldn't copy the image, is wl-copy installed?"

Install `wl-clipboard` on Wayland or `xclip` on X11.

### Copied content disappears after the window closes

On Wayland, clipboard contents are owned by the application that set them. Install `wl-clipboard`; akshat then hands clipboard ownership to `wl-copy`, which persists after the window closes.

## Performance

### The window is slow or stutters

Compare against the figures in [Performance](performance.md). If no working GPU driver is available, rendering falls back to the CPU, which is slower. In that mode, animations are intentionally replaced with short fades.

### Indexing slows down other work

Indexing already runs at idle priority. On systems with few cores, reduce "Cores per screenshot" in settings.
