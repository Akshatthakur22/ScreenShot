# Implementation Status

PROJECT IMPLEMENTATION STATUS
=============================

Original system preserved: 100% of the five audited subsystem categories remain in source (core, OCR, CLI, SQLite/FTS5, GPUI); behavior-level regression verification is incomplete.

New product implemented: 11% by phase count (Phase 1 of the nine implementation phases after Phase 0); this is a phase-count indicator only. Capture-to-memory product functionality is 0%.

Working end-to-end vertical slice: NO

Current phase: Phase 1 — Tauri shell proof

Phase completion: 100% of the narrow React → Tauri → Rust IPC proof goal; the required root workspace build remains unverified because the pinned GPUI dependency could not be fetched.

Biggest completed feature:
The React shell invokes a registered Tauri command, which calls the existing Rust core to resolve its data directory and returns a typed status.

Biggest missing feature:
User-initiated capture/import through a persisted, user-authored Memory searchable in the app.

Biggest architectural risk:
The existing schema version reset drops and recreates `shots`, `lines`, and `shots_fts`. It must be replaced with preserving migrations before storing durable user-authored records there.

Biggest bug:
No runtime bug was proven by this audit. The concrete data-loss hazard is the destructive schema reset, which would erase indexed OCR data on a schema-version change; original image files are not dropped by that SQL.

Next thing we should implement:
Phase 2: a narrow Rust application/core adapter exposing existing index capabilities to Tauri, with typed DTOs and work kept off the UI command path.

DO NOT IMPLEMENT YET:
AI, embeddings, actions/automation, cloud services, screenshot capture, or Memory persistence. Establish migration and adapter boundaries before adding durable product data.

## 1. Executive Summary

The original Rust screenshot indexer and GPUI application remain in the repository. The new code adds an isolated macOS Tauri + React + TypeScript landing shell. Its only command is `check_core_connection`; it resolves the Akshat data directory and returns a status object. It does not open the database or prove OCR/search is connected to Tauri.

The phase-one IPC path was exercised during the preceding implementation session: the Tauri app launched and its Rust debug output recorded `check_core_connection invoked through Tauri IPC`. The current TypeScript typecheck and both workspace metadata inspections pass. A fresh full root build/test was not completed in this audit. The pinned GPUI Git dependency was unavailable to the earlier root workspace check, so preservation is established by source presence, not a passing current root build.

There is no new capture, CaptureContext implementation, Memory model, optional Why field, AI engine, action system, or memory search UI. Those concepts exist in architecture documentation only. The existing OCR/FTS5 search system is a separate legacy flow used by the CLI and GPUI app.

## 2. Current Product State

| Area | Actual state |
|---|---|
| Existing screenshot index/search | Rust CLI and GPUI code for indexing selected folders, running OCR, persisting shot/line data, and searching OCR text. Runtime behavior was not re-exercised end to end in this audit. |
| New desktop application | One welcome/status screen in Tauri; no product workflow. |
| User capture | Not implemented. |
| Memory and user intent | Not implemented. |
| AI and actions | Not implemented. |
| Product vertical slice | 0/10 steps implemented for the requested capture-to-memory flow. |

The welcome screen's “Private visual memory” and “A quieter way to remember” are product positioning, not evidence of memory functionality.

## 3. Actual Architecture

```text
New desktop shell:
React App (local state)
  -> @tauri-apps/api/core invoke("check_core_connection")
  -> registered Tauri Rust command
  -> akshat_core::data_dir()
  -> typed { applicationName, coreConnected } response
  -> React status text

Existing screenshot search system:
configured image folders
  -> akshat CLI indexer / filesystem watcher
  -> akshat-ocr PP-OCRv6 + ONNX Runtime
  -> akshat-core SQLite shots/lines + FTS5
  -> akshat-app GPUI search and screenshot display
```

The Tauri crate depends on `akshat-core`, not GPUI or `akshat-ocr`. There is no Tauri service layer wired to search, indexing, OCR, or SQLite. The Tauri command checks that the core can resolve a data directory; it does not instantiate `Index`.

