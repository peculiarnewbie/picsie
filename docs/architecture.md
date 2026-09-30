# Rust engine, native desktop UIs

Accepted user requirement, 2026-09-23. Implemented for the existing editor features. This does not imply full Compositor feature parity.

On 2026-09-29, after the successful experiment, the user authorized building a
**Rust UI / GPUI Kit application with parity against the QuickGUI UI**. It lives in
`crates/picsie-desktop`, calls the existing Rust engine directly, and has its own
Cargo workspace and pinned toolkit lockfile. The desktop UI owns only presentation,
forms, native dialogs, and input mapping; its engine worker owns editor state and
all image/file work. The TypeScript rules below continue to govern the retained
QuickGUI application. See [the experiment report](gpui-kit-experiment.md) and
[the parity record](gpui-ui-parity.md).

On 2026-09-30 the user authorized making this the default application. `npm run dev`
launches Rust / GPUI Kit; `npm run build` packages its native binary. Default checks
and tests include the desktop crate. Releases build native Linux/Windows installers
and portable archives. `dev:quickgui` and `build:quickgui` preserve the explicit
parity reference; the native app has no QuickGUI, JavaScript, or Node-API runtime.

The desktop's pinned GPUI Linux backend includes a local implementation of X11
frame waking, so a completed preview can request the normal toolkit draw without
waiting for its periodic monitor timer. The timer and GPU presentation settings
retain their original values. The small patch and Apache-2.0 source attribution
are recorded in [the backend source record](../crates/picsie-desktop/vendor/gpui-pre-linux/PICSIE.md).
This changes presentation scheduling; editor semantics and the Rust engine
boundary remain the same.

## Ownership

| Responsibility                                                                  | Implementation         |
| ------------------------------------------------------------------------------- | ---------------------- |
| GPUI Kit views, controls, dialogs, shortcuts, transient forms and focus         | Rust desktop UI        |
| QuickGUI views, controls, dialogs, shortcuts, accessibility, display formatting | TypeScript             |
| QuickGUI transient forms, focus and open panels                                 | TypeScript             |
| Native loading, generated types, batched input, metadata subscriptions          | Thin TypeScript bridge |
| Documents, layers, masks, selection, edit transactions, undo/redo               | Rust                   |
| Document geometry, transforms, brushes, filters, compositing, pixel caches      | Rust                   |
| Image codecs, project validation, imports, atomic saves, exports                | Rust                   |

Rust is authoritative. The UI receives metadata with image contents and brush point arrays removed. It cannot set assets or strokes through a property patch. Opening a picker belongs to the UI; decoding the selected file belongs to Rust. Canvas-size form calculations remain transient UI state; Rust validates and applies the actual resize. `@napi-rs/canvas` is a development dependency used only for independent pixel checks and icon tooling.

## Runtime boundary

The default application:

```mermaid
flowchart LR
    UI[Rust / GPUI Kit] -->|Commands and pointer samples| Worker[Rust engine worker]
    Worker --> Core[picsie-core / CPU Skia]
    Worker -->|Sanitized metadata and completion events| UI
    Core -->|Owned BGRA pixels| Mailbox[Bounded latest-frame mailbox]
    Mailbox -->|GPUI RenderImage / GPU upload| UI
```

The worker executes commands in order, retaining all pointer samples and file
results. Only preview delivery coalesces. A bounded notification channel wakes
the UI when a frame or result is ready; there is no idle polling. The UI owns the
presented GPUI image and evicts retired atlas entries. No preview files or encoded
image transport are used. Compositing and brushes remain CPU Skia.

The retained QuickGUI reference:

```mermaid
flowchart LR
    UI[TypeScript / QuickGUI] -->|Typed commands and batched pointers| Bridge[Node-API adapter]
    Bridge --> Core[Rust editor engine]
    Core -->|Metadata snapshots| Bridge
    Core --> Skia[Native Skia surfaces]
    Skia -->|Uncompressed native frame resource| Image[QuickGUI Image]
```

`crates/picsie-core` has no dependency on JavaScript or QuickGUI. `crates/picsie-native` owns one engine per window and exposes Node-API through napi-rs. The addon is tested in Node and Bun. A static CommonJS loader lets Bun embed it in the packaged executable; no working-directory dependency or external JS engine fallback exists.

Rust model/command types generate `src/engine/types.ts` through `ts-rs`. napi-rs generates `native-api.d.ts`, including async return contracts declared on the Rust exports. Regenerate with `npm run build:native`; do not hand-edit these files. Pointer moves are batched for up to 8 ms; down/up/cancel and subsequent commands flush queued samples. Each completed editing gesture commits one Rust history transaction. History captures pixel-selection coverage and feather alongside document state; immutable coverage buffers are shared and counted once against the retained-history budget. Pixel selections remain session-only and are not serialized in the supported project formats.

