# Experiments following the GIMP rendering investigation

2026-09-30. These are changes and measurements in Picsie's codebase, following
the [pinned-source investigation](gimp-rendering-analysis.md). The comparison
baseline is `d8a459ef436784aae1d0efc26777bba3fbf59612`. Compositor remains the
behavior reference, QuickGUI the UI parity reference, and GIMP a functionality
and performance reference only. No GIMP code or fixtures were copied.

The first pass below used software Vulkan. The [second pass](#second-pass-presentation-and-real-gpu-experiments)
also verified real AMD GPU rendering on this host. Read its results separately:
the presentation configuration and keyboard spacing differ between cohorts.

## What we tested

1. **Retain the document composite.** Rust keeps its pixels and an immutable
   snapshot. Simple moves clear and recompose the union of the old/new layer
   bounds; unchanged document pixels can be reused. The existing per-layer cache
   already survives ordinary moves and was not the missing cache.
2. **Avoid drawing the old frame immediately after queuing a command.** The UI
   waits for the existing worker notification before explicitly invalidating
   itself for that command. Focus and transient control changes retain their
   own notifications.
3. **Retain the checkerboard/background.** Its existing algorithm is unchanged;
   document dimensions and viewport geometry determine its cache key.
4. **Prepare frames at the consumer's pace.** While a completed frame awaits the
   UI, the worker keeps applying commands and delivering reliable outcomes,
   but defers another full preview. Consuming that frame wakes pending rendering
   even if no more input arrives. No editing commands or pointer samples are lost.

The first composite prototype retained a Skia `Surface`. It worked in the desktop
binary but failed the Node addon build because `Surface` is not `Send`. The final
implementation retains Rust-owned pixels, borrows them during Skia rendering,
and takes an immutable snapshot when pixels change. It introduces no unsafe
thread-sharing workaround.

Exact comparisons also caught different Skia rounding when a stationary rotated
layer overlapped a partial redraw. Partial recomposition therefore requires an
entirely simple, untransformed stack. Masks, hierarchy, blur, clipping dependencies,
or transforms use full recomposition. This favors unchanged pixels over a broader
but incorrect fast path. Export and destructive rendering retain the full path.

## Final repeated application comparison

Original, complete candidate, and GIMP each received one discarded full warm-up
and three measured launches in rotating order. This gives 72 measured nudges and
six measured drags per application. Latency/interval percentiles pool samples;
update rates, CPU and memory are medians of run values. Tracing was disabled and
no builds, tests or other editors ran concurrently.

| Measurement | Original Picsie | Complete candidate | GIMP 3.2.6 |
| --- | ---: | ---: | ---: |
| Nudge → visible change, median | 75.6 ms | **59.5 ms** | 33.9 ms |
| Nudge → visible change, p95 | 83.1 ms | **72.2 ms** | 38.4 ms |
| Faster drag, visible updates/sec | 20.7 | **23.0** | 43.0 |
| Faster drag, median visible interval | 47.8 ms | 43.3 ms | 18.1 ms |
| Faster drag, p95 visible interval | 52.5 ms | 51.5 ms | 39.4 ms |
| Faster drag CPU, 100% = one logical core | 568% | 543% | 186% |
| Idle resident memory | 210 MiB | 217 MiB | 311 MiB |

The complete candidate improved median isolated-input latency **21%** and median
drag throughput **11%** against the original in this cohort. It remains behind
GIMP. The nearly unchanged p95 drag interval also shows that typical gains do
not establish smooth tail behavior. **Neither application demonstrates p95
visible intervals or input latency at most 20 ms in this test.**

Candidate run rates were 22.0–24.2 updates/sec; GIMP's were 42.0–44.7. One
candidate run had 91.3 ms p95 nudge latency, compared with about 61.5–61.7 ms
in its other runs. It is retained in the pooled 72.2 ms p95. GIMP also had slower
keyboard tails in one run. Short repeats are not confidence intervals, and
GIMP's earlier intermediate cohort varied more broadly, at 30.0–42.5 updates/sec.
Those older measurements are not substituted for this cohort's GIMP result.

Estimated continuous-drag position lag was 89.2 → 90.4 ms for Picsie, versus
25.9 ms for GIMP. This approximate diagnostic is distinct from keyboard latency:
more frequent updates did not establish less position lag. All 216 measured
nudges and 18 measured drags passed displacement/return assertions.

## Controlled intermediate results

Each pair uses the same external input/observation loop, a discarded full warm-up
per variant, and rotating launch order. Separate experiments are not pooled.

| Experiment | Measured launches per variant | Median nudge latency, before → after | Faster drag updates/sec, before → after |
| --- | ---: | ---: | ---: |
| Composite prototype alone | 2 | 76.9 → 75.9 ms | 21.1 → 21.6 |
| Same prototype, defer command redraw | 1 | 75.5 → 59.7 ms | 21.7 → 21.5 |
| Original → safe composite + background + redraw change | 3 | 75.9 → 59.8 ms | 19.7 → 20.5 |
| That candidate → consumer-paced preparation | 2 | 59.7 → 59.8 ms | 20.5 → 24.5 |

Retaining the composite alone did **not** establish a meaningful application
speedup. Deferring the premature redraw saved approximately one 60 Hz interval
for isolated keyboard edits. Consumer pacing improved drag throughput about 20%
in its paired test; its p95 visible interval fell from 51.7 to 47.3 ms. CPU use
rose from 549% to 581% (100% is one logical core), so it is not a CPU-use saving.
The background was tested in the combined safe candidate, not independently.

The pacing experiment's estimated drag position lag was 89.3 → 88.9 ms. This
diagnostic matches recorded edge positions to the scripted triangle, with roughly
8.3 ms per detector pixel plus observation delay. It supports no large lag
regression in these runs; it cannot establish a sub-millisecond improvement.

Three alternating paired CPU microbenchmarks measured repeated, **unchanged**
document previews. The table reports medians of the three run medians, each
containing 40 samples. These exclude application scheduling, GPU upload and
presentation, and must not be described as moving-frame or visible latency.

| CPU stage | Original | Safe retained renderer |
| --- | ---: | ---: |
| Complete unchanged preview, 936×734 | 17.15 ms | 6.96 ms |
| BGRA extraction + RenderImage construction, 936×734 | 2.59 ms | 2.42 ms |
| Complete unchanged preview, 1920×1080 | 11.71 ms | 3.75 ms |
| BGRA extraction + RenderImage construction, 1920×1080 | 7.97 ms | 7.87 ms |

Even the improved renderer still builds a full viewport, extracts every pixel,
and publishes a fresh GPUI image. The 936×734 image contains 2,748,096 bytes.
These experiments reduce some preparation work; they do not implement GIMP's
retained display conversion or partial image-resource updates.

## Separate diagnostic timing probe

After the untraced comparison, one additional launch per binary used the existing
paint-record diagnostics and external observation hook. Each retained 24 nudges
after the first four, and every recorded paint advanced the edit sequence by
exactly one. These diagnostic samples are not pooled into the application results.

| Complete candidate diagnostic stage | Median |
| --- | ---: |
| Apply command | 0.07 ms |
| Render changed preview | 13.63 ms |
| Extract BGRA | 2.63 ms |
| Enqueue → end of CPU UI paint | 29.99 ms |
| External input → visible change | 59.11 ms |

The observer saw the new paint record a median 26.08 ms before the changed edge.
That remainder includes submission, software Vulkan, virtual presentation and
observation delay; the paint callback is not a GPU completion fence. The stages
use different start points, so their medians are not an exact additive latency
decomposition.

The original binary's diagnostic visible median was 108.02 ms, materially slower
than its 75.56 ms untraced result. Both raw diagnostic launches are retained under
`stage-probe/`; that diagnostic before/after difference is not evidence of the
normal application speedup. Instrumented runs can alter timing and vary.

These results support a next experiment that carries reuse through viewport
publication and measures UI receipt/paint, image staging and submission separately.
The remaining full-image publication is visible in the code; its individual
contribution to the remaining latency has not been isolated by this pass.

## Correctness and workflow verification

Local exact-pixel tests cover translucent shape/gradient/native raster content,
all sixteen blend modes, three sampling modes, fractional and off-canvas moves,
overlaps, viewport/handle changes, effects, masks, live source changes, hierarchy,
layer order, resize, and restoring earlier state. A cache test verifies older
snapshots remain immutable. The new transport test holds a frame while 32 nudges
and a barrier complete, then requires the final frame without any new edit.
These are local regression cases, not newly translated upstream fixtures.

All required checks passed: `npm run check:architecture`, `npm run check`,
`npm test`, `npm run test:bun`, `npm run check:desktop`, and the desktop Rust test
suite (10 tests with pacing). The core/native checks were run on the final
renderer before the desktop-only pacing change; the desktop checks and native
workflow verification were repeated after that change. The release build passes.

The final candidate passed all **55 real-window workflow checks**, including
pointer release outside the canvas, one-undo gestures, masks, selection editing,
text, transforms, saves, exports, imports and dirty-window closing. Native layout
and canvas-size-dialog screenshots were inspected for alignment. Cropped canvas
screenshots from the original and final candidate were exactly equal at startup
and after both drag excursions: zero differing pixels across 936×734 pixels.
All nine matching canvas pairs in the final three-run cohort also compared
exactly.

The pinned Compositor implementations and tile/downsample tests were inspected;
their macOS test suite was not run on this Linux host. See the
[implementation source map](compositor-port.md#retained-preview-experiment-2026-09-30).

## Reproduction and evidence

Stage the runtime, tools and fixture described in
[the GIMP benchmark](gimp-performance.md#reproduction-and-evidence). Save the
baseline binary before building the candidate, then run:

```sh
python3 scripts/compare-desktop-experiment.py \
  --before artifacts/gimp-optimization/before/picsie-desktop \
  --after artifacts/gimp-optimization/demand/picsie-desktop \
  --gimp --trials 3 --output artifacts/gimp-optimization/comparison-final
python3 scripts/estimate-desktop-drag-lag.py \
  artifacts/gimp-optimization/comparison-final/application-results.json
```

The CPU microbenchmark is the release binary's `--measure` mode. Real-window
workflow verification uses:

```sh
python3 crates/picsie-desktop/verify.py \
  --binary artifacts/gimp-optimization/demand/picsie-desktop \
  --tools artifacts/selection-history/tools/usr --software --display :97 \
  --output artifacts/gimp-optimization/verification-demand
```

Run builds, tests and traced diagnostics separately from the application
comparison. The final comparison retains the original moderate document,
120 Hz motion input, approximately 4 ms X11 observation polling, software Vulkan
for Picsie, and GIMP's CPU GTK/Cairo path. It excludes a physical GPU, desktop
compositor and display scanout. Native keyboard displacement differs between
applications; direct drag motion is identical. This is not a universal app frame
rate or a physical-display latency guarantee.

Raw evidence lives under ignored `artifacts/gimp-optimization/`: intermediate
`comparison-composite`, `comparison-deferred`, `comparison-cache-ui`, and
`comparison-demand` directories; before/candidate binaries; source patches;
`transport-{before,final}-{1,2,3}.json`; test/build logs; native screenshots; and
exact canvas comparison results. `transport-final` and older command strings
using a `final/` binary path refer to the safe candidate before pacing, now stored
in `cache-ui/`. Binary hashes disambiguate these records. No measured samples were
removed after seeing results.

`comparison-final/application-results.json` contains the final pooled summary,
every measured sample and run, and driver/binary SHA-256 hashes. `environment.json`
adds production source hashes and the original runtime/fixture environment.
`verification-demand/report.json` lists the 55 final workflow checks;
`canvas-comparisons/results.json` records the exact original/candidate canvas
comparisons. The approximate lag diagnostic is retained beside each comparison
as `position-lag.json`. The nine final-cohort pixel comparisons are recorded in
`canvas-comparisons/final-cohort/results.json`.

## Second pass: presentation and real GPU experiments

The installed AMD device was accessible through `/dev/dri/renderD128`. Selecting
its RADV ICD alone did not prove that GPUI used it: that launch could fall back
to software OpenGL. With `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/radeon_icd.json`
and `MESA_VK_WSI_DEBUG=sw`, GPUI reported **AMD Radeon Graphics (RADV RENOIR)**,
driver `radv`, and `is_software_emulated: false`. This establishes real GPU
rendering with a software Xvfb presentation path. It still measures neither a
physical compositor nor display scanout. These environment overrides are for
this diagnostic setup and are not new application defaults.

In the same-binary control (one discarded warm-up and one measured launch per
configuration), software → verified hardware rendering changed median nudge
latency **59.7 → 44.0 ms**, p95 **61.5 → 46.2 ms**, drag throughput
**23.2 → 42.2 visible updates/sec**, and drag CPU **551% → 136%**. All three canvas
pairs matched exactly. This was the largest improvement in this pass, and it is
a rendering configuration difference, not a code optimization.

### Candidates and decisions

Each row is its own paired cohort, with discarded full warm-ups and rotating
launch order. Hardware rows use the verified AMD adapter unless stated otherwise.
Rows are not pooled with each other. CPU is expressed relative to one logical
core. The last two rows use reproducibly varied keyboard gaps; the other
application rows retain the earlier fixed-gap protocol.

| Candidate | Measured launches per variant | Median nudge, before → after | Faster drag updates/sec, before → after | Decision |
| --- | ---: | ---: | ---: | --- |
| Keep one transparent image in the atlas, software GPU | 2 | 65.8 → 60.4 ms | 20.4 → 21.4 | Reject: variable result, CPU increased |
| Publish the viewport as cached 256 px image tiles, software GPU | 2 | 59.6 → 59.0 ms | 24.5 → 25.0 | Reject: small change, CPU increased |
| Retain viewport pixels through Skia surface/snapshot copies | 2 | 43.7 → 43.7 ms | 41.5 → 42.1 | Reject: no visible latency gain |
| Copy already-opaque native BGRA bytes | 2 | 44.1 → 43.9 ms | 42.0 → 42.7 | Keep: independently demonstrated CPU copy saving |
| Request mailbox presentation | 1 | 43.3 → 43.6 ms | 42.0 → 43.5 | Reject: no latency gain, low repetition |
| Suppress repeated unchanged control updates | 2 | 43.5 → 43.6 ms | 39.9 → 39.2 | Reject: no measured benefit |
| Retain viewport bytes, avoiding intermediate Skia copies | 2 | 41.9 → 39.3 ms | 41.2 → 32.7 | Reject: drag regression |
| Wait for CPU canvas paint before preparing another frame | 2 | 39.5 → 38.6 ms | 39.5 → 28.6 | Reject: drag regression |

Declaring an actually opaque preview image opaque was also checked in three
paired CPU runs. It saved only about 0.3 ms at 936×734 and changed little at
1920×1080; the extra representation path was removed. The paint-paced experiment
preserved all commands and passed its queue/final-frame tests, but worse dragging
outweighed the lower CPU use. The previously verified consumer-paced mailbox is
retained. Tile publication matched the tested 1× canvas pixels, but did not prove
fractional-scale presentation parity and was not retained.

The retained BGRA helper checks the surface's actual alpha, layout, stride and
color tag. When all pixels are opaque and already tightly packed untagged BGRA,
premultiplied and straight-alpha bytes are identical; one native copy replaces
Skia's conversion. Skia's [pixel-opacity check](https://github.com/google/skia/blob/main/src/core/SkPixmap.cpp)
examines the stored alpha for BGRA, rather than relying on the image's declaration.
Other cases keep the original conversion. Three alternating
paired moving-preview microbenchmarks reduced median BGRA extraction plus
`RenderImage` construction **2.56 → 0.29 ms** at 936×734 and **7.96 → 1.79 ms**
at 1920×1080. The common preview rendering stage did not improve materially.
These measurements exclude application scheduling, upload and presentation; the
paired application test did not establish a material visible-latency improvement.

The final binary was checked again in three alternating paired moving-preview
runs, retaining all samples. Its median extraction/construction time was
**2.56 → 0.78 ms** at 936×734 and **8.21 → 2.31 ms** at 1920×1080. Every TIFF/native
comparison remained exact. The final binary's 936 px run medians varied from
0.32 to 1.39 ms; common rendering varied from 13.25 to 22.28 ms during the host's
changing load. This confirms the copy saving while reinforcing that these CPU
measurements do not establish a complete application speedup.

### Repeated hardware comparison of the final candidate

The completed fresh cohort used three measured launches per application, a full
discarded warm-up, rotating order, and varied keyboard gaps. Both Picsie binaries
reported the AMD hardware adapter on every launch. All 216 measured nudges and
18 measured drags passed the interaction assertions.

| Measurement | Prior verified renderer | BGRA-copy candidate | GIMP 3.2.6 |
| --- | ---: | ---: | ---: |
| Nudge → visible change, median | 44.2 ms | 43.6 ms | 30.6 ms |
| Nudge → visible change, p95 | 52.3 ms | 51.4 ms | 37.9 ms |
| Faster drag, visible updates/sec | 38.2 | 35.5 | 42.0 |
| Faster drag, median visible interval | 26.2 ms | 26.6 ms | 20.8 ms |
| Faster drag, p95 visible interval | 33.6 ms | 34.1 ms | 39.0 ms |
| Faster drag CPU, 100% = one logical core | 131% | 123% | 188% |
| Idle resident memory | 136 MiB | 136 MiB | 290 MiB |

This cohort does not establish a visible-latency gain from the copy change. Its
median drag throughput is lower, conflicting with the earlier paired copy test;
this prompted a further comparison of both paths in the same executable. Run
rates were 37.7–41.0/sec before and 35.5–40.5/sec after. The copy-stage saving is
established, but its complete application effect is not settled by these short
cohorts alone. Neither application meets the 20 ms p95 target in these measurements.

All twelve matching canvas pairs (three warm-up and nine measured) compared
exactly: zero differing pixels. The final hardware cohort and its source/binary
manifest are in `comparison-final-hardware/`; the earlier failed attempt is in
`comparison-final-hardware-interrupted/`. No completed run from the interrupted
attempt was inserted into this cohort.

### Isolating the copy in one executable

A separate desktop source copy adds only a canonical-conversion switch; a
temporary GPUI Linux source copy adds only a timer-interval override. Production
source, manifest, lockfile and the verified release binary remain unchanged.
Both variants in each cohort use the identical executable, driver, timer and
editor code; only the BGRA extraction path differs.

| Same-executable copy control | Measured launches per variant | Median nudge, canonical → copy | Faster drag updates/sec, canonical → copy | p95 nudge, canonical → copy |
| --- | ---: | ---: | ---: | ---: |
| Explicit 16.666 ms scheduling interval | 3 | 46.4 → 44.3 ms | 42.2 → 44.7 | 53.8 → 53.2 ms |
| Original scheduling interval, verified 16 ms | 2 | 46.0 → 44.3 ms | 42.0 → 43.2 | 53.3 → 54.0 ms |

These controlled cohorts did not reproduce the drag regression. Together with
the substantial CPU-stage saving and exact pixels, they support retaining the
copy. They do not prove a tail-latency improvement or invalidate the lower rate
in the three-application cohort. All cohorts and samples remain available; they
are not pooled or selectively substituted for each other.

The timer investigation also corrected a detail of the headless environment:
`gpui-pre-linux` 0.3.7 uses **16 ms**, not exactly 1/60 second, when XRandR reports
invalid timing. The diagnostic confirmed this value in the running Xvfb app.
Valid physical monitor timings take a different source path. The explicit
16.666 ms cohort therefore changes the headless scheduling clock and must be
reported separately from the original-timer cohort. Neither changes scanout.

The diagnostic source, original/patched backend hashes, and binary are in
`toolkit-ticker/`. The comparison records are `same-binary-copy-comparison/` and
`same-binary-default-copy-comparison/`. `PICSIE_CANONICAL_BGRA` and
`PICSIE_X11_FRAME_INTERVAL_US` exist only in that temporary source copy.

### X11 scheduling cadence probe

Two further measured launches per variant used that same executable and the
fast BGRA copy in both variants. Only the X11 scheduling interval changed:
the original 16 ms fallback → an explicit 8.333 ms interval. GPU presentation
mode, surface queue depth, input, viewport and rendering stayed the same.
This changes the frequency of toolkit refresh requests, not a physical monitor's
refresh rate or a guaranteed display frame rate.

| Scheduling probe | Original interval | Shorter interval |
| --- | ---: | ---: |
| Nudge → visible change, median | 43.2 ms | 39.9 ms |
| Nudge → visible change, p95 | 50.2 ms | 44.7 ms |
| Faster drag, visible updates/sec | 43.0 | 50.9 |
| Faster drag, median visible interval | 21.7 ms | 18.1 ms |
| Faster drag, p95 visible interval | 31.0 ms | 22.2 ms |
| Faster drag CPU, 100% = one logical core | 134% | 153% |

Both shorter-interval drag runs were 50.7–51.0 updates/sec; both controls were
42.5–43.5. All nine matching canvas pairs were exactly equal. The improvement
shows that scheduling cadence limits this virtual-display workload even with
real GPU rendering and faster extraction. It does not isolate every remaining
latency stage, nor establish the same benefit on a physical display. CPU rises
as more frames are prepared and presented. **The 20 ms p95 target remains unmet
for both visible intervals and input latency.**

No scheduling override or toolkit fork was retained from this cadence probe. Real
monitors normally provide valid timing, and forcing a faster timer without
measuring their presentation would be an unsupported production choice.
`ticker-gpu-comparison/` contains all measured samples, adapter/interval logs,
host resource records, source/binary hashes and exact canvas comparisons.
This is a concrete follow-up area for GPUI scheduling/presentation investigation;
an engine GPU rewrite is not needed to reproduce this particular gain.

### Toolkit diagnostics and host interference

GPUI's pinned WGPU backend requests two frames of maximum surface latency. A
temporary dependency copy under `artifacts/` tested a value of one and instrumented
CPU time in acquisition, render/submission, and presentation. No toolkit fork or
dependency change was retained from this second pass. Mesa's documented
[presentation-mode override](https://docs.mesa3d.org/envvars.html) supported the
separate mailbox experiment; it is not evidence of physical scanout timing here.

The first queue-depth comparison and first instrumented launch were inconclusive:
the host experienced severe memory stalls and full swap use, with failed input
assertions/timeouts. Their raw output and resource records are retained. A later
instrumented retry completed, but only one measured launch per setting: median
nudge 45.3 → 42.1 ms and drag 33.5 → 41.0 updates/sec. This is insufficient to
justify shipping a dependency patch.

Across that diagnostic's warm and measured draws, median surface acquisition was
0.04 ms, render/submission about 0.8–0.9 ms, and `present()` about 3.2–3.3 ms.
These are CPU elapsed times, not GPU timestamps or completion fences. They show
no sustained long CPU acquisition wait in those samples; they do not attribute
the remaining visible latency to one stage or rule out presentation delay.

The benchmark now records host CPU/memory/I/O pressure, available memory, swap and
load before and after the major phases, including failed runs. This makes host
interference visible without silently removing inconvenient samples. Our own
builds, tests, profilers and pixel comparisons were stopped during application
comparisons. Unrelated host workloads were not controlled.

The first attempted final three-application cohort also stopped on a GIMP nudge
timeout. At that failure, CPU pressure reached 53.7% and memory full-stall pressure
13.9% over ten seconds. The whole incomplete cohort is retained as inconclusive;
its completed Picsie run is not substituted into a later cohort.

### Reproducing this pass

`--jitter-nudges` uses seed `7391 + trial` and gaps of 80–147 ms after each visible
change, shared across applications within a trial. It samples more refresh phases
than the original fixed 100 ms spacing. Treat the two protocols as separate
cohorts. Inherited `PICSIE_*` switches are cleared; the variant's explicit
overrides are then applied. The driver saves the environment/binary/source
manifest before launching, so failed cohorts also retain their provenance.

```sh
python3 scripts/compare-desktop-experiment.py \
  --before artifacts/presentation-experiments/keeper/picsie-desktop \
  --after artifacts/presentation-experiments/final/picsie-desktop \
  --trials 3 --gimp --jitter-nudges \
  --before-env PICSIE_GPU_DIAGNOSTICS=1 --after-env PICSIE_GPU_DIAGNOSTICS=1 \
  --before-env VK_DRIVER_FILES=/usr/share/vulkan/icd.d/radeon_icd.json \
  --after-env VK_DRIVER_FILES=/usr/share/vulkan/icd.d/radeon_icd.json \
  --before-env MESA_VK_WSI_DEBUG=sw --after-env MESA_VK_WSI_DEBUG=sw \
  --before-env MESA_VK_WSI_PRESENT_MODE= --after-env MESA_VK_WSI_PRESENT_MODE= \
  --output artifacts/presentation-experiments/comparison-final-hardware
```

The `keeper/` comparator contains the first pass's verified renderer/mailbox,
adapter logging, and an optional atlas-keeper experiment that is **disabled** in
this command. It supplies adapter proof for the old rendering path. The `final/`
binary contains only the retained BGRA improvement and diagnostics in addition to
the first pass. No prototype switch or toolkit patch is enabled.

CPU transport checks use the release binary's `--measure-moving` mode. Native
workflow checks use `verify.py` with the final binary and the same staged tools
as the first pass. The source map identifies the
[local backend adaptation](compositor-port.md#native-bgra-extraction-second-experiment-pass).

The final source passed `npm run check:architecture`, `npm run check`, `npm test`,
`npm run test:bun`, `npm run check:desktop`, `npm run test:desktop` (10 tests),
and the locked desktop release build. Node ran 22 application/color tests; Bun
ran 19 actual-addon tests. The final binary passed all **55 real-window workflow
checks in both software Vulkan and verified AMD hardware configurations**.
Its layout and canvas-size-dialog screenshots were inspected. The moving CPU
checks and the alpha/layout/stride/color-space regressions all preserve exact pixels.

An additional AMD workflow launch was interrupted by SIGTERM after 46 passing
checks, including canvas editing, selections, masks, text, history, saving and
PNG export. Its termination cause was not established; it is not counted as a
completed verification. The process/output records are preserved separately.
A fresh AMD workflow retry completed all 55 checks with the same final binary,
default toolkit scheduling, and verified hardware adapter. Its report and
screenshots are in `verification-final-hardware-retry/`; the complete software
pass is in `verification-final/`.

Evidence is under ignored `artifacts/presentation-experiments/`: named candidate
binaries/patches, each `*-comparison/application-results.json`, the verified
`gpu-sw-presentation` control, moving CPU samples, and the temporary toolkit
sources/timing records. `prototypes-before-cleanup/` preserves rejected source;
`final/source.json` records the retained candidate's binary and production source
hashes. Results from interrupted or failed runs are retained with their limitations.

## Third pass: X11 frame wakeup

The next investigation kept the real AMD GPU, the retained renderer and full
viewport upload. It sought to close the remaining GIMP gap without forcing a
faster monitor timer or changing editor behavior.

### CPU candidate rejected

A separate source copy measured the existing preview stages while testing an
actually opaque document composite declared opaque to Skia. Three alternating
paired CPU runs, forty post-warmup samples per viewport/run, changed total
936×734 preview time only **13.24 → 12.97 ms**. The common baseline stages were
about 0.38 ms for background drawing, 6.24 ms for document recomposition/snapshot,
and 6.38 ms for scaled composite drawing. At 1920×1080, total time changed
9.30 → 9.10 ms. This small representation change was rejected; production
compositing and pixel conversion remain as in the previous retained candidate.
These are CPU timings, not application latency measurements. Source, executable
and all samples are in `artifacts/squeeze-experiments/opaque-composite/`.

### Same-executable wakeup control

The pinned GPUI `PlatformWindow` trait already exposes a frame-waker contract,
used by `WindowInvalidator` when a window becomes dirty. The published X11
backend did not implement it, so a finished canvas waited for the next periodic
refresh. A temporary backend source copy implemented the contract through a
coalescing calloop ping, delivering the ordinary GPUI frame request to a visible
window. The existing monitor timer, presentation mode and queue depth stayed
unchanged. Requests made inside a frame callback kept the periodic retry path,
preventing throttled animations from spinning immediate wakeups.

Two measured launches per setting, after full discarded warm-ups, used the same
executable with the wakeup disabled/enabled. Launch order rotated and keyboard
gaps used the existing seeded varied-spacing protocol. Every launch reported
the AMD RADV adapter with software emulation false. All interaction assertions
passed; all nine matching startup/settled-drag canvas pairs were exactly equal.

| Same-executable control | Original scheduling | Frame wakeup |
| --- | ---: | ---: |
| Nudge → visible change, median | 43.8 ms | 34.2 ms |
| Nudge → visible change, p95 | 59.3 ms | 57.6 ms |
| Faster drag, visible updates/sec | 39.6 | 51.0 |
| Faster drag, median visible interval | 26.1 ms | 18.1 ms |
| Faster drag, p95 visible interval | 33.7 ms | 24.7 ms |
| Faster drag CPU, 100% = one logical core | 128% | 160% |

The two candidate run medians were 34.5 and 34.1 ms, with 50.7–51.2 drag updates
per second. Input tails were uneven: candidate run p95 values were 66.1 and
37.3 ms, versus 51.4 and 63.3 ms for the controls. This establishes a repeatable
median/throughput improvement in this cohort, with increased CPU use, but does
not establish a comparable tail improvement. All samples and host-pressure
records remain in `demand-comparison/`; the uneven run is not excluded.
`demand-gated/source.json` records the exact diagnostic source and executable.

### Retained platform integration

The desktop now uses a local copy of the published `gpui-pre-linux` 0.3.7 crate.
Only its two X11 implementation files differ. The original normalized manifest,
Apache-2.0 license, original file hashes, readable patch and upgrade/removal
instructions are retained in
[the backend source record](../crates/picsie-desktop/vendor/gpui-pre-linux/PICSIE.md).
No toolkit/dependency version changes were made; the lockfile changes only that
crate's source from registry to local. The diagnostic environment switch and
CPU instrumentation are absent from production. Frame waking is active through
the existing toolkit contract.

Hidden windows ignore demand pings; mapping uses the existing refresh path.
Closing a window removes its ping registration. Requests within a frame use the
unchanged monitor timer. The standard draw callback and its reentrancy handling
remain authoritative. Other backend sources and all engine code are unchanged
in this third pass. This is local platform integration, not copied GIMP code or
translated Compositor editing behavior; see the
[source map](compositor-port.md#x11-frame-waking-third-experiment-pass).

The production source passed the architecture check, Rust/TypeScript check,
Rust/Node application tests (22 Node tests), actual-addon Bun integration
(19 tests), desktop check and all ten desktop worker tests. The locked release
build passed. Native verification passed all **55 workflow checks in both
software Vulkan and verified AMD hardware rendering**; layout and canvas-size
screenshots were inspected in both configurations.

The new `verify-frame-wakeup.py` passed four real-window checks on the AMD
adapter: hidden idle, remapping a pending edit, inactive idle, and recovering
input/canvas state after focus returns. Hidden CPU was 0%; inactive CPU was
0.49% of one logical core across two-second observation windows. These checks
cover visibility/focus recovery and detect a busy idle loop; they do not replace
physical-display testing. Reports, logs and screenshots are in
`verification-{hardware,software,lifecycle}/`. All temporary documents and
experimental source copies remain under ignored `artifacts/`.

### Fresh comparison with GIMP

The production candidate was compared with the previous retained BGRA-copy
build and GIMP 3.2.6. This is a fresh three-application cohort: one full discarded
warm-up per application, three measured launches each, rotating order, varied
keyboard gaps and the same viewport/document/input/observer protocol. No tracing
or prototype switches were enabled. All eight Picsie launches reported the AMD
RADV hardware adapter. All 216 measured nudges and 18 drags passed assertions.

| Measurement | Prior retained build | X11 frame wakeup | GIMP 3.2.6 |
| --- | ---: | ---: | ---: |
| Nudge → visible change, median | 43.4 ms | 35.1 ms | 33.5 ms |
| Nudge → visible change, p95 | 53.3 ms | 39.1 ms | 38.1 ms |
| Faster drag, visible updates/sec | 43.0 | 52.2 | 46.5 |
| Faster drag, median visible interval | 21.5 ms | 17.8 ms | 17.6 ms |
| Faster drag, p95 visible interval | 31.0 ms | 22.6 ms | 35.4 ms |
| Faster drag CPU, 100% = one logical core | 133% | 160% | 186% |
| Idle resident memory | 138 MiB | 138 MiB | 311 MiB |

The candidate reduced median keyboard latency about 19% and p95 about 27%;
drag throughput rose about 22%, with CPU rising about 21%. Its three keyboard
medians were 34.6–35.2 ms. The remaining median/p95 gaps to GIMP are small
relative to the observer's approximately 4 ms polling interval. This is evidence
of closing the gap in this workload, not proof that all interactions or
platforms have equal latency. Neither input p95 nor visible-interval p95 reaches
20 ms. These cohorts are not pooled with the earlier timer experiment or the
same-executable diagnostic.

Continuous dragging still trails GIMP in estimated position lag: **54.3 →
46.4 ms**, versus **24.9 ms**. This matches recorded blue-edge displacement to
the scripted triangular motion and has about 8.3 ms of motion per detector pixel,
plus observation delay. It is separate from directly measured keyboard latency.
The higher visible-update rate therefore does not establish equally prompt
continuous-drag tracking.

All twelve matching startup/settled-drag canvas pairs were exactly equal.
`final-comparison/` contains complete samples, adapter logs, manifests, exact
comparisons and estimated position-lag results. At recorded phase boundaries,
maximum measured-run CPU pressure was 1.89% and memory full-stall pressure 1.95%
over ten seconds. Unrelated host activity was not controlled and no runs or
samples were discarded based on pressure. Our builds, tests and other native
applications were stopped during measurements. This remains Xvfb presentation
with real AMD rendering, before physical compositor/display scanout.

The before binary SHA256 is
`240e314fa994b6e6a31940d8498a3e032b7d478d88ab04c04e58c57227111c0a`;
the retained candidate is
`001c2b54eb41c795624f7fcd9544a662ce3fb886baf5b5a7e73ea3febd4e27a0`.
`final/source.json` records production source and binary hashes.

```sh
python3 scripts/compare-desktop-experiment.py \
  --before artifacts/squeeze-experiments/baseline/picsie-desktop \
  --after artifacts/squeeze-experiments/final/picsie-desktop \
  --trials 3 --gimp --jitter-nudges \
  --before-env PICSIE_GPU_DIAGNOSTICS=1 --after-env PICSIE_GPU_DIAGNOSTICS=1 \
  --before-env VK_DRIVER_FILES=/usr/share/vulkan/icd.d/radeon_icd.json \
  --after-env VK_DRIVER_FILES=/usr/share/vulkan/icd.d/radeon_icd.json \
  --before-env MESA_VK_WSI_DEBUG=sw --after-env MESA_VK_WSI_DEBUG=sw \
  --before-env MESA_VK_WSI_PRESENT_MODE= --after-env MESA_VK_WSI_PRESENT_MODE= \
  --output artifacts/squeeze-experiments/final-comparison
```

### Additional canvas-request probe rejected

After the GIMP comparison exposed the remaining continuous-position lag, a
separate same-executable control tested explicitly scheduling a public
`Window::on_next_frame` callback whenever a new canvas arrived. Ordinary dirty
notifications wake only when a window becomes dirty; the extra callback was a
way to request presentation even if another notification had already dirtied
the window. Both variants kept the backend wakeup enabled, and only the
additional canvas request differed. Two measured launches per variant followed
full discarded warm-ups, rotating order and varied input gaps.

Keyboard median changed **35.0 → 35.7 ms**, p95 **36.3 → 48.8 ms**, drag updates
49.9 → 51.9/sec, and CPU 156% → 160% of one core. Estimated position lag changed
46.9 → 46.0 ms, much smaller than its approximately 8.3 ms motion-per-pixel
granularity. All nine canvas pairs were exact and every launch verified the AMD
adapter. This did not establish a useful latency/tracking gain, so the extra
callback and its diagnostic switch are not retained. The production source and
verified release binary remained unchanged; the earlier GIMP cohort remains
the final candidate comparison.

`explicit-demand/` records the diagnostic source/executable, and
`explicit-demand-comparison/` retains all samples, host-resource logs, exact
comparisons and position-lag estimates. No samples were selectively substituted
into the production comparison.
