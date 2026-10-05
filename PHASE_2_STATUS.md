# Phase 2 Status

## Phase 2 Real-Data Verification

This investigation updates the earlier empty-index report below. It used one synthetic test image, not personal screenshots. No product source code or dependency manifests were changed for this investigation.

### Environment

- macOS on Apple Silicon; app data directory: `~/Library/Application Support/akshat`.
- Existing Homebrew ONNX Runtime was 1.23.2. The OCR crate's `ort` rc.13 rejects it because it requires ONNX Runtime 1.27 or newer.
- OCR was successfully run using the existing `ORT_DYLIB_PATH` override pointed at a temporary official ONNX Runtime 1.28.2 Darwin ARM64 library. The library and verification workspace are outside the repository.
- The test image and copied Rust crates are under `/var/folders/.../akshat-phase2-isolated-oy45ugec`; the CLI output contains the exact current path.

### Build Investigation

- `cargo run -p akshat --offline -- ocr ...` from the repository is blocked before compilation because Cargo attempts to resolve the pinned GPUI Git revision `0fb9a9d…` for another root workspace member, and that checkout is not cached.
- A temporary workspace containing copies of `crates/core`, `crates/ocr`, and `crates/cli` passed `cargo check -p akshat` without GPUI. This confirms the CLI/core/OCR dependency graph itself does not depend on GPUI.
- Running OCR against the installed 1.23.2 runtime failed with an explicit runtime-version incompatibility. Repeating with ONNX Runtime 1.28.2 through `ORT_DYLIB_PATH` succeeded. This is an environment workaround; the repository's default OCR runtime selection remains Linux-only and was not changed.

### OCR Pipeline

The existing CLI indexed a controlled PNG through the existing image decode, PP-OCR, `Indexer`, SQLite, and FTS5 path. Output reported one searchable image and four OCR lines. The same CLI OCR path also read `docs/banner.png` successfully and returned three lines. No Tauri OCR command or OCR implementation was added.

### Test Image

A synthetic 1500 × 560 PNG was created outside the repository and outside personal screenshot folders. It contains four text lines: `PHASE 2 TEST`, `RGPV RESULT`, `INTERNSHIP DEADLINE`, and `AKSHAT DESKTOP`. OCR stored all four lines. The indexed image is at the temporary test path recorded in the SQLite row; it remains there for review.

### SQLite Verification

Read-only SQLite inspection after indexing reported `user_version = 1`, one `shots` row, four `lines` rows, and one `shots_fts` row. Shot ID 1 points to the synthetic test image and records dimensions 1500 × 560. These are the app's configured database counts after the explicitly requested controlled test.

### FTS5 Verification

The existing CLI search command returned the image path for the matching shot ID and matching OCR line for `PHASE 2 TEST`. A search for `ZZZZZZZZ_nonexistent_test_term` returned no result. No FTS schema, tokenizer, query construction, or matching semantics were changed.

### Tauri Verification

`npm run tauri dev` launched the macOS app successfully with the populated database. Runtime output confirmed `check_core_connection`, `get_stats`, and `search_shots` were invoked through Tauri IPC. The current debug output does not include command arguments or returned DTO values, so a positive search result observed in the React view is not claimed. The adapter's automated tests remain fixture-based.

### Screenshot Preview Verification

The app window opened against the populated database, but selecting the test result and visually confirming the rendered image was not completed. The `get_shot_image` adapter path is implemented and fixture-tested; real image bytes through Tauri and React rendering remain unverified.

### Invalid ID Verification

The adapter's existing automated test covers invalid/missing IDs using its temporary fixture. A live Tauri IPC request for ID `999999999` was not made, so the IPC behavior for that specific request remains unverified.

### Database Safety Verification

Before this investigation, the configured database had zero shots, lines, and FTS rows. The only change made was adding the one explicitly authorized synthetic image through the normal CLI indexer. Final read-only counts remain one shot, four lines, and one FTS row at schema version 1. No personal screenshots were read, indexed, modified, or deleted; no rows or schema were manually changed.

### Blockers

