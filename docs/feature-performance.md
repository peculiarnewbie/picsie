# Feature performance workloads

Expanded on 2026-10-03 by three sequential OpenCode agents using Muse Spark
1.3 Contributor on high, followed by parent review. Added 19 workloads:
7 layer/mask, 6 selection, and 6 transform/gradient/adjustment controls.
18 launch both applications; opacity preview is Picsie-only. Accepted cohorts
contain 12 measured samples per app/case/size (3 trials × 4 samples), with
2 warmups, at 1200×800 and 3600×2400: 888 accepted timing samples.
Superseded cohorts and failed pilots remain available separately.

These measure CPU commands and complete image/channel availability, not
visible input latency or warm GPUI frame rates. Each scenario's limits and
output comparisons below determine which speed comparisons are meaningful.
The mask-gradient control remains unmatched in this headless GIMP harness.
Use `performance.features`, `performance.selections`, or
`performance.recent-features` to rerun one batch without the others.

Corrected layer/mask cohort: 2026-10-03, `performance.features` scenario (reviewed
harness). GIMP is a secondary functionality/performance reference, never a
UX reference. This replaces the batch-1 draft; its numbers are preserved as
superseded history at the bottom, not silently dropped.

## Reproducing

For each paired workload, the accepted protocol measures 3 trials × 4 samples
per app per size: 48 timed executions across the two apps and two sizes.
There are also 24 warmup executions (2 per trial/app/size), plus untimed fixture,
validation and restoration work. The Picsie-only opacity preview has 24 timed
and 12 warmup executions. These short cohorts support an initial median
comparison; 12 samples per app/size give weak p95 evidence, not a statistically
established tail-latency target.

```sh
# Full paired cohort: 3 trials, 4 samples per size, 2 warmups, alternating launch order.
npm run reproduce -- run --profile performance --scenario performance.features \
  --build --trials 3 --samples 4 --warmups 2

# One-case pilot (methodology check only, not a performance conclusion).
npm run reproduce -- run --profile performance --scenario performance.features \
  --build --case mask-paint --trials 1 --samples 1 --warmups 0

# Picsie-only case: the GIMP launch is skipped cleanly by the orchestrator.
npm run reproduce -- run --profile performance --scenario performance.features \
  --build --case opacity-preview --trials 1 --samples 1 --warmups 0
```

The underlying driver also runs standalone (see its `--help`); the catalog
runner adds provenance, locking and evidence hashing. Unknown `--case`
values are rejected before fixture preparation. Outputs belong under
ignored `artifacts/feature-performance/`; the corrected cohort is
`artifacts/feature-performance/layers-masks-reviewed` (the old
`layers-masks` directory is untouched). The driver validates the pinned
`GIMP 3.2.6` version at startup and records it in the manifest and report.

## Source references

- Compositor pinned `609dbeae` (functionality only, adapted to Rust commands):
  `Compositor/Document/LayerAppearance.swift` (`beginOpacityEdit`,
  `finishOpacityEdit`, `setLayerOpacity`, `previewBlendMode`,
  `setLayerBlendMode`, `cycleBlendMode`); `Compositor/Document/EditorSession.swift`
  (`toggleLayerVisibility`, `beginVisibilitySwipe`, `setVisibilityInSwipe`,
  `endVisibilitySwipe`); `Compositor/Document/LayerMask.swift` (`addMask`,
  `toggleLayerMask`, `deleteLayerMask`, mask brush painting).
- Rust implementation: `crates/picsie-core/src/editor.rs` (`UpdateLayer`,
  `BeginPropertyEdit`/`FinishGesture`/`CancelGesture`, `PreviewBlendMode`,
  `BeginVisibilitySwipe`/`SwipeVisibility`/`EndVisibilitySwipe`, `AddMask`,
  `SetPaintTarget`), `crates/picsie-core/src/editor/layer_polish.rs`
  (property-transaction preview, blend preview, visibility swipe),
  `crates/picsie-core/src/brush.rs` (mask-target painting).
- GIMP pinned `e101dd19`: `app/core/gimplayer.c` (`gimp_layer_set_opacity`,
  `gimp_layer_set_mode`), `app/core/gimplayermask.c`, `app/pdb/layer-cmds.c`
  (`gimp-layer-add-mask`, `gimp-layer-remove-mask`,
  `gimp-layer-get/set-apply-mask` — the apply flag lives on the layer, not the
  mask object — and `gimp-layer-set-composite-space`). GIMP runtime 3.2.6,
  OpenCL off, isolated config/cache.
- Native tests: `crates/picsie-core/tests/layers_masks_colors.rs`
  (opacity drag as one undo, visibility swipe, blend fixtures, mask painting).

## Workloads and measurement boundaries

Matched 3-layer fixtures at 1200x800 and 3600x2400, built procedurally on both
sides from the same constants (bottom `#e0d040` full-canvas, middle `#3f7fbf`
3/4 rect at 1/8 offset, top `#c85050` full-canvas selected layer; mask cases
reveal the left half). Fresh document/image per sample; fixture preparation,
mask creation, composite-space selection and brush context are untimed.

