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
| `editor/group_transform.rs`, transform header/overlay | [LayerTransform.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/LayerTransform.swift) (`TransformGroup`, `following`, `placing`, `TransformDrag.updated`), [EditorSession.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/EditorSession.swift) (`transformsAsGroup`, `groupTransformMembers`, `groupTransformBox`, `begin/commitTransform`, `nudgeLayer`), [LayerFlip.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/LayerFlip.swift) (`mirrored`, `flipLayers`), [TransformInspector.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/UI/TransformInspector.swift), [TransformOverlay.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Rendering/TransformOverlay.swift) | One transform box for a folder or multi-selection; resize/rotate/numeric edits carry every member from its original placement in one undo, with linked-mask following, Apply/Cancel and a published box capability for the UI. Adaptations: box reuse of the local resize/rotate math with Shift-to-preserve (`shift \|\| lock`) and unrounded drags; member rotation normalized to ±180° for the local ±360° validation; locked/hidden/blank members excluded per local move semantics (upstream has no locked field); group distortion and group sampling not ported; member replacement is one indexed pass with a selection+`retained_eq` memo. |
| `editor.rs`: `duplicate`                           | Same `SelectionClipboard.swift`, `duplicateActiveLayer`; [LayerGroups.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/LayerGroups.swift)                                                                                                                                                 | Preserve placement and immutable content/masks; duplicate selected folder subtrees, using local multi-selection as one transaction. Clipping references inside the copied selection/subtree are remapped to the new IDs.                                 |

`geometry.rs` still contains independently written pixel-coordinate and viewport adapters. Its source attribution applies to the listed transform routines, not an assertion that every function was translated.

## Five feature imports

