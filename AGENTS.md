# Compositor port

This project ports Robbie Tilton's Compositor to a Rust engine and GPUI Kit UI, following the pinned Compositor UI, retaining TypeScript/QuickGUI as a historical comparison. The user explicitly wants as little independent invention as possible.

## Required architecture

The user has authorized migrating the UI to **Rust / GPUI Kit**, with UI and workflow parity
against the existing QuickGUI application (2026-09-29). The new application is in
`crates/picsie-desktop` and is the default for development, packaging, and releases;
the QuickGUI application remains available through `dev:quickgui` and
`build:quickgui`. On 2026-09-30 the user explicitly made **Compositor the UI reference**;
follow its shell, tool controls, panel structure and workflows instead of preserving
the rough QuickGUI layout. Rust owns
the editor engine and image processing in both applications. Read [docs/architecture.md](docs/architecture.md)
before implementing features. The TypeScript boundary below still applies to the legacy UI.

- TypeScript owns views, controls, dialogs, shortcuts, transient form state, presentation, and the thin native bridge. Rust owns authoritative document/layer/mask state, editing commands, transforms, history, pixel buffers, brushes, filters, compositing, codecs, project persistence, and exports.
- Put new engine implementations in Rust under `crates/`. Do not implement an engine feature in TypeScript first, add a JavaScript fallback, or move engine algorithms into UI/bridge helpers. Calling a native drawing library from a TypeScript engine does not satisfy this boundary.
- The old `src/core/` engine has been removed. There are zero legacy exemptions in [scripts/architecture-policy.json](scripts/architecture-policy.json). Do not recreate it, add an exemption, or introduce a JavaScript engine fallback. Generate bridge contracts from the Rust types with `npm run build:native`.
- Cross the bridge with typed commands, batched pointer samples, opaque resource IDs, and small metadata snapshots. Rust retains image/mask buffers and owns their lifetime. No per-pixel JS calls, full-image JSON/base64 transport, or new PNG-encoded preview loops. Treat native presentation integration as required follow-up work, not as permission to add a new JS raster path.
- Keep Compositor's pinned algorithms and fixtures as the reference when translating Swift to Rust. A language change does not authorize new editor semantics. Preserve existing project readability and the explicit Shift-to-preserve resize behavior.
- `npm run check:architecture`, `npm run check`, and `npm test` must pass. Dev/build/test entry points run the architecture guard; CI runs the guard tests and application tests. The guard catches structural violations; review must also reject engine logic disguised as UI code. Keep Rust behavior tests and actual Node/Bun addon integration tests passing.

## Source fidelity and verification

- Keep [the feature and polish registry](docs/compositor-registry.md) current when changing editor behavior or UI. Update its stable rows in `docs/compositor-registry.json`, including source references, implementation/polish status, remaining gaps and actual verification evidence; run `npm run registry:update`. Feature existence or passing tests alone must not be marked as verified polish. The pinned source/test index is a discovery inventory, not a full-parity claim.

- GIMP is a secondary **functionality reference only**, explicitly never a UX reference. Use its engine code and tests to investigate capabilities and edge cases; keep Compositor authoritative for ported behavior, defaults, and workflows. See [docs/gimp-reference.md](docs/gimp-reference.md) for the external checkout, pinned revision, and source entry points.
- Read the corresponding upstream implementation and tests before changing editor behavior. Translate the existing algorithm, data semantics, defaults, and workflows when feasible; use new code primarily to bridge QuickGUI, TypeScript, and the rendering backend.
- Use the pinned revision and source map in [docs/compositor-port.md](docs/compositor-port.md). If the reference checkout is missing, obtain that revision of https://github.com/robbietilton/Compositor outside this repository. Do not silently switch reference revisions.
- Port applicable upstream test fixtures alongside the implementation. Distinguish translated upstream fixtures from additional local tests. Passing local tests alone does not establish complete upstream parity.
- Keep source references and the MIT attribution when translating code. List necessary adaptations and remaining deviations in the source map. Do not describe independently written behavior as a direct port.
- Preserve explicit user choices, including GPUI Kit for the new UI, Compositor as the current UI reference, QuickGUI as a historical comparison, Rust for the engine, and Shift for proportional resizing. Explain how these map to upstream behavior instead of silently overriding them.
- Before extending a custom subsystem, check whether the corresponding upstream subsystem should be ported instead. The custom project format, mask stroke storage, and brush engine remain prototype deviations, not requirements of the target stack.
- Verify native interaction changes in the running app and inspect actual screenshots, especially alignment. Keep temporary test files under ignored `artifacts/`.
- Keep reproducible flows tracked in `scripts/reproduction-scenarios.json` and follow [docs/reproducibility.md](docs/reproducibility.md). When adding/changing a flow or feature evidence, run `npm run reproduce:coverage` and `npm run check:reproduce`. Keep correctness, native visual review and performance evidence distinct; explicitly retain missing coverage and failed runs.
