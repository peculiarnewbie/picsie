# Picsie

A desktop image editor with a Rust engine and GPUI Kit interface, porting selected
functionality from [Compositor](https://github.com/robbietilton/Compositor).
The TypeScript/[QuickGUI](https://quickgui.dev/docs/typescript) application remains
available as a historical comparison. The native UI now follows Compositor’s
pinned layout and tool controls. This is not a feature-complete Compositor port.

Rust owns editor state, history, geometry, brushes, Skia rendering, codecs and
project files. The native UI calls the engine directly and presents BGRA frames
through GPUI. The [architecture decision](docs/architecture.md) and
[AGENTS.md](AGENTS.md) define ownership and enforcement. TypeScript/JavaScript in
the reference application remains restricted to UI and a thin native bridge.

Compositor, pinned to `609dbeae2ef68ef4fc82d67e4981a49852eb6e13`, is the source of
truth for editor behavior. The [source map](docs/compositor-port.md) records
translated routines and remaining prototype deviations. GIMP is a secondary
functionality and performance reference, never a UX reference.

## Run

Use a current Rust toolchain, a C/C++ linker, and Node 22+ for repository tooling.
The native application itself does not need Node, Bun, or QuickGUI. First builds
obtain Skia binaries and compile the pinned GPUI dependencies.

```sh
npm ci
npm run dev
# Open projects directly, each in its own window:
npm run dev -- --open /path/to/project.picsie
npm run dev -- first.picsie second.electropic
```

Run in a graphical desktop session; there is no HTTP dev server. The first window
contains an editable sample composition. **New** creates a transparent canvas.
**Open** accepts `.picsie` and legacy `.electropic` files; **Open .comp** accepts
Compositor directory packages.

Linux requires a C/C++ toolchain, clang, pkg-config, OpenSSL, fontconfig, FreeType,
X11/Wayland and libxkbcommon development libraries, and a Vulkan driver. Native
file dialogs need `xdg-desktop-portal` plus a chooser backend, such as
`xdg-desktop-portal-gtk`. See [desktop setup](crates/picsie-desktop/README.md) for
an Ubuntu dependency command and verification tools.

## Build and check

```sh
npm run build:desktop   # Compile the native release binary
npm run setup:packager  # Install pinned cargo-packager 0.11.8 once
npm run build           # Native packages and checksums in dist/<platform>-<arch>/
npm run check           # Registry, architecture, engine/reference types, and native desktop
npm test                # Architecture tests, Rust behavior, Node addon, desktop tests
npm run test:bun        # Actual-addon integration under Bun
npm run smoke           # Render sample artwork into artifacts/
```

Build on the target OS and architecture. Linux produces a Debian installer and a
portable `.tar.gz`; Windows produces an NSIS installer and portable `.zip`.
The macOS packaging path produces an application bundle and DMG, but has not been
validated and is not in the release matrix. Packages include dependency license
records, available license files, and Compositor attribution. Linux packages
register `.picsie` and `.electropic` project types.

`dev:desktop` remains an alias for the default launch. `check:desktop` and
`test:desktop` are available for focused native work. The default checks include
both applications so the retained reference and its Rust bridge stay usable.

## QuickGUI parity reference

```sh
npm run dev:quickgui
npm run build:quickgui
npm run test:quickgui
```

QuickGUI 0.1.6, Solid 2.0.0-rc.8 and TypeScript 7.0.2 stay pinned. Bun 1.4+ is
required for this reference application's development and packaging. Its native
Node addon is built by these commands; Rust still owns all image processing.
The Windows QuickGUI packaging command applies the existing NSIS compatibility
patch. QuickGUI release uploads and its updater are not part of the native app.

See the [UI parity record](docs/gpui-ui-parity.md),
[performance comparison](docs/desktop-performance.md), and
[GIMP experiments](docs/desktop-gimp-experiments.md) for measured coverage and limits.

The [reproducibility guide](docs/reproducibility.md) indexes correctness,
native interaction and performance flows. The latest
[feature benchmarks](docs/feature-performance.md) cover 19 additional workloads,
with measured stages, output-quality limits and the next profiling priorities.

## Working features

- PNG, JPEG, and WebP import, including dropping multiple files onto the canvas.
- Layers with range and individual multi-selection, drag reordering, nested folders with collapse and inherited visibility/opacity, subtree duplication/deletion, visibility, locking, renaming, opacity, and 24 blend modes.
- Resize using all eight edge/corner handles; drag the round handle to rotate. Shift preserves existing proportions during resizing and snaps rotation to 15°. The opposite edge/corner stays anchored, including on rotated or flipped layers. Alt/Option resizes from the center; crossing an anchor flips the content, following Compositor's transform algorithm.
- Brush and eraser strokes on individual layers, including transformed layers. Brush size is in document pixels. Hardness, smoothing, curved interpolation, Shift-click lines, and whole-stroke opacity follow Compositor's software brush. A stroke or drag is one undo step.
- Non-destructive 8-bit grayscale layer masks with Hide/Reveal painting, opacity control, enable/disable, reveal/hide all, linking, independent placement, and removal. Folder masks multiply every descendant's coverage. Live clipping links can use a lower sibling or another layer's alpha; soft edges retain the base alpha. Masks and links survive project saves and affect PNG/JPEG export.
- Editable rectangles, ellipses and diagonal two-color gradients. In-canvas point and paragraph text with searchable installed fonts, size/color, alignment, tracking/leading, growing point boxes and paragraph reflow handles.
- Layer brightness, saturation, and Gaussian blur.
- Pan, zoom, fit-to-window and merged-color sampling. Rulers, draggable/lockable guides, a layout grid, configurable edge/center snapping, Auto Select and Show Controls.
- Canvas Size with nine anchors, pixels/percent, relative dimensions, aspect-ratio lock, and transparent or colored extensions. Artwork keeps its original scale; content outside a smaller canvas remains recoverable.
- Crop with eight handles, ratio presets, edge snapping, Apply/Cancel, and retained source pixels. Rectangular/elliptical marquee, freehand/polygonal lasso and magic wand support New/Add/Subtract selection, Select → Modify → Feather to soften selection edges, Fill, Invert, Expand, Contract, and clearing selected layer pixels. Completed selections and selection operations are undoable; Escape cancels an unfinished outline before deselecting.
- Pixel Cut/Copy/Paste, Copy Merged and Layer via Copy with the system image clipboard. Drag selection outlines; Control/Command-drag moves pixels, with Alt/Option to duplicate. Control/Command+T starts a persistent transform with Apply/Cancel and one undo step.
- Merge Down, selected layers or a folder through Merge / Control+E. Image Size resamples layer assets or changes only resolution, with physical units, aspect locking and sampling choices. PNG/JPEG exports carry resolution metadata.
- Compositor-style undo/redo with selection restoration, nested transactions, saved revisions, and up to 100 entries within a 256 MB retained-asset budget. Native Save/Cancel/Discard prompts protect dirty windows and application quit.
- Validated, self-contained `.picsie` JSON projects, with embedded PNG assets. Existing `.electropic` v1 projects also open. The supported raster/folder/mask/clipping/text/guide subset of Compositor `.comp` packages opens and saves as directory packages with a manifest and PNG assets.
- Full-resolution PNG export preserving transparency, and JPEG export with a white background through **File → Export JPEG** on macOS or Ctrl+Alt+Shift+S on Linux.

## Basic workflow

For a visual walkthrough, see the [five-flow UX tour](docs/ux-tour.md) with step-by-step screenshots from the native app.

1. Import an image, or start with the included composition.
2. Select a layer in the panel, or enable Auto Select in Move to pick its bounds. Shift-click a layer row to select a range; Ctrl/Cmd-click toggles individual layers. Shift-click on the canvas toggles layers. Locked and hidden layers are skipped by canvas hit testing.
3. Drag the canvas with Brush, Eraser, Rectangle, or Ellipse selected. Click with Text for point text, or drag a paragraph box, then type directly on the canvas. The Type header controls typography; Enter/Done commits, Shift+Enter inserts a line, and Escape cancels.
4. Change the foreground color, then use **Fill with foreground** to recolor an existing shape or text layer. Gradient layers expose both endpoint colors.
5. Drag a layer’s dotted grip to reorder it. Selected layers move and duplicate together; copies retain the originals' placement. New layers insert above the active layer. Locked layers stay put. Select one layer for resize/rotate handles, painting, or inspector edits.
6. Use **Add mask** in the inspector, then **Hide** or **Reveal** in the toolbar. **X** swaps modes; Eraser reverses the current mode. **Layer pixels** returns to content painting. Removing or disabling a mask restores the original content.
   **Linked** in the mask panel controls whether the mask follows layer transforms. Select the mask and use Move to place an unlinked mask independently.
7. Click the dimensions at the bottom left, or press **Ctrl/⌘+Alt+C**, to open **Canvas Size**. Choose the size and anchor; a colored extension is added as a separate bottom layer.
8. Use **Folder**, **Group**, and **Into/Out of folder** in the layer panel to organize layers. Crop with **C**, drag a handle, then Apply. Select pixels with **M** or **L**; the **Pixel selection** inspector offers Fill, Invert, Expand, Contract, Feather, and Clear. The **Clipping** inspector creates/releases a link or chooses its alpha source. Delete clears a selection; without one it removes the targeted mask or layer. Deleting a live source bakes its coverage into dependent layers and is undoable.
9. Save an editable project, save a `.comp` package, or export the flattened result.

Tool shortcuts work while the canvas has focus. Standard text shortcuts remain available in inputs.

| Shortcut                  | Action                                                              |
| ------------------------- | ------------------------------------------------------------------- |
| V / B / E                 | Move / Brush / Eraser                                               |
| U / O / T                 | Rectangle / Ellipse / Text                                          |
| H / I                     | Hand / Sample merged color                                          |
| C / M / L                 | Crop / Marquee / Lasso                                              |
| [ / ]                     | Brush size                                                          |
| Shift-click with Brush    | Line from previous stroke endpoint                                  |
| Ctrl/⌘+Shift+I / Ctrl/⌘+D | Invert / deselect pixels                                            |
| Ctrl/⌘+Alt+G              | Create/release clipping mask                                        |
| Arrow / Shift+Arrow       | Nudge 1 / 10 pixels                                                 |
| Delete / Escape           | Clear selected pixels or delete layer / Cancel gesture or selection |
| ⌘N / ⌘O / ⇧⌘O             | New / Open project / Import images                                  |
| ⌘S / ⇧⌘S / ⌥⌘S            | Save / Save as / Export PNG                                         |
| ⌘Z / ⇧⌘Z                  | Undo / Redo canvas edit                                             |
| ⌘J / ⇧⌘N                  | Duplicate / New paint layer                                         |
| ⌘[ / ⌘]                   | Lower / Raise layer                                                 |
| ⌥⌘C                       | Canvas Size                                                         |
| ⌘0 / ⌘1                   | Fit canvas / Actual size                                            |

Use Ctrl in place of ⌘ on Linux and Windows, with the canvas focused. Linux uses the visible controls and canvas shortcuts in place of application menus. Scroll pans; Ctrl/⌘+scroll zooms around the cursor. The inspector scrolls to reveal additional properties.

## Scope and limits

This implementation follows Compositor's bottom-to-top layer model and compositing workflow. It reads and writes the supported `.comp` package subset described in the [source map](docs/compositor-port.md); it does not read PSD files.

Still to port: object selections, healing and cloning, content-aware tools, perspective distortion, adjustment layers/curves/levels, layer effects, richer shapes, tabs and autosave/recovery. `.comp` import explicitly rejects those richer upstream records; `.comp` retains editable RGB text and guides; local shapes, gradients and legacy translucent text rasterize. Layer grips, buttons, and shortcuts reorder the selection. Resize and rotation operate on one layer at a time. Drag reordering does not yet auto-scroll the layer list. Canvas picking uses layer bounds, not per-pixel alpha; middle-click or Super/Command-click cycles layers under the pointer; Control bypasses move/crop snapping. Paragraph text wraps within 12px padding and clips overflow, with an overset marker while editing. Point text grows as typed. Skia/system font output and native composition details can differ from AppKit; see the source map.

Canvas Size currently offers pixels and percent; physical-unit controls are not yet exposed. The history budget counts encoded PNG assets, grayscale masks, unique retained pixel-selection buffers, and estimated vector-stroke storage. It does not measure all native allocations.

The editor supports up to 10,000 layers, 8192 pixels per dimension, 24 megapixels
per surface, and 96 MB per project/import file. These limits are not intended for
very large production files. A Rust worker owns each editor and renders through
CPU Skia. Retained compositing reuses unchanged pixels, previews coalesce into a
bounded latest-frame mailbox, and completed frames wake the UI. GPUI presents
BGRA memory through a GPU upload; viewport copies and uploads remain. Brushes and
image processing are still on the CPU. The QuickGUI reference retains its native
uncompressed TIFF transport.

## Structure

```text
crates/picsie-desktop/     Default Rust / GPUI Kit application and engine worker
crates/picsie-core/        Shared model, commands, geometry, history, Skia and files
crates/picsie-core/tests/  Engine behavior and pinned upstream fixtures
scripts/desktop.mjs       Native launch, build and packaging entry point
app.tsx                   QuickGUI reference startup and window lifecycle
src/ui/                   Reference UI, controls, dialogs and transient forms
src/engine/               Generated contracts and thin native adapter
crates/picsie-native/      Reference Node-API addon and worker tasks
tests/                    Node/Bun integration and legacy project fixtures
```

## Validation

CI tests the Rust engine, the native desktop, architecture guards, reference
TypeScript types, and the actual addon under Node and Bun. Its native job builds
release packages, checks the extracted binaries and package metadata, then drives
the extracted Debian application through real X11 input and GTK portal dialogs.

The native workflow harness covers 56 checks, including controls, selections,
painting, masks, crop, transforms, multiple windows, save/reopen, export, and dirty
close/quit prompts. The frame-wakeup harness checks hidden/inactive CPU use and
edit recovery after remapping/refocusing. Actual screenshots are inspected for
layout and alignment. See [the parity record](docs/gpui-ui-parity.md) and
[desktop setup](crates/picsie-desktop/README.md).

Linux X11 has been exercised with software Vulkan and an AMD hardware adapter
under Xvfb. Windows CI passed native compilation, 13 Rust tests, NSIS/portable
packaging, and executable checks outside the checkout. Physical display
presentation, macOS/Windows editor interaction, Wayland, HiDPI, signing, and
notarization still need platform validation.

## Release

[The release workflow](.github/workflows/release.yml) builds the native application
for Linux x64 on the Namespace runner and Windows x64 on `windows-latest`.
The Namespace GitHub App must be installed on this repository.

1. Bump `version` in both `package.json` and `crates/picsie-desktop/Cargo.toml`,
   update its Cargo.lock, and add a dated version section to [CHANGELOG.md](CHANGELOG.md).
2. Tag `v<version>` and push the tag. CI checks the tag, package and binary versions.
3. Both targets run checks and tests, build native packages, verify extracted
   binaries, and upload artifacts. A final job uploads the packages and SHA-256
   checksums to one **draft** GitHub release with the changelog's release notes.
4. Review the artifacts, then publish the draft when ready.

The native application currently uses manual upgrades from release downloads.
It does not use the QuickGUI Sparkle appcast or `QUICKGUI_UPDATER_PRIVATE_KEY`.
Existing QuickGUI installations do not automatically migrate to this native app.
Windows installers are unsigned; macOS is not in the release matrix. No release
is published by ordinary branch pushes.

## Attribution

[Compositor](https://github.com/robbietilton/Compositor) by Robbie Tilton / Wonder Assembly LLC provided the feature and architecture reference. Selected routines and test fixtures are translated from the pinned upstream source; the remaining prototype differences are listed in the [source map](docs/compositor-port.md). Its upstream MIT notice is retained in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). QuickGUI is MIT/Apache-2.0 licensed; package dependencies retain their own licenses.

Track feature coverage, interaction details, intentional adaptations and native verification in the [Compositor feature and polish registry](docs/compositor-registry.md).
