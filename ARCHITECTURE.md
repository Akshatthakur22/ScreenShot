# Architecture assessment and macOS migration plan

## Scope

This is a Phase 0 assessment of the current Akshat repository, whose implementation originated as Gyotaku. It describes the inspected code and a proposed path toward a local-first macOS capture and memory application. No Tauri shell, capture implementation, database schema change, or behavior change is part of this assessment.

The repository currently provides screenshot-folder indexing, local OCR, SQLite/FTS5 search, a Linux-oriented background watcher, and a GPUI search window. It does not currently capture the screen, represent memories or actions, or contain a Tauri or React application.

## 1. Current architecture

The workspace uses Rust 2024 and is pinned to Rust 1.95. It has four packages:

| Package | Current responsibility |
|---|---|
| `akshat-core` | Shared screenshot/OCR types, configuration, data paths, SQLite index/search, thumbnail geometry, and reversible trash operations |
| `akshat-ocr` | Image decode limits, PP-OCRv6 detection and recognition, ONNX Runtime setup, and model downloads |
| `akshat` | CLI commands, indexing pipeline, and background filesystem watcher |
| `akshat-app` | GPUI search window, onboarding, settings, image display, selection, clipboard, and trash/undo interaction |

The current indexing data flow is:

```text
configured image folders
  -> initial scan or filesystem watcher
  -> bounded image decode
  -> OCR detection and recognition
  -> SQLite screenshot/line rows and FTS5 text row
  -> thumbnail cache
  -> search results and lazily loaded match lines in the app
```

The CLI's `indexer::Indexer` owns an OCR engine and a core `Index`. It checks a path and modification time before doing work, indexes supported PNG/JPEG/WebP images, writes a cropped JPEG thumbnail, and stores OCR lines. The watcher waits for files to settle, handles rename event ordering, follows config changes, and runs at idle priority on Linux. The UI reads the same database and uses WAL mode for concurrent reader/writer access.

Search uses one FTS5 trigram row per screenshot, plus a normal `lines` table holding text, normalized bounding boxes, and confidence. Query terms are quoted before reaching `MATCH`; terms shorter than three characters fall back to escaped `LIKE`. Result metadata is fetched first, and the UI asks for matching lines only for visible tiles. Existing tests are primarily Rust unit tests embedded in modules. There is no separate integration-test fixture directory in this checkout.

The current UI is a resident GPUI process. It uses an internal Unix socket to toggle the window, attempts a Wayland layer-shell overlay, then falls back to a regular window. It implements a custom text input, keyboard navigation, justified thumbnail rows, detail view, settings/onboarding, system clipboard integration, and reversible bulk trash. Thumbnail decoding is bounded and cached because the original GPUI path loader caused memory growth.

The target no-AI memory flow is `Capture/Import -> OCR -> optional user context/Why -> Memory -> Search`. Memory creation, persistence, and retrieval must work without an understanding engine. AI is an optional enrichment layer that may propose titles, summaries, categories, entities, deadlines, or actions; it must not be a prerequisite for a Memory record or for finding one.

`user_why` is optional. The capture/review UI may phrase the same optional field as either "Why are you saving this?" or "What should I remember about this?" Users can save without answering. OCR text and the original source remain sufficient to create a basic Memory.

Every source observation should have a lightweight `CaptureContext` boundary, separate from `Memory` and any AI `Interpretation`:

```text
CaptureContext
  source_type                 (screen capture, import, clipboard, etc.)
  captured_at                 (timestamp)
  application / window        (optional, only when safely available)
  url                         (optional, only when safely available)
  capture_region              (optional geometry and display coordinate space)
  ocr_reference               (reference to existing OCR/source record)
  user_context / user_why     (optional, user-authored)
```

This is a data boundary, not permission to collect context. Phase 1 and the initial capture implementation must not add application/window inspection, URL extraction, or broad Accessibility collection. Each optional platform field needs a documented source, permission, and failure behavior before it is populated. Safe absence is a normal value.

## 2. Existing reusable components

