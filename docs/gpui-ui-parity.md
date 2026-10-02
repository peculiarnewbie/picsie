# GPUI UI fidelity and migration record

The user authorized Rust / GPUI Kit on 2026-09-29 and explicitly changed the
native UI target to **Compositor** on 2026-09-30. The pinned source is revision
`609dbeae2ef68ef4fc82d67e4981a49852eb6e13`. QuickGUI remains available as the
historical migration comparison; GIMP is never a UX reference.

This is the default application as of 2026-09-30. Run `npm run dev`; build native
packages with `npm run build`. The QuickGUI application remains available with
`npm run dev:quickgui`. See [desktop setup](../crates/picsie-desktop/README.md).

The complete applications have also been [benchmarked against each other](desktop-performance.md),
including visible input latency, startup, memory, and drag updates.

## Current Compositor UI pass

The native app follows `ContentView.swift`, `ToolHeaderStyle.swift`,
`TransformInspector.swift`, `BrushControls.swift`, `LassoControls.swift`,
`ShapeControls.swift`, `TypeControls.swift`, `LayersPanel.swift`,
`LayerAppearanceControls.swift`, `NativeLayerList.swift`, `CanvasThumbnail.swift`
and `ImageSizeSheet.swift` from that revision.

| Area | Current native UI |
| --- | --- |
| Shell | Compact document/navigation bar with platform menus, Fit/100%/zoom; neutral dark surfaces; 42 px tool header; 56 px rail; 30 px status bar; 800×520 minimum. |
| Tool rail | Upstream order for supported tools, with Paint/Erase and Rectangle/Ellipse modes grouped in their headers. Tools scroll when a short window cannot fit them. |
| Transform | Auto Select and Show Controls; X/Y/W/H/angle/flips, ratio lock, percentage scale and sampling in the top header. Numeric edits and flips open a persistent engine transform, with Apply/Cancel, rather than committing each field separately. Shift preserves proportions. |
| Brush | Paint/Erase modes; size, hardness, opacity, smoothing and foreground/mask controls in the header. Kit sliders and numeric fields share the existing engine settings. |
| Selection | Marquee/lasso mode, New/Add/Subtract, wand tolerance/sample size/sampling scope/contiguous, Anti-alias, expand/contract/feather and Deselect in the header. Select Layer Pixels and amount panels are available from Select. Clipboard and other operations live in menus. |
| Layers | Dedicated panel, initially 252 px, draggable within 202–352 px; blend/opacity above the full-height list; 52 px rows with cached canvas-shaped image/grayscale mask thumbnails and dimensions; plain icon footer. |
| Secondary properties | Masks/clipping, local adjustment settings, rename/lock/legacy fill properties use Kit popovers instead of occupying the Layers panel. |
| Type | In-canvas point/paragraph editing; searchable installed fonts, size, color, left/center/right, tracking and leading in the header; compact alignment icons, blinking caret, word/paragraph selection and resize handles that reflow text. |
| Crop | Source five-preset 170 px ratio selector, live pixel dimensions and disabled Cancel/Apply Crop without a frame. |
| Canvas | Compositor's dark pasteboard, soft canvas shadow and 10-point transparency checkerboard; optional 18-point rulers, cyan draggable guides, layout grid and configurable snapping. |
| Image Size | 430 px form with separate width/height rows, aspect lock, resolution, resampling and sampling controls. |

All current engine operations remain accessible. Platform menus use Kit's popup
navigation and restore the previous action context, including clipboard commands
for focused text fields. A scoped `NoAction` binding lets Space remain text inside
an Input embedded in a Popover. Kit owns text editing, clipboard and local undo; Rust resolves canvas glyph, selection and caret geometry.

Adaptations and remaining gaps: Linux/Windows menus replace macOS application
menu/titlebar integration. Existing documents still use separate windows, not
upstream's project tabs. The layers/masks/color pass below adds foreground/background swatches, swap/default controls and the movable picker. Mask rows now show
grayscale thumbnails with a distinct editing-target border. The existing brightness/saturation/blur controls are retained in a palette; these are not upstream's full floating
adjustment/effect panels. Panel width and view options now persist in platform configuration; tool, brush and font defaults remain session-local. The 2026-10-01 pass adds ratio lock, scale percentage and transform sampling. The 2026-10-02 shapes pass adds the editable shape/gradient headers below; Tab cycling required canvas-scoped routing (see below). No nonfunctional controls were added for remaining gaps.