- Normal root-workspace CLI execution still cannot resolve the pinned GPUI Git dependency in this environment.
- Default OCR runtime configuration does not provide a macOS runtime, and the installed ONNX Runtime version is too old. OCR works with the available runtime override and compatible temporary runtime, but macOS runtime support is not implemented.
- Positive React-visible search output, screenshot preview rendering, and a live invalid-ID IPC request remain unverified.

### Final Conclusion

**Classification: B.** The existing core/OCR/CLI pipeline builds independently of GPUI and completed real OCR indexing plus positive and negative FTS5 searches on a controlled test image. The Tauri app also launched and reached its IPC commands. Phase 2's full real-data UI proof is incomplete because React-visible search results, image preview rendering, and a live invalid-ID request were not verified. The root workspace and default macOS OCR runtime constraints remain.

No Phase 3 work was performed.

## 1. Objective

Create a small reusable application boundary over the existing Rust index and make it available to the Tauri + React shell. Phase 2 adds no capture, Memory, Why, AI, or action functionality.

## 2. What Was Implemented

- Added `akshat-application`, a reusable Rust crate over `akshat-core`.
- Added DTOs for index statistics, screenshot metadata, and selected screenshot image data.
- Added typed application errors and stable IPC error codes/messages.
- Exposed core readiness, stats, recent indexed screenshots, existing FTS5 search, shot metadata, and shot image commands.
- Added a typed React API module and replaced the welcome-only shell with real index statistics, recent/search results, and selected screenshot preview.
- Moved SQLite-backed operations to Tauri's blocking worker pool and retained one lazily initialized shared `Index` behind managed state.
- Changed schema initialization to create a fresh schema transactionally and reject unknown/unversioned non-empty schemas instead of dropping tables.

## 3. Architecture Before

```text
React welcome page
  -> raw invoke("check_core_connection")
  -> Tauri command
  -> akshat_core::data_dir()
```

The command proved access to a core helper but did not open the index or access indexed data.

## 4. Architecture After

```text
React components
  -> typed frontend API (src/lib/api.ts)
  -> typed Tauri commands
  -> managed shared application state
  -> blocking worker task
  -> akshat-application DTO/service boundary
  -> existing akshat-core Index
  -> SQLite / existing FTS5
```

The UI receives screenshot IDs, timestamps, and dimensions, not arbitrary filesystem paths. For preview, Rust resolves an indexed ID, reads that image only, enforces a 25 MiB limit, and returns a data URL. The frontend has no direct filesystem or SQLite access.

## 5. New Files

- `crates/application/Cargo.toml`
- `crates/application/src/lib.rs`
- `apps/desktop/src/lib/api.ts`
- `PHASE_2_STATUS.md`

## 6. Modified Files

- Root `Cargo.toml` and `Cargo.lock`: register and lock the application crate.
- `crates/core/src/index.rs`: add OCR line count and screenshot lookup; refuse destructive reset on unsupported schema versions and create a fresh schema in a transaction.
- `apps/desktop/src-tauri/Cargo.toml` and its `Cargo.lock`: depend on the adapter and add test-only SQLite access.
- `apps/desktop/src-tauri/src/lib.rs`: add managed state, worker-pool command handlers, and adapter tests.
- `apps/desktop/src/main.tsx` and `src/style.css`: real stats, recent list, FTS5 search, and selected-shot preview.

Pre-existing changes to README/docs/untracked files were present before Phase 2 and are not attributable to this phase.

## 7. Application Adapter

`Application` in `crates/application/src/lib.rs` wraps the existing `Index`. It implements `stats`, `recent_shots`, `search_shots`, `shot`, and `shot_image`. Limits are capped at 100 results and 512 query bytes. Search delegates to the existing `Index::find`, preserving trigram/short-term behavior and ranking. DTOs use camelCase serialization and do not expose the original path. `shot_image` resolves only a positive indexed ID and accepts PNG/JPEG/WebP extensions with a 25 MiB cap.

## 8. Tauri Commands

| Command | Behavior |
|---|---|
| `check_core_connection` | Lazily opens the local index; returns application name and ready status. |
| `get_stats` | Returns visible screenshot and OCR line counts. |
| `get_recent_shots(limit)` | Uses empty-query ordering from the existing index. |
| `search_shots(query, limit)` | Calls the existing FTS5-backed `Index::find`. |
| `get_shot(id)` | Returns typed indexed metadata. |
| `get_shot_image(id)` | Returns bounded image data for that indexed ID. |