Per-sample stages: `command_ms` is the feature call only (no getters —
validation getters run after `total_ms`); `render_ms` is the cold fresh-Renderer
composite on the Picsie side and the destructive visible-layer merge
(projection/flatten) on the GIMP side; `read_ms` is the RGBA buffer
extraction; `total_ms` is the end-to-end CPU availability boundary. Picsie
creates a fresh cold `Renderer` per sample, so totals exclude any warm
retained preview; GIMP totals include a destructive flatten Picsie does not
need plus warm tile-cache effects. These are labeled, genuinely different
rendering boundaries — not UI latency, not GPUI frames, and not a claim that
either side's total equals interactive cost.

GIMP commands are nondestructive property sets or undoable paint on a
per-sample duplicate, which is discarded afterward. Picsie commits one history
undo per timed workload except the preview case, which holds an open property
gesture. Untimed restoration controls run per case and size: true history
Undo plus pixel re-render on the Picsie side (including a
preview-then-cancel control that must leave zero undos); inverse round-trips
back to a pristine merge on the GIMP side, because batch GI exposes no
single-step undo (verified: no `Image.undo` attribute and no
`gimp-image-undo` PDB procedure at e101dd19 — this corrects the earlier
session note claiming otherwise). Mask repaint cannot restore antialiased dab
edges byte-for-byte, so its restoration is checked at fully covered probes.

| Case | Picsie command | GIMP command | GIMP pairing |
| --- | --- | --- | --- |
| opacity-preview | `BeginPropertyEdit` + `UpdateLayer{opacity:0.35}` (no commit) | none | Picsie-only |
| opacity-commit | above + `FinishGesture` | `set_opacity(35)` over RGB_NONLINEAR | paired, nondestructive |
| visibility-toggle | visibility-swipe triple hiding top | `set_visible(False)` | paired, nondestructive |
| blend-multiply | `UpdateLayer{blend:multiply}` | `set_mode(MULTIPLY_LEGACY)` | paired, nondestructive |
| blend-screen | `UpdateLayer{blend:screen}` | `set_mode(SCREEN_LEGACY)` | paired, nondestructive |
| mask-disabled | `UpdateLayer{mask:{enabled:false}}` | layer `set_apply_mask(False)` | paired, nondestructive |
| mask-paint | 61-sample brush `Pointer` gesture on mask | `paintbrush` on mask drawable | paired; tips differ |

Each sample asserts geometry (canvas size, 3 layers, middle offsets), history
count, a primary probe pixel, and a second consistency probe (bottom-only
corner, or the unaffected mask side / unaffected stroke region). Paired probes
at BOTH sizes are quantified with straight and premultiplied metrics by the
test-only `compare` subcommand of the Rust example (256-row streaming;
bounded memory, not a production renderer).

## Measured results (medians, 12 samples per cell)

Picsie `total` = command + cold fresh-Renderer composite + RGBA read.
GIMP `total` = call + visible-layer merge + RGBA read. Full p95s in
`artifacts/feature-performance/layers-masks-reviewed/results.json`; GIMP
version `GNU Image Manipulation Program version 3.2.6` is recorded there.
No samples were discarded, including one slow 55 ms GIMP mask-disabled total
at 1200. Short repeats are not confidence intervals.

| Case @1200x800 | Picsie cmd / render / read / total | GIMP cmd / render / read / total |
| --- | ---: | ---: |
| opacity-preview | 0.03 / 36.8 / 4.4 / 41.2 | — (Picsie-only) |
| opacity-commit | 0.04 / 36.8 / 4.5 / 41.2 | 0.29 / 24.1 / 4.4 / 29.3 |
| visibility-toggle | 0.01 / 23.3 / 4.3 / 27.7 | 0.15 / 14.0 / 4.5 / 18.8 |
| blend-multiply | 0.04 / 35.9 / 4.4 / 40.9 | 0.31 / 24.0 / 4.4 / 28.7 |
| blend-screen | 0.03 / 36.4 / 4.4 / 41.4 | 0.32 / 25.1 / 4.5 / 30.0 |
| mask-disabled | 0.04 / 37.4 / 4.5 / 41.9 | 0.09 / 21.2 / 4.7 / 26.3 |
| mask-paint | 158.4 / 47.3 / 3.3 / 209.3 | 37.4 / 22.7 / 4.8 / 64.8 |

| Case @3600x2400 | Picsie cmd / render / read / total | GIMP cmd / render / read / total |
| --- | ---: | ---: |
| opacity-preview | 0.03 / 351.9 / 49.9 / 400.8 | — (Picsie-only) |
| opacity-commit | 0.04 / 352.5 / 50.3 / 403.7 | 0.28 / 155.5 / 77.8 / 235.8 |
| visibility-toggle | 0.01 / 208.2 / 50.0 / 259.4 | 0.16 / 90.0 / 73.2 / 165.3 |
| blend-multiply | 0.04 / 351.6 / 49.9 / 401.7 | 0.33 / 152.0 / 78.5 / 233.1 |
| blend-screen | 0.04 / 350.2 / 50.2 / 400.1 | 0.34 / 155.0 / 75.2 / 230.2 |
| mask-disabled | 0.05 / 349.2 / 50.2 / 398.7 | 0.09 / 130.6 / 79.2 / 209.0 |
| mask-paint | 561.8 / 438.0 / 49.5 / 1047.9 | 107.7 / 132.8 / 77.9 / 316.0 |

## Quality alignment (not parity claims)

- `visibility-toggle`, `blend-multiply`, `blend-screen`, `mask-disabled`,
  `opacity-commit`: probes **byte-identical** at both sizes (0 differing
  pixels, straight and premultiplied).