The preceding shell pass's complete native workflow run passed **90 checks**, including panel resize
limits, short-window scrolling, all previously verified engine workflows, and
real GTK save/import/export dialogs. A focused run additionally checks text
Popover spaces/newlines, automatic transform sessions and cancellation,
fractional numeric-field/menu clipboard focus, and the 800×520 layout.
`npm run check`, `npm test`, and the actual-addon Bun suite pass.

Actual screenshots and the interaction report for this pass are under
`artifacts/desktop/compositor-ui/verification/`. Source inspection is against the
pinned checkout; Compositor itself cannot run here without macOS, so this is not
a pixel-for-pixel comparison against a running upstream application. Thumbnail
placement/cache regressions include local cache checks against translated source
rules. The later registry audit also ports the grayscale edge-tone scenario from
upstream CanvasThumbnailTests; its complete source fixture inventory is tracked.

## Historical QuickGUI migration coverage

The following records the original 2026-09-29 layout and comparison. Its fixed
inspector/palette layout has been superseded by the Compositor UI pass above.


| Area             | Desktop behavior                                                                                                                                                                                                               |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Layout           | Original palette and SVG icons; 52 px header, 46 px context bar, eleven-tool 64 px rail, native canvas, scrolling 280 px inspector, 28 px footer. Minimum window 960×640.                                                      |
| Documents        | New and open in separate windows; Picsie/legacy projects and `.comp` packages; native image import and file drop; Save, Save As, Save `.comp`, PNG/JPEG export; dirty titles; Save/Cancel/Discard on close and quit.           |
| Tools            | Move, resize, rotate, Shift proportions; brush/eraser size, opacity, hardness, smoothing; rectangle, ellipse, text, hand, eyedropper; crop ratios/apply/cancel; marquee rectangle/ellipse and lasso with replace/add/subtract. |
| Layers           | Range/toggle selection, visibility, lock, row reorder and folder drop, collapse, paint/gradient/folder creation, grouping, duplicate/delete, Into/Out of folder.                                                               |
| Properties       | Name, opacity slider and typed value, sixteen blend modes, transform fields and flips, multiline text, font size/family, fill picker, gradient end color, brightness/saturation/blur sliders and typed values.                 |
| Masks            | Add/remove/reset, layer or folder masks, content/mask target, hide/reveal, enable/link state, clipping and live alpha source picker.                                                                                           |
| Pixel selections | All, inverse, deselect, expand, contract, feather, fill, clear, and shared engine history.                                                                                                                                     |
| Color            | Foreground field and swatches, HSB square/hue strip, RGB/hex fields, preview, OK/Cancel, layer fill, eyedropper.                                                                                                               |
| Canvas Size      | Pixels/percent, relative sizing, original aspect lock, nine anchors, transparent/foreground/black/white/custom extension fills, engine limits and validation.                                                                  |
| Interaction      | Editor shortcuts, input focus isolation and clipboard, Enter commits text and Shift+Enter inserts a line, gesture capture/cancellation, zoom/fit, native application menus.                                                    |

Forms and event mapping are Rust UI code. All document mutations, geometry,
history, painting, masks, selections, compositing, codecs, and file operations use
`picsie-core`. Text and slider previews retain one undo entry per editing session
or drag. File requests run on the engine worker, after queued edits. Errors and
file completions use a reliable channel independent of coalesced preview frames.
Worker notifications wake the UI when those results arrive, without a periodic
mailbox timer.

The frame path is CPU Skia → BGRA memory → GPUI image upload. No JavaScript pixels,
TIFF preview files, new image algorithms, or toolkit patches were introduced.
This does not add GPU compositing, tiled rendering, or broader Compositor parity.

## Verification