These components should remain the engine behind the new app where their behavior and platform constraints permit:

- `crates/core/src/index.rs`: current OCR index, FTS5 query semantics, database access, and visible/all-item counts.
- `crates/core/src/lib.rs`: `Shot`, `Line`, normalized `Rect`, app data path helpers, and the single crop rule shared by thumbnails and highlight geometry.
- `crates/core/src/config.rs`: user config serialization, theme choice, folder defaults, and broad-folder guard.
- `crates/ocr/src/{lib,det,rec}.rs`: CPU inference, screenshot-tuned resize/detection, recognition batching, CTC decode, confidence filtering, and normalized OCR boxes.
- `crates/ocr/src/models.rs`: pinned model checksums and atomic cache writes. Its Linux ONNX Runtime handling needs a platform adapter before macOS use.
- `crates/cli/src/indexer.rs`: file discovery, mtime skip logic, image validation, OCR orchestration, and thumbnail generation. Extract the reusable service from the binary rather than making Tauri invoke the CLI process.
- `crates/app/src/images.rs`: useful memory-bounded image decode/cache behavior and thumbnail rendering strategy. The GPUI `RenderImage` output itself is not reusable by a web frontend; the cache limits and decode policy are.
- `crates/app/src/grid.rs`: layout logic can inform the web UI, though React/CSS should own presentation and scrolling.
- `crates/app/src/spring.rs`: timing behavior is a useful interaction reference, not a required Rust dependency for a CSS/React UI.
- Existing SQLite and OCR unit tests provide a regression starting point after build access is restored.

The database index should continue to be the source for exact/substring OCR search. New concepts should layer on top of it, not replace it with embeddings or a second indexing system.

## 3. Existing limitations

### Platform

- The app dependency enables GPUI's `wayland` and `x11` features. `main.rs` uses Wayland layer-shell types. Neither is a macOS app integration.
- `resident.rs` uses Unix domain sockets and the app's launch lifecycle is designed around a Linux launcher shortcut, not a menu bar app.
- `setup.rs` installs systemd user units or XDG autostart entries, reads Linux `/proc`, and invokes `systemctl`.
- `core/src/trash.rs` imports Unix filesystem and OS-string APIs and implements Linux trash behavior. It is not a cross-platform macOS trash implementation.
- `indexer::become_idle` is Linux-specific, although it is conditionally compiled.
- `models.rs` only supplies Linux `.so` runtime URLs for x86_64 and aarch64. The model URLs may be portable, but the runtime setup is not a macOS `.dylib` solution.
- There is no Tauri config, frontend package manifest, React/TypeScript app, macOS entitlement/plist setup, signing setup, or macOS packaging workflow in the listed checkout.

### Product and data model

- The domain model contains screenshots and OCR lines only. It has no capture, memory, user-provided rationale, categories, tags, reminder/task, action, or interpretation records.
- Folder watching is not intentional capture. There is no screen, window, region, clipboard, selected-text, or global-shortcut capture flow.
- Search is text-only. It has no memory metadata filters, natural-language query interpretation, semantic ranking, or timeline grouping.
- The UI has search, detail, onboarding, and settings pages, but no memory detail, review/confirmation, task, reminder, or export screens.
- OCR/session work and most SQLite calls are synchronous. A Tauri command must not run large image decode, OCR, full scans, or broad search work on the UI command path.

### Persistence and error behavior

- `Index::init` has schema version `1` and drops the existing index tables when the version differs. This is acceptable only because the current database is treated as rebuildable OCR cache. It is not acceptable once memories or user-authored context are persisted.
- There is no migration framework or legacy namespace discovery. The recent app rename moved the default app data namespace to `akshat`; old Gyotaku data is not automatically opened by the current default path.
- Config parse failure can be hidden by `load_or_default`, which falls back to defaults. A new UI should surface recoverable config errors rather than silently implying saved settings were loaded.
- Some UI paths use `unwrap_or_default` or discard errors for optional thumbnails. That is reasonable for decoration, but command/API boundaries need typed, user-safe errors.
- Deleting a screenshot from a watched folder prunes its index row. Existing trash flows move files to a reversible system trash, but that policy must be made explicit before introducing separate memory deletion and source-image deletion.