## 4. Repository Structure

| Path | Responsibility / audit finding |
|---|---|
| `crates/core` | Rust types, config/data paths, SQLite index/FTS5, thumbnail geometry, reversible trash. |
| `crates/ocr` | PP-OCRv6 detection/recognition, model downloads, ONNX Runtime setup. |
| `crates/cli` | `akshat` CLI, image folder indexing, background watcher. |
| `crates/app` | Existing GPUI search application and Linux-oriented integration. |
| `apps/desktop` | New Vite + React + TypeScript Tauri frontend and nested Rust workspace. |
| `docs`, `README.md`, `ARCHITECTURE.md` | Existing docs plus current architecture plan/clarifications. |
| `.gitignore` | New ignore rules for common build outputs. |

The root Cargo workspace has four members (`akshat-core`, `akshat-ocr`, `akshat`, `akshat-app`). The Tauri package declares its own nested workspace and is excluded from root workspace membership. This keeps GPUI out of the Tauri dependency graph.

## 5. Original System Preservation

The five audited major areas remain present: core, OCR, CLI, SQLite/FTS5, and GPUI. Their source files are tracked at the current HEAD and were not modified in the working tree shown by `git status`. The root README has a two-line addition describing the Phase 1 shell. New untracked project files are `.gitignore`, `ARCHITECTURE.md`, and `apps/`.

The repository exposes only one commit in the inspected history (`6834388 v1`). Thus Git history cannot reliably separate earlier Akshat rebranding from the original Gyotaku code, and cannot establish a more granular “before/after implementation” file list. No deleted files are reported in the working tree. Existing behavior is not declared regression-free because the complete root build and tests were not run successfully in this audit.

## 6. New Implementation

Added Tauri v2 configuration, a nested Rust package, a React/TypeScript/Vite frontend, one Rust IPC command, one default capability, and icon assets. The frontend has a single static welcome view plus local connection/error state. The Rust command returns `Akshat` and `coreConnected: true` if `akshat_core::data_dir()` succeeds.

This is a shell proof, not an integrated screenshot or memory application. `bundle.active` is false, so the current config does not produce a distributable app bundle.

## 7. Tauri Status

- Tauri dependency: major version 2; lockfile resolves the v2 line.
- Location: `apps/desktop/src-tauri`.
- Registered command: `check_core_connection` only.
- Frontend call: `invoke<CoreConnection>("check_core_connection")` in `apps/desktop/src/main.tsx`.
- Error handling: promise rejection is rendered as a local error string; component cleanup prevents state update after unmount.
- State/events: local React state only; no Rust-managed state or event stream.
- Permissions: `core:default` for the `main` window; no filesystem or shell plugin capability is declared.
- CSP: local self sources, Tauri IPC connect endpoints, and inline style allowance. It is not a remote-hosted frontend configuration.
- Meaningful result: confirms the core data-directory helper succeeds, not database connectivity or availability of index/search.
- Runtime evidence: the app was launched on macOS in the preceding implementation session; the Rust IPC debug line was observed. This audit did not produce a fresh screenshot or independently inspect the rendered result.

Command classification: `check_core_connection` is REAL for the narrow shell-health check, and PARTIAL as a product-core connectivity claim. It does not fail immediately under the observed run.

## 8. React Status

One screen exists: a welcome/status screen. There are no Home/Search/Recent/Memory/detail/Capture/Tasks/Reminders/Settings screens. It uses no mock record list, SQLite data, or screenshot data. Only connection/error state is dynamic, obtained from the Tauri command. The rest is hardcoded copy and status presentation. It is not a functional screenshot-memory UI.

## 9. Rust/Core Status

The original Rust crates remain separate and the Tauri package links only `akshat-core`. Tauri business operations for indexing/search have not been created. `check_core_connection` is intentionally a proof command, not a service adapter. Existing CLI and GPUI responsibilities remain in their existing crates.

## 10. OCR & Search Status

