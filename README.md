# Picsie

An experimental port of [Compositor](https://github.com/robbietilton/Compositor), using a TypeScript/[QuickGUI](https://quickgui.dev/docs/typescript) interface with a Rust engine. This is an initial compositing editor, not a feature-complete port.

**Architecture requirement: TypeScript/JavaScript is for UI and a thin native bridge only. All new engine and image-processing code must be Rust.** The [architecture decision](docs/architecture.md) defines ownership, enforcement, and migration. [AGENTS.md](AGENTS.md) makes it mandatory for future implementation work.

The source of truth for editor behavior is Compositor, pinned to revision `609dbeae2ef68ef4fc82d67e4981a49852eb6e13`. The [source map and remaining deviations](docs/compositor-port.md) distinguish translated routines from the independently written prototype subsystems.

The UI uses QuickGUI's native Solid renderer, windows, menus, dialogs, inputs, and captured pointer events. There is no browser or webview. The Rust engine owns editing state, history, geometry, Skia rendering, image codecs and project files. QuickGUI receives metadata and paths to native uncompressed frame buffers; pixels stay out of JavaScript.

## Run

QuickGUI currently recommends **macOS 14+, Bun 1.4+, and Xcode Command Line Tools**. Versions are pinned to QuickGUI 0.1.6, Solid 2.0.0-rc.8, and TypeScript 7.0.2; the Solid prerelease version matters.

```sh
npm ci
npm run build:native
bun run dev
```

The first window contains an editable sample composition. Use **New** to create a transparent canvas. Each new or opened project gets its own native window.
Use **Open** for `.picsie` and legacy `.electropic` files, or **Open .comp** for Compositor directory packages.

```sh
npm run check:architecture # UI/Rust boundary, with no legacy exemptions
npm run check          # Architecture guard, Rust, and TypeScript
npm test               # Rust behavior tests and actual-addon tests under Node
npm run test:bun        # Actual-addon integration tests under Bun
npm run smoke          # Render sample artwork and a canvas preview into artifacts/
bun run build          # Build Rust and package for the host platform into dist/
```

Rust 1.88+ and a C/C++ linker are required to build the addon. The first build downloads Skia binaries (or builds Skia from source if unavailable). Node 22+ supports the development tests. Bun is required for QuickGUI development and packaging. Build on the target OS and architecture: the Skia dependency includes a platform-specific native addon. The Linux build is experimental and requires a graphical session; this project is a desktop application and has no HTTP dev server.

Dev/build/test commands enforce the architecture policy. Production TypeScript is restricted to UI or native bridge locations; pixel APIs/dependencies are rejected and there are no legacy engine exemptions. CI runs the guard, its regression tests, typechecking, and application tests. See [the enforcement limits and review requirements](docs/architecture.md#enforcement).

## Working features

- PNG, JPEG, and WebP import, including dropping multiple files onto the canvas.
- Layers with range and individual multi-selection, drag reordering, nested folders with collapse and inherited visibility/opacity, subtree duplication/deletion, visibility, locking, renaming, opacity, and 16 blend modes.
- Resize using all eight edge/corner handles; drag the round handle to rotate. Shift preserves existing proportions during resizing and snaps rotation to 15°. The opposite edge/corner stays anchored, including on rotated or flipped layers. Alt/Option resizes from the center; crossing an anchor flips the content, following Compositor's transform algorithm.
- Brush and eraser strokes on individual layers, including transformed layers. Brush size is in layer-local pixels. A stroke or drag is one undo step.
- Non-destructive 8-bit grayscale layer masks with Hide/Reveal painting, flow control, enable/disable, reveal/hide all, linking, independent placement, and removal. Masks survive project saves and affect PNG/JPEG export.
- Editable rectangles, ellipses, diagonal two-color gradients, and multiline text with three system font families.
- Layer brightness, saturation, and Gaussian blur.
- Pan, zoom, fit-to-window, and merged-color sampling.
- Canvas Size with nine anchors, pixels/percent, relative dimensions, aspect-ratio lock, and transparent or colored extensions. Artwork keeps its original scale; content outside a smaller canvas remains recoverable.
- Crop with eight handles, ratio presets, edge snapping, Apply/Cancel, and retained source pixels. Rectangular/elliptical marquee and freehand lasso support New/Add/Subtract selection, Select → Modify → Feather to soften selection edges, and clearing selected layer pixels.
- Compositor-style undo/redo with selection restoration, nested transactions, saved revisions, and up to 100 entries within a 256 MB retained-asset budget. Native Save/Cancel/Discard prompts protect dirty windows and application quit.
- Validated, self-contained `.picsie` JSON projects, with embedded PNG assets. Existing `.electropic` v1 projects also open. The supported raster/folder/mask subset of Compositor `.comp` packages opens and saves as directory packages with a manifest and PNG assets.
- Full-resolution PNG export preserving transparency, and JPEG export with a white background through **File → Export JPEG** on macOS or Ctrl+Alt+Shift+S on Linux.

## Basic workflow

For a visual walkthrough, see the [five-flow UX tour](docs/ux-tour.md) with step-by-step screenshots from the native app.

1. Import an image, or start with the included composition.
2. Select a layer in the panel or click its bounds using Move. Shift-click a layer row to select a range; Ctrl/Cmd-click toggles individual layers. Shift-click on the canvas toggles layers. Locked and hidden layers are skipped by canvas hit testing.
3. Drag the canvas with Brush, Eraser, Rectangle, or Ellipse selected. Click with Text to create a text layer, then edit its content in the inspector.
4. Change the foreground color, then use **Use foreground color** to recolor an existing shape or text layer. Gradient layers expose both endpoint colors.
5. Drag a layer’s dotted grip to reorder it. Selected layers move and duplicate together; copies retain the originals' placement. New layers insert above the active layer. Locked layers stay put. Select one layer for resize/rotate handles, painting, or inspector edits.
6. Use **Add mask** in the inspector, then **Hide** or **Reveal** in the toolbar. **X** swaps modes; Eraser reverses the current mode. **Layer pixels** returns to content painting. Removing or disabling a mask restores the original content.
   **Linked** in the mask panel controls whether the mask follows layer transforms. Select the mask and use Move to place an unlinked mask independently.
7. Click the dimensions at the bottom left, or press **Ctrl/⌘+Alt+C**, to open **Canvas Size**. Choose the size and anchor; a colored extension is added as a separate bottom layer.
8. Use **Folder**, **Group**, and **Into/Out of folder** in the layer panel to organize layers. Crop with **C**, drag a handle, then Apply. Select pixels with **M** or **L** and use **Clear pixels** or Delete.
9. Save an editable project, save a `.comp` package, or export the flattened result.

Tool shortcuts work while the canvas has focus. Standard text shortcuts remain available in inputs.

| Shortcut            | Action                             |
| ------------------- | ---------------------------------- |
| V / B / E           | Move / Brush / Eraser              |
| U / O / T           | Rectangle / Ellipse / Text         |
| H / I               | Hand / Sample merged color         |
| C / M / L           | Crop / Marquee / Lasso             |
| [ / ]               | Brush size                         |
| Arrow / Shift+Arrow | Nudge 1 / 10 pixels                |
| Delete / Escape     | Clear selected pixels or delete layer / Cancel gesture or selection |
| ⌘N / ⌘O / ⇧⌘O       | New / Open project / Import images |
| ⌘S / ⇧⌘S / ⌥⌘S      | Save / Save as / Export PNG        |
| ⌘Z / ⇧⌘Z            | Undo / Redo canvas edit            |
| ⌘J / ⇧⌘N            | Duplicate / New paint layer        |
| ⌘[ / ⌘]             | Lower / Raise layer                |
| ⌥⌘C                 | Canvas Size                        |
| ⌘0 / ⌘1             | Fit canvas / Actual size           |

Use Ctrl in place of ⌘ on Linux, with the canvas focused. QuickGUI 0.1.6 does not implement Linux application menus; the toolbar and canvas shortcuts provide these actions. Scroll pans; Ctrl/⌘+scroll zooms around the viewport center. The inspector can scroll to reveal additional properties.

## Scope and limits

This implementation follows Compositor's bottom-to-top layer model and compositing workflow. It reads and writes the supported `.comp` package subset described in the [source map](docs/compositor-port.md); it does not read PSD files.

Still to port: folder masks and clipping relationships, polygonal lasso and wand/object selections, selection move/transform and undo history, healing and cloning, content-aware tools, perspective distortion, guides/rulers, adjustment layers/curves/levels, layer effects, richer text layout, tabs, and autosave/recovery. `.comp` import explicitly rejects those richer upstream records; `.comp` export rasterizes Picsie's live text, shapes, and gradients. Layer grips, buttons, and shortcuts reorder the selection. Resize and rotation operate on one layer at a time. Drag reordering does not yet auto-scroll the layer list. Canvas picking uses layer bounds, not per-pixel alpha. Text uses explicit line breaks and clips to its source box; resizing scales that box rather than reflowing text.

Canvas Size currently offers pixels and percent; physical-unit controls are not yet exposed. Retained history storage counts encoded PNG and grayscale mask assets and estimates vector-stroke payloads, rather than measuring all native allocations.

The editor supports up to 100 layers, 8192 pixels per dimension, 24 megapixels per surface, and 96 MB per project/import file. These bounds keep this prototype usable; it is not intended for very large production files. Rust worker tasks render through Skia. QuickGUI displays uncompressed native TIFF resources; this still involves native copies and is not direct GPU texture sharing. Large images and dense painting can therefore lag. Pixel buffers are cached within a 96 MB budget, preview requests are coalesced, and idle windows do not continually render. Performance work should move to tiled rendering and direct texture updates before extending those limits.

## Structure

The engine is independent of the QuickGUI UI:

```text
app.tsx                       Startup and native window lifecycle
src/ui/                       UI, controls, dialogs and transient forms
src/engine/                   Generated contracts and thin native adapter
crates/picsie-core/        Rust model, commands, geometry, history, Skia and files
crates/picsie-native/      Node-API addon and asynchronous worker tasks
crates/picsie-core/tests/  Engine behavior and upstream fixtures
tests/                        Node/Bun integration and legacy project fixtures
```

## Validation

The migrated behavior suite runs in Rust, with real-addon tests under both Node and Bun. Rust/TypeScript checks, sample rendering and Linux native packaging are also checked. The packaged application was also run on an isolated X11 display using Xvfb, Mesa software Vulkan, and the GTK file-dialog portal.

Native interaction checks covered anchored canvas expansion, relative sizing and ratio lock, undo/redo selection restoration, range selection, group movement and duplication, drag insertion and reordering, anchored edge resizing, rotation snapping, mask hiding/revealing and disabling, plus the earlier opacity, shape, brush/eraser, undo, text, blend, and fit-to-window checks. Layouts were inspected at 1280 × 860 and 960 × 640. A project was saved and reopened through native file dialogs. Its PNG export was byte-for-byte equal to rendering the saved project through the compositor.

The earlier screenshot pass caught and fixed font-dependent tool icons, inconsistent button and field alignment, shrinking layer rows, a clipped New canvas dialog, missing select labels and popup colors, and text edits losing characters. The new crop, mask, folder, selection, and `.comp` paths are tested through the real addon; this workspace lacks a graphical display for fresh window screenshots. See the [native screenshots and test notes](docs/native-testing.md). **macOS execution, signing, and notarization remain unverified.**

## Release

Releases are built by GitHub Actions: Linux on the [Namespace](https://namespace.so) runner profile `namespace-profile-peculiarnewbie`, Windows on the GitHub-hosted `windows-latest` runner (Namespace profiles are bound to one OS; a Windows profile can replace it in [release.yml](.github/workflows/release.yml)). The Namespace GitHub App must be installed on this repository.

1. Bump `version` in `package.json` and add a `## x.y.z - YYYY-MM-DD` section to [CHANGELOG.md](CHANGELOG.md). The changelog section becomes the release notes and appcast description.
2. Tag the release `v<version>` and push the tag. The release workflow refuses tags that do not match `package.json`.
3. Each target runs the architecture guard, Rust/TypeScript checks, and the Node/Bun addon tests, then runs `quickgui build --upload`. This packages Linux (`tar.gz`, `install.sh`, `.deb`) and Windows (NSIS installer and portable archive), signs the Sparkle-compatible appcast, and uploads everything to a **draft** GitHub release.
4. After both targets have uploaded, publish the draft to make the update feed live: `gh release edit v<version> --draft=false`.

The updater's private signing key is the `QUICKGUI_UPDATER_PRIVATE_KEY` repository secret (the contents of the `quickgui-update.key` file from `npx quickgui keygen`); its public half is committed in [quickgui.config.ts](quickgui.config.ts). Installed apps read the appcast and `install.sh`/`latest-*.txt` pointers from `releases/latest/download`, so nothing moves until the draft is published. Windows installers are unsigned (SmartScreen will warn), macOS is not built, and Windows/Linux packages have only been smoke-tested through the app's own checks.

## Attribution

[Compositor](https://github.com/robbietilton/Compositor) by Robbie Tilton / Wonder Assembly LLC provided the feature and architecture reference. Selected routines and test fixtures are translated from the pinned upstream source; the remaining prototype differences are listed in the [source map](docs/compositor-port.md). Its upstream MIT notice is retained in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). QuickGUI is MIT/Apache-2.0 licensed; package dependencies retain their own licenses.