- Opacity alignment control: the GIMP top layer composites in
  `RGB_NONLINEAR` (untimed setup), producing `[111, 111, 152]` exactly like
  Picsie's sRGB composite. Verified by retained probe
  (`probes/composite-space-1`): default AUTO compositing yields
  `[133, 113, 163]` instead.
- `mask-paint`: 1,356 of 960,000 pixels differ at 1200 (3,356 of 8,640,000
  at 3600), max step 100 at stroke edges. Both strokes hide the same region;
  brush dab rendering differs.
- `opacity-preview` has no GIMP counterpart and reports Picsie only.
- The observed mask-paint command cost (Picsie ~158/562 ms vs GIMP ~37/108 ms)
  is flagged for future profiling; no causal component claim is made here.

## Superseded batch-1 results (quality-limited history)

The initial `artifacts/feature-performance/layers-masks` cohort used default
(AUTO/linear) GIMP compositing for opacity, looser GIMP assertions, getters
inside the command timer, 1200px-only probe comparison, and always launched
GIMP even for `opacity-preview`. Its opacity numbers (e.g. 1200 totals
Picsie 42.1 ms vs GIMP 26.7 ms with max channel step 22 across the full
canvas) remain on disk untouched but are SUPERSEDED by the corrected cohort
above; do not quote them as current. Its pilot directories were deleted in
batch 1; recoverable failure excerpts live in
`artifacts/feature-performance/recovery/RECOVERY.md` with explicit
deleted/unavailable labeling.

## Restoration metadata clarification

GIMP's control is a manual inverse/reset, not an executed Undo. New reports use
`restoration.actual_undo_verified: false`, an explicit method and verification
scope. Mask painting only checks fully covered interior/exterior probes after
white repaint; it does not restore antialiased edges byte-for-byte. Picsie uses
actual history Undo and preview Cancel.

The reviewed cohort's original GIMP rows named this manual control
`undo_restored`; that name was misleading. Original measurements are retained
unchanged. `layers-masks-reviewed/RESTORATION-METADATA.json` records the correction
and source hash. The label correction changes no timed operation, sample or
comparison; it does not require another measured cohort.

## Selection evolution cohort (batch 2)

Measured 2026-10-03, `performance.selections` scenario (same reviewed
harness, suite-locked so layer/mask workloads never rerun). GIMP is a
secondary functionality/performance reference, never a UX reference. Prior
layer/mask cases, semantics, results and review fixes above are unchanged;
the orchestrator additionally supports `--suite` selection and a
`coverage_ms` stage, and its GIMP restoration assertion now checks the
corrected `restoration.verified` / `actual_undo_verified: false` metadata
(the stale `undo_restored` key never existed in reviewed reports).

### Reproducing

```sh
# Full paired selection cohort: 3 trials, 4 samples per size, 2 warmups.
npm run reproduce -- run --profile performance --scenario performance.selections \
  --build --trials 3 --samples 4 --warmups 2

# One-case pilot (methodology check only, not a performance conclusion).
npm run reproduce -- run --profile performance --scenario performance.selections \
  --build --case wand-contiguous --trials 1 --samples 1 --warmups 0
```

Unknown `--case` values are rejected before fixture preparation, including
cross-suite names (`mask-paint` under `performance.selections` and vice
versa). Outputs belong under ignored `artifacts/feature-performance/`; the
batch-2 cohort is `artifacts/feature-performance/selections` (runner layout:
`selections/performance.selections/evidence`, fixtures alongside). The
driver validates the pinned `GIMP 3.2.6` version at startup and records it
in the manifest and report.

### Source references

- Compositor pinned `609dbeae` (functionality only, adapted to Rust commands):
  `Compositor/Document/Selection.swift` (`DocumentSelection.coverage()`,
  `coverageBounds`, `clip`, `resizeSelection` band union/subtract, invert as
  canvas-minus-outline, add/subtract `applySelection`);
  `Compositor/Document/MagicWand.swift` + `WandPixels.c` (point sample,
  per-channel tolerance on premultiplied pixels, contiguous flood vs
  full-canvas match, exact pixel-edge outlining);
  `Compositor/Document/EditorSession.swift` (thumbnail/layer-alpha selection).
  Corresponding tests: `SelectionTests.swift` (expand/contract/invert
  fixtures), `MagicWandTests.swift` (active-layer vs merged sampling, modes).
- Rust implementation: `crates/picsie-core/src/pixel_selection.rs`
  (`inverted`, `resized`, `from_path_with_smoothing`),
  `crates/picsie-core/src/wand.rs` (`select`, `outline`),
  `crates/picsie-core/src/editor.rs` (`InvertSelection`,
  `ExpandSelection`/`ContractSelection`, `SelectLayerPixels`,
  `SetWand`/marquee `Pointer` flow), `render.rs::selection_coverage`.
- GIMP pinned `e101dd19`: `app/core/gimpchannel-select.c`
  (`gimp_channel_select_fuzzy`, `gimp_channel_select_by_color`,
  `gimp_channel_select_alpha`), `app/pdb/selection-cmds.c`
  (`gimp-selection-invert/grow/shrink`, `gimp-selection-bounds/value/is-empty`),
  `app/pdb/image-select-cmds.c`
  (`gimp-image-select-color/contiguous-color/item`), `app/core/gimpchannel.c`
  (`gimp_channel_real_grow/shrink` morphology). GIMP runtime 3.2.6, OpenCL
  off, isolated config/cache.