The original code includes PP-OCRv6 detection and recognition, image decoding, confidence/bounding boxes, model caching/checksums, and SQLite persistence. The CLI indexer invokes OCR and writes the index. GPUI uses the core search results and displays matching screenshots/lines. The Tauri shell does not invoke OCR, index, or search.

The OCR model artifacts are fetched on first use. `crates/ocr/src/models.rs` supplies Linux `.so` runtime downloads for x86_64 and aarch64; it does not provide a macOS `.dylib` runtime. Therefore the existence of a macOS Tauri window does not demonstrate macOS OCR support.

FTS5 uses a trigram tokenizer and one FTS row per screenshot. Search requires each whitespace-separated query term to appear, with short terms falling back to escaped `LIKE`. No change to these semantics appears in the current working diff. Search modes such as semantic, embedding, AI-assisted, title/Why/tag/category search are absent.

## 11. Screenshot/Capture Status

The legacy CLI imports/indexes files from configured image directories. That is not a native screen capture feature. No capture shortcut, region selector, ScreenCaptureKit bridge, screenshot permission flow, capture-store write, source identity generation, atomic native capture, or capture confirmation flow is implemented. All steps in the requested new capture flow are NOT IMPLEMENTED. The source does include thumbnail generation for indexed images; that is not capture storage.

## 12. CaptureContext Status

`CaptureContext` is described in `ARCHITECTURE.md` as a future boundary. It is not a Rust type, database table, serialization type, API payload, or UI state. Timestamp, source type, application/window, URL, capture region, OCR reference, and optional user context are therefore not stored by a new capture pipeline. Existing `shots` hold image path, mtime, dimensions; OCR lines are related to shot ID. That is the existing indexing model, not CaptureContext.

## 13. Memory Status

No first-class Memory entity exists in source. There is no Memory table/model/service/API/UI, no Memory-to-shot relationship, no title/summary/status/tags/category/provenance fields, and no support for multiple memories per source. `Shot`, `Line`, and search `Hit` are evidence/index objects, not Memory.

## 14. Why/User Context Status

There is no optional Why field or user context UI/storage. The two approved UX phrasings and optionality are architecture documentation only. No data survives reload because the field and persistence do not exist. This requirement is not implemented.

## 15. AI/Understanding Status

No AI provider, local model, `UnderstandingEngine`, interpretation contract, validation/provenance layer, or model inference path exists. OCR is conventional text extraction, not an AI memory-understanding layer. Classification: NOT IMPLEMENTED. No AI output can execute actions because neither AI nor actions exist.

## 16. Action System Status

No Action entity, registry, typed handler, confirmation UI, execution/outcome flow, tasks, or reminders exists. The repository has no product automation flow and no arbitrary shell command feature exposed by the Tauri app. Existing legacy setup code invokes platform utilities for Linux startup/settings behavior; it is unrelated to AI-driven actions.

## 17. Database Status

### Current tables and columns

| Table | Columns | Keys / indexes |
|---|---|---|
| `shots` | `id INTEGER`, `path TEXT`, `mtime INTEGER`, `width INTEGER`, `height INTEGER` | `id` primary key; `path` unique and not null. |
| `lines` | `id INTEGER`, `shot_id INTEGER`, `text TEXT`, `x REAL`, `y REAL`, `w REAL`, `h REAL`, `score REAL` | `id` primary key; `shot_id` FK to `shots(id)` with `ON DELETE CASCADE`; `lines_by_shot` index. |
| `shots_fts` | FTS5 `text` | Virtual table, trigram tokenizer; `rowid` corresponds to shot ID. |

SQLite enables WAL, `synchronous=NORMAL`, and foreign keys. Schema version is `1` (`PRAGMA user_version`). There is no migration framework. On any version mismatch, initialization drops `shots_fts`, `lines`, and `shots` and recreates them. That destroys the index/OCR rows; screenshot files are not deleted. This behavior has not been fixed. There are no Memory or Action tables and no durable user edits.