Failures return serialized `{ code, message }` values without exposing internal anyhow details. SQLite work runs through `spawn_blocking`; the shared connection is protected by a managed mutex.

## 9. Frontend API

All `invoke()` calls are in `apps/desktop/src/lib/api.ts`. React components use typed wrappers and DTOs. The UI has no SQLite, filesystem, or shell dependencies. Errors are converted to displayable messages.

## 10. Real Data Flow (original empty-index snapshot)

This was the original Phase 2 snapshot before the controlled-image investigation above. Its empty-index counts have been superseded: the current state is recorded under “SQLite Verification.”

Adapter tests use an in-memory SQLite fixture to prove that non-empty OCR text returns through the existing FTS5 search path. Fixture results are not reported as real user data.

## 11. Database Changes

No tables, columns, indexes, or FTS definitions were added or changed. `line_len()` adds a read-only count query; `get(id)` adds a read-only metadata lookup. Fresh schema setup uses a transaction and sets `user_version = 1` in that transaction. A non-empty version-0 database and any unknown nonzero version now return an error without dropping tables. This is a preservation guard, not a general forward-migration runner; future schema changes still need explicit transactional migrations.

## 12. FTS5 Preservation

The FTS5 table, trigram tokenizer, query construction, short-query fallback, matching behavior, and result ordering were not changed. `search_shots` calls `Index::find` directly. In-memory adapter tests confirmed a multi-term OCR query returns the indexed shot and a missing term returns no result.

## 13. Security Changes

- No shell plugin, filesystem plugin, remote frontend, cloud call, or telemetry was added.
- The frontend does not receive paths. Image reads are keyed by an indexed shot ID and constrained by file type and size.
- Stable IPC errors omit internal stack details.
- The core database can only be accessed through the Rust adapter.

The image data URL can be sizable (up to the configured 25 MiB source-file cap, plus base64 overhead); the UI requests it only for the selected screenshot. This path was not tested against a populated image.

## 14. Tests Added

Three Tauri crate unit tests passed:

1. Adapter stats, recent retrieval, and FTS5 search over a temporary in-memory index.
2. Invalid shot ID returns the typed application error.
3. Unsupported database schema is rejected while pre-existing table data remains intact.

The application crate also contains focused unit tests for DTO serialization and input bounds. The root workspace could not resolve its pinned GPUI Git dependency offline, so those root-workspace tests were not executed separately.