Rendering, image import, canvas resize, file parsing, saves, exports and color sampling run in Node-API worker tasks. A save carries its captured revision, so later edits remain dirty. Imports reject a changed document revision instead of applying stale decoded results. Closing a window releases its native owner, rejects outstanding results, and frees surfaces and frame resources after the last worker finishes. No image bytes or per-pixel calls cross JavaScript.

## QuickGUI reference preview transport and its limits

QuickGUI 0.1.6's public `Image` node accepts an image source string. Its raw `ImageSource` API is for native window/system icons, not the canvas view, and the extension ABI has no public shared-texture primitive. The current integration therefore publishes **32-bit uncompressed TIFF frame buffers from Rust**, then sends only the path to QuickGUI's native image loader.

On Linux the private temporary directory uses `/dev/shm` when available; other systems use their temporary directory. Up to eight recent frame resources are retained per window and deleted on close. Each resource has a unique path, avoiding stale native image caches. Preview backgrounds are opaque; PNG exports still preserve transparency.

This is not zero-copy GPU sharing. Skia reads pixels into a native buffer, the resource is written, and QuickGUI loads/uploads it. There is no PNG compression, base64, or JS pixel-array handoff in this path. At 936×734 a frame occupies 2,748,096 pixel bytes, plus a small TIFF header. This is a compatibility integration with a real native copy cost; replacing it with a future QuickGUI shared-texture API remains a performance improvement, not permission to add a JS renderer. No end-to-end speedup is claimed without benchmarking.

QuickGUI decodes image paths asynchronously and paints nothing while a load is in flight, so a single `Image` node blanks the canvas on every frame swap. The UI therefore keeps the last few frames stacked in `src/ui/shell.tsx`: the last decoded frame shows through until the next finishes loading, and older nodes retire after 500 ms. This adds no latency and hides the per-frame decode gap; it does not remove the copy cost above. The long-term update-in-place options — including the open-source QuickGUI core patch path — are analyzed in [the preview transport deep dive](preview-transport.md).

## Layout and compatibility

```text
crates/picsie-desktop/     Default Rust UI, engine worker and memory preview transport
scripts/desktop.mjs       Default launch, build and native packaging
app.tsx                       QuickGUI startup and window lifecycle
src/ui/                       UI and transient form state
src/engine/                   Native loader, generated types, command adapter
crates/picsie-core/        Model, commands, geometry, history, rendering, files
crates/picsie-native/      Node-API ownership and worker tasks
crates/picsie-core/tests/  Rust behavior and pinned Compositor fixtures
tests/                        Actual-addon integration and old project fixtures
```

`src/core/` has been removed. There are **zero legacy TypeScript engine exemptions**. New projects use `.picsie` files and the `picsie` format marker. Existing `.electropic` v1 projects remain readable and writable, retaining their original marker when saved. Their vector stroke masks and embedded PNG assets are preserved internally in Rust for compatibility. New masks use Rust-owned grayscale assets, and the Rust package adapter reads/writes the supported `.comp` raster/folder/mask subset. New brush strokes follow the upstream software coverage path and retain immutable native images; they do not encode PNGs during painting or preview. Folder raster masks and live mask links round-trip through both formats. Tip rasterization, incremental source tile publishing, GPU brush coverage, and richer `.comp` features remain fidelity work; see [the source map](compositor-port.md).

## Enforcement

`npm run check:architecture` restricts production JS/TS to the UI and bridge, rejects engine/pixel dependencies, pixel APIs, binary UI storage, imports from tooling/tests, and computed module imports. The policy contains no legacy imports, pixel snippets or frozen engine exemptions. Never add an exemption or JS fallback to implement an engine feature.

Dev/build/test entry points run the guard. CI checks and tests the native desktop,
packages it, and exercises the extracted Linux installer through real window
input. It also builds the reference Rust addon, checks TypeScript, and runs the
architecture suite, Rust behavior tests, and real-addon tests under Node and Bun.
Build on the target OS/architecture; only the native Linux x64 package has been
exercised here. Native releases do not use QuickGUI's updater or packaging.

The guard is structural lint, not a semantic proof. Review must identify the Rust implementation, pinned upstream source/fixtures, command boundary and resource owner. Reject engine logic hidden in a UI file. UI changes require inspecting actual native screenshots, especially alignment.
