# Usage

- [Searching](#searching)
- [Keyboard shortcuts](#keyboard-shortcuts)
- [Moving screenshots to the trash](#moving-screenshots-to-the-trash)
- [Mouse](#mouse)
- [Settings](#settings)
- [Background indexing](#background-indexing)
- [Command-line interface](#command-line-interface)
- [Flags and environment variables](#flags-and-environment-variables)
- [File locations](#file-locations)

## Searching

Open the search window with your shortcut and start typing. Results update on every keystroke.

| Behavior | Detail |
|---|---|
| Partial matching | Any substring matches. `nutsmp` finds `donutsmp.net`, and `invoi` finds `invoice`. Words the OCR misread slightly remain findable from their correct parts. |
| Multiple words | All words must appear in the screenshot, in any order and on any line. `invoice march` matches a screenshot containing both. |
| Case | Matching is case-insensitive. |
| Special characters | Input is always treated as literal text. `c++`, `NOT` and unbalanced quotes are valid queries. |
| Short words | Words of one or two characters are supported. The index is built from three-character sequences, so these are matched by scanning the results of the other terms. |
| Ordering | Results are ranked by match quality, then by modification time, newest first. An empty query lists all screenshots, newest first. |

While a query is active, each thumbnail is dimmed and the matched lines are highlighted in place.

## Keyboard shortcuts

| Key | Result grid | Open screenshot |
|---|---|---|
| Typing | Search | Return to the grid and search |
| Arrow keys, Page Up, Page Down | Move the selection | Left and Right move to the adjacent result |
| Enter | Open the selected screenshot | Open the file in the default image viewer |
| Ctrl+C | Copy the screenshot's text | Copy the selected lines, or all text if none are selected |
| Ctrl+Shift+C | Copy the image | Copy the image |
| Ctrl+O | Open the file in the default image viewer | Same |
| Ctrl+Shift+O | Show the file in its folder | Same |
| Ctrl+, | Open settings | |
| Shift+Arrow keys | Mark a run of screenshots | |
| Ctrl+Shift+A | Mark every result of the current search | |
| Ctrl+Delete | Move the marked screenshots, or the selected one, to the trash | Move this screenshot to the trash |
| Ctrl+Z | Put back what was last moved to the trash | |
| Escape | Cancel a pending move, clear marks, clear the search, then close the window | Return to the grid |

The shortcut bound to `akshat-app` also closes the window. When reopened, the window restores its previous state: the same query, selection and open screenshot.

## Moving screenshots to the trash

Search for what you no longer need, mark the results, and move them to the trash in one step. For example, search `otp`, press Ctrl+Shift+A to mark every match, then Ctrl+Delete.

- Nothing is deleted. Screenshots go to the system trash following the [freedesktop.org trash specification](https://specifications.freedesktop.org/trash-spec/latest/), so they appear in your file manager's trash and can be restored from there.
- Screenshots on another drive go to that drive's own trash (`.Trash-<uid>` at its root) and are never copied across drives.
- Ctrl+Delete always asks for confirmation, showing how many screenshots will be moved. Press Enter (or Ctrl+Delete again) to confirm, Escape to cancel.
- Ctrl+Z puts the last batch back in place, with its text, so nothing is read again. A screenshot is not restored over a new file that has since taken its name.
- Marking all results requires an active search, so the entire library cannot be marked by accident.
- Marks are cleared when the search changes.

## Mouse

- Click a thumbnail to open it.
- Ctrl+click a thumbnail to mark or unmark it. Shift+click marks every thumbnail between the selection and the one clicked.
- The bar that appears while screenshots are marked can also be clicked.
- On an open screenshot, hover to show the detected lines, click a line to copy it, or drag a rectangle to copy every line it intersects.

## Settings

Open settings with Ctrl+,.

| Setting | Description |
|---|---|
| Folders | Folders to index. The home directory and `/` are rejected; select the specific folders your screenshots are saved to. |
| Theme | System, light or dark. |
| Background indexing | Enables or disables the background indexer. See [Background indexing](#background-indexing). |
| Cores per screenshot | Number of CPU cores used to read a single screenshot. Higher values are faster; lower values leave more capacity for other work. Indexing always runs at idle priority. |
| Thumbnail cache | Clears cached thumbnails. They are regenerated on demand. |

Settings are stored in `~/.config/akshat/config.toml` and can also be edited directly. The background indexer applies changes to the folder list without a restart.

## Background indexing

`akshat watch` indexes every image in the configured folders, then monitors them with inotify and indexes new screenshots as they are saved (0.77 s from save to searchable on the reference machine). Renamed and moved files keep their existing text without being read again. Deleted files are removed from the index.

Resource usage is constrained:

- The process runs at idle CPU and I/O scheduling priority.
- On battery power, processing of an existing backlog slows to one screenshot every few seconds. New screenshots are still indexed immediately.
- The systemd service limits memory to 400 MB (`MemoryHigh`).
- Images larger than about 64 megapixels, or smaller than 16 pixels on either side, are skipped.

Enabling background indexing during first run or in settings installs one of the following:

| System | Mechanism |
|---|---|
| systemd | User service at `~/.config/systemd/user/akshat-watch.service`, enabled and started immediately |
| Other init systems | XDG autostart entry at `~/.config/autostart/akshat-watch.desktop`, started at login |

The app can install the service during onboarding or from settings. To enable an already installed service, run:

```sh
systemctl --user enable --now akshat-watch
```

Status and logs:

```sh
systemctl --user status akshat-watch      # service state
journalctl --user -u akshat-watch -f      # follow the log
akshat stats                              # index location and size
```

## Command-line interface

| Command | Description |
|---|---|
| `akshat search <words>...` | Print matching screenshot paths and the lines that matched. `-n, --limit <n>` sets the maximum number of results (default 20). |
| `akshat stats` | Print file locations and the number of indexed screenshots. |
| `akshat ocr <image>` | Read one image and print its text without indexing it. `--boxes` also prints each line's bounding box and confidence. |
| `akshat index [folders]...` | Index the given folders, or the configured folders, then exit. |
| `akshat watch [folders]...` | Index, then continue monitoring for new screenshots. Without arguments, follows configuration changes. |

`index`, `watch` and `ocr` accept `--threads <n>` to set the number of cores used per screenshot. Run `akshat --help` or `akshat <command> --help` for full details.

## Flags and environment variables

| Option | Effect |
|---|---|
| `akshat-app --window` | Open as a regular window instead of an overlay. |
| `akshat-app --once` | Exit when the window closes instead of remaining resident in the background. |
| `AKSHAT_THEME=light\|dark` | Override the theme setting. |
| `AKSHAT_FRAME_STATS=1` | Print frame timing statistics when the window closes. |
| `RUST_LOG=warn` | Print warnings from the window and GPU layers. |
| `ORT_DYLIB_PATH=<path>` | Use an existing `libonnxruntime.so` instead of downloading one. See [Compatibility](compatibility.md#cpu-architectures). |

## File locations

| Path | Contents |
|---|---|
| `~/.config/akshat/config.toml` | Folders, theme, cores per screenshot |
| `~/.local/share/akshat/index.db` | Text index |
| `~/.local/share/akshat/models/` | OCR models |
| `~/.local/share/akshat/runtime/` | ONNX Runtime |
| `~/.cache/akshat/thumbs/` | Thumbnails. Safe to delete. |

akshat never modifies, moves or deletes screenshots.
