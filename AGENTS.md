# Compositor port

This project ports Robbie Tilton's Compositor to a TypeScript/QuickGUI UI and a Rust engine. The user explicitly wants as little independent invention as possible.

## Required architecture

The user has explicitly chosen **TypeScript/JavaScript for UI only; Rust for the editor engine and image processing**. This is a requirement, not an optimization to defer until profiling. Read [docs/architecture.md](docs/architecture.md) before implementing features.

- TypeScript owns views, controls, dialogs, shortcuts, transient form state, presentation, and the thin native bridge. Rust owns authoritative document/layer/mask state, editing commands, transforms, history, pixel buffers, brushes, filters, compositing, codecs, project persistence, and exports.
- Put new engine implementations in Rust under `crates/`. Do not implement an engine feature in TypeScript first, add a JavaScript fallback, or move engine algorithms into UI/bridge helpers. Calling a native drawing library from a TypeScript engine does not satisfy this boundary.
- The old `src/core/` engine has been removed. There are zero legacy exemptions in [scripts/architecture-policy.json](scripts/architecture-policy.json). Do not recreate it, add an exemption, or introduce a JavaScript engine fallback. Generate bridge contracts from the Rust types with `npm run build:native`.
- Cross the bridge with typed commands, batched pointer samples, opaque resource IDs, and small metadata snapshots. Rust retains image/mask buffers and owns their lifetime. No per-pixel JS calls, full-image JSON/base64 transport, or new PNG-encoded preview loops. Treat native presentation integration as required follow-up work, not as permission to add a new JS raster path.
- Keep Compositor's pinned algorithms and fixtures as the reference when translating Swift to Rust. A language change does not authorize new editor semantics. Preserve existing project readability and the explicit Shift-to-preserve resize behavior.
- `npm run check:architecture`, `npm run check`, and `npm test` must pass. Dev/build/test entry points run the architecture guard; CI runs the guard tests and application tests. The guard catches structural violations; review must also reject engine logic disguised as UI code. Keep Rust behavior tests and actual Node/Bun addon integration tests passing.

## Source fidelity and verification

- Read the corresponding upstream implementation and tests before changing editor behavior. Translate the existing algorithm, data semantics, defaults, and workflows when feasible; use new code primarily to bridge QuickGUI, TypeScript, and the rendering backend.
- Use the pinned revision and source map in [docs/compositor-port.md](docs/compositor-port.md). If the reference checkout is missing, obtain that revision of https://github.com/robbietilton/Compositor outside this repository. Do not silently switch reference revisions.
- Port applicable upstream test fixtures alongside the implementation. Distinguish translated upstream fixtures from additional local tests. Passing local tests alone does not establish complete upstream parity.
- Keep source references and the MIT attribution when translating code. List necessary adaptations and remaining deviations in the source map. Do not describe independently written behavior as a direct port.
- Preserve explicit user choices, including TypeScript/QuickGUI for the UI, Rust for the engine, and Shift for proportional resizing. Explain how these map to upstream behavior instead of silently overriding them.
- Before extending a custom subsystem, check whether the corresponding upstream subsystem should be ported instead. The custom project format, mask stroke storage, and brush engine remain prototype deviations, not requirements of the target stack.
- Verify native interaction changes in the running app and inspect actual screenshots, especially alignment. Keep temporary test files under ignored `artifacts/`.
