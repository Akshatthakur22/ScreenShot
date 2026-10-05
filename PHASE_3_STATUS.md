# Phase 3 Status

## 1. Objective

Turn the Tauri proof screen into a local screenshot library for browsing and searching indexed screenshots with OCR evidence. Existing SQLite, FTS5, OCR, GPUI, and CLI behavior remain in place.

## 2. What Was Implemented

- Replaced the connectivity proof page with a screenshot library workspace.
- Added All and Recent navigation, a prominent submitted search, result cards, and a selected screenshot detail panel.
- Added actual indexed thumbnail, screenshot image, and OCR line retrieval through the application adapter and typed Tauri commands.
- Added loading, empty, no-result, recoverable error, and image-unavailable states.
- Added system light/dark appearance, keyboard search focus, result arrow navigation, Enter activation through focused buttons, Escape close/clear behavior, visible focus, and reduced-motion handling.
- No mock or hardcoded screenshot records are used.

## 3. UI Architecture

`src/pages/Library.tsx` owns view, search, result, and selected-shot state. Reusable components in `src/components/` render the search bar, screenshot cards, detail panel, and async states. `src/lib/api.ts` is the only React-to-Tauri boundary.

## 4. Search Experience

Search submits on Enter through the typed API, so each keystroke does not start a request. The existing application adapter calls `Index::search`, preserving FTS5 behavior and returning OCR lines that match the query as the result snippet. Empty search returns to the library. No-result and error states offer a clear or retry action.

## 5. Screenshot Library

All requests up to the adapter's 100-result limit and Recent requests the latest 12. Cards show a cached thumbnail, indexed OCR snippet where available, timestamp, and dimensions. Thumbnails load when cards approach the visible area and use the existing core thumbnail cache by indexed shot ID. Missing thumbnails leave a neutral placeholder.

## 6. Screenshot Detail

Selecting a result requests its detail and original image by indexed ID. The side panel separates Image, OCR text, and Metadata. Closing the panel does not change the index. Image reads remain limited by the Phase 2 adapter's indexed-ID and file-size checks.

## 7. OCR Evidence

The adapter returns the existing stored OCR lines. Search results use matching lines for their snippets; detail view shows every stored OCR line. No OCR engine or index schema change was made.

## 8. Frontend API

React continues to use the typed API module. Added typed methods for `get_shot_detail` and `get_shot_thumbnail`. No component invokes Tauri `invoke()` directly.

## 9. Backend Adapter Changes

`akshat-application` now includes OCR snippets on result DTOs, a detail DTO with all OCR lines, and a thumbnail read resolved through `akshat_core::thumb_path`. Tauri commands remain thin wrappers over the adapter. `crates/core/src/index.rs`, schema, SQLite, FTS5, the OCR crate, GPUI, and CLI were not modified for Phase 3.

## 10. Real Data Flow

The adapter was run against the configured local index containing the controlled Phase 2 test image. It returned 1 screenshot and 4 OCR lines, a recent result with the stored OCR snippet, a positive search result for `PHASE 2 TEST`, no result for a missing term, a JPEG thumbnail, and the original PNG image. Tauri launched and invoked the real stats command. A person-driven search, selection, and rendered preview in the Tauri window were not confirmed, so the full React end-to-end path remains unverified.

## 11. Loading / Empty / Error States

- Loading: library and selected image/detail loading states are rendered.
- Empty database: explains that screenshots will appear after indexing, with no capture promise.
- No results: reports no screenshots found and allows clearing the search.
- Errors: display the adapter's stable user-facing message with retry.
- Missing thumbnail or OCR: neutral image placeholder and explicit no-OCR message.

These UI branches compile, but were not manually exercised in both populated and empty Tauri sessions.

## 12. Accessibility

Navigation and actions are buttons with names, the search field has a label, result buttons include their OCR/date/dimension text, screenshots have descriptive alt text, and focus indicators are visible. Up/down moves focus through results, Enter activates the focused result, Escape closes details or clears search, and reduced-motion preferences suppress the loading animation.

## 13. Performance

Search runs on submit, not per keystroke. Library cards request existing cached thumbnails as they approach the viewport. The detail panel requests a full image only for the selected indexed ID. Recent snippet retrieval uses the existing stored OCR rows. No new image decode pipeline or frontend state library was added.

## 14. Tests

- Application adapter tests in an isolated temporary workspace without GPUI: 7 passed, including empty-index behavior, OCR snippets, detail OCR lines, search, limits, and invalid IDs.
- Tauri crate tests: 3 passed.
- Direct adapter verification against the real local index: 1 result, 4 OCR lines, positive and negative search, thumbnail MIME `image/jpeg`, and source image MIME `image/png`.
- No frontend component test framework exists in this project. Empty, loading, and error React branches were not driven by automated UI tests.

## 15. Build Verification

- `npm run build`: passed, including TypeScript project build and Vite production build.
- `npx tsc --noEmit`: passed.
- Tauri `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --offline`: passed.
- Tauri crate tests: passed, 3 tests.
- Tauri clippy with `--all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- `git diff --check`: passed.
- Root workspace application tests remain blocked because Cargo cannot resolve the pinned GPUI Git dependency offline. Adapter tests passed in a temporary workspace that excluded GPUI.

## 16. End-to-End Verification

The adapter successfully returned real library, OCR, FTS5, thumbnail, and image data. The macOS Tauri app opened and called the configured index for stats. The search and detail code paths are connected through typed Tauri commands, but a live React search result and selected screenshot rendering were not manually observed. Therefore the complete React-to-Tauri-to-image vertical slice is partially verified.

## 17. What Was NOT Implemented

No screenshot capture, ScreenCaptureKit, shortcut registration, CaptureContext, Memory, Why, AI, embeddings, semantic search, actions, reminders, automation, cloud, authentication, or notification functionality was added. No Memory or Action tables were created. No new work was done on the known macOS OCR runtime limitation.

## 18. Known Limitations

- The root workspace still cannot run its full tests offline while the pinned GPUI revision is unavailable.
- The default macOS ONNX Runtime setup remains a Phase 2 limitation; Phase 3 does not invoke OCR.
- Live React search, detail selection, image rendering, empty database UI, and error UI were not manually verified in the native window.
- No frontend interaction test framework is configured.
- Search requires Enter or the search field's form submission; it intentionally does not query on every keystroke.

## 19. Phase 3 Completion

**Implementation: substantially complete. Verification: partial. Overall Phase 3 completion: 85%.** The local workspace and adapter paths are implemented and the adapter returns real indexed data. Completion is not reported as 100% because the React-visible results and screenshot preview were not verified live, and UI state branches lack interaction tests.

## 20. Recommendation for Phase 4

Stop for review. Do not begin Phase 4 automatically. Phase 4 should be considered only after review of this workspace and its live-data verification gap. Keep capture platform-specific, permission-aware, and separate from the existing index/search behavior.