| Rust implementation                                   | Pinned source and fixtures                                                                                                                                                                                                                                                                                                                                                                                                                                                  | Ported behavior and necessary adaptation                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| ----------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crop.rs`, `canvas_size.rs`, `editor.rs`, `render.rs` | [Crop.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/Crop.swift), [CropTests.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/CompositorTests/CropTests.swift)                                                                                                                                                                                          | Eight handles, aspect presets, edge snapping, reversible draft, and crop that translates layers while retaining source pixels. QuickGUI's C/Enter/Escape controls and Skia overlay replace AppKit controls. The local 8192 side/24MP limits apply.                                                                                                                                                                                                                                                                                                                                                                |
| `model.rs`, `render.rs`, `editor.rs`                  | [LayerMask.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/LayerMask.swift), [LayerMaskTests.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/CompositorTests/LayerMaskTests.swift), [MaskTransformTests.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/CompositorTests/MaskTransformTests.swift)       | New masks begin as 1×1 grayscale assets; completed strokes materialize immutable 8-bit coverage. Masks may be linked or placed independently; linked placement follows layer transforms. Old stroke lists remain readable. Local brush stamps and placed-mask resampling still differ from CoreGraphics.                                                                                                                                                                                                                                                                                                          |
| `model.rs`, `editor.rs`, `render.rs`                  | [LayerGroups.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/LayerGroups.swift), [GroupTests.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/CompositorTests/GroupTests.swift)                                                                                                                                                                          | Parent IDs, cycle/depth validation, depth-first rows, collapse, inherited visibility, pass-through opacity, grouping, and subtree deletion/duplication. QuickGUI offers folder drag targets and Into/Out actions. Folder masks multiply each descendant's coverage; live clipping is described below.                                                                                                                                                                                                                                                                                                             |
| `pixel_selection.rs`, `editor.rs`, `render.rs`        | [Selection.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/Selection.swift), [SelectionTests.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/CompositorTests/SelectionTests.swift), [SelectionFeatherTests.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/CompositorTests/SelectionFeatherTests.swift) | Rectangular/elliptical marquee and freehand lasso, replace/add/subtract, antialiased coverage, select all/deselect, coverage-clipped pixel clearing, and Select → Modify → Feather (`featherSelection`, `coverageBounds`). Rust stores coverage and rasterizes edited layer pixels. Fill, Invert, Expand, and Contract are supported. Polygonal lasso, color wand, selection movement and selected-pixel transforms are supported in the native app; object selection and persisted selection state remain unsupported. Completed selection edits, including Feather, participate in undo/redo.                                                                                 |
| `comp.rs`, `files.rs`                                 | [ProjectStore.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/IO/ProjectStore.swift), [ProjectTests.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/CompositorTests/ProjectTests.swift)                                                                                                                                                                          | Reads versions 1–8 and writes version 8 directory packages with `manifest.json`, UUID-named PNG assets, grayscale mask PNGs, transforms, sampling, folders, opacity, and blend modes. Validates paths and writes a complete sibling package before replacement. Picsie's limits apply. Folder masks (version 6+) and linked mask sources (version 5+) round-trip. Live effects, adjustments and editable upstream shape metadata are rejected on import. Guides and live text round-trip. Picsie shapes/gradients and legacy translucent text rasterize on export, with a UI notice. `.picsie`/`.electropic` files remain readable. |

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
| Transform session  | `LayerTransform.swift`, `EditorSession.swift` | Persistent Apply/Cancel layer and selected-pixel transforms are present in the native UI. Guides and distortion remain. Drag commits retain fractions. Canvas Shift-click toggles membership rather than upstream Cmd+Shift.                                                                                                                                                                |
| Ratio modifier     | `TransformDrag.updated`, `lockRatio != shift` | User-requested Shift-to-preserve maps to upstream with `lockRatio = false`; a persistent ratio-lock toggle is absent.                                                                                                                                                                                                                  |
| Masks and painting | `LayerMask.swift`, upstream brush engine      | Software brush spacing, curves, coverage union/accumulation, and opacity follow BrushStroke.swift. Native tip rasterization and placed-mask sampling differ from CoreGraphics. GPU coverage, tiled mask publication, healing, and cloning remain. Legacy vector strokes stay readable.                                       |
| Selection          | `Selection.swift`                             | Coverage remains session-only but is restored by undo/redo, including feather metadata and explicit empty selections. Polygonal lasso, color wand, outline movement, selected-pixel movement/duplication and transforms, and the pixel clipboard are present in the native UI. Object selection, the antialiasing toggle, and pixel-color inversion remain. Selection inversion and foreground Fill are implemented. |
| Project files      | `ProjectStore.swift`                          | The `.comp` adapter supports the raster/folder/raster-mask/live-mask subset within Picsie's lower limits. `.picsie` remains a single JSON file; `.comp` retains guides and live RGB text; Picsie shapes/gradients and legacy translucent text rasterize. Rich upstream records are rejected explicitly.                                                                |
| UI and limits      | Upstream `UI/`, model validation              | GPUI Kit controls, the retained QuickGUI reference, and 8192 side/24MP/100-layer bounds are adaptations.                                                                                                                                                                                                                                                                 |

## Canvas Size and history port

- `crates/picsie-core/src/canvas_size.rs` translates `CanvasSizeDraft` and `CanvasSizeOptions.offset` from [CanvasSize.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/CanvasSize.swift), plus the document translation and colored extension routine from [CanvasResizer.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/IO/CanvasResizer.swift). Nine anchors, floor rounding, source preservation, off-canvas content, and transparent holes in colored extensions follow that source. Skia creates the extension PNG instead of CoreGraphics.
- `src/ui/canvas-size.tsx` follows [CanvasSizeSheet.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/UI/CanvasSizeSheet.swift). Center/absolute/pixels/unlocked/transparent defaults and the Cmd/Ctrl+Alt+C shortcut match upstream. Local differences: no inches/centimeters despite stored resolution metadata; no background-palette choice because this editor exposes only a foreground color; existing 8192-side/24MP/100-layer limits remain. The dimensions button gives Linux access without application menus. Native guides now follow the canvas translation; see the placement/type pass below.
- `crates/picsie-core/src/history.rs` adapts [DocumentHistory.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/DocumentHistory.swift): before/after snapshots, UUID revisions, nested transaction depth, value-equivalent no-ops, selection restoration, 100 entries/256 MB, and oldest-first pruning across undo and redo. `Editor` captures selection before and after each complete operation. Local adaptations retain multi-selection/range anchors, cancellation of live gestures, and the document captured by an asynchronous save. New documents still open separate windows, so creation of a window is not an undo entry.
- [crates/picsie-core/tests/behavior.rs](../crates/picsie-core/tests/behavior.rs) translates the four scenarios in [CanvasSizeTests.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/CompositorTests/CanvasSizeTests.swift) (physical units excluded; local limits substituted) and the layer/selection, navigation/no-op/revision, nested-transaction, and retained-storage scenarios in [HistoryTests.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/CompositorTests/HistoryTests.swift). Additional cases cover local validation, recoverable cropped artwork, multi-selection, and gesture cancellation.

## Selection feather port

- `crates/picsie-core/src/pixel_selection.rs` translates `DocumentSelection.feather` and `coverageBounds` from [Selection.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/Selection.swift), and `crates/picsie-core/src/editor.rs` translates `featherSelection(by:)` plus `confirmSelectionAmount`'s 1…250 Feather range. A Skia path and its cached coverage stand in for upstream's `CGPath`: coverage stays crisp as `applySelection` keeps only `antialiased`, the feather is metadata, and repeated applications combine like the blurs they represent (`min(250, √(f² + a²))`), matching upstream's rule exactly. The bridge rejects amounts outside 1…250 instead of the sheet's silent no-op.
- `render.rs`: `selection_coverage` translates `DocumentSelection.coverage()` — `clampedToExtent().applyingGaussianBlur(sigma: feather / 2)` is replaced by Skia's blur with `TileMode::Clamp`, which smears edge pixels the same way before the crop. Coverage-clipped clearing and the overlay read this softened buffer; `clip`'s region growth (`ceil(feather * 2)` plus the one-pixel allowance) is `PixelSelection::coverage_bounds`. `clampedToExtent` also means a feathered Select All still covers the canvas edges, which a local regression guards.
- `src/ui/selection.tsx` exposes Feather (default 2, range 1…250), Expand/Contract (default 5, range 1…500), Invert, Fill, and Clear through typed Rust commands. The QuickGUI inspector replaces upstream's header/menu prompts to fit compact windows. Feather uses upstream's `setSelection` transaction pattern. The 2026-10-01 pass below adds captured Anti-alias settings and the source `antialiased || feather > 0` rule.
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

- Native hard-tip edges use four coverage samples; soft tips evaluate the falloff directly instead of CoreGraphics's sampled gradient/tip cache. The image path now publishes flat immutable source/replacement patches as described in the performance pass below; mask snapshots still assemble contiguous grayscale pixels. Metal continuous coverage remains unported. No performance parity is claimed. Images use native identity for history comparisons; no PNG encoding runs during stroke comparison/preview.
- Diameter defaults to upstream's 40px, hardness 100%, opacity 100%, smoothing 0. The existing foreground swatch, standalone Eraser tool, zero-opacity setting, and automatic blank layer creation remain local UI/workflow adaptations. Bracket size shortcuts retain the existing 5px step. There is one foreground palette, so separate background-color Fill is unavailable. Healing, cloning, and other brush tools remain outside this pass.
- Ordinary layer masks retain the existing nearest placed-mask sampling. Folder mask rendering follows upstream's folder transform and pass-through clipping. Live coverage is resolved iteratively within the canvas to bound temporary memory; deletion renders source coverage through the inverse target transform, including source pixels outside the canvas. Skia sampling can differ at transformed/subpixel edges from CoreGraphics.
- Deleting a live source automatically takes upstream's **Bake and Delete** path as one undoable operation. The existing immediate Delete workflow is retained; the upstream choice dialog for unlinking instead is not ported. Source links can be released explicitly before deletion. QuickGUI exposes **Clip to layer below** and a source picker in place of upstream thumbnail gestures. GIMP UX is not used.
- Existing limits remain 8192px per side, 24MP per surface, and 10,000 layers; source growth beyond those limits fails atomically. Selections remain session-only. Object selection, pixel-color inversion and richer `.comp` live records remain unported; the 2026-10-01 pass below adds the antialias toggle.

`reference_features.rs` adapts the upstream `SelectionTests` expansion/contraction fixtures; feathered fill from `SelectionFeatherTests`; seven pixel scenarios from `BrushTests` (opacity cap, soft accumulation, spacing ripple, sparse curves, provisional tail replacement, trimmed bounds, and document-space diameter on a rotated/nonuniform/flipped layer); folder coverage/painting from `LayerMaskTests`; and soft-alpha, hidden-source, graph edit, persistence, and deletion scenarios from `LiveMaskTests`. Test comments identify the source scenarios. Local regressions separately cover transformed fill/mask placement, empty selections, live-text recoloring, interrupted smoothed strokes, native image persistence, folder/multi-layer copies, off-canvas dependency baking, and `.picsie` round-trips. The actual-addon suite exercises the commands under Node and Bun, including Shift-click and `.comp` reopen. These are selected translated/adapted fixtures, not execution of the Swift/AppKit suite or full upstream parity.

## Editor UX port (text, inspector, color picker, layer list, picking)

- **Text** — `render.rs` translates `TypeTool.textImage`'s layout: box text word-wraps at the box width minus `LayerTextStyle.padding` (12 px each side), with the padded box clipping overflow. `beginTextGesture`'s click-on-live-text becomes the `EditText` command (also reached by a second press on a layer row, as `NativeLayerList`'s `doubleAction` opens what a row holds). Commit keys are the user's explicit choice — Enter commits and Shift+Enter inserts a line — instead of upstream's ⌘Return on its inline `NSTextView`. Editing uses QuickGUI's controlled value contract: every keystroke previews through `beginPropertyEdit("Edit text")` and finishes as one undo entry, matching `applyText`'s single entry. A second press is read from QuickGUI's `MouseEventDetails.clickCount` (Button `onDoubleClick` delivery proved unreliable). This describes the earlier QuickGUI adapter. The native placement/type pass below supersedes those gaps.
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
artifacts; Windows native compilation, tests and package executable checks also
passed in CI. Windows/macOS editor interaction and macOS file-open callbacks remain
unverified.

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


## Native clipboard, selection tools, merging and Image Size (2026-09-30)

This pass implements all four requested feature groups in the default Rust / GPUI Kit
application. The QuickGUI application remains the earlier UI parity reference; its
UI has not been expanded with these new controls. Rust owns every pixel operation.

| Implementation | Pinned Compositor source | Ported behavior and adaptations |
| --- | --- | --- |
| `editor/operations.rs`, desktop `engine.rs`, `ui/clipboard.rs` | `Document/SelectionClipboard.swift` | Copy/Cut/Paste, Copy Merged and Layer via Copy. Copy uses the selected path's tight canvas-clipped bounds, including feather growth; active-layer copying ignores appearance, opacity and masks, while mask copying produces opaque gray and Copy Merged uses the visible composite. Internal paste preserves origin across native windows; external images center on the canvas. New layers use the first unused `Layer N` name. PNG encoding/decoding occurs only at the operating-system clipboard boundary, on the engine worker. Kit preserves text-field clipboard handling. |
| `pixel_selection.rs`, `editor.rs`, `editor/operations.rs`, desktop `ui/input.rs` | `Document/Selection.swift`, `SelectionEdits.swift`, `FloatingSelection.swift`, `Rendering/EditorCanvas.swift` | Whole-pixel outline movement retains off-canvas geometry. Control/Command-drag moves selected pixels; Alt/Option duplicates. Floating transforms support existing handles and numeric fields, Apply/Cancel, source growth, white mask coverage in newly exposed source regions, and one outer history transaction. An unchanged transform restores the exact source, avoiding feather round-trip loss. Shift retains the user's proportional-resize behavior. Polygonal lasso supports hover preview, corner removal, Enter/double-click/near-first-point closure (8 view pixels), and Escape cancellation. |
| `wand.rs`, `editor/operations.rs`, native wand controls | `Document/MagicWand.swift`, `Rendering/WandPixels.c` | Default tolerance 32, point sampling, contiguous enabled, and active-layer sampling. Point/3×3/5×5 averaging, premultiplied RGBA tolerance including alpha, scanline flood fill, noncontiguous matching, exact directed pixel outlines, holes and diagonal contacts are translated. The upstream eight-million-edge cap remains. Sample All Layers reads the visible composite. Apple Vision object selection is not part of this port. |
| `editor/operations.rs::merge_layers` | `Document/LayerMerge.swift` | Single-layer Merge Down uses the lower sibling; multiple selected layers merge their subtrees; Merge Group includes descendants and removes the folder. Appearance, masks and internal clipping bake into a trimmed raster. The highest selected stack position supplies name/parent/anchor; Merge Down retains the lower name. External clipping dependents redirect to the result. Hierarchy validation precedes the single undo transaction. |
| `image_size.rs`, desktop `ui/dialogs/image.rs`, `render.rs::export` | `IO/ImageResizer.swift`, `UI/ImageSizeSheet.swift` | Image Size independently resamples transformed layers in document axes, retaining identity, hierarchy, visibility, opacity, blend and adjustments. Resolution-only edits retain pixel assets/transforms. Independent masks retain pixels and scale placement; attached masks resample separately, with resolution-independent uniform masks retained. Pixels/percent/inches/centimeters, aspect locking, resample toggle and sampling choices follow the upstream form. Resolution-only mode permits inches/centimeters. Resolution persists in project files and PNG/JPEG density metadata. |

Remaining adaptations in this pass:

- Skia replaces CoreGraphics. Existing `High` sampling uses the engine's smooth
  interpolation; it does not reproduce CoreGraphics high-quality filtering.
  Placed-mask transforms use the existing rotation/scale placement model and its
  shear-dropping rule. Live text, shape and gradient assets rasterize on Image Size.
- Existing limits apply: 8192 pixels per side, 24MP per surface and per aggregate
  resized source/mask asset set, and 10,000 layers. A floating selection uses one
  temporary layer, so beginning one in a 100-layer document fails atomically.
- GPUI exposes image clipboard bytes but no portable pasteboard change counter.
  Internal origin recognition uses equality with the application's last copied
  image, shared across windows. An externally re-copied identical PNG can therefore
  retain the internal origin. Different external images center normally.
- Pixel selections remain session-only. New operations preserve them in history;
  reopening a project starts without a selection. Guides, distortion, object
  selection and the selection antialias toggle remain outside this pass.
- The retained addon exposes `dispatchAsync` for new raster commands, keeping
  image processing off the JavaScript thread. Callers must await Apply/Cancel
  before capturing a save/export; unfinished transform captures are rejected.
  The native desktop automatically applies transforms before save/export.

`crates/picsie-core/tests/workflow_features.rs` translates selected scenarios from
`SelectionClipboardTests.swift`, `SelectionEditTests.swift`, `SelectionTests.swift`,
`MagicWandTests.swift` and `ImageSizeTests.swift`: clipboard placement/clear/undo, lasso-shaped copies and visible-layer merged copies,
Layer via Copy, movement and duplication, off-canvas outlines, transform apply/cancel
and source/mask growth, polygon corners, wand tolerance/alpha/averaging/holes/row
orientation, layer-independent resizing, rotated hidden assets,
resolution-only persistence/export, and invalid allocation rejection. Comments
identify source scenarios; separately labeled local tests cover merging, clipboard
mask/feather behavior, independently placed masks during resizing, actual pointer
modifiers, and stack selection order. These
are selected fixture adaptations, not execution of the Swift/AppKit suite.

The real desktop worker tests cover cut/paste ordering, bad-image recovery, Image
Size and undo. `tests/native.test.ts` exercises generated typed commands through the
actual Node/Bun addon and independently reads exported pixels and saved packages.
`crates/picsie-desktop/verify.py` exercises all four feature groups through real
X11 events and the system image clipboard, alongside the earlier UI workflow suite.

## Native Compositor UI pass (2026-09-30)

The user changed the UI target from QuickGUI parity to Compositor. The native
shell in `crates/picsie-desktop/src/ui/layout.rs`, tool headers in `toolbars.rs`,
and layer rows/mask adapters in `layers.rs` adapt the pinned `ContentView.swift`,
`ToolHeaderStyle.swift`, `TransformInspector.swift`, `BrushControls.swift`,
`LassoControls.swift`, `TypeControls.swift`, `ShapeControls.swift`,
`LayersPanel.swift`, `LayerAppearanceControls.swift` and `NativeLayerList.swift`.
The port retains the 42/56/30 px tool-header/rail/status dimensions, the Layers
panel's 252 px default and 202–352 px resize range, 52 px layer rows, and neutral
chrome. Existing Rust commands remain authoritative. Numeric transform edits and
flips now begin an Apply/Cancel session, matching `TransformInspector.change`.

`crates/picsie-core/src/thumbnail.rs` translates `CanvasThumbnail.layer`'s
canvas-shaped 36-point, 2× checkerboard preview and transformed source placement.
Its cache follows `NativeLayerList.ThumbnailKey`: selection/name/opacity changes
reuse the picture; source/geometry/canvas changes invalidate it. Sources render
without appearance or masks, then small BGRA resources travel in the existing
bounded native frame mailbox. GPUI reuses unchanged uploads and evicts deleted or
replaced pictures. Mask thumbnails are not included in this pass. Two local
regressions check placement/aspect ratio and cache invalidation; these are not
claimed as translated upstream tests.

`render/composite.rs::preview_background` uses `EditorCanvas.draw`'s 0.105 gray
pasteboard and 0.30/0.35 gray, 10-point viewport-clipped transparency grid. The
cached Skia backing and shared engine boundary are unchanged; the source canvas shadow is now adapted to Skia Gaussian blur (see the polish pass below). Rulers are covered in the following feature pass. `ui/dialogs/image.rs` follows `ImageSizeSheet`'s separate
width/height rows, 430 px form, resolution and resampling arrangement.

Kit menus/popovers replace AppKit menus and floating panel mechanisms. Current
project windows, hidden native text input, single foreground picker and
local adjustment palette remain explicit adaptations, not direct upstream ports.
See [the UI record](gpui-ui-parity.md) for supported controls and remaining gaps.

## Native placement and typography (2026-09-30)

Pinned source: Compositor `609dbeae2ef68ef4fc82d67e4981a49852eb6e13`, MIT
© 2026 Wonder Assembly LLC. Source references remain in the translated modules.

| Rust implementation | Pinned upstream source | Behavior / adaptation |
| --- | --- | --- |
| `placement.rs`, `editor/placement.rs`, `ui/placement.rs` | [Guides.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/Guides.swift), [CanvasRulers.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Rendering/CanvasRulers.swift), `TransformSnap`, `CropSnap`, `TransformPress` | 18-point rulers, source nice-number ticks, document guides and drag drafts, clear/lock/undo, 64-pixel grid with eight subdivisions, nearest edge/center snapping within ten screen points. Canvas/crop offsets and Image Size scale guides. Auto Select defaults off; Show Controls defaults on. Control moves freely; local cycling uses Super/Command or middle-click. Ruler BGRA resources use the existing bounded mailbox/cache. |
| `text.rs`, `editor/text.rs`, `ui/text.rs`, `ui/toolbars.rs` | [TypeTool.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Document/TypeTool.swift), [InlineTextEditor.swift](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Rendering/InlineTextEditor.swift), `TypeControls.swift` | Blank point/box drafts, whitespace discard, one commit/undo, cancellation, transformed corner-preserving point growth, box resizing, 12-pixel padding, alignment, tracking, leading and overset marker. SkParagraph replaces AppKit layout; Kit supplies text input and local undo. Glyph geometry paints the caret/selection and maps UTF-8 input offsets to Skia UTF-16 positions. Installed family and PostScript face names resolve through Skia's font manager. Rendered rows and caret affinity adapt wrapped keyboard navigation. |
| `model.rs`, `comp.rs` | `LayerTextStyle`, `CanvasGuide`, `ProjectStore.Manifest` | Backward-compatible optional `textLayout`, document guides, live `.comp` text records and version-8 guide metadata. Point text omits `boxSize`; paragraph text writes `[width,height]`, following [Foundation CGSize Codable](https://github.com/swiftlang/swift-corelibs-foundation/blob/main/Sources/Foundation/NSGeometry.swift). Import also accepts the object representation. Translucent legacy text preserves pixels by rasterizing because upstream text metadata stores RGB only. |

`placement_text.rs` includes eleven selected translated/adapted GuideTests,
TransformPressTests and TypeToolTests scenarios, with eight labeled local
regressions for resized reflow, typography pixels, UTF-8 validation, translucent
text packages, empty-caret placement, wrapped keyboard navigation, the retained reference property contract and
flipped text overlay placement/clipping. These are selected fixtures, not the executed Swift/AppKit suite.

Necessary adaptations and remaining differences:

- Draft layers use Rust history previews and sanitized metadata until commit;
  upstream keeps a separate TextDraft outside its document. Save/export/import
  in the native worker finish the draft first.
- Legacy text without `textLayout` retains box semantics and generic families.
  New text uses a system sans family rather than macOS's exact default face.
  OS fonts, Skia shaping, antialiasing and preserved Canvas2D baseline alignment
  can differ from AppKit raster output.
- Enter commits and Shift+Enter inserts a line, retaining the user's accepted
  shortcut choice instead of upstream Command+Return. View options and panel width now persist through local platform configuration.
  Tool/brush/font defaults remain session-local rather than ToolDefaults-backed preferences.
- Existing limits remain 20,000 text characters, 1…1000 font size, 8192 pixels
  per side, 24MP and 10,000 layers, rather than upstream's larger allocations.
  Guide validation allows at most 10,000 finite UUID guides within ±100,000
  document pixels.
- Skia glyph/row navigation bridges the hidden native input's layout to the
  displayed paragraph. IME preedit decorations and full bidirectional/platform
  keyboard fidelity require additional runtime verification.
- Guides/grid/caret/handles are non-printing. `.picsie` preserves local metadata;
  `.comp` retains source guide/text records within the supported package subset.

## Feature registry and existing-feature polish (2026-09-30)

[The registry](compositor-registry.md) tracks implementation, polish, remaining
gaps and evidence independently. Its JSON inventories every Swift/C/header file
and upstream fixture name at the pinned revision with source hashes; the reviewed
behavior rows are a feature-level checklist, not proof of every branch or full
macOS runtime parity. `check:registry` validates statuses, evidence, named native
checks and generated documentation. Set `COMPOSITOR_REFERENCE_PATH` to the
external pinned checkout to also verify every indexed file's digest.

| Implementation | Pinned Compositor source | Behavior and necessary adaptations |
| --- | --- | --- |
| `feedback.rs`, `geometry.rs`, `editor/placement.rs`, desktop `ui/polish.rs` | `Rendering/TransformOverlay.swift`, `Rendering/EditorCanvas.swift`, `Rendering/InlineTextEditor.swift`, `Rendering/BrushCursorOverlay.swift` | Full-edge transform reach, rotation-aware resize direction, text-box edge reach capped for tiny boxes, handles preceding guides, I-beam/move/copy/hand feedback, and zoomed brush diameter outline. Small engine geometry metadata drives UI hover without engine commands or raster work per mouse move. Toolkit hand/copy/crosshair cursors replace upstream custom assets; inner hardness ring and advanced selection cursors remain absent. |
| `text.rs`, `editor/text.rs`, `render.rs`, desktop `ui/text.rs`, `ui/polish.rs`, `ui/toolbars.rs` | `Rendering/InlineTextEditor.swift`, `UI/TypeControls.swift`, `ContentView.swift::ArrowStepping` | Text-colored 500ms blinking caret resets on input/selection; blink commands cannot finish a later editing gesture. Double-click selects a shaped word; triple-click selects a hard paragraph; dragging expands by complete units. SkParagraph word boundaries and UTF-16/UTF-8 conversion adapt native NSTextView selection. Shared numeric fields step 1 or Shift 10; empty leading starts at Auto=120%. Source compact size/unit, swatch and alignment metrics are retained with vector icons replacing SF Symbols. IME/preedit and full bidi fidelity remain unverified. |
| `thumbnail.rs`, desktop `ui/layers.rs` | `UI/CanvasThumbnail.swift`, `UI/NativeLayerList.swift` | Cached 36-point, 2× canvas-shaped grayscale mask thumbnails; source mean edge tone fills the region outside the placed mask. Target border distinguishes image/mask editing. Existing BGRA resources carry thumbnails without encoded-preview or JavaScript pixel paths. |
| `shape.rs`, `editor.rs` | `Document/Selection.swift::DragBox`, `Document/ShapeTool.swift`, `ShapeToolTests.swift` | Rounded drag coordinates; Shift square/circle; Alt draws from center; reverse drag/off-canvas bounds; zero-size clicks and tool switches discard the draft. Existing fill-only local shape records remain prototype deviations. Rounded corners, line shapes and upstream editable shape/package metadata remain absent. |
| `editor.rs`, desktop `ui/input.rs` | `Document/EditorSession+Brush.swift`, `Document/EditorSession.swift::typeOpacityDigit` | Shift-brackets step to the next/previous hardness quarter; opacity digits use a 600ms two-digit window (Move layer opacity or Brush/Eraser setting), ignored during a stroke. GPUI-normalized braces are recognized on Linux. Plain brackets retain the accepted local 5px diameter step rather than source fifth-size stepping. |
| `render/composite.rs`, desktop `preferences.rs`, `ui/layout.rs`, `ui/toolbars.rs` | `Rendering/EditorCanvas.swift`, `ContentView.swift::AppStorage`, `Document/ToolDefaults.swift`, `UI/CropControls.swift` | Source black35%, down3pt, blur14pt canvas shadow uses Skia sigma7 approximation. Panel width/view options persist through platform config. Dynamic operation/merge menu titles and crop ratio/dimensions/enabled state follow source workflows. TransformOverlay.drawCrop supplies the 60% surround, 40% thirds lines and bordered 8-point handles; default canvas frame stays separate from an active crop edit. Kit menus/selectors replace AppKit/SwiftUI; tool/font/brush default persistence remains incomplete. |

`tests/polish.rs` distinguishes seven selected source-derived geometry, shape,
shortcut and mask edge-tone scenarios from four local regressions for mask cache
lifetime, queued caret presentation, UTF-8 word/paragraph boundaries and nonprinting
crop overlays. The mask scenario adapts CanvasThumbnailTests.masksFillTheCanvasWithTheirEdgeTone.
The actual Node/Bun addon fixture also exercises caret visibility and text-unit
selection. These are translated/adapted scenarios, not execution of the Swift
suite. Source retains MIT © 2026 Wonder Assembly LLC attribution. No GIMP UX,
code or fixtures are used in this pass.

## Layers, masks and color polish (2026-09-30)

This pass reads the same pinned revision, 609dbeae2ef68ef4fc82d67e4981a49852eb6e13.
The following are selected translations/adaptations, with MIT © 2026 Wonder
Assembly LLC attribution retained in the Rust sources. The Swift suite did not
run here.

| Local implementation | Pinned source | Behavior and adaptation |
| --- | --- | --- |
| `editor/layer_polish.rs`, desktop `ui/layers.rs` and `ui/input.rs` | `UI/NativeLayerList.swift`, `Document/LayerGroups.swift` | Inline rename with Return/Escape, visible-row range selection, context actions, eye swipe as one undo, edge autoscroll, Alt duplicate/reorder and clipping boundary without changing selection. Adjacent image/mask slots, chain link button, Shift mask enable/disable and red disabled-mask mark follow source controls. GPUI list/option accessibility and portable copy/link cursors replace AppKit table/bitmap cursors. |
| `model.rs`, `render/blends.rs`, `render.rs`, `comp.rs`, desktop `ui/layers.rs` | `Document/LayerAppearance.swift`, `Rendering/SeparableBlend.swift`, `UI/BlendModePicker.swift`, `Rendering/EditorCanvas.swift` | All 24 source modes/order, grouped menu, transient canvas-only preview, commit/cancel and Shift +/− wrapping. Skia sRGB runtime blenders replace Core Image for eight additional separable modes and Burn/Dodge, preserving premultiplied alpha. Export/save use committed state. All modes round-trip through `.comp` using source names. |
| `editor/layer_polish.rs`, `render.rs`, desktop `ui/layers.rs` | `Document/LayerMask.swift`, `Document/MaskTracing.swift`, `UI/LayerMaskMenu.swift` | One-click Add Mask consumes selection without changing tools. Command/Control thumbnail selection traces unmasked image alpha or mask black pixels; Shift adds and Alt subtracts. Alt mask copy keeps document placement, adapting scale to the destination's legacy local grid. Thumbnail link actions preserve active selection; the footer trash deletes a targeted mask. |
| `distort.rs`, `editor.rs`, `feedback.rs`, desktop `ui/toolbars.rs` | `Document/LayerMask.swift`, `Document/Distort.swift`, `Document/EditorSession.swift::beginTransform/commitTransform/cancelTransform`, `Rendering/TransformOverlay.swift` | Independent mask affine/numeric/gizmo transforms, nudge/flip, Cancel/Apply and one undo. Command/Control corner/edge dragging creates a persistent distortion draft: perspective for convex quads, two diagonal triangles for folded quads, uniform-mask identity and majority-edge background. Skia replaces Core Image/Core Graphics. Preview caps at 2048; Apply rasterizes at full supported size. Folder mask placement participates in child compositing. Image distortion remains tracked separately. |
| `editor/layer_polish.rs`, `editor.rs`, desktop `ui/dialogs.rs`, `ui/toolbars.rs`, `preferences.rs` | `Document/ColorPalette.swift`, `Document/SelectionEdits.swift`, `UI/ColorPaletteControls.swift`, `UI/ColorPickerSheet.swift`, `UI/CanvasSizeSheet.swift` | Independent black/white-default foreground/background and mask palettes, X/D, foreground/background fill and background canvas extension. Whole-text fills keep text editable. Palette edits update only an open Type draft; picker OK retains its draft identity. Working picker colors remain transient; Cancel does not change palette/document. The movable remembered GPUI panel uses the source 256-point SV field, padded hue strip and compact 180-point channel/button column. Same-window panel/window chrome and the new/current split preview are UI adaptations. |
| `render.rs::sample`, desktop `engine.rs`, `ui/polish.rs` | `Document/ColorPalette.swift::sampleCompositeColor`, `Rendering/EditorCanvas.swift`, `Rendering/SampleRingOverlay.swift` | One-pixel sRGB compositing samples displayed blend previews and ignores transparent/outside pixels. Picker samples update working color only. Alt Brush/Eraser sampling changes the image foreground without changing the black/white mask palette. The persisted 116-point comparison ring has the source gray 24-point rim and 16-point new/original halves; the source has no magnifier. GPUI Crosshair replaces the AppKit eyedropper cursor/hotspot. |

`tests/layers_masks_colors.rs` distinguishes translated source scenarios from local
cross-grid, transaction, folder placement, alpha, package and one-pixel rendering
regressions. Corresponding selected upstream fixtures were reviewed in
`ColorPickerTests`, `BlendShortcutTests`, `LayerAppearanceTests`, `SelectionTests`,
`LayerMaskTests`, `MaskTransformTests` and `DistortTests`. Node/Bun tests exercise
the real addon, including palette/preview isolation, background fill, mask black
selection, placed mask export and save/reopen. Raster selection/distortion/placement
commands run through the addon worker, with metadata-only results.

The layer count now matches the source 10,000 limit throughout model, import,
creation/duplication and package validation. The existing 8192-side/24MP surface
and 96MB file limits remain explicit allocation adaptations. The legacy `.picsie`
mask-grid/project compatibility is preserved. Effect and adjustment child rows
require the separately unimplemented effect stack and adjustment-layer model;
they remain recorded under `filters-effects.effect-rows` and
`adjustments.layer-adjustments`. No GIMP UX is used in this pass.

## Selections, crop and transform polish (2026-10-01)

This pass uses the same pinned revision and MIT © 2026 Wonder Assembly LLC
attribution. It changes the Rust engine and default GPUI Kit application;
QuickGUI remains the historical UI reference. Apple Vision is an Apple-platform
framework, so the source Object/Select Subject implementation cannot run on
Linux. A replacement model/runtime remains a separately tracked feature.

| Local implementation | Pinned source | Behavior and adaptation |
| --- | --- | --- |
| `pixel_selection.rs`, `editor.rs`, `feedback.rs`, desktop `ui/toolbars.rs`, `ui/layout.rs`, `ui/polish.rs` | `Document/Selection.swift`, `UI/LassoControls.swift`, `Rendering/EditorCanvas.swift`, `Rendering/TransformOverlay.swift` | Captured Anti-alias setting for ellipse/lasso/wand, smoothing when feather is positive, exact winding-path cursor feedback, stationary Shift/Add and Alt/Subtract without changing the default, and frozen mode during a draft. Off-canvas outline geometry remains intact while coverage is canvas-sized. Native marching ants use source 120 ms phase steps and four-point black/white dashes. Rust caches flattened vector contours; GPUI animates them without edit commands or raster previews. Curve flattening at 0.75 screen-point intervals, bounded toolkit paths and platform cursors with +/− overlays are presentation adaptations. QuickGUI keeps a static outline. |
| `editor/layer_polish.rs`, desktop `ui/dialogs.rs`, `ui/toolbars.rs`, `assets.rs` | `Document/MaskTracing.swift`, `Document/Selection.swift`, `UI/LassoControls.swift` | Select Layer Pixels uses the existing Rust alpha-silhouette trace. Source ellipse/polygon icons, independent Expand/Contract defaults of 1, Feather default 2, and integer amount panels with slider/value, Cancel and OK. Expand/Contract accept 1…500; Feather accepts 1…250. Return applies once; invalid values and Cancel do not edit the document. Kit replaces source panels. |
| `editor/interaction_polish.rs`, `editor/operations.rs`, desktop `ui/controls.rs`, `ui/toolbars.rs` | `UI/TransformInspector.swift`, `Document/LayerTransform.swift`, `Document/EditorSession.swift` | Typed live X/Y/W/H/rotation, ratio lock, percentage scale about center, sampling, persistent Apply/Cancel and Escape restoration. Asset pixels or pre-edit blank/mask dimensions form a frozen percentage baseline. Auto Select and Show Controls stay outside the horizontally scrolling numeric controls. Explicit user adaptation: free resize is the default, and lock or Shift preserves proportions; source defaults locked and Shift reverses the lock. Enter releases field focus, retaining the existing native Apply workflow. |
| `distort.rs`, `editor/interaction_polish.rs`, `editor.rs`, `editor/operations.rs` | `Document/Distort.swift`, `Document/LayerTransform.swift`, `Rendering/TransformOverlay.swift` | Image/blank-shape and floating-pixel corner/edge distortion: eight handles without a rotation stalk; convex perspective or two triangles for folded shapes; Ctrl/Command begins a persistent draft; Shift confines free motion to one axis; Cancel restores original pixels/placement; Apply rerasterizes at full supported resolution, trims visible alpha bounds and commits one undo. Linked masks follow and unlinked masks preserve world placement. Skia replaces Core Image/Core Graphics; previews cap at 2048. Text/group distortion and the absent source effect stack remain gaps. |
| `model.rs`, `render.rs`, `comp.rs`, desktop `state.rs`, `ui/controls.rs` | `Document/LayerTransform.swift::LayerSampling`, `UI/TransformInspector.swift`, `Document/ProjectWorkspace.swift` | Nearest, Smooth and High metadata/controls survive transform cancellation and both project formats, including mask placement. Image sampling maps to nearest, linear and Skia Catmull-Rom cubic rather than platform Core Graphics quality hints. Independent-mask nearest placement is distinct; Smooth/High still share the existing bilinear placement adapter. |
| `crop.rs`, `editor.rs`, `render.rs`, `render/composite.rs`, desktop `ui/input.rs` | `Document/Crop.swift`, `Document/LayerTransform.swift::CropSnap`, `Rendering/EditorCanvas.swift` | A full-canvas crop frame starts a fresh body drag; symmetric snapping preserves the center; Control bypass reaches the crop tool rather than layer picking. Original ∪ crop bounds drive checkerboard/shadow and retained off-canvas artwork. Expanded previews draw into the viewport-sized surface clipped to that union, avoiding a huge temporary raster. Apply translates retained assets/guides without resampling; selection reset on canvas replacement remains an adaptation. |

`tests/interaction_polish.rs` adapts selected `SelectionTests` scenarios
(`antialiasingControlsEdgeCoverage`,
`cursorBadgeFollowsModifiersButKeepsAnOutlinesStartingMode`,
`movingOffCanvasAndBackKeepsTheWholeShape`),
`TransformTests.scalePercentSetsBothSidesAboutTheCenter` and
`moveRotateAndShiftConstraints`, and the two raster/trim scenarios in
`DistortTests`. Additional source-implementation checks cover exact path hit
feedback, positive-feather smoothing and `TransformDrag.corners`' Shift rule.
Local regressions are separately labeled for frozen blank baselines, symmetric
crop snapping, expanded preview pixels, linked/unlinked cross-grid masks,
floating-pixel merging and cached outline animation. Existing translated crop
fixtures continue in `behavior.rs` and `placement_text.rs`; the Swift suite did
not run here. The real Node/Bun addon verifies typed metadata, worker-only raster
commands, transaction cancellation/undo and exported distorted alpha pixels.

The registry now explicitly tracks the missing `TransformGroup` box model for
folder/multi-layer numeric, resize and rotation edits. Multi-layer translation
already works; the source box behavior must be ported rather than treating a
folder as one raster layer. Windows/macOS GUI, Wayland and physical HiDPI remain
unverified. There is no claim of pixel-identical Core Graphics/Core Image output.


## Immutable raster and bounded pixel-work performance pass

Pinned sources remain `609dbeae2ef68ef4fc82d67e4981a49852eb6e13`, MIT
© 2026 Wonder Assembly LLC:

| Local implementation | Compositor source | Adaptation |
| --- | --- | --- |
| `raster_snapshot.rs`, `asset.rs`, `history.rs` | `Rendering/RasterSnapshot.swift`, `Rendering/LayerRenderer.swift` | Flat base plus disjoint replacement patches, spatially indexed replacement/splitting, crop offsets, immutable sharing and lazy complete-image materialization. Skia images replace CGImages; owned decoded PNG pixels are memoized. Existing project formats are unchanged. |
| `brush.rs`, `geometry.rs` | `Document/BrushStroke.swift`, `Document/EditorSession+Brush.swift` | Source reads and publication are bounded to touched 256px tiles. Published images are cropped to coverage bounds and unchanged patches retain identity. Original per-stroke pixels and provisional tail backups preserve opacity/cancellation. Cached tips reuse exact quarter-pixel phases only; arbitrary phases retain procedural rasterization. A conservative hard-tip interior/edge test and saturated-coverage skip reduce work without phase rounding. Transform terms are cached without changing arithmetic order. |
| `render.rs`, `render/composite.rs`, `thumbnail.rs` | `Rendering/TiledLayerRenderer.swift`, `UI/CanvasThumbnail.swift` | Padded pieces retain unchanged document/source pixels. The conservative Skia preview path accepts simple integer-position stacks with raster patches; ordinary source images keep the existing compositor and seed unchanged pieces from its retained image on the first patch edit; eligible pieces survive undo to an ordinary source and are validated against their recorded document on reuse; changed source patches invalidate affected pieces; ordinary moves reuse the existing bounds-based damage calculation. Source thumbnail pieces use layer-local coordinates and apply canvas placement afterward; moves retain their source pixels. Source-edge antialiasing is applied to the full textured layer rectangle, with only internal cache boundaries clipped. Two-pixel gutters support the existing linear/Catmull-Rom sampling. The upstream halving pyramid, transformed tiled rendering and GPU coverage are not ported. Complex scenes keep the existing compositor. |
| `render.rs`, `editor.rs`, `editor/operations.rs` | `Document/BrushStroke.swift::paintCanvas`, `Document/SelectionEdits.swift` | Fill/clear touch only tiles intersecting selection coverage while retaining the same source-over/destination-out formulas, source growth, mask placement and document-space clipping. Mask fills retain their contiguous path. |

`tests/raster_snapshot.rs` adapts upstream
`RasterSnapshotTests.mouseUpAndNextStrokeNeverFlattenTheDocument` and the
immutable snapshot/persistence portion of
`snapshotsStayImmutableAndDisplayMatchesExportAcrossSuccessiveStrokes` to a
768×512 CPU fixture and the `.picsie`/`.comp` adapters. It separately labels
local transparent-patch/crop, arbitrary-phase hard-tip, transformed translucent
fill/clear, and retained decoder identity regressions. Existing upstream brush
coverage/tail/opacity fixtures remain unchanged. The Swift/AppKit test suite was
not run and tiled mask parity is not claimed.

The local preview comparison covers replacement/undo at five zoom factors and
fractional pan. At unit zoom it is exact. At fractional zoom Skia's local tile
origin can change interpolation rounding by one 8-bit channel step in a small
number of pixels; this is recorded explicitly rather than called byte-identical.
A further local regression verifies moved/restored translucent raster pixels and
sharing of remote unchanged cache pieces. Thumbnail regressions additionally
compare moved/off-canvas translucent source placement with the original contiguous
renderer, preserving source-edge antialiasing and at most one channel-step sampling
rounding. Final document output is compared separately. The desktop queue test
verifies an intermediate frame during sustained painting, all final pointer pixels and
one undo transaction. An 8 ms scheduling boundary is a native worker adaptation
between requests, not a copied GIMP implementation or a guaranteed frame time.

See [the sixth experiment pass](desktop-gimp-experiments.md#sixth-pass-immutable-raster-and-bounded-pixel-work) for the actual
before/after evidence and remaining costs. GIMP contributes no code or UX.

The same pass integrates the earlier bounded-feather experiment: following
`DocumentSelection.coverageBounds/clip`, Skia blur runs inside four-sigma padded
coverage bounds. Existing immutable coverage is cropped rather than rerasterizing
the outline, preserving combined/raster selections. Alpha8 readback replaces the
full RGBA temporary, then the result is restored to the existing complete mask
contract. The local full-canvas reference compares every byte for rectangle,
edge ellipse, full, inverted, off-canvas, combined-hole and empty selections at
feather 1/2/20/80/250 on two canvas sizes. Existing brush, fill/clear and selection
history consumers retain their fixtures. Source Gaussian/antialias behavior is
unchanged; Skia remains the documented adaptation of Core Image.

### 2026-10-01 layer graph and clipping projection rewrite

`crates/picsie-core/src/layer_index.rs` adapts `LayerHierarchy.entries`,
`LiveLayerMask.swift::LiveMaskGraph.validate/canLinkMask`, and their 256-node graph
semantics to a borrowed Rust index. It avoids cloning raster-bearing documents
and repeated validation for each menu option. The invalid-graph fallback preserves
edge replacement/repair semantics. A local deterministic graph oracle and depth
boundary tests supplement the translated live-mask fixtures; they are not new
upstream fixtures. Duplicate live-mask IDs are rejected as in the pinned source.

`render/projection.rs` adapts `Rendering/LiveMaskRenderer.swift::drawComposite`
and `coverage`, called with `context.boundingBoxOfClipPath` by `drawLiveComposite`.
The shared-alpha color math is retained. Rust caches clipping nodes across frames
and sizes scratch images to the base support intersected with the document.
The document-edge intersection preserves the pre-rewrite Skia blur boundary;
this is a backend adaptation, not a literal Swift translation. Frozen-renderer
comparisons permit at most one premultiplied channel step from bounded origins;
retained redraws match fresh new rendering byte-for-byte. High interpolation,
opacity/blend defaults, masks, history and source pixels are preserved.

`render/composite.rs` now tracks damage by layer identity, changed relative order,
and old/new hierarchy and live-mask dependencies rather than treating shifted
vector positions as full damage. Source caches use indexed LRU records. GIMP's
pinned `gimpprojection.c` and tile-validation architecture informed retention and
damage concepts only; no GPL implementation was copied or translated. Visible-priority tiled
projection scheduling and a downsample pyramid remain unimplemented adaptations.

`picsie-desktop/src/ui/layers.rs` follows `UI/NativeLayerList.swift`'s visible-cell
reuse with GPUI `uniform_list`, preserving 52-point rows and the command handlers.
The toolkit list uses the same base scroll handle for visibility swipes and edge
autoscroll. Platform accessibility/HiDPI and broader workflow stress verification
remain registry gaps. MIT attribution: Compositor 609dbeae, © 2026 Wonder Assembly LLC.

Projection chunks retain sparse premultiplied backdrop checkpoints every 64
atomic compositing nodes plus a checkpoint before their final eight nodes. Old/new dependency closure and ordered
node comparison establish the reusable prefix; only the changed suffix is replayed.
Clipping stacks remain atomic, and hidden/live sources above a prefix invalidate
lower consumers. Empty spans share immutable checkpoint images; unique checkpoint storage is
bounded by 24 million pixels, with older images evicted before a chunk's newest. Checkpoint publication/copying
stays in Rust; no new transport format, blend math or source sampling is introduced.
Local cross-chunk/prefix tests compare every byte with fresh rendering and check
cache reuse and external-source invalidation. Earlier-prefix edits still require
replaying the lower stack; visible-priority scheduling and a pyramid remain gaps.

### Native viewport publication adaptation (2026-10-01)

`editor/publication.rs` owns typed, immutable metadata for the GPUI application.
The policy follows pinned `Rendering/CanvasViewport.swift` (viewport is separate
from document pixels), `Rendering/EditorCanvas.swift::synchronizeDisplay/draw`
(redraw on relevant state changes), and `UI/NativeLayerList.swift::Coordinator.update`
(reuse unchanged thumbnails/cells and update affected rows). MIT © 2026 Wonder
Assembly LLC; Compositor revision remains `609dbeae2ef68ef4fc82d67e4981a49852eb6e13`.
The Rust publisher and sharing/invalidation protocol are transport adaptations,
not translated Swift algorithms or a new editor-state authority. No viewport,
interpolation or raster algorithm changes accompany this publication pass.

The worker shares document metadata, row/candidate collections and thumbnails
across viewport-only updates; inspector synchronization follows changed inputs.
Cache checks include every authoritative scalar and immutable resource identity,
including live edits before history commits. Six local publication regressions
compare consumed metadata with the existing serialized contract and check sharing,
live edits, Undo, collapsed-target capability queries and pixel-resource changes.
These are supplemental transport tests, not newly translated upstream fixtures.
Existing translated viewport/transform/guide and rendering tests remain applicable.

Pinned GIMP `app/display/gimpdisplayshell-scroll.c` and `gimpdisplayshell-scale.c`
were inspected as secondary performance references: retained viewport pixels and
cached scale values explain why navigation need not revisit all layer data.
No GIMP implementation was copied or translated. GIMP's UX is not adopted.

## Levels and Curves adjustment layers (2026-10-02)

Pinned sources remain `609dbeae2ef68ef4fc82d67e4981a49852eb6e13`, MIT
© 2026 Wonder Assembly LLC:

| Local implementation | Compositor source | Adaptation |
| --- | --- | --- |
| `adjustment.rs` | `Document/LayerAdjustment.swift`, `Document/Levels.swift`, `Document/LevelsAutomatic.swift`, `Document/Curves.swift`, `Rendering/LevelsPixels.c`, `Rendering/BrushPixels.c` | Levels channels/ranges/normalization, RGB-mean alpha-weighted histogram, display-scale capping, auto contrast/color/neutral, eyedropper calibration and Hermite curves are translated. Only Levels/Curves kinds exist; the C kernels are exact integer ports over premultiplied RGBA, including soft-alpha rounding. Legacy `hue/saturation/lightness/colorize` scalars serialize so Compositor still decodes our packages. |
| `editor/adjustments.rs`, `model.rs`, `layer_index.rs` | `Document/AdjustmentEditing.swift`, `Document/LayerAdjustment.swift::addAdjustment`, `Document/LiveLayerMask.swift::LiveMaskGraph` | Adjustment layers are document-sized Paint layers with validated settings; live-mask sources can never be adjustments while clipped adjustments stay valid targets. Add/reopen share one undo transaction per edit with live preview, preview toggle, cancel and commit; sampling re-renders the live composite below instead of a frozen source image. |
| `render/projection.rs`, `render/composite.rs` | `Rendering/LiveMaskRenderer.swift::drawComposite/adjust`, `Rendering/AdjustmentSurface.swift`, `Rendering/EditorCanvas.swift::drawLayers` | Sourceless adjustments remap the accumulated composite in stack order; clipped adjustments compose inside their base's shared-alpha stack (unpremultiply, recolor, restore alpha once) before the base blend. Non-normal blends run on opaque colors with alpha restored. Identity/normal adjustments are exact no-ops. Retained chunk checkpoints are bypassed once per frame while adjustments are present; other documents keep the optimized path. |
| `comp.rs`, `files.rs` | `IO/ProjectStore.swift::validate/save` | Version-7+ Levels/Curves records round-trip with masks, clipping links, opacity and blend modes; every other live adjustment/shape/effect record is still rejected explicitly. `.picsie` JSON carries the same settings with old projects decoding to no adjustment. |
| `picsie-desktop/src/ui/adjustments.rs` | `UI/LevelsSheet.swift`, `UI/CurvesControls.swift` | Modeless 440pt floating panel (draggable title, Escape cancels, Enter applies): 150pt histogram with draggable input black/gamma/white handles using the source log formula, output ramp with output handles, numeric fields, channel picker, Black/Gray/White canvas eyedroppers, Contrast/Color/Neutral auto, Reset, Preview toggle and Cancel/Apply. Curves keeps the 260pt graph with click-add, neighbor-clamped drag, interior-only removal and channel-switch selection reset. Upstream slider-track dragging is not ported. |

`crates/picsie-core/tests/adjustments.rs` translates the input-clipping, gamma,
output-inversion, alpha, channel-order, histogram, display-scale, auto and
sampling scenarios from `LevelsTests.swift`; the global/clipped/opacity/mask,
blend-with-alpha-restoration, persistence, duplication and undo scenarios from
`AdjustmentLayerTests.swift`; and the settings-shape scenario from
`ImageAdjustmentTests.swift`. Separately labeled local fixtures cover soft-alpha
preservation on transparency, stacked/double adjustments, non-normal base blends,
folder stack-position boundaries, merge baking, export equality and reopened
editing. Destructive image-menu Levels/Curves editing, HSV and all other
adjustment kinds remain unported gaps. The Swift/AppKit suite was not executed.