- Native tests: `crates/picsie-core/tests/reference_features.rs`
  (expand/contract edges, invert), `workflow_features.rs` (wand sampling and
  history), `interaction_polish.rs` (wand cursor/mode behavior).

### Workloads and measurement boundaries

Segmented fixtures at 1200x800 and 3600x2400, built procedurally on both
sides from the same constants (opaque `#e0d040` base; transparent selected
top layer with two separated opaque `#c85050` squares, side = width/8 at
x = width/8 and x = width − width/8 − side, vertically centered). A solid
canvas would make the wand cases vacuous; the disconnected squares
separate contiguous from noncontiguous matching and the transparency
exercises alpha handling. Fresh document/image per sample; fixture
preparation, input-shape setup, wand context and composite-space selection
are untimed.

Per-sample stages: `command_ms` is the selection call only (no getters —
validation getters run after `total_ms`); `coverage_ms` materializes the
selected-coverage availability every sample (`render::selection_coverage`
on the Picsie side, selection-mask buffer extraction on the GIMP side).
Only the first measured sample's `.selcov` channel dump per case/size/trial
is written; the paired comparison uses the trial-1 dumps, so later samples
contribute timing plus in-driver assertions, not full paired comparisons.
`render_ms` is the cold fresh-Renderer composite
(Picsie) or the destructive visible-layer merge (GIMP); `read_ms` is the
RGBA buffer extraction; `total_ms` is the end-to-end CPU availability
boundary. The unchanged document RGBA is never treated as the selection
output check. GIMP history stays enabled; every timed command runs on a
per-sample duplicate, discarded afterward. Picsie commits one history undo
per timed workload on top of its untimed input setup. Untimed restoration
controls run per case and size: true history Undo plus coverage/pixel
re-render on the Picsie side; manual inverse/reset on the GIMP side
(batch GI exposes no single-step undo), explicitly labeled with
`actual_undo_verified: false`.

| Case | Input (untimed setup) | Picsie command | GIMP command |
| --- | --- | --- | --- |
| select-inverse | center-half rectangle, AA on, feather off | `InvertSelection` | `Selection.invert` |
| select-expand5 | same rectangle | `ExpandSelection{5}` | `Selection.grow(5)` |
| select-contract5 | same rectangle | `ContractSelection{5}` | `Selection.shrink(5)` |
| wand-contiguous | none; tolerance 0, point sample, selected layer only, AA on, feather off, 4-connected | wand click at left-square center | `select_contiguous_color` (threshold 0, sample-merged off, COMPOSITE, transparent off) |
| wand-noncontiguous | same wand context | same click | `select_color` with the opaque red |
| select-layer-alpha | none; AA on, feather off | `SelectLayerPixels` | `select_item` on the top layer |

The rectangle is antialias-neutral on both engines. Picsie matches
per-channel tolerance on premultiplied pixels including alpha; GIMP uses
threshold 0 under the COMPOSITE criterion with transparent sampling off.
Picsie's selected-layer wand path renders the bare layer (no mask/opacity
— absent here by construction), matching GIMP's drawable-scoped sampling.
The squares are binary alpha (fully opaque on fully transparent), so this
cohort exercises only transparent-vs-opaque alpha matching; partial alpha,
soft layer edges and feathered-selection boundaries are explicitly not
covered here.

Each sample asserts selection dimensions, bounds, changed
interior/exterior coverage probes, and history count. The trial-1 paired
`.selcov` dumps at BOTH sizes are quantified with the test-only
`compare-sel`
subcommand of the Rust example (256-row streaming; bounded memory, not a
production renderer).

### Measured results (medians, 12 samples per cell)

Picsie `total` = command + coverage + cold fresh-Renderer composite + RGBA
read. GIMP `total` = call + mask extraction + visible-layer merge + RGBA
read. Full p50/p95/min/max/counts in
`artifacts/feature-performance/selections/performance.selections/evidence/results.json`;
GIMP version `GNU Image Manipulation Program version 3.2.6` is recorded
there. No samples were discarded. Short repeats are not confidence
intervals. Fixture hashes `1200-selections.png 17a8fe9f…`,
`3600-selections.png b43131dd…` (full in manifest); the shared
`1200.png`/`3600.png` hashes match the layers/masks cohort exactly.
Binaries: picsie `e9e41209…`, gimp-console `48fdbc42…` (same GIMP binary
as the reviewed cohort).

| Case @1200x800 | Picsie cmd / coverage / render / read / total | GIMP cmd / coverage / render / read / total |
| --- | ---: | ---: |
| select-inverse | 2.54 / 0.04 / 32.2 / 4.4 / 39.2 | 1.11 / 2.8 / 18.6 / 5.8 / 29.8 |
| select-expand5 | 1.40 / 0.04 / 30.7 / 4.5 / 36.6 | 8.74 / 2.8 / 18.8 / 5.2 / 35.8 |
| select-contract5 | 1.24 / 0.04 / 30.7 / 4.4 / 36.4 | 9.17 / 2.9 / 19.1 / 4.5 / 36.1 |
| wand-contiguous | 16.4 / 0.04 / 29.1 / 4.5 / 50.2 | 12.3 / 3.0 / 19.5 / 6.7 / 42.2 |
| wand-noncontiguous | 17.9 / 0.04 / 28.8 / 4.4 / 51.2 | 13.4 / 2.9 / 20.1 / 6.0 / 42.3 |
| select-layer-alpha | 15.4 / 0.04 / 29.3 / 4.5 / 49.2 | 66.4 / 2.9 / 20.3 / 6.2 / 95.8 |