## 15. Verification Results

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo metadata --locked --no-deps --format-version 1` (root) | PASS; reports five root packages. |
| `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --locked --offline` | PASS. |
| `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --locked --offline --all-targets -- -D warnings` | PASS. |
| `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --offline` | PASS; 3 tests passed. |
| `npm run build` | PASS; TypeScript project build and Vite production build. |
| `npx tsc --noEmit` | PASS. |
| macOS `npm run tauri dev` | PASS; fresh launch opened the app and readiness/stats IPC calls were observed. Search IPC had also been observed in the prior Phase 2 runtime. |
| Local index before startup | `shots = 0`, `lines = 0`, `FTS rows = 0`, `user_version = 1`. |
| Local index after startup | `shots = 0`, `lines = 0`, `FTS rows = 0`, `user_version = 1`; no count or schema change. |
| Existing image availability | Image files exist in `~/Pictures/goa`, but none are indexed; no user image was added to the database. |
| Existing OCR indexing pipeline | BLOCKED; `cargo run -p akshat --offline -- ocr docs/banner.png` stops before build because the pinned GPUI Git revision is unavailable. OCR runtime source provides Linux `.so` downloads only. |
| `cargo test -p akshat-application -p akshat-core --offline` | BLOCKED before test execution because Cargo resolves workspace GPUI from pinned Git revision `0fb9a9d…`, unavailable in offline cache. |
| Populated-index search + real screenshot display | UNVERIFIED: no populated index available. |

### Final verification table

| Verification | Result | Evidence |
|---|---|---|
| Core initialization | ✅ VERIFIED | Fresh Tauri launch; `check_core_connection` IPC reached the adapter and opened the configured index. |
| Real stats | ✅ VERIFIED | `get_stats` was invoked through IPC; database has 0 shots and 0 OCR lines. |
| Real recent shots | 🟡 IMPLEMENTED BUT UNVERIFIED | UI and command call the index, but no record exists to return. |
| Real FTS5 search | 🟡 IMPLEMENTED BUT UNVERIFIED | Search IPC was observed, but there are no real OCR terms or rows to match. In-memory adapter search passed. |
| FTS5 semantics preserved | ✅ VERIFIED | Adapter delegates to existing `Index::find`; tokenizer/query code is unchanged. A fixture search returned expected result. |
| Real screenshot preview | 🟡 IMPLEMENTED BUT UNVERIFIED | The ID-scoped preview path exists; no indexed image is available to display. |
| Invalid ID handling | 🟡 IMPLEMENTED BUT UNVERIFIED THROUGH IPC | Adapter test checks invalid/missing IDs; no live Tauri invalid-ID request was made. |
| Existing data preserved | 🟡 IMPLEMENTED BUT UNVERIFIED ON POPULATED DATA | Before/after counts are 0/0/0. There was no populated user data to compare. |
| Migration safety | ✅ VERIFIED | Safe temporary database test refused schema version 42 and retained its existing row. |
| Frontend API | ✅ VERIFIED | React routes IPC through `src/lib/api.ts`; typecheck passed. |
| Tauri IPC | ✅ VERIFIED | Runtime debug output recorded readiness, stats, and search command calls across the Phase 2 launches. |
| Rust tests | ✅ VERIFIED | Three nested Tauri tests passed. |
| TypeScript check | ✅ VERIFIED | `npx tsc --noEmit` passed. |
| Vite build | ✅ VERIFIED | `npm run build` passed. |
| Tauri build/check | ✅ VERIFIED | Nested Tauri `cargo check --locked --offline` passed. |
| Root workspace | ⚠️ ENVIRONMENT BLOCKED | Root tests and CLI OCR cannot resolve pinned GPUI Git revision `0fb9a9d…` offline. |

## Original Final Score (superseded by the real-data investigation above)

**Phase 2 implementation: 100% of the scoped code paths are implemented.**

**Original Phase 2 verification: 63% (10 of 16 verification rows verified).**

This score predates the controlled-image investigation and is retained as historical context. Current findings and classification are in “Final Conclusion” above.

## 16. What Was NOT Implemented

No screenshot capture, import UI, CaptureContext runtime type, Memory, Why, AI, embeddings, semantic search, actions, reminders, automation, cloud backend, authentication, notifications, or GPUI replacement. OCR was not invoked by Tauri. No Memory/action schema was added.

## 17. Known Limitations

- The current app data index is empty, so this run cannot demonstrate real result rows or image preview.
- Existing PP-OCR/ONNX runtime remains Linux-configured; Phase 2 does not claim macOS OCR support.
- Screenshot preview sends base64 data through IPC and caps original files at 25 MiB.
- Unknown schema versions are safely refused, but no migration path between versions is implemented yet.
- The complete root workspace build and application/core unit test suites remain unverified due the unavailable pinned GPUI Git source.

## 18. Remaining P0/P1 Issues

**P0:** Before any schema change that adds durable user-authored data, implement and test a forward-only migration from schema version 1. The current guard prevents data loss but cannot upgrade an unknown schema.

**P1:** Restore root workspace dependency access and run `cargo test --workspace`, `cargo check --workspace`, and clippy. Plan macOS ONNX Runtime support before wiring OCR into this shell. Confirm real screenshot preview and FTS search with an existing populated index.

## 19. Phase Completion (original snapshot)

This statement predates the controlled-image investigation. Refer to “Final Conclusion” above for current Phase 2 status.

## 20. Recommended Next Phase

Stop for review. Do not proceed automatically. After approval, Phase 3 should continue only after reviewing this status and the local-data result. A useful verification follow-up is to open an already indexed screenshot database or use the user's normal indexing flow, then confirm search result display and selected image preview without creating or altering screenshots for the test.