## 4. What must remain untouched in the first integration step

- Do not remove or replace the existing core, OCR, CLI, watcher, or GPUI packages while proving Tauri integration.
- Do not change the FTS5 schema or its current query semantics as part of the Tauri shell.
- Do not make the frontend open SQLite directly or duplicate OCR/indexing logic in TypeScript.
- Do not silently move, rewrite, delete, or re-encode user screenshots during import or migration.
- Do not let AI output trigger shell commands or unconfirmed actions.
- Keep the current GPUI app buildable as a reference/fallback until the Tauri vertical slice is validated on macOS.
- Preserve source screenshots as evidence and keep OCR text and any future memory/interpretation records linked to the source capture.

## 5. What should be refactored, minimally

1. Extract index/import operations from the CLI binary into a library-facing service that Tauri and the CLI can both call.
2. Separate platform services from reusable domain/storage code: runtime loading, screenshot capture, background startup, global shortcut, notifications, clipboard, file reveal/open, and trash.
3. Replace schema-version drop/recreate behavior with forward-only migrations before adding durable memory records.
4. Add typed domain models and DTOs rather than exposing SQLite row layouts or frontend-shaped arbitrary JSON.
5. Make long-running indexing and OCR jobs asynchronous from the UI's perspective, with progress/cancellation messages and bounded work queues.
6. Make app data namespace resolution explicit and able to discover old Gyotaku locations without writing to them until the user consents to migration.

Do not reorganize all crates solely to match a conceptual directory tree. Introduce boundaries where the macOS/Tauri integration needs them.

## 6. Tauri integration strategy

Create a separate Tauri application package while leaving the existing GPUI app in place. The Tauri Rust side should depend on `akshat-core` and a refactored indexing/application library, not on the GPUI crate. The frontend should call typed Tauri commands and subscribe to progress events; it should not access SQLite, arbitrary filesystem paths, or a shell directly.

Initial command surface:

| Command | Purpose |
|---|---|
| `search(query, limit)` | Run existing text search and return result DTOs, including match lines only as needed |
| `get_screenshot(id)` | Return metadata and a scoped image reference for the original screenshot |
| `get_recent(limit)` | Return recent indexed/captured items |
| `index_screenshot(path)` | Import one user-selected image into the existing indexing pipeline |
| `get_stats()` | Return index counts and current processing state |
| `get_settings()` / `update_settings(...)` | Read and validate explicit settings |

Keep app commands narrow and typed. Use a worker/actor or blocking task pool for SQLite and OCR work. Do not hold a global database mutex while OCR runs. Separate DB transactions from file decode/inference, and ensure the UI can remain responsive while work continues.

Configure Tauri with a strict local content security policy, no remote frontend origins, minimal plugin permissions, scoped asset access, and no generic shell plugin. Treat every path and URL coming from the frontend as untrusted and validate it in Rust.

## 7. macOS capture strategy

The MVP should start with explicit user-initiated region capture and a user-selected image import path. The platform layer should request macOS screen-recording permission and use Apple's screen capture APIs through a small isolated native bridge or a vetted Tauri-compatible plugin. Do not begin with continuous capture, accessibility scraping, selected-text extraction, or clipboard monitoring.

The first capture flow should be:

```text
user invokes explicit capture shortcut
  -> native capture picker/region selection
  -> capture written atomically to Akshat's capture store
  -> capture ID returned to Rust service
  -> existing image validation/OCR/indexing path
  -> progress event and saved confirmation
```

Capture permission should be requested only when capture is invoked, with clear UI explanation. Cancel and permission denial must leave the existing index and files unchanged. Full-screen and window capture can follow the region slice. Global shortcut registration and menu bar presence should be isolated behind a macOS platform service so they can be tested independently from OCR and storage.