| Case @3600x2400 | Picsie cmd / coverage / render / read / total | GIMP cmd / coverage / render / read / total |
| --- | ---: | ---: |
| select-inverse | 25.3 / 1.4 / 298.0 / 51.3 / 376.4 | 3.58 / 20.1 / 99.8 / 94.7 / 216.9 |
| select-expand5 | 14.7 / 1.2 / 291.3 / 50.9 / 358.2 | 54.6 / 19.6 / 107.6 / 93.9 / 275.3 |
| select-contract5 | 15.0 / 1.3 / 293.6 / 50.7 / 361.1 | 62.5 / 19.6 / 107.2 / 88.4 / 277.6 |
| wand-contiguous | 189.5 / 4.3 / 278.1 / 50.7 / 522.9 | 48.9 / 19.9 / 111.7 / 92.0 / 273.5 |
| wand-noncontiguous | 195.8 / 4.5 / 276.6 / 51.2 / 528.3 | 51.7 / 19.6 / 108.4 / 88.6 / 267.0 |
| select-layer-alpha | 157.7 / 1.6 / 289.1 / 51.4 / 504.0 | 475.3 / 20.0 / 107.8 / 87.3 / 697.6 |

The Picsie wand/layer-alpha command cost (full-canvas match plus exact
pixel-edge outline tracing: ~16–18/190–196 ms vs GIMP ~12–13/49–52 ms)
and the GIMP layer-alpha cost at 3600 (~475 ms vs Picsie ~158 ms) are
reported without causal component claims; no application code was
optimized to improve numbers. The coverage stage itself is negligible on
Picsie (immutable shared buffer) and ~3/20 ms of GEGL mask extraction on
GIMP.

### Quality alignment (not parity claims)

- `select-inverse`, `select-contract5`, `wand-contiguous`,
  `wand-noncontiguous`, `select-layer-alpha`: paired `.selcov` dumps
  **byte-identical** at both sizes (0 differing pixels), with matching
  selected counts (inverse 720,000/6,480,000; contract 230,100/2,130,100;
  contiguous wand 22,500/202,500; noncontiguous and layer-alpha
  45,000/405,000).
- `select-expand5`: 35 of 960,000 pixels differ at 1200 (34 of 8,640,000
  at 3600), max step 127, confined to the grown edge ring. Picsie's
  stroked-band resize rasterizes antialiased edges; GIMP grow keeps hard
  edges with feather off. Both selections cover the same grown rectangle;
  the edge rasterization differs and is labeled as a quality limit, not
  parity.
- Coverage limit: the fixture squares are binary alpha, so paired
  comparisons prove transparent-vs-opaque matching only. Partial alpha,
  soft layer edges and feathered-selection boundaries remain unmeasured;
  do not generalize these byte-identical results to soft-boundary
  selections.
- GIMP restoration methods (all `actual_undo_verified: false`): inverse by
  `double-invert-full-coverage`, expand by `grow-then-shrink-full-coverage`
  (closing is exact on the hard-edged rectangle), wand/layer-alpha by
  `deselect-then-empty-predicate`. Contract uses
  `deselect-then-reselect-input-full-coverage` because shrink-then-grow
  (opening) rounds convex corners and does not restore the input
  byte-for-byte — verified in the retained failed pilot
  (`selections/pilots/sel-pilot-suite/gimp-1/app.log`, preserved alongside
  the cohort with all other pilots). Picsie uses actual history Undo
  restoring exact pre-command coverage in every case.

## Recent-features cohort (batch 3)

Measured 2026-10-03, `performance.recent-features` scenario (same reviewed
harness, suite-locked so layer/mask and selection workloads never rerun).
GIMP is a secondary functionality/performance reference, never a UX
reference. Prior cases, semantics, results and review fixes above are
unchanged; the orchestrator additionally supports the `recent-features`
suite and a paired `.maskcov` real-coverage comparison for the mask
gradient. The shared dispatch still accepts the older workloads (verified
by a minimal `mask-paint` + `select-inverse` executable smoke after the
shared edits, not a remeasured cohort).

### Reproducing

```sh
# Full paired recent cohort: 3 trials, 4 samples per size, 2 warmups.
npm run reproduce -- run --profile performance --scenario performance.recent-features \
  --build --trials 3 --samples 4 --warmups 2

# Corrected 3-case subset (review corrections; identical 3/4/2 shape).
# Never launches unlisted batches; originals are preserved.
npm run reproduce -- run --profile performance --scenario performance.recent-features \
  --cases group-resize,group-rotate,gradient-mask-linear \
  --build --trials 3 --samples 4 --warmups 2 \
  --output artifacts/feature-performance/recent-features/corrected

# One-case pilot (methodology check only, not a performance conclusion).
npm run reproduce -- run --profile performance --scenario performance.recent-features \
  --build --case group-resize --trials 1 --samples 1 --warmups 0
```

Unknown `--case` values are rejected before fixture preparation, including
cross-suite names. Outputs belong under ignored
`artifacts/feature-performance/`; the batch-3 cohort is
`artifacts/feature-performance/recent-features` (runner layout:
`recent-features/performance.recent-features/evidence`, fixtures
alongside, methodology pilots under `recent-features/pilots/`). The driver
validates the pinned `GIMP 3.2.6` version at startup and records it in the
manifest and report.