## 18. Privacy/Security Status

The new Tauri code inspected here resolves a local data directory and returns status. The UI has no direct SQLite/filesystem access. Tauri has no shell or filesystem plugin permission configured. No screen capture, clipboard monitor, accessibility scrape, screenshot upload, AI network call, or automatic action path appears in the new shell. The OCR crate does download model/runtime assets on first use from configured upstream URLs after checksum validation; screenshots are not uploaded by that code path. The welcome-screen privacy text is not treated as proof beyond inspected code.

No capture permissions are requested because capture does not exist. The configured `core:default` is the only declared Tauri capability. This is a source-level audit, not a formal security review of all transitive dependencies.

## 19. macOS Status

Classification: MACOS PARTIALLY SUPPORTED. The Tauri shell launched on macOS during the prior Phase 1 verification, and the current metadata/typecheck inspections pass. The older GPUI app is Linux-oriented (Wayland/X11, systemd/XDG setup, Linux trash/runtime assumptions). OCR runtime configuration is Linux `.so` only. There is no configured app bundle (`bundle.active: false`), signing, notarization, custom entitlements, screen-recording permission flow, Info.plist customization, menu bar integration, global shortcut, or capture API. Apple Silicon/Intel packaged application behavior has not been verified. The shell is runnable as a development app; the whole product is not macOS-ready.

## 20. Performance Status

The proof command does only data-directory resolution on the synchronous Tauri command path; it does not run OCR or SQLite queries. New frontend state is local and simple. The old CLI performs image processing/indexing in its own process; GPUI source has image cache/background decoding strategies, but search and parts of database access are synchronous. The new Tauri app has no workers, actor, queue, cancellation, or progress events because it has no indexing/search commands. No latency or memory measurements were taken in this audit.

## 21. Testing Status

The source contains 61 Rust `#[test]` functions by direct source scan. They cover core index/config/trash, OCR detection/recognition, and GPUI grid/setup/spring/app behavior. No Tauri IPC integration tests, React UI tests, database integration fixture suite, macOS permission tests, or end-to-end tests were found in the scanned Rust test attributes/source layout.

| Verification | Written | Executed in this audit | Result |
|---|---:|---:|---|
| Existing Rust unit tests | Yes, 61 annotated tests | No | Unverified here. |
| Tauri IPC automated test | No | No | Not implemented. |
| React/UI tests | None found | No | Not implemented. |
| `npx tsc --noEmit` | N/A | Yes | Passed. |
| Root Cargo metadata (`--locked --no-deps`) | N/A | Yes | Passed; reports four root packages. |
| Nested Tauri Cargo metadata (`--locked --no-deps`) | N/A | Yes | Passed; reports separate `akshat-desktop` package. |
| Nested Tauri `cargo check` | N/A | Prior phase verification | Passed previously; not rerun in this audit. |
| Tauri development launch + IPC | N/A | Prior phase verification | Launched and emitted the command debug line; no fresh launch in this audit. |
| Root `cargo check --workspace` | N/A | Prior phase attempt | Blocked fetching pinned GPUI Git dependency; offline mode also lacked checkout. Root build remains unverified. |

## 22. Git Change Analysis

- HEAD: `6834388 (HEAD -> main, origin/main, origin/HEAD) v1`.
- Working tree: modified tracked `README.md`; untracked `.gitignore`, `ARCHITECTURE.md`, and `apps/`.
- The tracked README diff adds two lines describing the desktop shell.
- The existing Rust crates are tracked and show no working-tree edits in the status output.
- No deleted paths are shown. No new commit is present beyond the one inspected.
- Git history has only the single displayed commit, so it cannot provide a reliable original-vs-new comparison before that commit. The Tauri code is newly introduced relative to the current working tree's tracked set; finer provenance of older rebranding is unavailable.

## 23. Implementation Scorecard