`crates/picsie-desktop/verify.py` drives the actual release application using X11
mouse/keyboard events, a private D-Bus session, and the real GTK portal file picker.
It checks authoritative engine metadata and records screenshots at 1280×860 and
960×640. Opening a saved `.comp` through the GTK directory picker was additionally
verified with mouse navigation; the automated run does not depend on that picker's
location-entry keyboard behavior under Xvfb. Results and screenshots are under
`artifacts/gpui-parity/verification/`; the run log is
`artifacts/gpui-parity/verification-run.log`.

Verified on 2026-09-29:

- Release build and all **55 native workflow checks** pass, including dropdown
  navigation without accidental canvas nudges, save-on-close, and quit cancellation.
- `npm run check:desktop` and all **9 desktop Rust tests** pass, including worker
  notification delivery, idle quiescence, shutdown, and startup failure.
- `npm run check`, `npm test`, and `npm run test:bun` pass, covering the architecture
  guard, Rust engine, generated bridge contracts, and real Node/Bun addon integration.
- Screenshots were inspected for the shell, mask and text inspectors, color and
  Canvas Size dialogs, unsaved-close dialog, and minimum window size.

Desktop Rust tests cover ordered control updates, property history grouping,
pointer/file completion delivery, real project/package saves, PNG/JPEG exports,
image imports, failed-save recovery, and three selected upstream color-picker
fixture groups. The original architecture, Rust engine, Node, and Bun suites are
also retained. CI includes a separate desktop build/test/native interaction job.

## Adaptations and platform limits

Kit provides text editing, clipboard, selectors, sliders, modal focus, and native
path dialogs. Component text rendering and operating-system dialogs differ from
QuickGUI; the application layout and workflows follow the reference. The GPUI
path prompt currently has no extension-filter API, so the engine validates the
chosen files. Linux application menus follow the toolkit's platform support;
visible controls and shortcuts expose the same actions.

The original parity pass used Linux X11 at 1× with Mesa **software Vulkan**.
Subsequent [performance experiments](desktop-gimp-experiments.md) also exercised
an AMD hardware adapter under Xvfb. The default-application promotion built and
extracted the Debian installer and portable archive, checked their native binaries
and metadata, and passed all 56 workflow checks against the extracted installer.
Screenshots at both window sizes were inspected. CI repeats package and workflow
verification and checks hidden/inactive frame waking.

Windows CI passed native compilation, all 13 Rust tests, NSIS/portable packaging,
and extracted executable checks. macOS, Windows editor interaction, Wayland, HiDPI,
physical display presentation, and macOS package execution remain unverified. The [initial transport experiment](gpui-kit-experiment.md)
preserves the narrower historical measurements separately.


## Native feature pass after promotion (2026-09-30)