### Source references

- Compositor pinned `609dbeae` (functionality only, adapted to Rust
  commands): `Compositor/Document/LayerTransform.swift`
  (`TransformGroup`, `LayerTransform.following/placing`) and
  `EditorSession.swift` (`transformsAsGroup`, `groupTransformMembers`,
  `groupTransformBox`, `begin/commitTransform`) for the folder box carrying
  every member from its original placement in one undo;
  `Compositor/Document/Gradient.swift` (linear shape,
  foreground-to-background colors, drag line + `commitGradient` as one
  undo); `Compositor/Document/Levels.swift` (`beginLevels`,
  `updateLevels` live preview, `commitLevels`) and `Curves.swift` for the
  adjustment live-update transaction.
- Rust implementation: `crates/picsie-core/src/editor/group_transform.rs`
  (`carry_members`, `preview_group_box`), `editor/interaction_polish.rs`
  (`set_transform_field` box branch), `crates/picsie-core/src/gradient.rs`
  (`fill_layer`), `crates/picsie-core/src/editor/adjustments.rs`
  (`add_adjustment`, `update_adjustment_levels/curves`,
  `cancel_adjustment_edit`), `crates/picsie-core/src/adjustment.rs`
  (Levels tables, Curves Hermite interpolation).
- GIMP pinned `e101dd19`: `app/pdb/item-transform-cmds.c`
  (`gimp-item-transform-scale/rotate`, CUBIC interpolation context),
  `app/pdb/drawable-edit-cmds.c`
  (`gimp-drawable-edit-gradient-fill` LINEAR),
  `app/pdb/drawable-filter-cmds.c` + `libgimp/gimpdrawablefilter.h`
  (non-destructive `gimp:levels` / `gimp:curves` filters: new + config +
  append + update — the endorsed replacement for the deprecated
  `gimp-drawable-levels/curves-spline`),
  `app/operations/gimplevelsconfig.c` (`trc` defaults to LINEAR; the
  interactive tool sets PERCEPTUAL), `libgimp/gimpcurve.h` (curve points
  on `[0, 1]`). GIMP runtime 3.2.6, OpenCL off, isolated config/cache.
- Native tests: `crates/picsie-core/tests/group_transform.rs` (box carry,
  member fractions, Apply/Cancel), `shapes_gradients.rs` (linear commit,
  mask coverage), `adjustments.rs` (levels/curves liveness).

### Workloads and measurement boundaries

Modest matched fixtures at 1200x800 and 3600x2400, built procedurally on
both sides from the same constants. Fresh document/image per sample;
fixture preparation, group targeting, gradient configuration, adjustment
creation and filter attachment are untimed.

| Case | Fixture (untimed) | Picsie timed command | GIMP timed call |
| --- | --- | --- | --- |
| group-resize | base + folder with red/blue raster children | `BeginTransform` + `SetTransformField{ScalePercent:200}` + `CommitTransform` (one `Transform Layers` undo, hierarchy preserved) | `transform_scale` doubling the member box, CUBIC (bakes resampling into children) |
| group-rotate | same group fixture | Rotation 30 about the box center, one undo | `transform_rotate` 30° about the explicit box center, CUBIC (baked) |
| gradient-image-linear | single mid-gray layer | full-width drag + `CommitGradient` (one `Gradient` undo), FG black / BG white foreground-to-background | `edit_gradient_fill` LINEAR with matched endpoints/colors (no supersample/dither) |
| gradient-mask-linear | base + red layer under solid white mask | same drag on the mask target (`Gradient Mask` undo) | same fill on the mask drawable; real `.maskcov` channel dumped |
| levels-update | gray layer + white square | `UpdateAdjustmentLevels{black:64}` live preview inside the open edit (no commit) | identity `gimp:levels` filter attached untimed (perceptual TRC); timed `low-input` set + `update` |
| curves-update | same adjustment fixture | `UpdateAdjustmentCurves{RGB 128->192}` live preview (no commit) | identity `gimp:curves` filter untimed; timed curve mid-point add + `update` |

Per-sample stages keep the reviewed split: `command_ms` is the feature
call only; `render_ms` is the cold fresh-Renderer composite (Picsie) or
the destructive visible-layer merge (GIMP); `read_ms` is the RGBA
extraction. The mask gradient materializes its real mask coverage once
per sample in `coverage_ms` on BOTH sides (the same channel is validated
and dumped; GIMP reads the mask buffer pre-merge by necessity, Picsie
reads the committed raster). Group hierarchy snapshot getters run on a
separate untimed `metadata_ms` stage on the GIMP side only, excluded from
every stage and from the total: `total_ms` is wall time minus
`metadata_ms`, and the reported stages sum to it within ~0.01 ms of
timer-call gaps (verified per sample; older cases have `metadata_ms`
0.0 and unchanged wall-clock totals). This is an exact CPU stage account,
not a continuous end-to-end wall claim over the validation reads. Getter
validation and pixel assertions otherwise run after `total_ms`, outside
every timer. GIMP history stays enabled;
every timed call runs on a per-sample duplicate, discarded afterward.
Only the first measured sample's `.rgba` (plus `.maskcov` for the mask
gradient) per case/size/trial is dumped; the paired comparison uses the
trial-1 dumps. Every sample asserts geometry, history/transaction counts,
nontrivial changed output against the untimed pristine render, and
engine-appropriate probes.

