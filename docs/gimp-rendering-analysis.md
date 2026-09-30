# What Picsie can learn from GIMP's rendering

Baseline source investigation, 2026-09-30, against Picsie `d8a459e`.
GIMP is a functionality/performance reference
only. Compositor remains authoritative for editing behavior, defaults and
workflows; the QuickGUI application remains the UI parity reference.

The measured target for this workload is GIMP's **33.9 ms median / 37.7 ms p95
nudge latency and 41.0 visible drag updates/sec**, against Picsie's
75.5 / 79.0 ms and 20.7/sec. These are virtual X11 results on one document,
not physical-display guarantees or universal application frame rates. See the
[benchmark and its limitations](gimp-performance.md).

## The layer-move path

This investigation follows the path exercised by that benchmark, rather than
using brush timers or tool-overlay frame limits to explain layer movement.
Sources are pinned to GIMP `e101dd19b165f927d3ba0a74658a71537c5661b9`.

1. **Change placement, preserve source pixels.**
   [`gimp_edit_selection_tool_update_motion`](https://gitlab.gnome.org/GNOME/gimp/-/blob/e101dd19b165f927d3ba0a74658a71537c5661b9/app/tools/gimpeditselectiontool.c#L480)
   translates the selected items when their integer coordinates change.
   [`gimp_layer_real_translate`](https://gitlab.gnome.org/GNOME/gimp/-/blob/e101dd19b165f927d3ba0a74658a71537c5661b9/app/core/gimplayer.c#L1646)
   marks the old drawable bounds for update, changes placement, then marks the
   new bounds. `gimp_item_set_offset` changes offsets and the corresponding
   `gegl:translate` nodes; it does not rewrite the layer's raster buffer.

2. **Accumulate damage in document coordinates.**
   `gimp_drawable_stack_drawable_update` adds the layer offsets to the update
   rectangle; image invalidation reaches the projection's update region.
   [`gimp_projection_add_update_area`](https://gitlab.gnome.org/GNOME/gimp/-/blob/e101dd19b165f927d3ba0a74658a71537c5661b9/app/core/gimpprojection.c#L622)
   rounds rectangles outward on a 32×32 update grid, clips them to bounds and
   unions them. This grid simplifies damage tracking; it is not a claim that
   GEGL's storage tiles are 32×32.

3. **Update a persistent composite.**
   `gimp_projection_allocate_buffer` retains a GEGL projection buffer across
   ordinary edits. A projectable tile-validation handler connects it to the
   rendering graph. The
   [validation handler](https://gitlab.gnome.org/GNOME/gimp/-/blob/e101dd19b165f927d3ba0a74658a71537c5661b9/app/gegl/gimptilehandlervalidate.c#L275)
   passes through clean tiles and validates dirty coverage. GIMP does not need
   to allocate and populate a new whole-document composite for each move.

4. **Schedule damage, prioritize what is visible.**
   The move tool calls `gimp_projection_flush`. This schedules projection
   construction in chunks on the **main thread**. When new damage arrives,
   `gimp_projection_chunk_render_start` merges unfinished coverage with the
   new region. The display shell supplies its viewport as a priority rectangle.
   [`GimpChunkIterator`](https://gitlab.gnome.org/GNOME/gimp/-/blob/e101dd19b165f927d3ba0a74658a71537c5661b9/app/core/gimpchunkiterator.c#L450)
   adapts chunk area using measured work time and yields after its work budget.
   Its default 1/15-second budget is a work slice, **not a 15 fps cap**.

5. **Retain the display conversion too.**
   [`gimp_display_paint_area`](https://gitlab.gnome.org/GNOME/gimp/-/blob/e101dd19b165f927d3ba0a74658a71537c5661b9/app/display/gimpdisplay.c#L844)
   transforms damage into display coordinates, expands it for sampling,
   coarsens it and invalidates that portion of the display cache.
   [`gimp_display_shell_draw_image`](https://gitlab.gnome.org/GNOME/gimp/-/blob/e101dd19b165f927d3ba0a74658a71537c5661b9/app/display/gimpdisplayshell-draw.c#L190)
   tests cache validity per display chunk. Invalid chunks are rendered and
   marked valid; valid chunks are painted from the retained Cairo surface.
   Color conversion and scaling operate on requested regions rather than
   requiring a new complete canvas image resource every update.

A small move still invalidates the union of the **whole old and new layer
bounds**, not just the thin strips at its edges. A textured or translucent layer
changes the overlapping interior too. Recomposition must include the background
and overlapping layers in their correct order.

GIMP also freezes selected items' viewable previews during the move gesture and
thaws them at release. Projection-derived thumbnail invalidation waits for
projection completion. It separates interactive canvas work from some ancillary
preview work.

## Where the baseline Picsie build differed

| Stage | GIMP | Picsie at `d8a459e` |
| --- | --- | --- |
| Layer source | Retained pixels; placement changes separately | Already cached in `Renderer::layer_surface`; ordinary movement reuses them |
| Document composite | Retained projection with dirty coverage | Fresh document-sized Skia surface; composites all visible layers |
| Viewport | Retained converted display cache; region validity | Fresh full viewport surface, checkerboard, scaled composite and overlays |
| UI-only updates | Image damage tracked separately | Worker builds a preview after every request batch, even if document pixels are unchanged |
| Presentation | Requested regions reach retained Cairo display cache | Fresh full BGRA buffer and fresh GPUI `RenderImage`; old image removed |
| Scheduling | Merges unfinished damage; viewport priority; chunk budgets | Preserves all commands/samples; coalesces completed frames, but each render remains a complete preview |

The Picsie paths are in
[`render.rs`](../crates/picsie-core/src/render.rs),
[`engine.rs`](../crates/picsie-desktop/src/engine.rs) and
[`ui/mod.rs`](../crates/picsie-desktop/src/ui/mod.rs).
It would be incorrect to describe Picsie as having no caches or to recommend
adding a layer cache as the main fix: that cache already exists and normally
survives movement. Independently placed masks correctly depend on placement.

At 936×734, every published canvas contains **2,748,096 bytes**. In the locked
GPUI 0.3.7 source, `RenderImage::new` assigns a new image ID;
`Window::paint_image` inserts the full image into the sprite atlas; the WGPU
atlas stages its bytes for `queue.write_texture`. `Window::drop_image` removes
the old atlas entry. Atlas storage may be reused, but the new image contents
are still uploaded. Reducing engine damage alone does not remove this full
canvas publication cost.

## What the existing traces tell us

Two additional diagnostic launches used the unchanged release binary and its
existing `PICSIE_TRACE_DIR` paint callback. Each supplied 28 nudges; the first
four were excluded, giving 48 measured diagnostic nudges. Each observed frame
advanced the sequence by exactly one. The external edge detector and native
screenshots confirmed the same canvas and interaction.

| Diagnostic stage | Pooled median |
| --- | ---: |
| Apply command batch | 0.07 ms |
| Render full preview | 20.6 ms |
| Extract BGRA pixels | 4.2 ms |
| Enqueue → UI paint callback | 47.5 ms |
| External input → observed visible edge | 75.1 ms |

The paint callback runs at the end of GPUI's CPU element paint phase; it is not
a GPU completion fence or visible presentation. The per-sample median remainder
between enqueue and that callback, after the three timed worker stages, was
22.0 ms. That combines unmeasured queue/wake delays, snapshot work, state/control
synchronization, UI layout/paint and image staging. It cannot be assigned to
one component from this trace.

The external observer first saw the updated paint record around 50 ms after
input; a visible edge followed a median 24.7 ms later. That also does not isolate
GPU time: frame submission, software Vulkan execution, X11 presentation and
observation delay are not separately timed. Stage medians use different start
points and must not be added/subtracted as an exact latency decomposition.

These runs enable tracing and extra JSON reads. They remain separate from the
untraced application comparison. Evidence is under ignored
`artifacts/gimp-performance/stage-probe/`, with the diagnostic driver at
`artifacts/gimp-performance/trace-stage-probe.py`.

**Consequence:** avoidable full rendering is real, but it does not account for
the entire gap. Reaching GIMP's latency likely requires improvements in both
retained rendering and presentation. Command execution itself is not the large
cost in these isolated nudges; dragging has not received the same stage analysis.

## A sequence for reaching the measured GIMP level

1. **Start with a retained document composite and conservative damage for
   simple layer moves.** Keep Rust/Skia, the current layer cache and exact
   compositing order. Clear and recompose the union of old/new bounds in a
   persistent surface; reuse it unchanged for viewport or overlay changes.
   Begin with ordinary unfiltered layers. Use a full redraw for effects or
   dependency cases until their damage rules are verified. Benchmark this
   separately while initially preserving the existing full canvas transport.

2. **Carry reuse through the presentation boundary.** If full publication and
   UI/presentation remain limiting, measure receive/control sync, CPU image
   staging, frame submission and presentation explicitly. Then evaluate a
   supported persistent image update or retained canvas tiles in GPUI. Clean
   regions should retain their image resources; changed regions should update.
   Moving handles or changing a tool should not require recompositing document
   pixels. A CPU engine can feed that presentation path; GPU image processing
   is a separate decision.

3. **Add chunk scheduling and viewport priority where document size warrants
   them.** Continue applying every editing command and pointer sample, preserve
   gesture boundaries/history, and coalesce only presentation work. Prioritize
   visible dirty regions and carry remaining damage forward. Adopt the principle
   of a measured work budget, not GIMP's numeric slice as a new Picsie default.

The first experiment should measure the gain from step 1, then use those results
to decide the next bottleneck. A twofold speedup is not established by source
inspection or area estimates. Our comparison already shows that replacing the
editable demo with raster layers does not close the gap.

Compositor already supplies relevant precedent:
[`EditorCanvas`](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Rendering/EditorCanvas.swift#L503)
uses dirty brush rectangles;
[`TiledLayerRenderer`](https://github.com/robbietilton/Compositor/blob/609dbeae2ef68ef4fc82d67e4981a49852eb6e13/Compositor/Rendering/TiledLayerRenderer.swift)
rebuilds affected pieces with sampling margins and draws unchanged content
elsewhere. `EffectsPreviewCache` separates cached layer pixels from placement,
with an exception for independently placed masks. These are candidates to port
when extending those subsystems; GIMP's projection machinery is an architectural
reference, not a claim that Compositor implements the same machinery.

Before accepting an incremental renderer, compare it with the existing full
renderer across movement, translucent edges, blend modes, fractional placement,
blur, masks, clipping dependencies, groups, undo/redo and viewport changes.
Use conservative invalidation and a full-render fallback. Inspect actual native
screenshots for seams, stale pixels and handle alignment. Compositor's
`TiledLayerTests`, `DownsampleTests` and mask/brush fixtures provide relevant
source scenarios; their tests were inspected, not run on this Linux host.
GIMP core/tool test entry points were also inspected; they do not establish the
performance target, and the GIMP source test suite was not executed.

GIMP's pinned configuration defaults `use-opencl` to false. The measured result
therefore gives us a useful target before adding GPU compute. This analysis
does not require a UI framework change, new editing semantics or GIMP UX.
No GIMP implementation or fixtures were copied. Application experiments followed
this initial source investigation; their implementation scope is recorded in
the [source map](compositor-port.md#retained-preview-experiment-2026-09-30), with
measured outcomes in [the codebase experiment report](desktop-gimp-experiments.md).