The new capture store must have a documented ownership rule. Imported images should remain at their original path. App-created captures may live in Akshat's data directory, but deleting a memory must not implicitly delete its source capture. A separate confirmed delete-source operation can be added later.

## 8. Database migration strategy

Do not extend the current destructive schema-version reset. First establish a migration system that records applied migration versions and runs each migration transactionally. Preserve `shots`, `lines`, and FTS rows. Validate migrations against copies of the current database schema and retain a backup/rollback path before changing a real user database.

The current database already has `shots.id` and per-shot OCR lines, so an initial memory relationship can reference the shot row rather than duplicate image/OCR data. A conceptual evolution is:

```text
shots (existing source record)
  1 -> 0..n memories
memory_sources or source_capture_id -> shots.id
memory_tags -> memories.id
actions -> memories.id
```

Whether the relationship is one-to-one or one-to-many should be decided with product behavior. The schema should distinguish app-captured sources from imported files without rewriting either. Use stable IDs, explicit timestamps, foreign keys, and indexes for common timeline/status filters. Store model/provider provenance and confidence with the interpretation, not as an implicit property of OCR rows.

Backward compatibility should first locate existing `akshat` and legacy Gyotaku data directories using platform-aware path resolution. Open/copy from the old location read-only during preview, show the user the discovered index and screenshot count, then perform an explicit transactional database copy/migration. Do not delete the old database or screenshots after migration. If the old index can remain in place and be opened without destructive migration, prefer that over copying. Preserve original configured folders and let the user review them.

## 9. Memory architecture

Add a first-class `Memory` domain object in a core/application layer, distinct from `Shot` and `Line`. Keep the initial model small and extensible:

```text
Memory
  id
  source_shot_id
  created_at / updated_at
  title / summary
  user_why
  category
  status
  tags
  confidence
  interpretation_version / provenance
```

Avoid a rigid category enum that requires a schema migration for every new label. A validated string or normalized category table is sufficient initially. Entities, deadlines, extracted URLs, and inferred intent can be separate structured fields/tables as they become useful. Preserve raw OCR separately and never replace source OCR with generated summary text. User-edited values must be distinguishable from model suggestions so reprocessing cannot overwrite user intent.

Lifecycle state should represent capture/indexing/review separately from a memory's user status. A processing job should not be modeled as a user task. Low-confidence interpretation should remain `needs_review` or `needs_clarification`; it must not become a confirmed deadline or action automatically.

## 10. AI architecture

Define a replaceable `UnderstandingEngine` service at the application boundary. Input should include an existing `CaptureContext`, source-shot ID, OCR text, bounded image access, optional user-provided `user_why`, and relevant settings. Output should be a validated `Interpretation` proposal with provenance and confidence. The UI or an explicit user rule decides which fields become confirmed memory data. The service may enrich a Memory that already exists; it cannot gate Memory creation, persistence, indexing, or retrieval.

The complete no-AI path is capture/import, OCR, optional user context/Why, Memory creation, and search, including original screenshot view. Start with a disabled/unavailable implementation and an interface, not a large model dependency. Add local Core ML/MLX/ONNX or a remote provider only after the product slice, privacy controls, size/performance costs, and macOS packaging have been evaluated. No screenshot or OCR text leaves the device unless a future provider is explicitly enabled and clearly disclosed.

Treat model output as untrusted data. Parse into bounded types, reject malformed fields, cap string/list sizes, and never interpret generated text as a command, filesystem path to open, URL to navigate, or action to execute without validation and confirmation.

## 11. Action architecture

Keep an `Action` separate from `Memory` and from action execution. An initial record can contain an ID, memory ID, action type, display title, validated payload, status, creation/schedule timestamps, confirmation requirement, and result/error. Initial handlers should be narrow capabilities such as create a local reminder/task, open a validated HTTP(S) URL through Launch Services, copy selected text, or create a local note.

Use an explicit registry of typed action handlers. Do not execute arbitrary shell strings. AI may suggest a typed pending action; the UI presents the exact action and payload; only a user confirmation transitions it to executable. Record outcomes for auditability. Keep reminder scheduling behind a platform service using macOS notifications, with a no-notification fallback that leaves the action visible in the app.