The default native app now also exposes pixel Cut/Copy/Paste, Copy Merged and Layer
via Copy; outline movement, selected-pixel move/duplicate and Apply/Cancel
transforms; polygonal lasso and configurable color wand; layer/folder merging and
Image Size. These extend the native app beyond the retained QuickGUI UI. Their
algorithms and defaults follow the pinned Compositor sources; see
[the source map](compositor-port.md#native-clipboard-selection-tools-merging-and-image-size-2026-09-30).

Verified locally on Linux X11 with software Vulkan:

- All **85 combined native workflow checks** pass, including the real system
  clipboard, in-place paste across windows, external image centering, undo/cancel,
  wand controls, all merge modes and resample/resolution-only Image Size.
  External clipboard ownership uses optional `xclip`; without it the harness runs
  84 checks. Metadata and painted controls are awaited before interaction.
- A focused follow-up against the final build confirms that Control-click still
  picks layers outside selection tools. Selection-tool Control-drag reaches the
  pixel mover, and the text double-click handler cannot interrupt that drag.
- `npm run check`, `npm test` and `npm run test:bun` pass: 100 core Rust tests,
  14 desktop Rust tests, 23 Node tests (20 actual-addon tests) and 20 Bun addon tests,
  plus the architecture guard and its seven regression tests.
- Screenshots were inspected for the selection transform, polygonal lasso, wand,
  both Image Size modes and minimum window size. All twelve tool buttons fit the
  minimum 960×640 window; the inspector and remaining rail content scroll.

The combined report and screenshots are in `artifacts/desktop/features-verification/`;
its log is `artifacts/desktop/features-verification.log`. The final layer-picking
check is in `artifacts/desktop/features-routing/`. This pass's GUI interaction was
verified on Linux; Windows/macOS GUI interaction remains unverified. Existing
sampling, clipboard-origin and allocation-limit adaptations are listed in the
source map.

## Placement and type pass (2026-09-30)

The next feature pass adds the source workflows for placement and typography:

- View → Rulers, Guides, Grid, Lock Guides, Snap and individual Snap To targets.
  Drag a ruler into the canvas to create a guide; Move drags an existing guide;
  dropping it on a ruler deletes it. Each completed edit is one undo entry.
- Move header Auto Select (off by default) and Show Controls (on). Movement
  snaps edges and centers to canvas bounds, layers, visible guides and the grid.
  Control bypasses move/crop snapping; Super/Command and middle-click retain
  local layer cycling.
- Type clicks create growing point text; drags create paragraph boxes. Click a
  text layer or use Edit Text to edit on the canvas. Kit supplies typing, Unicode
  clipboard and local text undo. Rust paints glyphs, selection, caret, box handles
  and the overset marker; rendered rows drive Up/Down/Home/End navigation.
- Installed family/face search, size, color, left/center/right, tracking and
  leading are in the Type header. Alt-arrow spacing follows InlineTextEditor.
  Enter/Done commits once; Shift+Enter adds a line; Escape/Cancel restores content
  and geometry. Box handles reflow text without scaling glyphs.
- Guides and text layout survive both `.picsie` and `.comp`. Legacy text remains
  boxed unless explicitly resized/edited; source limits and platform shaping
  adaptations are recorded in the source map.

The regular X11 verifier passed **120 checks**, including these workflows,
real GTK saves and the 800×520 layout. A focused run covers all 30 placement/type
checks, and 19 Rust placement/type fixtures cover source and local regressions. Artifacts are under
`artifacts/desktop/placement-text/verification/`. The actual-addon tests exercise
typed guide/text commands and both project round-trips under Node and Bun.

This remains source-guided fidelity, not a runtime macOS comparison. Native IME
composition decoration, bidirectional editing details and physical HiDPI input
need platform verification; the content buffer uses Kit rather than AppKit.

## Registry and existing-feature polish pass (2026-09-30)

[The maintained registry](compositor-registry.md) now contains **199 behavior rows
across 20 areas**, with the complete pinned inventory of **184 Swift/C/header
files and 342 upstream fixture names**. Each row separates implementation, polish
and verification, records remaining gaps/adaptations, and links evidence. Native
rows name their actual verifier checks; verified polish requires native evidence.
`npm run registry:update` regenerates the tables; `check:registry` runs in default
checks and CI and rejects stale documentation, invalid states, missing evidence,
unknown source files and removed named native checks. The optional external
checkout hash check passed against the exact pinned revision.

This pass polishes existing tools and shared controls:

- Source compact Type size/unit, swatch and alignment icon spacing; numeric Up/Down
  and Shift-ten stepping; computed Auto leading and text focus after field commits.
- Text-colored 500ms caret blinking/reset; shaped double-click word and triple-click
  paragraph selection with complete-unit drag behavior.
- Shared full-edge/corner geometry for transform, crop and text feedback; rotated
  resize, I-beam, move/copy/hand cursors and white/black brush diameter outline.
- Canvas-shaped grayscale mask thumbnails with mean edge tone, source soft canvas
  shadow, remembered panel width/view options, and dynamic Undo/Redo/merge titles.
- Correct Shift square/circle and Alt-centered shape dragging, zero-click/tool-switch
  draft cancellation, source quarter-hardness and two-digit opacity shortcuts.
- Source Crop ratio picker, live dimensions, enabled state, default nonediting frame,
  dimmed surround, rule-of-thirds lines and bordered handles. Expanded-crop rendering
  outside current document bounds remains a tracked gap.

The 2026-10-02 shapes/gradients pass ran **24 focused native checks** on Linux X11 at 1x (display :112, software Vulkan): editable shape headers (kind buttons, radius/width sliders and numerics, fill swatch), Shift-snapped line drawing, Shift+U and Tab cycling (Tab routes through a canvas-scoped `PicsieCanvas` key context so fields/selects keep traversal), gradient headers (linear/radial, style, reverse, opacity, swatch, Mask badge), pending-line Apply/Cancel, Enter/Escape, endpoint grabbing, opacity percent with arrow stepping, slider track-click, and mask-target Apply. Screenshots live with the focused script run. Tab cannot work through the generic bubble handler because the toolkit Root binding outranks it; the canvas action binding was verified natively instead.

The final release binary passed **all 140 combined native checks** on Linux X11 at
1× with software Vulkan, including real clipboard exchange, GTK file dialogs,
new-window preference loading, saves/reopens and quit cancellation. Assertions now
await expected state transitions rather than relying only on fixed delays.
`npm run check`, `npm test` and `npm run test:bun` pass: **132 core Rust tests,
15 desktop Rust tests, 24 Node tests (21 actual-addon cases), 21 Bun addon cases**,
plus the architecture guard and its seven tests. The eleven core polish scenarios
separate seven selected source-derived/adapted cases from four local regressions.

The registry records **87 rows with named native evidence**, **23 with test
evidence**, **16 reviewed against source** and **73 still unverified**. Only **11
narrow polish rows** are marked verified; broader rows retain their remaining
interaction/platform gaps. Missing behaviors remain missing. These figures are
an audit state, not a percentage of complete Compositor parity.

Actual screenshots were inspected for Type alignment, paragraph selection/reflow,
mask thumbnail/brush outline, Crop overlay and the 800×520 layout. The final report
and screenshots are under `artifacts/desktop/compositor-polish/verification/`;
logs and earlier focused runs are under `artifacts/desktop/compositor-polish/`.

| Inspected native screenshot | Evidence |
| --- | --- |
| Type toolbar and point text | `20-typography.png` |
| Paragraph box after reflow | `22-resized-paragraph.png` |
| Grayscale mask thumbnail and brush outline | `02-mask-inspector.png` |
| Crop ratio/dimensions and composition overlay | `24-crop-controls.png` |
| Rulers, transform header and fixed footer at 800×520 | `23-minimum.png` |

Kit input/menus and Skia rasterization remain platform adaptations. IME preedit,
full bidi editing, physical HiDPI and Windows/macOS/Wayland interaction still need
runtime verification; source inspection and Linux screenshots do not establish
macOS pixel equivalence. Source startup/workspace tabs and advanced image/shape
controls remain tracked separately. The layers, masks and color pass below
replaces the earlier background-palette and list-gesture gaps.

## Layers, masks and color pass (2026-09-30)

The registry now records the completed interactions for existing layers, masks
and color controls: inline rename, context actions, visibility swipe, autoscroll,
Alt duplication/clipping, all 24 blend modes with preview/cancel/cycling, adjacent
mask thumbnails and chain control, Shift enable/disable, selection-consuming Add
Mask, coverage selection, mask copying, independent affine/distortion transforms,
and targeted-mask deletion. The color palette has independent foreground,
background and mask colors, X/D shortcuts, palette fills and canvas extensions.

The color picker is movable and nonmodal. It uses source-sized SV/hue controls
and the compact channel/button column; it retains a working color until OK and
remembers its position. RGB arrow stepping, Type draft identity, picker canvas
sampling, temporary Alt sampling and the old/new comparison ring have native
interaction coverage. The compact picker fits the 800×520 minimum window; the
rail scrolls like the source when all tools and swatches do not fit.

Verification commands:

```sh
npm run check:architecture
COMPOSITOR_REFERENCE_PATH=/tmp/electropic-compositor-reference npm run check
npm test
npm run test:bun
python3 crates/picsie-desktop/verify.py --tools artifacts/selection-history/tools/usr --software --output artifacts/desktop/layers-masks-colors/verification --display :99
```

The complete native verifier passed all 190 checks: the existing 140 workflow
checks plus 50 layers, masks and color checks. Architecture and pinned-source
registry validation, formatting, compilation and TypeScript checks passed. The
test suites passed 152 Rust core tests, 16 desktop tests, 25 Node tests (including
22 actual-addon tests), 22 Bun addon tests and seven architecture-guard tests.
Native interaction evidence was captured on Linux X11 at 1× using Xvfb and
lavapipe software Vulkan. Final results, traces and inspected screenshots live
under `artifacts/desktop/layers-masks-colors/verification/` and its sibling run log.

| Inspected native screenshot | What it shows |
| --- | --- |
| `24-picker-sampling.png` | Compact nonmodal picker, RGB/hex controls and canvas comparison ring |
| `25-sample-ring.png` | Original/new color halves while the eyedropper is held |
| `26-mask-distortion.png` | Mask distortion draft and nonprinting handles |
| `27-layers-masks-colors.png` | Source-style adjacent thumbnails and independent mask target |
| `28-color-minimum.png` | Picker and scrolling tool rail at 800×520 |

The same-window panel, Kit component/menu styles and standard GPUI cursor shapes
remain platform adaptations. Accessibility roles/labels are supplied; platform
screen-reader, Windows/macOS/Wayland and physical HiDPI interaction remain
unverified. Effect/adjustment child rows depend on the separate missing effect
stack/adjustment-layer systems and remain explicitly tracked in those areas.

## Selection, crop and transform pass (2026-10-01)

The native UI now follows the pinned selection amount panels and transform
inspector more closely. Ellipse and polygonal lasso have their source rail icons;
ellipse/lasso/wand expose Anti-alias; held Shift/Add and Alt/Subtract update the
header and cursor without changing defaults, and a draft keeps its starting mode.
Select Layer Pixels is in Select. Expand/Contract/Feather menu panels have the
source integer ranges/defaults, a slider/value, validation and Cancel/OK; Return
applies exactly once. Native black/white marching ants advance every 120 ms using
cached Rust vector resources, without engine edit commands or raster previews.

Transform has live Rust-owned numeric fields, a ratio lock, percentage scale
about center, Nearest/Smooth/High sampling and one Apply/Cancel transaction.
Auto Select and Show Controls stay pinned while numeric controls scroll; Apply
and Cancel remain visible at 800×520, including mask mode. Shift still preserves
proportions as explicitly requested. Ctrl/Command corner/edge drags distort image
and floating pixels; Shift constrains free motion; eight handles are shown with
no rotation stalk, matching the source. Cancel restores pixels and placement;
Apply resamples at full supported resolution, trims alpha and commits one undo.

Expanded crop previews reveal retained off-canvas artwork and checkerboard over
the union of original/crop bounds. Control bypasses snapping; a full-canvas frame
starts a new body drag. Crop Apply preserves source pixels and Cancel leaves the
document unchanged. The native verification caught and fixed Control routing to
layer picking, the transform scroller wrapper pushing Apply outside the window,
and the distortion rotation stalk. Harness updates await actual selector changes,
scroll to clipped controls, and safely observe pending mask-placement metadata.

The final release binary passed **all 226 combined native checks** on Linux X11
at 1× with lavapipe software Vulkan: the prior 190 workflows and **36 new
interaction checks**. `npm run check:architecture`, `npm run check`, `npm test`
and `npm run test:bun` pass: **168 core Rust tests, 16 desktop Rust tests, 26 Node
tests (23 actual-addon cases), 23 Bun addon cases**, plus the seven architecture
regressions. Contracts were regenerated from Rust with `npm run build:native`.
The source map distinguishes selected translated fixtures from local adapter
regressions; this is not a running macOS comparison.

Final report, traces and inspected screenshots are under
`artifacts/desktop/selection-crop-transforms/verification/`; the full log is
`artifacts/interaction-native-verification.log`. Screenshots 29–35 cover modifier
feedback, ellipse selection, amount panel, transform fields, distortion, expanded
crop and minimum window. The earlier focused run passed 35 checks; the final
combined run additionally verifies the source's hidden distortion rotation handle.

The [registry](compositor-registry.md) now has **200 behavior entries**. Remaining
material gaps include Apple Vision Object/Select Subject (a cross-platform model
is a separate feature), group/multi-layer transform boxes, text/group distortion
and effect-stack warping. Independent-mask Smooth/High placement still shares a
bilinear backend adapter; image High uses Skia Catmull-Rom rather than a Core
Graphics quality hint. QuickGUI retains a static outline. Tool defaults remain
session-local, and Windows/macOS GUI, Wayland and physical HiDPI remain unverified.