| Component | Status | Evidence | Location | Notes |
|---|---|---|---|---|
| Existing OCR | 🟡 PARTIAL | PP-OCRv6 code and indexing integration exist; runtime not exercised in this audit. | `crates/ocr`, `crates/cli/src/indexer.rs` | Linux runtime configuration; macOS runtime absent. |
| SQLite | ✅ IMPLEMENTED | SQLite index initialization and shot/line CRUD/search code exist. | `crates/core/src/index.rs` | No migrations; reset is destructive on version change. |
| FTS5 | ✅ IMPLEMENTED | Trigram virtual table and query code exist. | `crates/core/src/index.rs` | Existing semantics retained. |
| GPUI | 🟡 PARTIAL | Search UI and app code remain. | `crates/app` | Linux integration; root build not verified. |
| Tauri | 🟡 PARTIAL | Tauri v2 shell and registered command exist; dev launch observed previously. | `apps/desktop/src-tauri` | No product service operations or active bundle. |
| React | 🟠 SCAFFOLD / PLACEHOLDER | One welcome/status view. | `apps/desktop/src` | No product screens. |
| IPC | ✅ IMPLEMENTED | Frontend invoke reaches registered typed command; path was observed in prior runtime output. | `src/main.tsx`, `src-tauri/src/lib.rs` | Only a core data-directory status check. |
| macOS capture | ❌ NOT IMPLEMENTED | No capture API/permission/shortcut/flow. | None | — |
| CaptureContext | 🟠 SCAFFOLD / PLACEHOLDER | Concept and fields appear in architecture doc. | `ARCHITECTURE.md` | No source type or persistence. |
| Memory | ❌ NOT IMPLEMENTED | No model/table/service/UI. | None | — |
| Why | ❌ NOT IMPLEMENTED | Optionality and wording documented only. | `ARCHITECTURE.md` | No UI/storage. |
| AI understanding | ❌ NOT IMPLEMENTED | No engine/provider/interpretation. | None | OCR is separate. |
| Actions | ❌ NOT IMPLEMENTED | No action model/registry/confirmation. | None | No automation feature. |
| Human confirmation | ❌ NOT IMPLEMENTED | No proposed-action flow. | None | — |
| Search | 🟡 PARTIAL | Existing OCR FTS5 search feeds GPUI. | `crates/core/src/index.rs`, `crates/app` | Not connected to Tauri and does not search Memory metadata. |
| Migrations | 🔴 BROKEN | Version mismatch drops current index tables. | `crates/core/src/index.rs` | Dangerous before persistent user-authored data is added. |
| Privacy | 🟡 PARTIAL | Narrow local command and minimal declared Tauri capability; old app/model downloads also inspected. | Tauri config, OCR/core source | No capture exists; not a formal audit. |
| Tests | 🟡 PARTIAL | 61 Rust unit-test annotations exist. | `crates/**` | Not run in this audit; no Tauri/UI/E2E tests. |
| macOS packaging | ❌ NOT IMPLEMENTED | Bundle disabled; no signing/notarization setup found. | `tauri.conf.json` | Development shell only. |

## 24. Vertical Slice Verification

| Workflow step | Status | Finding |
|---|---|---|
| User captures screenshot | ❌ NOT IMPLEMENTED | No new capture flow. |
| Screenshot stored | ❌ NOT IMPLEMENTED | No capture store; legacy import indexes existing paths. |
| OCR | 🟡 PARTIAL | Existing OCR pipeline exists, but it is not called by Tauri and was not executed in this audit. |
| User adds optional Why | ❌ NOT IMPLEMENTED | No field/UI/storage. |
| Memory created | ❌ NOT IMPLEMENTED | No Memory entity. |
| Memory saved | ❌ NOT IMPLEMENTED | No Memory persistence. |
| Memory searched | ❌ NOT IMPLEMENTED | No Memory search. |
| Memory opened | ❌ NOT IMPLEMENTED | No Memory detail UI. |
| Original screenshot shown | 🟡 PARTIAL | Legacy GPUI displays indexed screenshots; new Tauri UI does not. |
| OCR evidence shown | 🟡 PARTIAL | Legacy search UI uses OCR match lines; Tauri UI does not. |