## 12. UI architecture

Build the React UI around the smallest understandable shell: a sidebar or compact navigation for All/Recent, Tasks or Reminders when they exist, and Settings. The primary surface should be a searchable recent-memory timeline. A result should show the source thumbnail, title or OCR fallback, date, concise summary/why when present, category/tags when present, and action status. The detail surface should always expose the original screenshot and distinguish OCR evidence, user-authored context, and generated interpretation.

Prioritize the first complete UI flow: search existing index, import one screenshot, view source and OCR, add/edit a title and optional why, save a memory, and find it again. Capture overlay, AI interpretation, action confirmation, and timeline can be layered in that order. Support system light/dark appearance, macOS keyboard navigation, VoiceOver labels, focus visibility, and reduced motion. Do not reproduce the existing GPUI visual hierarchy by translating every component literally; reuse interaction lessons and image-loading limits.

The frontend should not own domain state that can conflict with the Rust database. It may own transient view state, filters, and draft edits. Persist durable state through typed commands and handle progress/errors as events.

## 13. Security model

- User action is required for every capture. No background screen recording or silent capture.
- Capture permission is requested only for capture features and denial is a normal state.
- SQLite and image files are accessed only through Rust services. Frontend paths are validated and asset access is scoped.
- No unrestricted shell execution, arbitrary AI-driven automation, or generic command endpoint.
- Validate external URLs and present their target before consequential navigation/action.
- Keep local data local by default. No telemetry or cloud account is necessary for core behavior.
- Downloaded model/runtime artifacts remain checksum-verified and atomically installed. Reassess package provenance and macOS artifact hashes before adding downloads.
- Memory deletion and source-image deletion are distinct actions with clear confirmation. Bulk destructive operations should be undoable where practical.
- Keep secrets out of the frontend bundle. If optional provider credentials are ever supported, use macOS Keychain-backed storage and explicit opt-in.

## 14. Performance risks

- OCR model/runtime compatibility and model load time on Apple Silicon and Intel Macs.
- Large image decode peaks, especially during import. Keep current allocation limits and do not load original-resolution images into result grids.
- SQLite calls and OCR on Tauri's command/UI executor. Move heavy work to bounded workers and emit progress.
- Concurrent capture, folder watcher, and app imports. Use idempotent source IDs, mtime/content checks, WAL, and serialized writes or independent connections.
- Large FTS result sets and per-row OCR fetching. Preserve lazy matched-line loading, add SQL indexes for new filters, and paginate/virtualize the UI.
- Memory/thumbnail cache growth in a long-running menu-bar process. Retain bounded LRU caches and instrument memory across long sessions.
- A local multimodal model may dominate disk, memory, startup, and battery. It is optional and outside the first vertical slice.
- Packaging, code signing, privacy usage descriptions, screen-capture permission, and update flow are new release engineering work.

## 15. Proposed implementation phases

1. **Phase 0, reconnaissance:** this document; no behavior changes. Complete.
2. **Phase 1, Tauri shell:** add a separate Tauri/React/TypeScript app and prove a macOS window can call a harmless typed Rust command. Keep GPUI intact. Complete; implementation record below.
3. **Phase 2, core adapter:** extract reusable indexing/search services; expose search, recent, stats, image retrieval, and one-file indexing. Keep OCR and FTS behavior unchanged.
4. **Phase 3, basic UI:** Apple-native-feeling search/timeline, screenshot detail, import, loading/empty/error states, system theme, keyboard and VoiceOver basics.
5. **Phase 4, capture:** explicit region capture and global shortcut, permission flow, atomic local source creation, and handoff to the shared indexer.
6. **Phase 5, safe persistence:** introduce forward migrations and legacy data discovery/preview before adding memory tables.
7. **Phase 6, memory review:** title, summary, optional why, category/status/tags, source linkage, editing, and provenance.
8. **Phase 7, understanding:** optional replaceable local understanding engine with confidence and user review.
9. **Phase 8, actions:** typed pending actions, explicit confirmation, limited handlers, and auditable outcomes.
10. **Phase 9, retrieval and polish:** metadata filters, timeline grouping, optional semantic layer, export/delete controls, packaging, accessibility, performance, and migration/regression coverage.

