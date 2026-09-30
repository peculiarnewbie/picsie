# Compositor source map

Reference repository: [robbietilton/Compositor](https://github.com/robbietilton/Compositor).

Pinned revision: [`609dbeae2ef68ef4fc82d67e4981a49852eb6e13`](https://github.com/robbietilton/Compositor/tree/609dbeae2ef68ef4fc82d67e4981a49852eb6e13), dated 2026-09-21. The development reference checkout is `/tmp/electropic-compositor-reference`; it is not a runtime dependency. Copyright and license are preserved in [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md), which is also included in native packaging.

## Direction

GIMP is available as a secondary [functionality reference](gimp-reference.md) for capabilities and edge cases, explicitly never for UX. Compositor remains authoritative for this port's behavior and workflows.

The accepted [architecture decision](architecture.md) is implemented: TypeScript owns the QuickGUI UI and Rust owns editor behavior and image processing. The current pass adds raster masks, crop, folders, pixel selections, and a bounded `.comp` package adapter while keeping old projects readable.

Port Compositor's existing behavior and algorithms before designing alternatives. QuickGUI replaces AppKit/SwiftUI, and the current Skia backend replaces CoreGraphics/Metal calls; neither requires inventing different editor semantics. Existing prototype differences are tracked below rather than claimed as upstream parity.

## Translated or adapted implementations

| Local implementation                               | Pinned upstream source                                                                                                                                                                                                                                                                                                                                       | What is carried over / adaptation                                                                                                                                                                                                                        |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `geometry.rs`: `boundsPoint`, resize handles       | [LayerTransform.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/LayerTransform.swift), `point` / `handles`                                                                                                                                                                               | Center-based clockwise geometry; bounds handles are independent of pixel flips. Local displayed dimensions are source dimensions multiplied by scale.                                                                                                    |
| `geometry.rs`: `resizeLayer`                       | Same file, `TransformDrag.updated`, resize branch                                                                                                                                                                                                                                                                                                            | Opposite-anchor resizing, original-diagonal projection for proportions, Option/Alt center resizing, and mirroring when crossing the anchor. The caller supplies initial handle plus pointer delta to avoid a jump. Local scale/coordinate limits remain. |
| `geometry.rs`: `rotateLayer`                       | Same file, rotation branch                                                                                                                                                                                                                                                                                                                                   | Angle delta around center and 15° snapping, including Swift's rounding of negative halfway values. The local schema normalizes angles to −180…180.                                                                                                       |
| `geometry.rs`: handle positions and rotation stalk | [TransformOverlay.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Rendering/TransformOverlay.swift), `TransformOverlayGeometry`                                                                                                                                                                   | Same eight positions and 28-screen-point rotation stalk. Local hit testing is still simpler: 6-point handle targets; whole-edge hit regions and platform resize cursors have not been ported.                                                            |
| `editor.rs`: `withInsertedLayers`                  | [EditorSession.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/EditorSession.swift), `addBlankLayer`; [SelectionClipboard.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/SelectionClipboard.swift), `addPixelLayer` | Insert new paint, gradient, shape, text, and imported layers above the active layer or inside its folder.                                                                                                                                                |
| `editor.rs`: `duplicate`                           | Same `SelectionClipboard.swift`, `duplicateActiveLayer`; [LayerGroups.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/LayerGroups.swift)                                                                                                                                                 | Preserve placement and immutable content/masks; duplicate selected folder subtrees, using local multi-selection as one transaction. Clipping references inside the copied selection/subtree are remapped to the new IDs.                                 |

`geometry.rs` still contains independently written pixel-coordinate and viewport adapters. Its source attribution applies to the listed transform routines, not an assertion that every function was translated.

## Five feature imports

| Rust implementation                                   | Pinned source and fixtures                                                                                                                                                                                                                                                                                                                                                                                                                                                  | Ported behavior and necessary adaptation                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| ----------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crop.rs`, `canvas_size.rs`, `editor.rs`, `render.rs` | [Crop.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/Crop.swift), [CropTests.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/CompositorTests/CropTests.swift)                                                                                                                                                                                          | Eight handles, aspect presets, edge snapping, reversible draft, and crop that translates layers while retaining source pixels. QuickGUI's C/Enter/Escape controls and Skia overlay replace AppKit controls. The local 8192 side/24MP limits apply.                                                                                                                                                                                                                                                                                                                                                                |
| `model.rs`, `render.rs`, `editor.rs`                  | [LayerMask.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/LayerMask.swift), [LayerMaskTests.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/CompositorTests/LayerMaskTests.swift), [MaskTransformTests.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/CompositorTests/MaskTransformTests.swift)       | New masks begin as 1×1 grayscale assets; completed strokes materialize immutable 8-bit coverage. Masks may be linked or placed independently; linked placement follows layer transforms. Old stroke lists remain readable. Local brush stamps and placed-mask resampling still differ from CoreGraphics.                                                                                                                                                                                                                                                                                                          |
| `model.rs`, `editor.rs`, `render.rs`                  | [LayerGroups.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/LayerGroups.swift), [GroupTests.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/CompositorTests/GroupTests.swift)                                                                                                                                                                          | Parent IDs, cycle/depth validation, depth-first rows, collapse, inherited visibility, pass-through opacity, grouping, and subtree deletion/duplication. QuickGUI offers folder drag targets and Into/Out actions. Folder masks multiply each descendant's coverage; live clipping is described below.                                                                                                                                                                                                                                                                                                             |
| `pixel_selection.rs`, `editor.rs`, `render.rs`        | [Selection.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/Selection.swift), [SelectionTests.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/CompositorTests/SelectionTests.swift), [SelectionFeatherTests.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/CompositorTests/SelectionFeatherTests.swift) | Rectangular/elliptical marquee and freehand lasso, replace/add/subtract, antialiased coverage, select all/deselect, coverage-clipped pixel clearing, and Select → Modify → Feather (`featherSelection`, `coverageBounds`). Rust stores coverage and rasterizes edited layer pixels. Fill, Invert, Expand, and Contract are supported. Polygonal lasso, wand/object selection, selection move/transform, and persisted selection state remain unsupported. Completed selection edits, including Feather, participate in undo/redo.                                                                                 |
| `comp.rs`, `files.rs`                                 | [ProjectStore.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/IO/ProjectStore.swift), [ProjectTests.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/CompositorTests/ProjectTests.swift)                                                                                                                                                                          | Reads versions 1–8 and writes version 8 directory packages with `manifest.json`, UUID-named PNG assets, grayscale mask PNGs, transforms, sampling, folders, opacity, and blend modes. Validates paths and writes a complete sibling package before replacement. Picsie's limits apply. Folder masks (version 6+) and linked mask sources (version 5+) round-trip. Live effects, adjustments, guides, and editable upstream text/shape metadata are rejected on import. Picsie text/shapes/gradients are rasterized on export and the UI announces that conversion. `.picsie`/`.electropic` files remain readable. |

The Rust behavior suite translates or adapts the cited crop, mask, group, selection, and package scenarios, including package overwrite and unsafe asset names; `tests/native.test.ts` runs those command paths through the actual addon. The upstream AppKit suite cannot run in this Linux workspace, and parity is claimed only for the behaviors stated above.

## Adapted upstream tests

[crates/picsie-core/tests/behavior.rs](../crates/picsie-core/tests/behavior.rs) identifies the relevant source fixtures:

- [TransformTests.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/CompositorTests/TransformTests.swift): `rotatedResizeKeepsOppositeAnchorAtEveryHandle` and the rotation/free-resize assertions from `moveRotateAndShiftConstraints`, preserving their input values and expected behavior.
- [LayerTests.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/CompositorTests/LayerTests.swift): the insertion ordering from `blankLayersAreTransparentAndInsertedAboveSelection`.
- [SelectionFeatherTests.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/CompositorTests/SelectionFeatherTests.swift): `featherSoftensTheSelectionAndWhatItClips`, preserving its 60 × 20 document, feather amount 6, probe rectangle (20, 0, 20, 20), and fading-coverage threshold. The original adaptation uses Clear; `reference_features.rs` additionally exercises Fill with feathered coverage.
- Additional local regression scenarios exercise the upstream diagonal-projection, center-resize, mirroring, rounding, and in-place duplication rules. These are labeled separately from translated fixtures.

The Swift/AppKit suite has not been executed in this Linux workspace. These are Rust adaptations of selected fixtures and additional regression checks, not a cross-runtime equivalence claim.

## Remaining prototype deviations

| Area               | Upstream reference                            | Remaining local deviation                                                                                                                                                                                                                                                                                                              |
| ------------------ | --------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Layer workflow     | `LayerGroups.swift`, `NativeLayerList.swift`  | Folder hierarchy and pass-through appearance are present. The QuickGUI panel offers folder drag targets, Into/Out actions, clipping controls, and folder masks. Option-drag copying, automatic scroll while dragging, and upstream thumbnail drag interactions remain.                                                                 |
| Transform session  | `LayerTransform.swift`, `EditorSession.swift` | Persistent Apply/Cancel transforms, guides, and distortion remain. Drag commits retain fractions. Canvas Shift-click toggles membership rather than upstream Cmd+Shift.                                                                                                                                                                |
| Ratio modifier     | `TransformDrag.updated`, `lockRatio != shift` | User-requested Shift-to-preserve maps to upstream with `lockRatio = false`; a persistent ratio-lock toggle is absent.                                                                                                                                                                                                                  |
| Masks and painting | `LayerMask.swift`, upstream brush engine      | Software brush spacing, curves, coverage union/accumulation, and opacity follow BrushStroke.swift. Native tip rasterization and placed-mask sampling differ from CoreGraphics. GPU coverage, optimized source tile publishing, healing, and cloning remain. Legacy vector strokes stay readable.                                       |
| Selection          | `Selection.swift`                             | Coverage remains session-only but is restored by undo/redo, including feather metadata and explicit empty selections. Polygonal lasso, wand/object selection, selection move/transform, selected-pixel transforms, the antialiasing toggle, and pixel-color inversion remain. Selection inversion and foreground Fill are implemented. |
| Project files      | `ProjectStore.swift`                          | The `.comp` adapter supports the raster/folder/raster-mask/live-mask subset within Picsie's lower limits. `.picsie` remains a single JSON file; `.comp` exports flatten Picsie's live text, shape, and gradient content. Rich upstream records are rejected explicitly.                                                                |
| UI and limits      | Upstream `UI/`, model validation              | QuickGUI controls and 8192 side/24MP/100-layer bounds are adaptations.                                                                                                                                                                                                                                                                 |

## Canvas Size and history port

- `crates/picsie-core/src/canvas_size.rs` translates `CanvasSizeDraft` and `CanvasSizeOptions.offset` from [CanvasSize.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/CanvasSize.swift), plus the document translation and colored extension routine from [CanvasResizer.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/IO/CanvasResizer.swift). Nine anchors, floor rounding, source preservation, off-canvas content, and transparent holes in colored extensions follow that source. Skia creates the extension PNG instead of CoreGraphics.
- `src/ui/canvas-size.tsx` follows [CanvasSizeSheet.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/UI/CanvasSizeSheet.swift). Center/absolute/pixels/unlocked/transparent defaults and the Cmd/Ctrl+Alt+C shortcut match upstream. Local differences: no inches/centimeters despite stored resolution metadata; no background-palette choice because this editor exposes only a foreground color; existing 8192-side/24MP/100-layer limits remain. The dimensions button gives Linux access without application menus. Guides remain unported.
- `crates/picsie-core/src/history.rs` adapts [DocumentHistory.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/DocumentHistory.swift): before/after snapshots, UUID revisions, nested transaction depth, value-equivalent no-ops, selection restoration, 100 entries/256 MB, and oldest-first pruning across undo and redo. `Editor` captures selection before and after each complete operation. Local adaptations retain multi-selection/range anchors, cancellation of live gestures, and the document captured by an asynchronous save. New documents still open separate windows, so creation of a window is not an undo entry.
- [crates/picsie-core/tests/behavior.rs](../crates/picsie-core/tests/behavior.rs) translates the four scenarios in [CanvasSizeTests.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/CompositorTests/CanvasSizeTests.swift) (physical units excluded; local limits substituted) and the layer/selection, navigation/no-op/revision, nested-transaction, and retained-storage scenarios in [HistoryTests.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/CompositorTests/HistoryTests.swift). Additional cases cover local validation, recoverable cropped artwork, multi-selection, and gesture cancellation.

## Selection feather port

- `crates/picsie-core/src/pixel_selection.rs` translates `DocumentSelection.feather` and `coverageBounds` from [Selection.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/Selection.swift), and `crates/picsie-core/src/editor.rs` translates `featherSelection(by:)` plus `confirmSelectionAmount`'s 1…250 Feather range. A Skia path and its cached coverage stand in for upstream's `CGPath`: coverage stays crisp as `applySelection` keeps only `antialiased`, the feather is metadata, and repeated applications combine like the blurs they represent (`min(250, √(f² + a²))`), matching upstream's rule exactly. The bridge rejects amounts outside 1…250 instead of the sheet's silent no-op.
- `render.rs`: `selection_coverage` translates `DocumentSelection.coverage()` — `clampedToExtent().applyingGaussianBlur(sigma: feather / 2)` is replaced by Skia's blur with `TileMode::Clamp`, which smears edge pixels the same way before the crop. Coverage-clipped clearing and the overlay read this softened buffer; `clip`'s region growth (`ceil(feather * 2)` plus the one-pixel allowance) is `PixelSelection::coverage_bounds`. `clampedToExtent` also means a feathered Select All still covers the canvas edges, which a local regression guards.
- `src/ui/selection.tsx` exposes Feather (default 2, range 1…250), Expand/Contract (default 5, range 1…500), Invert, Fill, and Clear through typed Rust commands. The QuickGUI inspector replaces upstream's header/menu prompts to fit compact windows. Feather uses upstream's `setSelection` transaction pattern. Coverage always antialiases, so the `antialiased || feather > 0` switch has no local equivalent yet.
- [crates/picsie-core/tests/behavior.rs](../crates/picsie-core/tests/behavior.rs) translates `featherSoftensTheSelectionAndWhatItClips` and adds labeled local regressions for stacking, the 250 clamp, hard-edged resets on a fresh outline, `canModifySelection` refusals, amount validation, and clamped coverage edges. `tests/native.test.ts` drives the command through the actual addon and checks the softened alpha falloff in an exported PNG.

## Pixel selection history port

- `history.rs` captures Rust-owned `PixelSelection` alongside each document snapshot, following `DocumentHistory.swift` (upstream stores its selection on the document itself). Coverage uses shared immutable `Arc<Vec<u8>>` buffers; feather changes share the same raster. Equality includes the outline, coverage, dimensions, bounds, and feather metadata, so equivalent edits preserve revisions and redo. Unique path storage (Skia generation IDs) and raster buffers retained only by history count toward the existing 256 MB budget across undo and redo. Paths are owned values with Skia copy-on-write storage, cloned before worker dispatch; no unsafe cross-thread wrapper is used.
- `editor.rs::set_pixel_selection` follows `Selection.swift::setSelection`, with one transaction for each completed rectangle, ellipse, or lasso, Select All, Deselect, and Feather. Layer/pixel edits restore both layer selection and pixel coverage. Changed selections advance the dirty revision as upstream does. Selection persistence is still outside the supported project subset: reopening starts without a pixel selection.
- `pixel_selection.rs::finish` follows `finishLasso` for clicks (Replace deselects; Add/Subtract leave the selection unchanged) and `applySelection` for subtraction from no selection (a no-op). A fully subtracted or off-canvas outline remains an explicit empty selection. This corrects the earlier prototype behavior that treated a Replace click as an empty selection.
- Escape calls the typed `CancelPixelSelection` command. Rust applies `EditorCanvas.swift`'s draft-first cancellation: an unfinished outline is discarded without an undo entry; the existing local Escape-to-deselect shortcut then uses a normal Deselect transaction when no draft exists. Tool changes also discard unfinished outlines. TypeScript owns only the shortcut and command dispatch.
- Crop and synchronous/asynchronous Canvas Size capture their selection reset in the same transaction as the canvas change, restoring matching geometry and coverage on undo. Clearing the selection on canvas replacement remains a local adaptation; this pass does not port selection geometry through resizing.
- [selection_history.rs](../crates/picsie-core/tests/selection_history.rs) translates `SelectionTests.clickDeselectsAndSelectionStepsUndo` and `emptySelectionIsDistinctFromNoSelection`, and adapts `replaceAddAndSubtractCombineOutlines` with full history round-trips. Separately labeled local regressions cover ellipse/click history, saved revisions and no-ops, feather and cleared pixels, cancellation, nested transactions, canvas changes, and retained raster budgets. Existing `SelectionFeatherTests` and `HistoryTests` adaptations continue to run. `tests/native.test.ts` checks batched drags, Escape cancellation, restored feathered PNG pixels, and asynchronous resize/crop through the actual Node/Bun addon.

## Selection operations, software brush, and clipping port

The four-item functionality pass implements selection history, basic selection operations, brush fidelity, and folder masks/clipping. The engine is Rust; TypeScript exposes controls and small metadata. GIMP supplied functionality questions only, with no copied code, fixtures, shortcuts, or UX.

| Implementation                                     | Pinned Compositor source                                                                                                                                                                                                                                                                                                                                   | Behavior and adaptation                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| -------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `pixel_selection.rs`, `render.rs`, `editor.rs`     | [Selection.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/Selection.swift), [SelectionEdits.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/SelectionEdits.swift), `BrushStroke.paintCanvas`                      | Invert subtracts the outline from the canvas. Expand/Contract union/subtract a round stroked band and preserve feather. Skia path operations replace CGPath operations; this supersedes the earlier four-sample polygon raster. Fill uses foreground color over selected coverage (whole canvas without a selection), grows source bounds while preserving transforms/mask placement, and recolors unselected live text without rasterizing it. On masks, Fill uses Hide/Reveal and Clear uses the opposite color. Explicit empty selections leave pixels and history unchanged.                                                                                                                                        |
| `brush.rs`, `asset.rs`, `editor.rs`                | [BrushStroke.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/BrushStroke.swift), [EditorSession+Brush.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/EditorSession%2BBrush.swift)                                 | Translated software Catmull–Rom interpolation, provisional tail replacement, hard/soft spacing (0.015/0.025 diameter, minimum 0.25), normalized Gaussian falloff, hard maximum/soft screen coverage, and whole-stroke opacity. Brush diameter is in document pixels, including rotated/flipped/nonuniform transforms. Smoothing uses a screen-space string and flushes to the pointer; Shift-click joins the last endpoint on the same target. Coverage uses sparse 256px copy-on-write tiles. New content is an immutable native image, masks are grayscale buffers, and PNG encoding occurs at persistence/export boundaries. Blank painting crops to touched coverage; source growth preserves image/mask placement. |
| `live_mask.rs`, `render.rs`, `model.rs`, `comp.rs` | [LiveLayerMask.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/LiveLayerMask.swift), [LiveMaskRenderer.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Rendering/LiveMaskRenderer.swift), `LayerMask.swift::FolderMaskClip` | Folder masks multiply each descendant and nested mask in the pass-through stack. Adding a mask consumes a selection in the same undo step. Live source graphs reject cycles, missing IDs, and folder sources/targets; hidden sources still supply alpha including their own masks and opacity. Contiguous sibling clipping stacks restore the base alpha after color blending, avoiding dark fringes. Reordering adopts/releases clipping links; copies remap internal links. `.picsie` v2 and `.comp` packages retain masks and dependencies.                                                                                                                                                                          |

Necessary adaptations and remaining limits:

- Native hard-tip edges use four coverage samples; soft tips evaluate the falloff directly instead of CoreGraphics's sampled gradient/tip cache. Preview assembly copies the affected bounding image from sparse coverage; it does not yet publish only changed source tiles or implement the Metal continuous-coverage path. No performance parity is claimed. Images use native identity for history comparisons; no PNG encoding runs during stroke comparison/preview.
- Diameter defaults to upstream's 40px, hardness 100%, opacity 100%, smoothing 0. The existing foreground swatch, standalone Eraser tool, zero-opacity setting, and automatic blank layer creation remain local UI/workflow adaptations. Bracket size shortcuts retain the existing 5px step. There is one foreground palette, so separate background-color Fill is unavailable. Healing, cloning, and other brush tools remain outside this pass.
- Ordinary layer masks retain the existing nearest placed-mask sampling. Folder mask rendering follows upstream's folder transform and pass-through clipping. Live coverage is resolved iteratively within the canvas to bound temporary memory; deletion renders source coverage through the inverse target transform, including source pixels outside the canvas. Skia sampling can differ at transformed/subpixel edges from CoreGraphics.
- Deleting a live source automatically takes upstream's **Bake and Delete** path as one undoable operation. The existing immediate Delete workflow is retained; the upstream choice dialog for unlinking instead is not ported. Source links can be released explicitly before deletion. QuickGUI exposes **Clip to layer below** and a source picker in place of upstream thumbnail gestures. GIMP UX is not used.
- Existing limits remain 8192px per side, 24MP per surface, and 100 layers; source growth beyond those limits fails atomically. Selections remain session-only. More advanced selection operations and richer `.comp` live records remain unported.

`reference_features.rs` adapts the upstream `SelectionTests` expansion/contraction fixtures; feathered fill from `SelectionFeatherTests`; seven pixel scenarios from `BrushTests` (opacity cap, soft accumulation, spacing ripple, sparse curves, provisional tail replacement, trimmed bounds, and document-space diameter on a rotated/nonuniform/flipped layer); folder coverage/painting from `LayerMaskTests`; and soft-alpha, hidden-source, graph edit, persistence, and deletion scenarios from `LiveMaskTests`. Test comments identify the source scenarios. Local regressions separately cover transformed fill/mask placement, empty selections, live-text recoloring, interrupted smoothed strokes, native image persistence, folder/multi-layer copies, off-canvas dependency baking, and `.picsie` round-trips. The actual-addon suite exercises the commands under Node and Bun, including Shift-click and `.comp` reopen. These are selected translated/adapted fixtures, not execution of the Swift/AppKit suite or full upstream parity.

## Editor UX port (text, inspector, color picker, layer list, picking)

- **Text** — `render.rs` translates `TypeTool.textImage`'s layout: box text word-wraps at the box width minus `LayerTextStyle.padding` (12 px each side), with the padded box clipping overflow. `beginTextGesture`'s click-on-live-text becomes the `EditText` command (also reached by a second press on a layer row, as `NativeLayerList`'s `doubleAction` opens what a row holds). Commit keys are the user's explicit choice — Enter commits and Shift+Enter inserts a line — instead of upstream's ⌘Return on its inline `NSTextView`. Editing uses QuickGUI's controlled value contract: every keystroke previews through `beginPropertyEdit("Edit text")` and finishes as one undo entry, matching `applyText`'s single entry. A second press is read from QuickGUI's `MouseEventDetails.clickCount` (Button `onDoubleClick` delivery proved unreliable). Not ported: point text auto-growing boxes, box-handle resizing of text boxes, tracking/leading controls, the overset `+` marker.
- **Inspector** — `FilterSheet`'s repeating row (caption, slider, value, unit) and `LayerAppearanceControls`' opacity slider become `SliderField` in `src/ui/controls.tsx`, with the `beginOpacityEdit`/`finishOpacityEdit` bracket generalized as `beginPropertyEdit`/`finishGesture`, so a slider drag is one undo entry. Adjustments stay local per-layer properties (upstream uses adjustment layers and floating filter sheets; those remain unported).
- **Color** — `ColorPickerSheet`'s saturation/brightness square, hue strip, live preview, integer R/G/B fields, and hex field are ported in `src/ui/color-picker.tsx`, with `ColorPalette.swift`'s `PaletteColor`/`PickerHSB` math in `src/ui/color-math.ts`: `RRGGBB`/`RGB` with or without `#`, uppercase `RRGGBB` formatting, 8-bit HSB round-trips, and Photoshop's rule that grays keep the previous hue and black keeps hue and saturation. Nothing is written until OK. The Fill swatch targets the selected layer's color, standing in for upstream's `ColorPickerTarget`. Adaptations: the picker is a modal QuickGUI dialog, so canvas sampling while open is not available (use the Sample tool first, as the caption says); QuickGUI's CSS-like gradient strings paint the square and strip.
- **Layer list** — `NativeLayerList`'s drop semantics are ported in `src/ui/layers.tsx`: whole rows drag (upstream has no grip), drops between rows reorder into the target's parent, drops on a folder row file inside it, and moves are single undo entries with the dragged selection retained. Adaptations: fixed-height-row coordinate math instead of `NSTableView`; Option-drag copy and effect/mask drags are not ported. Fixed as part of this pass: QuickGUI's release event does not carry the drag total in `delta` — displacement comes from `localPosition − localOrigin`.
- **Picking** — `EditorCanvas.transformPressLayer`'s Cmd-click re-picking becomes `pickUnder`, available as Ctrl/Cmd-click through the pointer stream and as middle-click where modifier tracking is unreliable. Cycling the whole bounds-hit stack is a local extension: upstream has no pick-through affordance at all, and a full-canvas layer would otherwise bury everything beneath it.

## Rust migration verification

The Rust suite covers geometry, crop, raster masks, folders, selection coverage, the `.comp` package subset, old project round-trips, and existing editor behavior. Actual-addon tests in `tests/native.test.ts` cover the new command paths and package reopening as well as worker lifetime, native previews, and metadata boundaries. The original saved projects remain in `tests/fixtures/`.

Native Skia replaces the previous TypeScript orchestration of Skia. Layer opacity is quantized to the same 8-bit value before compositing. Native SkParagraph provides shaping/fallback and preserves Canvas2D top-baseline placement. Generic family names now resolve through the system font manager; this can change text appearance from the old canvas package's fallback font. Antialiasing can also vary across Skia/platform versions; screenshot verification supplements numeric pixel fixtures. Preview transport uses native uncompressed TIFF resources because QuickGUI 0.1.6 exposes path-backed images rather than shared textures; this is documented with its copies in the architecture decision.

The legacy v1 stroke lists remain readable. New masks persist immutable 8-bit grayscale coverage and `.comp` packages use grayscale PNG assets. The brush rasterization/performance adaptations and project subset limits above remain explicit deviations from Compositor.

## GPUI Kit UI translation (2026-09-29)

The user authorized a Rust UI after the GPUI Kit experiment. `crates/picsie-desktop`
reproduces the QuickGUI controls and workflows while invoking the same editor
commands, renderer, history, codecs, and project adapters. It does not introduce
new editing semantics. The QuickGUI application remains available as the parity
reference; GIMP contributes no UX.

`crates/picsie-desktop/src/ui/dialogs/color.rs` translates `PickerHSB` and hex
parsing from the pinned `Compositor/Document/ColorPalette.swift`, through the
existing `src/ui/color-math.ts` form contract. Three test groups translate the
hex, round-trip, and gray/black hue-preservation cases in the pinned
`CompositorTests/ColorPickerTests.swift`. They are a selected subset, not a claim
of complete upstream fixture coverage. Dialog layout and canvas-size draft math
follow `src/ui/color-picker.tsx`, `canvas-size.tsx`, and `canvas-size-draft.ts`,
with the pinned `ColorPickerSheet.swift` and `CanvasSizeSheet.swift` consulted.
Kit supplies input editing, clipboard, selects, sliders, and modal focus handling.

Native file pickers have platform styling and no extension-filter API in this
GPUI version; the engine still validates opened files. Native menu availability
follows the platform. The native preview replaces QuickGUI's TIFF publication
with a BGRA memory upload, while image processing remains CPU Skia. Verification
and remaining platform work are documented in [the parity record](gpui-ui-parity.md).

On 2026-09-30 this became the default launch/build/release application. Native
packaging and `src/launch.rs` are local platform integration, not translated
Compositor editor algorithms. Positional paths open projects in separate windows,
matching the existing New/Open window workflow; `--open` remains compatible.
The shared engine still validates and reads all project contents. QuickGUI remains
the explicit UI parity reference. No automatic updater has been ported; native
upgrades use release downloads. Linux package verification exercises extracted
artifacts; Windows/macOS execution and macOS file-open callbacks remain unverified.

The pinned `CompositorApplicationDelegate.swift` open-URL callback,
`Document/ProjectWorkspace.swift` URL routing, and `ProjectWorkspaceTests.swift`
were consulted. Upstream routes projects into tabs; this integration preserves
the user-selected QuickGUI workflow of independent windows. The launch parser's
three test groups are local platform regressions, not translated upstream fixtures.

## Bounded desktop optimization (2026-09-29)

The desktop worker now signals frame/completion availability instead of polling
every 8 ms. Pointer commands and completion events keep their existing reliable
queues; only redundant wake notifications coalesce. This is toolkit scheduling,
not an editor behavior change.

Profiling identified the local Skia identity color matrix as a substantial cost
even for neutral appearance settings. After reviewing the pinned `LayerRenderer.swift`,
`ImageAdjustments.swift`, and appearance/adjustment tests, `render.rs` now omits that
matrix only for opaque rectangle/gradient fills drawn as pixel-aligned copies,
with full opacity, normal blending, no effects or masks, and no parent transform.
Other cases keep the existing matrix: broadly removing it changes Skia's rounding
on translucent pixels. This is a local backend optimization, not a new upstream
algorithm or a change to adjustment semantics.

`tests/render_equivalence.rs` adds local exact-pixel comparisons against the prior
identity-matrix path across alpha ramps, sixteen blend modes, sampling modes,
blur, transforms, and opaque/translucent fills. These are regression cases for
the optimization, not additional translated upstream fixtures. The release
`profile_preview` example times the composite, complete preview, and isolated
demo layers to make the profiling reproducible.

## Retained preview experiment (2026-09-30)

`render/composite.rs` adds a local Skia backend adaptation after inspection of
the pinned `EditorCanvas.swift` damage path, `TiledLayerRenderer.swift` replacement
regions, `EffectsPreviewCache.swift` identity/placement keys, and the corresponding
tile/downsample tests. It is not a direct port of Compositor's tile renderer or
GIMP's GPL projection implementation. No GIMP code or fixtures were copied.

Rust retains document-composite pixels and an immutable snapshot. Skia borrows
the Rust pixel buffer only during a render call, preserving the Node addon's
ability to move its locked renderer between worker threads. A Skia `Surface`
cannot be retained across those threads. The raster-direct snapshot makes an
immutable copy when document pixels change; preview publication still extracts
and uploads the full viewport through the existing UI integrations.

Ordinary unit-scale, unrotated moves recompose the union of the old/new bounds,
expanded outward for sampling, by clearing and drawing the entire layer stack
inside that clip. Masks, blur, folders, live clipping, and any transformed layer
in the stack use full recomposition. Even a stationary transformed overlapping
layer can produce different Skia pixels when its clip changes, which an exact
comparison caught; its full-render fallback preserves current rounding.
Unchanged composites can be reused for viewport or overlay changes. The existing
checkerboard algorithm is also retained as an opaque image keyed by document
dimensions and viewport geometry; its colors, scale and placement are unchanged.

Desktop command dispatch now relies on the existing worker wake to publish
changed state instead of explicitly notifying the UI to draw the old image
immediately after queueing a command. Focus changes still request their own
refresh; transient dialogs/file actions and worker errors/completions retain
their existing notifications. Commands and pointer samples are not dropped.

The desktop worker also waits until the UI takes its queued frame before
preparing another full preview. It continues applying every edit and delivering
ordered completion/error events while a frame waits. Taking a frame sends an
internal demand message, so pending final edits still produce a frame even if
no further input arrives; that message does not advance the edit sequence.
This is presentation scheduling only, not a change to gesture/history semantics.

`render_equivalence.rs` adds local cached-vs-full pixel comparisons across
translucent content, all sixteen blends, sampling modes, fractional/off-canvas
moves, overlapping layers, viewport/handle changes, effects, masks, hierarchy,
reordering and restoring prior state. The cache unit test checks that retained
snapshots remain immutable. These are backend regression cases; no additional
upstream fixture is claimed as translated. The separate experiment report records
the [application measurements and verification](desktop-gimp-experiments.md).

### Native BGRA extraction (second experiment pass)

`render::bgra_pixels` is a local Skia presentation adaptation. For tightly packed,
untagged BGRA surfaces whose pixels are all opaque, it copies the existing bytes
instead of asking Skia to convert premultiplied pixels to straight alpha. Opaque
pixels have identical representations. Other color layouts, padded rows,
translucent pixels, or tagged color spaces retain the previous Skia conversion.
The desktop calls this helper; its image upload still copies a complete viewport.
This is not shared GPU memory or a translated upstream renderer.

Local equivalence cases compare against the original Skia conversion across
BGRA/RGBA surfaces, opaque/translucent alpha, row padding, and linear-sRGB tags.
The `--measure-moving` diagnostic adds actual alternating layer moves to the
existing CPU transport microbenchmark. It changes no application workflow.
Optional `PICSIE_GPU_DIAGNOSTICS=1` prints the toolkit's selected adapter once
when opening a window, allowing hardware claims to be checked rather than inferred
from an installed driver. Rejected viewport caches, image tiling, toolkit patches,
and paint-paced delivery are retained only as ignored experimental artifacts.

### X11 frame waking (third experiment pass)

The desktop vendors the published `gpui-pre-linux` 0.3.7 crate, retaining its
Apache-2.0 license, original file hashes and a patch against the original source.
Only `linux/x11/client.rs` and `linux/x11/window.rs` change: they implement
GPUI's existing `PlatformWindow::frame_waker` contract through a coalescing
calloop ping. A visible window requests the normal toolkit frame callback when
invalidated; its monitor timer, surface queue depth and presentation mode are
unchanged. Hidden windows ignore demand pings and resume through the existing
visibility/timer path. Closing a window removes its ping registration.
Requests from inside a frame callback use the existing timer, avoiding a busy
retry loop when the toolkit throttles animation.

This is independently written platform integration, not translated editor
behavior. The pinned `EditorCanvas.swift` initializer's `refreshCanvasPreview`
calls `synchronizeDisplay()` and `displayIfNeeded()`; that supports investigating
prompt presentation but does not establish GPUI timing parity. GPUI's own
`window.rs` frame-waker implementation and `test_frame_waker_fires_on_frame_demand`
were inspected. The upstream toolkit test was not executed. No GIMP source or
fixtures were copied. The QuickGUI parity reference and explicit Shift resize
behavior remain authoritative and unchanged.

[The backend source record](../crates/picsie-desktop/vendor/gpui-pre-linux/PICSIE.md)
identifies the pinned package/revision, exact patch, licensing and upgrade/removal
instructions. `verify-frame-wakeup.py` adds local real-window visibility/focus
regressions, rather than claiming additional translated upstream fixtures.
The [experiment report](desktop-gimp-experiments.md) records native workflow,
exact canvas, untraced performance and hardware-adapter verification. Measured
improvements are limited to this Linux virtual-display workload; physical
display and other-platform performance remain unmeasured.