VERTICAL SLICE COMPLETION: 0% (0 of 10 requested new workflow steps is implemented end to end; legacy OCR/display components do not complete a Memory workflow.)

## 25. Bugs / Risks

- **Confirmed data risk:** changing the schema version drops all current screenshot/OCR index tables instead of migrating them. It does not delete screenshot files.
- **Platform gap:** PP-OCR runtime download configuration is Linux-specific, so macOS shell support must not be mistaken for macOS OCR support.
- **Unverified regression status:** current full root workspace build/tests were not completed due the unavailable pinned GPUI Git dependency in the previous build attempt.
- **Namespace compatibility:** current data directory uses the Akshat namespace; architecture docs state old Gyotaku data is not automatically discovered/migrated.
- No separate runtime defect in the narrow Tauri IPC check was observed. Do not classify missing capture or Memory features as bugs; they are unimplemented scope.

## 26. Architectural Debt

- No application/service adapter currently exposes core operations to Tauri.
- Existing index schema reset is incompatible with future durable Memory/Why data.
- OCR/runtime and other platform-specific integration is not separated into a macOS implementation.
- The React shell has no typed API layer beyond the single inline command/result type.
- Root workspace verification is constrained by the pinned GPUI source dependency not being available in the observed environment.
- There is no cross-platform integration/E2E test proving the old OCR flow or new IPC path from a clean checkout.

No React direct filesystem/SQLite access, AI coupling, arbitrary AI action execution, or broad shell permission was found in the new shell.

## 27. P0 Issues

1. Before persisting Memory or user-authored context in the current database, replace destructive version-reset behavior with forward migrations that preserve existing rows. The current behavior is tolerable only for the rebuildable OCR index assumptions documented in source.

## 28. P1 Issues

1. Restore a reproducible root workspace build by making the pinned GPUI Git dependency available in the build environment; then run existing tests before changing core behavior.
2. Build the Phase 2 typed application adapter for existing search/recent/index capabilities; do not place OCR or broad SQLite work on the synchronous IPC/UI path.
3. Plan macOS ONNX Runtime support separately before claiming the OCR feature works on macOS.
4. Define legacy Gyotaku data discovery/compatibility before changing persistent paths or user data.

## 29. P2 Issues

1. Add automated Tauri IPC, React UI, and end-to-end tests after stable commands and product screens exist.
2. Add real macOS packaging, signing, notarization, entitlements, permissions, menu-bar/shortcut integration only as their features are implemented.
3. Add bounded background jobs, progress, and cancellation when the shell begins real indexing or OCR operations.

## 30. Current Phase

CURRENT PHASE: Phase 1 — Tauri Shell

CURRENT PHASE COMPLETION: 100% of the narrow approved IPC proof goal; workspace-wide build verification is incomplete.

NEXT PHASE: Phase 2 — Core Adapter

Phase 0 architecture clarifications are reflected in `ARCHITECTURE.md`; this is documentation, not runtime functionality. Phase 2 has not started.

## 31. Exact Next Step

After review of this audit, implement only the Phase 2 adapter: expose a minimal typed read operation over the existing core (for example, counts or recent indexed shots) through a Tauri command and connect it to the React shell. Keep the adapter independent of GPUI, preserve FTS5 behavior and existing schema, and run checks available for both the nested Tauri crate and the root workspace. This is a recommendation for the next reviewed phase, not work performed here.

## 32. What NOT To Build Yet

- Do not add AI or embeddings.
- Do not add Action execution, automation, or generic shell access.
- Do not add cloud storage/backend.
- Do not implement screenshot capture until the platform/permission boundary is designed and reviewed.
- Do not persist Memory/Why fields until a non-destructive migration path is in place.
- Do not replace the existing FTS5 search semantics.
- Do not remove or rewrite the existing GPUI app, CLI, OCR, or index while the new shell is being validated.
- Do not proceed to Phase 2 automatically; review this report first.