At each phase, preserve the existing CLI and FTS workflow until the new path has equivalent coverage and a demonstrated benefit. Phase 2 and later remain unstarted pending review of Phase 1.

## 16. Files and modules proposed for change

Likely existing files to adapt in later phases:

- `Cargo.toml`, `Cargo.lock`: add workspace/package dependencies and frontend/Tauri tooling without replacing existing packages.
- `crates/core/src/index.rs`, `crates/core/src/lib.rs`: migration support, stable source IDs, new domain/storage APIs, while retaining existing FTS behavior.
- `crates/core/src/config.rs`: platform-aware settings path and new settings fields with defaults.
- `crates/ocr/src/models.rs`, `crates/ocr/src/lib.rs`: platform-specific ONNX Runtime loader and availability/errors; keep OCR pipeline API stable.
- `crates/cli/src/indexer.rs`: extract a reusable indexing service from the CLI executable boundary.
- `crates/cli/src/watch.rs`: retain legacy import/watch support and share the same indexing service; isolate or disable platform-specific priority behavior.
- `crates/core/src/trash.rs`, `crates/app/src/setup.rs`, `crates/app/src/resident.rs`, `crates/app/src/main.rs`: keep Linux-specific behavior scoped to the existing GPUI/Linux app rather than pretending it is the macOS implementation.
- Existing `crates/app/src/app.rs`, `images.rs`, `grid.rs`, `input.rs`, `theme.rs`: reference/retire only after equivalent Tauri behavior is proven; do not port GPUI abstractions mechanically.

## 17. Files and modules created for Phase 1

Phase 1 added the following isolated shell files:

```text
apps/desktop/                 Tauri configuration and app entry point
apps/desktop/src/              React/TypeScript application
apps/desktop/src-tauri/        Tauri Rust commands, capabilities, macOS packaging config
crates/application/            typed services, DTOs, workers, memory/understanding/action boundaries
crates/core/src/migrations/    versioned transactional migrations
crates/platform-macos/         capture, shortcut, notifications, open/reveal, appearance adapters
docs/DEVELOPMENT.md            frontend/backend setup and development workflow
docs/PRIVACY.md                storage, capture permission, local processing, optional AI disclosure
docs/DATA_MODEL.md             source, memory, action, and lifecycle model
docs/MIGRATIONS.md             legacy discovery, backup, migration, and recovery behavior
docs/MACOS.md                  permission, build, signing, and packaging notes
```

The Phase 1 shell is its own Cargo workspace under `apps/desktop/src-tauri`. This keeps its dependency graph limited to Tauri and `akshat-core`; it does not resolve or pull in GPUI. Future application/platform crates remain proposals only.

## 18. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Core does not compile on macOS because Linux APIs and runtime binaries are embedded in current packages | Establish a compile-only platform matrix; split/gate platform adapters; do not pull `akshat-app` into the Tauri dependency graph |
| Existing schema reset deletes data when schema version changes | Implement transactional forward migrations and test against copied real-schema databases before introducing any persistent memory data |
| Rebrand already changed the default data namespace | Detect legacy locations read-only, explain what was found, preview counts, and migrate only after explicit approval; never remove the original |
| GPUI and Tauri overlap causes two competing apps or duplicate background services | Keep GPUI as a development/reference target, define one shared indexer and one app data contract, and defer decommissioning until parity is verified |
| Webview gains broad filesystem or shell capability | Use strict Tauri capabilities/CSP, scoped asset protocol, typed commands, no generic shell bridge, and validation in Rust |
| Screen capture permission UX is denied or unavailable | Treat denial as normal; keep import/OCR/search useful and do not change data on cancellation |
| AI invents deadlines or actions | Preserve OCR/source evidence, attach confidence/provenance, require review, and require explicit confirmation for every action |
| Product becomes a surveillance or cloud-dependent app | Capture only after explicit user invocation; no continuous recording, telemetry, mandatory account, or default upload |
| Optional local model overwhelms a small Mac | Keep understanding optional and replaceable; benchmark before enabling and always retain the OCR-only path |
| Existing test/build state is not demonstrated in this environment | Restore dependency availability, run workspace checks and regression tests before behavior changes, then exercise capture/index/search on real macOS hardware |