Untimed restoration controls run per case and size: actual history Undo
(groups, gradients — exact member geometry, mask coverage and pixels) and
the adjustment Cancel path (keeps the added layer with identity settings,
one undo retained) on the Picsie side; manual inverse/reset on the GIMP
side (`actual_undo_verified: false`), full-RGBA sound for gradient
repaints and filter resets, geometry-only for group resize, and
hierarchy-only for group rotate (see quality limits). Picsie-only cases:
none — all six pair.

### Measured results (medians, 12 samples per cell)

Two cohorts, same 3-trial/4-sample/2-warmup shape. The original cohort
(`recent-features/performance.recent-features/evidence/`) measured all
six cases. Review corrections changed the timed path of three cases, so
a CORRECTED cohort (`recent-features/corrected/...`) remeasured only
`group-resize`, `group-rotate` and `gradient-mask-linear`; the
`gradient-image-linear`, `levels-update` and `curves-update` rows below
are the original measurements (their timed paths are unchanged) and are
marked accordingly. Full p50/p95/min/max/counts live alongside each
cohort; GIMP version `GNU Image Manipulation Program version 3.2.6` is
recorded in both. No samples were discarded. Short repeats are not
confidence intervals. Source hashes: original manifest HEAD `3e3df0b`
plus working-tree patch; corrected manifest HEAD `2ef333c` plus patch,
picsie binary `2ab71819…`, same `48fdbc42…` GIMP binary in both. The
first-measured GIMP filter samples pay a one-time GEGL operation cost
(~2.3–2.8 s at 1200 in zero-warmup pilots); both cohorts' 2 warmups
absorb it before measured samples.

Picsie `total` = command + coverage + cold fresh-Renderer composite +
RGBA read. GIMP `total` = call + coverage + visible-layer merge + RGBA
read. Untimed `metadata_ms` (groups only, ~0.8 ms) is excluded from
the measured stages and total, and reported separately. Rows marked ★ are corrected-cohort medians; the rest are
original-cohort medians.

| Case @1200x800 | Picsie cmd / coverage / render / read / total | GIMP cmd / coverage / render / read / total |
| --- | ---: | ---: |
| ★ group-resize | 0.03 / 0.00 / 22.12 / 3.50 / 25.67 | 39.19 / 0.00 / 20.18 / 5.23 / 64.61 |
| ★ group-rotate | 0.03 / 0.00 / 19.40 / 3.58 / 23.01 | 26.97 / 0.00 / 14.30 / 5.28 / 46.70 |
| gradient-image-linear | 17.08 / 0.00 / 9.5 / 3.8 / 30.8 | 43.27 / 0.00 / 2.2 / 4.6 / 50.1 |
| ★ gradient-mask-linear | 35.89 / 0.06 / 32.22 / 3.35 / 71.65 | 47.30 / 2.63 / 19.39 / 4.58 / 73.34 |
| levels-update | 0.01 / 0.00 / 54.2 / 3.4 / 57.7 | 0.37 / 0.00 / 21.2 / 4.3 / 25.9 |
| curves-update | 0.01 / 0.00 / 53.9 / 3.4 / 57.4 | 0.52 / 0.00 / 20.6 / 4.3 / 25.5 |

| Case @3600x2400 | Picsie cmd / coverage / render / read / total | GIMP cmd / coverage / render / read / total |
| --- | ---: | ---: |
| ★ group-resize | 0.04 / 0.00 / 202.01 / 49.11 / 251.22 | 230.86 / 0.00 / 192.32 / 97.93 / 513.18 |
| ★ group-rotate | 0.04 / 0.00 / 181.10 / 50.02 / 231.35 | 133.31 / 0.00 / 149.26 / 90.84 / 373.34 |
| gradient-image-linear | 194.15 / 0.00 / 94.6 / 51.5 / 341.5 | 219.05 / 0.00 / 5.2 / 90.3 / 320.6 |
| ★ gradient-mask-linear | 625.69 / 1.81 / 372.91 / 50.12 / 1050.57 | 219.92 / 19.39 / 102.51 / 78.03 / 417.49 |
| levels-update | 0.02 / 0.00 / 568.6 / 34.0 / 602.2 | 0.37 / 0.00 / 150.3 / 73.4 / 215.9 |
| curves-update | 0.02 / 0.00 / 567.0 / 33.7 / 600.3 | 0.49 / 0.00 / 133.9 / 74.8 / 209.4 |

### Quality alignment (not parity claims)