## Phase 1 implementation record

### Clarifications now part of the architecture

- Memory creation and retrieval do not depend on AI. The complete no-AI path is Capture/Import -> OCR -> optional user context/Why -> Memory -> Search. AI can enrich an existing Memory but cannot gate its creation or retrieval.
- `user_why` is optional. The UI can ask "Why are you saving this?" or "What should I remember about this?" and must allow saving without an answer.
- `CaptureContext` is a lightweight source-observation boundary containing source type, timestamp, optional safely available application/window and URL context, optional capture-region geometry, an OCR reference, and optional user-provided context/Why. This phase does not collect any of that advanced platform context.

### Phase 1 files changed or created

- `ARCHITECTURE.md`: added the three clarifications, implementation record, and exact verification state.
- `README.md`: points to the new shell and states that capture/memory functionality is not implemented there.
- `.gitignore`: excludes frontend dependencies/build output and the nested Tauri target/generated schemas.
- `apps/desktop/package.json`, `package-lock.json`, `index.html`, `tsconfig.json`, `vite.config.ts`: React/TypeScript/Vite frontend and pinned Tauri JS packages.
- `apps/desktop/src/main.tsx`, `src/style.css`: minimal system-styled welcome screen. It invokes one typed command on launch and displays the response.
- `apps/desktop/src-tauri/Cargo.toml`, `Cargo.lock`, `build.rs`, `src/main.rs`, `src/lib.rs`: an isolated Tauri Rust app depending on `akshat-core` but not GPUI.
- `apps/desktop/src-tauri/tauri.conf.json`, `capabilities/default.json`: window, frontend build/dev URLs, restrictive CSP, and core-only capability.
- `apps/desktop/src-tauri/icons/icon.svg`, `icons/icon.png`: source and generated application icon required by the Tauri build context.
- `apps/desktop/README.md`: macOS prerequisites and shell run command.

The only exposed command is `check_core_connection() -> Result<CoreConnection, String>`. It calls `akshat_core::data_dir()` and returns a serializable response with `applicationName` and `coreConnected`. Resolving this path does not open SQLite or write user data. A debug-build log confirms the command reached Rust through Tauri IPC.

### Existing code left unchanged in Phase 1

- `crates/core`, `crates/ocr`, and `crates/cli` source and package manifests.
- `crates/app`, including the existing GPUI window and Linux integration.
- SQLite schema and FTS5 query semantics.
- Screenshot folders, indexes, OCR models, and user configuration.
- No memory/action schema, capture implementation, AI, embeddings, automation, or cloud service was added.

### Verification

- `npm install`: passed; zero reported vulnerabilities. Tauri JS API/CLI packages are pinned to minor version 2.11 to match the resolved Rust Tauri 2.11 release.
- `npm run build`: passed (TypeScript and Vite production bundle).
- `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml`: passed on macOS and compiled `akshat-core` as a Tauri dependency without GPUI.
- `npm run tauri dev`: launched the macOS app. The React view invoked `check_core_connection`, and Rust printed `check_core_connection invoked through Tauri IPC`. The process is no longer running in the current process list.
- `cargo check --workspace --locked`: attempted but could not finish because Cargo stalled while fetching the pinned Zed/GPUI Git repository. The offline retry confirmed that revision is not cached. Therefore, the legacy workspace as a whole is not verified in this environment.
- No Rust test suite was run. No capture/OCR/search behavior was exercised because Phase 1 adds no capture path.

Phase 2 has not started. Review the shell and this report before proceeding.