- `levels-update`, `curves-update`: paired trial-1 `.rgba` dumps
  **byte-identical** at both sizes (0 differing pixels) — the perceptual
  TRC alignment (`gimp:levels/curves` config `trc=PERCEPTUAL`, matching
  Picsie's encoded-value tables) plays the same role as the batch-1
  RGB_NONLINEAR composite-space fix. Knot probes agree exactly (gray 85
  under levels black 64; gray 192 under the curves mid lift).
- `gradient-image-linear`: small interpolation rounding only (cohort
  trial-1 dumps: 12,800/960,000 px at 1200 differ by at most 1 step,
  33,600/8,640,000 at 3600; endpoints and midpoint agree).
- `group-resize` (corrected cohort): uniform interiors agree; 2,261/960,000 px differ at
  1200 (4,547 at 3600), max step ~100, confined to resampled member
  edges. GIMP bakes CUBIC resampling into the children (sizes round by
  a pixel on fractional 1200px box corners); Picsie keeps live layer
  transforms. Hierarchy snapshot getters are an untimed `metadata_ms`
  stage (~0.8 ms) excluded from the total. Labeled representation difference,
  never equal-quality speed parity.
- `group-rotate` (corrected cohort): rotated-edge resampling only (3,128 px at 1200,
  max step 132; 7,590 px at 3600). Same baked-vs-live labeling and the
  same untimed `metadata_ms` stage; the rotated control is explicitly
  the lower-alignment case.
- `gradient-mask-linear` (corrected cohort): systematic mid-band
  difference, quantified not claimed away — 955,200/960,000 mask pixels
  differ, max coverage step 74; composite max step 38 (was 50 before the
  sRGB compositing control). Both engines' ramps are deterministic at
  both sizes and each side is probed against its own ramp (Picsie
  0/13/128/242/255, GIMP 0/1/55/227/255, edges exact on both). GIMP
  paints masks in linear light: an explicit `RGB_LINEAR` blend context
  was investigated and verified to have no effect — the fill procedure
  reads the paintbrush tool preset's blend space (default
  `RGB_PERCEPTUAL`, `gimppaintoptions.c`, no batch setter) and
  `gegl:linear-gradient` is hidden as a drawable filter. The top layer
  does composite sRGB (`RGB_NON_LINEAR`, same control as
  opacity-commit), which improved endpoint agreement. No match is
  claimed from channel alignment alone.
- GIMP restoration methods (all `actual_undo_verified: false`):
  group-resize by `inverse-corners-geometry` (member geometry within
  2px only — baked resampling is lossy, so this is not a raster
  restoration and no pixel restoration is claimed); group-rotate by
  `structure-preserved-no-sound-inverse` (hierarchy only — inverse
  rotation is not a reversible raster restoration: a +30/-30 round-trip
  re-bboxes padded buffers to 576x548 instead of 400x200, retained in
  `recent-features/pilots/gi-group-diag5.json`, so no geometry or pixel
  restoration is claimed); gradients by base/mask repaint to full-RGBA
  equality; levels/curves by filter reset to full-RGBA equality. Picsie
  uses actual history Undo (exact member geometry, mask coverage and
  pixels) and the adjustment Cancel path (identity settings, added layer
  kept).
- Coverage limits: two-member groups only (no 1000-layer matrix in this
  batch); single linear black-to-white gradient (radial/reverse/opacity
  variants unmeasured); single levels/curves control each (channel and
  gamma variants unmeasured). Do not generalize beyond these controls.

### Parent timing-accounting audit

The corrected cohort's GIMP group totals already excluded the separately
recorded `metadata_ms`, but its reported `render_ms` still included that
interval. Parent review caught this through per-sample stage arithmetic.
The original `results.json` is preserved. Adjacent `stage-accounting.json`
is a derived report subtracting the captured metadata interval from each
GIMP group render stage; it records the original report's SHA-256. The
group render medians above use that derivation; command/read/total values,
sample counts and pixels are unchanged. This is accounting correction of
recorded intervals, not new measurements or discarded samples.

The tracked driver now excludes metadata from both cumulative stage
calculations and rejects negative/nonfinite timings or measured stages
exceeding total. A regression fixture uses the captured failing sample.
A separate one-sample smoke validates the corrected executable path; no
old measured matrix was rerun. Future runner evidence hashes include
`.selcov` and `.maskcov` alongside RGBA and JSON outputs.

## Checkpoint and next work

This pass expanded measurement and reproducibility; it did not optimize the
production engine. The tracked catalog now contains 30 scenarios and associates
performance workloads with 27 of the 200 registry features. Associations cover
the stated fixtures, not every setting or full polish. All 19 new workloads,
888 accepted samples, output differences and corrected accounting are recorded
above; failed pilots and superseded cohorts remain under ignored `artifacts/`.

Final integrated validation passed `npm run check:architecture`, `npm run check`,
`npm test` (275 core Rust tests, 18 desktop tests and 29 Node addon tests),
`npm run test:bun` (26 tests), and the runner's 12 tests. Native GUI evidence is
recorded separately in [feature-verification.md](feature-verification.md);
this CPU benchmark pass did not re-certify visual polish or physical-display
latency. No implementation/polish status was upgraded from timing evidence.

The next profiling order, not yet implemented:

1. **Levels/Curves rendering.** The tested two-tone outputs match GIMP exactly.
   At 3600×2400 the setting update is ~0.02 ms; the fresh full-image render is
   ~569/567 ms versus GIMP's ~150/134 ms merge/filter evaluation. Investigate
   the render path before changing the already-cheap setting update.
2. **Wand selection.** Contiguous/global command medians are ~190/196 ms versus
   GIMP's ~49/52 ms, with byte-identical coverage in these binary-alpha,
   zero-tolerance fixtures. Other tolerances and soft alpha remain unmeasured.
3. **Mask painting.** The 61-sample command is ~562 ms versus ~108 ms. Brush
   edges differ, so profiling must preserve Picsie's source-defined semantics
   and keep output-quality limits explicit.

Run these sequentially: validate one small pilot, profile the existing path,
read the pinned Compositor/GIMP implementations, settle the structural change,
then measure one final cohort of affected workloads. Reuse the drivers and
avoid broad matrix reruns. Most elapsed time in this first expansion was driver
development, API investigation and review corrections; it was not a larger
statistical sample. Settle methodology before collecting a full cohort.
