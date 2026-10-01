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

## Fourth pass: feature polish and document scale

Measured 2026-10-01 after the layer, mask, color, selection, crop and transform
polish. The application executable is SHA256
`12a68642030bfb5ee67d1736fd2f6d8442454578688e7d950709071bcbdea14a`,
built from `6a10df3` plus the uncommitted polish changes. This audit changes
benchmark setup and documentation; the production application is unchanged.
GIMP remains a functionality/performance reference, never a UX reference.

The small editable demo is reasonably close to GIMP, but this does not establish
equivalent performance on image documents, masks or larger canvases. The audit
found substantial gaps in large raster movement, masked movement and materializing
feathered selection coverage. A temporary region-limited feather experiment
demonstrates a concrete optimization opportunity with exact pixels on the checked
fixtures.

### Comparable desktop workloads

Both applications use a 936×734 canvas. Raster controls load identical layer PNGs:
the 1200×800 six-layer fixture and its Rust-resampled 3600×2400 counterpart.
The large document is **8.64 MP**, not 12 MP. The mask control adds a linked,
all-white layer mask to Electric blue in both applications; GIMP uses an XCF
prepared through its public API, with the layer content active. Selections use
separate 1200×800 and 4000×3000 blank canvases.

Each completed desktop cohort has one full discarded warm-up per application,
then three measured launches in rotating order. Each application contributes
72 measured keyboard gestures and six four-second drags. The principal drag
sends 120 pointer events/sec through the same 240-screen-pixel out-and-back
trajectory. All displacement and settled-endpoint assertions must pass.
Percentiles pool the measured samples; update rates and CPU are medians of run
rates. No measured sample is removed because it is slow.

| Fixture | Picsie nudge median / p95 | GIMP nudge median / p95 | Picsie drag updates/sec | GIMP drag updates/sec |
| --- | ---: | ---: | ---: | ---: |
| 0.96 MP raster | 42.6 / 62.0 ms | 33.2 / 41.8 ms | 39.2 | 42.0 |
| 0.96 MP raster + mask | 56.8 / 59.0 ms | 31.9 / 37.7 ms | 23.2 | 43.5 |
| 8.64 MP raster | 147.5 / 201.1 ms | 90.1 / 153.8 ms | 5.7 | 14.0 |

The small-raster row is the complete fresh `move-raster-repeat/` cohort.

| Faster-drag resource/interval measurement | Small raster, Picsie / GIMP | Small masked raster, Picsie / GIMP | Large raster, Picsie / GIMP |
| --- | ---: | ---: | ---: |
| p95 visible interval | 31.2 / 38.8 ms | 48.0 / 42.2 ms | 223.2 / 86.9 ms |
| CPU, 100% = one logical core | 142% / 186% | 126% / 188% | 109% / 352% |
| Idle RSS | 222 / 286 MiB | 226 / 299 MiB | 339 / 426 MiB |

The prior retained executable was rerun against the current **editable** demo
in a separate three-application cohort. Its median nudge latency changed
35.7→39.5 ms, p95 40.4→50.9 ms and faster drag rate 49.7→39.2/sec. Current
continuous position lag remains about 47 ms versus GIMP's 29 ms in that cohort.
These estimated lags use fixed interior portions of the two motion branches;
one detector pixel represents about 8.3 ms of motion. They are not keyboard
latencies or GPU timestamps.

The first completed default-settings cohort counted deliberate snapping holds
as long visible intervals. Picsie's default ten-screen-point edge/center snapping
differs from GIMP's fixture setup. That cohort is retained as a default-behavior
observation, **not** used as a free-movement rendering comparison. Subsequent
cohorts disable Picsie snapping in an isolated preference directory. GIMP's
default bounding-box/grid/canvas/path snapping is off and the fixtures have no
guides. The maintained driver explicitly disables these options through
`default-view`; GIMP 3.2.6 ignores its older `default-snap-to-*` fields.

### Selection coverage and a bounded-region experiment

The CPU comparison creates a 600×400 selection at (100,100), verifies inside and
outside values, and obtains the complete native 8-bit mask. The comparison bypasses Picsie history and disables GIMP undo.
Picsie calls its existing Rust coverage APIs directly;
GIMP also pays for libgimp IPC and a full GEGL mask read. This is a CPU/native-mask
availability measurement, separate from desktop input latency. It does not claim
identical antialiasing or general algorithm parity. Five warm-ups and sixteen
measured samples per case, per launch, provide 48 measured samples across three
alternating launches per application.

| Complete mask availability, median | Picsie | GIMP 3.2.6 |
| --- | ---: | ---: |
| 0.96 MP rectangle | 2.1 ms | 2.7 ms |
| 0.96 MP ellipse | 1.4 ms | 2.8 ms |
| 0.96 MP inversion | 2.9 ms | 3.3 ms |
| 0.96 MP feather, Gaussian sigma 10 | 27.3 ms | 21.3 ms |
| 12 MP rectangle | 18.8 ms | 25.4 ms |
| 12 MP ellipse | 18.0 ms | 26.2 ms |
| 12 MP inversion | 42.2 ms | 30.9 ms |
| 12 MP feather, Gaussian sigma 10 | **358.9 ms** | **122.8 ms** |

The feather settings are **Picsie 20 and GIMP 35**, giving sigma 10 in each
implementation. The pinned `gimp_gegl_apply_feather` divides its public radius
by 3.5; comparing equal numeric slider values would compare different blurs.
GIMP bounds the feather operation to the channel's affected region.

Picsie's `selection_coverage` currently blurs a document-sized surface, reads
four RGBA bytes per pixel, then extracts alpha. Compositor's
`DocumentSelection.clip` already translates the path into its padded coverage
region. A temporary Rust harness tested that region idea using the existing
coverage implementation, then copied the result into a complete output mask.
In three alternating paired CPU runs, median 12 MP time changed
**400.8→13.7 ms**, p95 **496.8→15.5 ms**. At 0.96 MP, median changed 29.2→9.9 ms.
These paired timings are a separate cohort from the GIMP comparison above.

Every output byte matched the current engine for interior rectangle, edge
ellipse, full, inverted and off-canvas selections at both sizes: **30 exact
fixture comparisons** over three launches. This is an isolated experiment,
not production integration or a general selection-parity proof. Integration
still needs coverage of complex/combined paths, feather ranges and brush/fill/
clear/history consumers.

### Engine findings and priorities

1. **Bound selection coverage first.** It is a large measured cost with an
   upstream region-based implementation to follow and an exact local experiment.
   It affects feathered fill/clear and brush startup, not just the Feather control:
   changing feather metadata alone does not materialize the coverage.
2. **Improve large-document composition and CPU scheduling.** The desktop large
   drag uses 109% of one logical core in Picsie versus 352% in GIMP. Picsie's
   worker and raster composition remain serial. GIMP's retained projection and
   display regions provide relevant precedent; Compositor's tiled renderer is
   the implementation reference. More effective CPU use and bounded rendering
   are justified investigations; a GPU engine rewrite is not established as
   necessary by these results.
3. **Reduce mask/thumbnail work.** Masks and transformed stacks take Picsie's
   conservative full-composite path. Mask thumbnails also materialize a full
   mask on placement changes. The direct large CPU profile spends a median
   52.7 ms on masked thumbnails versus about 0.3 ms on ordinary thumbnails.
   Separate thumbnail/preview caches saved about 2 ms in the small profile, but
   large results varied and no application improvement was established. Source
   inspection also found that rendering an unmasked thumbnail can replace the
   masked layer's renderer-cache entry. This is a measured follow-up, not a
   reason to broaden partial redraw without pixel-equivalence checks.

High-quality fractional placement is another measurable cost: the direct small
preview profile is 24.6 ms for cubic High sampling versus 16.1 ms for linear Smooth
and 12.6 ms for integer High placement. The sampling control changes output
quality. No default was downgraded. This helps explain rendering variation,
but does not isolate the entire application regression to one component.

The direct preview harness uses three alternating launches at each size and
sixteen measured samples per case. Its expanded-crop stress case also moves a
layer each iteration, so it is not a standard crop-gesture comparison. No GIMP
crop/brush/distortion, physical-display, macOS/Windows or HiDPI performance parity
is claimed by this audit.

### Limits, reproduction and evidence

Picsie logs verified AMD Radeon Graphics (RADV RENOIR), `radv`,
`is_software_emulated: false` on each hardware launch. This is hardware rendering
with Xvfb presentation; it excludes a physical compositor and display scanout.
The external framebuffer poll requests about 4 ms intervals. Native Shift-arrow
step sizes differ between applications; the pointer trajectory is identical.

The host is shared. No builds, tests, profilers or additional editor instances
from this task run concurrently with timed application cohorts. Unrelated
workloads remain uncontrolled. Pressure records are preserved, including a
small-raster launch with CPU pressure 34.4% and memory full-stall pressure 11.2%
that raised pooled keyboard p95 to 131 ms. That entire cohort remains in
`move-raster-current/`. A new full cohort follows a recorded prelaunch readiness
check; its samples are kept independently, without pooling or value-based
exclusions. The fresh small-raster cohort recorded at most 6.26% CPU pressure and
1.99% memory full-stall pressure at phase boundaries; it is not a guarantee of
an uncontended physical display. Large CPU profile run medians also vary markedly with host pressure.
These short repeats are descriptive measurements, not confidence intervals.

The maintained harness supports the current Compositor layout, legacy layout,
matched fit zoom for larger fixtures, explicit free movement, prepared XCF masks,
and a Vulkan ICD override. The before/after driver keeps legacy layout defaults
for reproducing historical experiments and allows explicit per-binary layouts.

```sh
python3 scripts/compare-gimp-performance.py \
  --apps picsie-raster gimp --trials 3 --jitter-nudges --disable-snapping \
  --gpu-icd /usr/share/vulkan/icd.d/radeon_icd.json --software-wsi \
  --output artifacts/gimp-audit-repeat
```

For the large raster control, add
`--fixture artifacts/perf-audit-2026-10-01/large-fixture`. For the mask control,
use its `mask-fixture` directory and `--gimp-project` pointing to `demo.xcf`.
These fixtures were prepared through Rust and GIMP public APIs; no GIMP code
or fixtures were copied into the application.

All evidence is under ignored `artifacts/perf-audit-2026-10-01/`: completed
cohorts, setup failures/default snapping observations, every measured sample,
host-pressure records, binary/source/fixture hashes, a complete native source
snapshot, the CPU harness source and release executables, exact comparisons,
and screenshots. `coverage/`, `bounded-repeat/` and `preview-stages/` contain
the separate CPU results. Application screenshots were inspected. Setup pilots
with the old layout or active GIMP mask are retained as failed setup, not included
in completed cohorts.

## Fifth pass: twelve behavior performance coverage

Measured 2026-10-01 in response to the request to double the previous six
directly compared behaviors. This adds six behavior families, bringing the
coverage to **twelve**. It adds maintained benchmark tooling and measurements;
it does not integrate an optimization or change editor behavior. The production
desktop executable is the same SHA256 recorded in the fourth pass. Compositor
remains the UI and behavior reference; GIMP is a functionality/performance
reference only.

### Coverage inventory

| Behavior | Direct comparison | Fixture/control |
| --- | --- | --- |
| 1. Keyboard layer nudge | Fourth pass, native windows | Small raster, linked white mask, large raster |
| 2. Pointer layer movement | Fourth pass, native windows | Same three fixtures, identical out-and-back trajectory |
| 3. Rectangular selection coverage | Fourth pass, native CPU APIs | 0.96 MP and 12 MP, complete mask |
| 4. Elliptical selection coverage | Fourth pass, native CPU APIs | Same sizes and complete mask |
| 5. Selection inversion | Fourth pass, native CPU APIs | Same sizes and complete mask |
| 6. Feathered selection coverage | Fourth pass, native CPU APIs | Matched Gaussian sigma 10 |
| 7. Selection fill | Fifth pass, native CPU/public APIs | Hard rectangle and feathered rectangle |
| 8. Selection clear | Fifth pass, native CPU/public APIs | Hard rectangle and feathered rectangle |
| 9. Retained-content canvas crop | Fifth pass, native CPU/public APIs | Canvas shrinks to 75%; source pixels retained |
| 10. Image resize | Fifth pass, native CPU/public APIs | Half-size linear and nearest controls, output differences recorded |
| 11. Brush painting | Fifth pass, native CPU APIs and native windows | 100-document-pixel hard round brush, 61-point straight stroke |
| 12. Erasing | Fifth pass, native CPU APIs and native windows | Same diameter, trajectory, opacity and sample count |

Variants and document sizes are not counted as additional behaviors. This is a
coverage inventory, not a claim that every editor behavior, GIMP feature, or
Compositor polish requirement has been benchmarked. Crop/image-size dialogs,
lasso, floating-selection transforms, undo latency, filters and file operations
still lack direct matched performance comparisons.

### CPU processing and complete output availability

The new fixtures have one opaque RGBA raster layer: 1200×800 (0.96 MP) and
3600×2400 (8.64 MP). Both applications receive identical PNG pixels. Most of the
image is blue; a high-frequency checker occupies the lower-right quadrant,
outside the editing trajectory. Every timed edit uses a fresh editor/image with
history enabled. Fixture creation, selection setup, brush configuration and
validation are outside the timer. Five warm-ups and eight measured samples per
case in each of three alternating application launches give **24 measured
samples per application, case and size**.

The Rust workload calls existing `Editor::command` APIs, then renders and reads
the complete document RGBA output. GIMP uses its public API through libgimp,
then reads the complete canvas region from the only raster layer's GEGL buffer.
That one-layer fixture needs no additional GIMP projection composition. GIMP
timings include IPC; Picsie timings include its complete one-layer rendering
step. These are CPU command/output measurements, not screen latency or identical
renderer work. File encoding, UI and GPU presentation are excluded. OpenCL is
disabled in GIMP; no GEGL thread-count or cache tuning is applied.

| Complete output availability, median | 0.96 MP Picsie / GIMP | 8.64 MP Picsie / GIMP |
| --- | ---: | ---: |
| Selection fill, hard | 79.1 / 17.4 ms | 704.0 / 136.1 ms |
| Selection clear, hard | 37.0 / 16.9 ms | 316.1 / 135.7 ms |
| Selection fill, Gaussian sigma 10 | 108.1 / 34.0 ms | 921.1 / 242.7 ms |
| Selection clear, Gaussian sigma 10 | 65.3 / 34.2 ms | 592.0 / 243.9 ms |
| Retained canvas crop plus complete output | 11.1 / 3.2 ms | 107.9 / 22.7 ms |
| Half-size linear resize, differing output | 10.6 / 76.7 ms | 104.8 / 607.9 ms |
| Brush stroke, 61 control points | 662.4 / 47.3 ms | 3681.4 / 216.6 ms |
| Eraser stroke, 61 control points | 619.7 / 34.5 ms | 3609.5 / 181.6 ms |

Command-only results separate editing from output rendering/readback. At 8.64 MP,
hard fill is **543.4 / 36.7 ms**, clear **149.0 / 35.2 ms**, brush **3503.0 /
124.4 ms** and eraser **3454.5 / 89.7 ms**. The crop command itself is about
**0.01 / 2.5 ms**: Picsie's 107.9 ms complete-output result is mostly subsequent
rendering/readback, not a slow crop command. GIMP `image.resize`, rather than
its destructive `image.crop`, matches this retained-content crop control.

Feathered edits include coverage materialization inside the timer in both apps.
Picsie's feather metadata is set during setup; GIMP's eager `Selection.feather`
runs during the timed edit. Picsie 20 and GIMP 35 each mean Gaussian sigma 10,
as in the fourth pass. The selected rectangle is half the document width and
height, starting at (width/12, height/8). Filling uses opaque #e53935. Brush and
eraser use diameter 100, hardness 100%, opacity 100%, smoothing/dynamics off and
1.5% hard-brush spacing. Both trajectories run from width/12 to 7×width/12 at
height/2; the Rust editor receives all control points and the terminal event.

The faster linear resize is **not an equivalent-quality performance win**.
The two implementations produce different checker-edge pixels despite both
settings being named linear. The original 13-pixel checker nearest control also
differs at sample boundaries; its complete-output medians are 8.7 / 4.2 ms at
0.96 MP and 84.3 / 21.8 ms at 8.64 MP. Separate aligned-grid measurements and
pixel comparisons below distinguish restricted exact-output controls from
general resize behavior.

### Output conformance and the resize control

All timed samples pass dimension, interior-pixel and unchanged-exterior checks.
The retained-crop samples also preserve the original layer dimensions and
offsets; each Picsie edit commits one history step. GIMP undo is enabled, and
the live stroke tests exercise actual Undo. Complete first-sample RGBA probes
from both sizes are compared separately, ignoring RGB only when both pixels
have zero alpha. Near-transparent pixels also receive an alpha and
premultiplied-color comparison, so invisible RGB differences do not exaggerate
the displayed error.

| Output comparison | 0.96 MP differing pixels | 8.64 MP differing pixels | Interpretation |
| --- | ---: | ---: | --- |
| Hard fill | 0 | 0 | Exact on both fixtures |
| Hard clear | 0 | 0 | Exact visible color/alpha; transparent RGB ignored |
| Retained crop | 0 | 0 | Exact on both fixtures |
| Brush | 1,928 | 4,328 | Same interior/trajectory; differing rasterized edge output |
| Eraser | 923 | 2,123 | Same cleared interior; differing rasterized edge output |
| Feathered fill | 154,732 | 474,732 | Maximum visible channel difference 45/255 |
| Feathered clear | 139,900 | 427,900 | Premultiplied comparison; maximum alpha/color difference 9/255 |
| Linear half resize, 13-pixel checker | 2,212 | 19,768 | Maximum channel difference 33/255; quality not matched |
| Nearest half resize, 13-pixel checker | 2,124 | 18,986 | Different sample phase at checker boundaries |
| Nearest half resize, aligned 16-pixel checker | **0** | **0** | Restricted exact-output control |

The aligned checker puts each edge on the two-pixel downsampling grid, making
the nearest sample-phase difference immaterial on this fixture. It has a
separate directory and full three-launch, 24-sample-per-case cohort. Its
complete-output medians are **8.5 / 4.2 ms** at 0.96 MP and **88.6 / 21.6 ms**
at 8.64 MP; command-only medians are 5.3 / 2.8 and 53.4 / 11.7 ms. This confirms
a resize cost gap on that exact-output control, not general resampling parity.
The original linear and unaligned-nearest results are retained independently.
GIMP's feather/brush pixels are not adopted as new port semantics: Compositor
remains the target, and the report records these output differences explicitly.

### Live brush and eraser interaction

Both windows use a 936×734 visible canvas and matched fit zoom: 71.3333% at
0.96 MP and 23.7778% at 8.64 MP. The brush diameter remains 100 document pixels.
The driver sends a one-second, 61-point straight stroke at a requested 60 Hz,
covering half the document width, or 428 screen pixels, in both apps. Native
Picsie uses its hardware RADV renderer with Xvfb presentation; GIMP uses its
normal GTK/GEGL display path. Measured applications are untraced. A separate
untimed traced Picsie launch only locates and verifies the brush-size control.

There is one complete discarded launch per app/size, then three measured
launches in alternating application order. Each launch also discards the first
stroke per tool. Two measured strokes per tool/launch yield **six observations
per app, size and tool**, or 48 measured strokes overall. No timed sample is
removed for being slow. These small cohorts describe the observed runs and are
not confidence intervals or physical-display measurements.

| Live interaction, median | First visible paint, Picsie / GIMP | Endpoint after release, Picsie / GIMP | Visible updates during input/sec, Picsie / GIMP |
| --- | ---: | ---: | ---: |
| 0.96 MP brush | 56.1 / 30.0 ms | 91.2 / 18.8 ms | 21.5 / 56.5 |
| 0.96 MP eraser | 54.1 / 28.1 ms | 83.1 / 24.3 ms | 9.0 / 55.5 |
| 8.64 MP brush | **5630.7 / 26.2 ms** | **4635.0 / 29.5 ms** | **0.0 / 55.5** |
| 8.64 MP eraser | **4866.4 / 26.4 ms** | **3867.0 / 20.9 ms** | **0.0 / 54.5** |

At 8.64 MP, first-visible p95 is 5894.1 / 30.2 ms for brush and
5707.3 / 31.0 ms for eraser. Picsie shows no detected paint during the one-second
large-document input in any of the six measured strokes per tool. The final
stroke appears afterwards. CPU usage over input plus endpoint catch-up is
114% / 40% for the large brush and 116% / 36% for eraser, with 100% meaning one
logical core. GIMP is doing substantially less total work in this control;
more CPU parallelism alone is not a complete explanation or remedy.

The observer reads a horizontal framebuffer row halfway between the brush
center and its radius. It requires eight consecutive painted black or erased
neutral pixels, so a thin cursor outline cannot count as paint. An untimed dab
calibrates the rasterized tip edge separately for each app/tool; the completed
stroke must have the expected span and endpoint, and Undo must restore the
unpainted row. Polls request 4 ms spacing. This detects visible stroke progress,
not every draw/refresh or a universal application frame rate. The resulting
latencies end at the virtual framebuffer, excluding a physical compositor,
scanout and input-device hardware.

Actual median input spacing was 16.66 ms in both apps; framebuffer poll spacing
was 4.61 ms in Picsie and 4.33 ms in GIMP. The nominal GIMP canvas bounds include
GTK border/selection-outline pixels, with a four-screen-pixel allowance in the
startup extent check. The input trajectory and observed stroke span are matched
independently of those borders.

The last GIMP large launch initially retained an off-center scroll position
after resizing. Its calibration dab landed outside the image, so no timed
stroke began. That failed setup was retained; GIMP's pinned Shift+J Center Image
action was added before calibration. The driver resumed the missing launches,
keeping every completed launch and recording both source versions. This was a
setup correction, not an exclusion of a slow measured result. The maintained
driver additionally verifies the blue image extent at fit zoom before timing,
and refuses to resume a launch marked as having started measured input. All
completed startup screenshots are checked separately against the expected
image extent. Final brush/eraser screenshots in both applications were inspected.

### Findings and next experiments

The expanded coverage changes the priority order from the fourth pass:

**Required follow-up: expand performance testing further.** The six newly
compared behaviors exposed costs that the original six did not reveal. Do not
assume unmeasured features or feature combinations are fast. Extend the same
repeated, output-checked comparisons to feathered/combined selections; image
and mask painting with transforms and linked masks; floating-selection moves,
scales and rotations; undo/redo; gradients and filters; large layer stacks;
clipboard operations; and project open/save/export. Keep live interaction,
command processing and full-output availability distinct, and retain this
coverage inventory as that audit grows.

1. **Bound brush source publication and keep intermediate frames flowing.**
   `brush.rs::snapshot` allocates a complete source-sized RGBA image, copies the
   retained source and revisits covered pixels for each pointer sample.
   `editor.rs` calls it during every brush update. Sparse coverage tiles alone
   do not bound the publication cost. The desktop worker drains up to 4095
   queued requests before rendering; new requests can arrive while that loop
   runs. Slow brush commands can therefore postpone an intermediate preview
   until the input queue empties. This source path is consistent with the
   measured large-stroke delay; these measurements do not isolate every stage's
   contribution. Follow the pinned Compositor `BrushStroke.swift` dirty tiles
   and `BrushPatch` publication before inventing a different brush subsystem.
2. **Bound selection coverage and selected-pixel editing.** The earlier exact
   bounded-feather experiment remains useful, but hard fill is already slow
   without feathering. `render.rs::fill_selected_pixels` loops over source
   pixels and performs repeated transformed coverage/compositing work. Coverage
   optimization alone cannot account for the entire fill gap.
3. **Improve large-document rendering, then mask/thumbnail caching.** The
   movement and masked-document findings still stand. Crop shows how complete
   rendering/readback can dominate a cheap editing command. GIMP's paint core
   and projection use bounded regions, while Compositor supplies the port's
   tile/patch implementation reference. A GPU engine rewrite is not established
   as necessary by these results.

The source review uses Compositor pin
`609dbeae2ef68ef4fc82d67e4981a49852eb6e13` (`BrushStroke.swift`,
`EditorSession+Brush.swift`, `Selection.swift`, `SelectionEdits.swift`,
`IO/ImageResizer.swift`, their corresponding brush/selection/image-size tests)
and GIMP pin `e101dd19b165f927d3ba0a74658a71537c5661b9`
(`app/paint/gimppaintcore.c`, `gimpbrushcore.c`, `gimppaintbrush.c`,
`app/core/gimpdrawable-edit.c`, `gimpimage-resize.c`, `gimpimage-scale.c`,
and selection PDB tests). The workloads are local benchmark scenarios that
exercise existing implementations, not translated upstream fixtures. No GIMP
implementation or test fixture was copied into Picsie.

### Reproduction and retained evidence

```sh
cargo build --locked --release -p picsie-core --example performance_behaviors
python3 scripts/compare-gimp-behaviors.py \
  --output artifacts/gimp-behaviors-reproduction \
  --trials 3 --warmups 5 --samples 8
python3 scripts/compare-gimp-strokes.py \
  --output artifacts/gimp-strokes-reproduction --trials 3 --strokes 2
python3 scripts/compare-gimp-behaviors.py \
  --fixtures artifacts/gimp-aligned-fixtures \
  --output artifacts/gimp-nearest-aligned-reproduction \
  --grid 16 --case image-resize-half-nearest \
  --trials 3 --warmups 5 --samples 8
```

Stage the extracted GIMP runtime and X11 tools as described in
[the baseline measurements](gimp-performance.md), and build the native release
application first. The stroke driver uses the CPU driver's generated fixtures.
Run timed cohorts sequentially, without this task's builds/tests or additional
editor instances. Both drivers preserve samples, logs and setup failures, and
clean up their own application process groups.

Evidence is under ignored `artifacts/perf-behaviors-2026-10-01/`.
`cpu/` contains the eight initial workload variants at both sizes; `nearest/`
contains the original nearest-sampling control; `nearest-aligned/` contains the
restricted exact-output resize control; `strokes-v2/` is the completed
stroke comparison with an untimed brush-edge calibration. The earlier pilots
and `strokes/` endpoint-geometry setup failure remain separate and are not
pooled into completed cohorts. Source/executable snapshots, hashes, all sample
timings, host-pressure records, complete RGBA probes and native screenshots
make the measurements reviewable. No slow measured sample is discarded.

The host is the same shared Ryzen 5 5600U (6 cores / 12 logical CPUs) with about
7.1 GiB RAM and swap. The initial CPU cohort recorded maximum phase-boundary
10-second CPU pressure of 4.12% and memory full-stall pressure of 4.47%; the live
stroke cohort recorded 8.32% and 7.37%. The aligned nearest control recorded
0.35% and 0.23%. No unrelated workloads were stopped. These records limit claims
about exact ratios on an uncontended machine, but no slow samples were filtered.
No builds, tests or additional benchmark cohorts from this task ran during timed
measurements. Required checks run afterwards.

Verification passed: `npm run check:architecture`, `npm run check`, `npm test`
and `npm run test:bun`. This includes 168 core Rust tests, 16 desktop tests,
26 Node tests, 23 Bun addon tests and seven architecture guard tests. The
benchmark example release build, all three Python syntax checks, both driver
CLI checks and `git diff --check` also passed. The new cohorts contain 960
measured CPU edits and 48 measured native strokes; their actual content/geometry
assertions, twenty full probe comparisons and sixteen completed startup extent
checks are retained alongside the checks' logs in the artifact directory.

## Sixth pass: immutable raster and bounded pixel work

Measured 2026-10-01 after integrating changes in the production Rust engine and
GPUI desktop worker. This pass addresses the large image-painting delays and
selected-pixel costs revealed by the fifth pass. It repeats those workloads; it
does not add new behavior families to the twelve-family coverage inventory.
Compositor remains the source and UI reference; GIMP remains a functionality
and performance reference only.

### Integrated changes and reference fidelity

The engine now retains immutable source images and disjoint replacement patches,
following the pinned Compositor `RasterSnapshot.swift`, `BrushStroke.swift`,
`LayerRenderer.swift` and `EditorSession+Brush.swift`. A pointer update publishes
changed coverage tiles and shares unchanged pixels. Pointer-up commits the
snapshot without a complete source-image copy. Subsequent strokes retain a flat
patch set, with overlap subtraction; they do not chain previous complete images.
History accounts for shared native allocations, including Skia subset backing
storage. Existing `.picsie` and `.comp` persistence materializes pixels only when
the file consumer requests them.

Exact quarter-pixel tip phases can share a precomputed hard-brush tip. Other
phases keep the procedural sampler. A conservative interior/edge test skips
unnecessary hard-tip distance samples, and fully saturated coverage skips work
that cannot change its value. Provisional-tail restoration, spacing, smoothing,
opacity, every input sample and terminal events retain their prior semantics.

Selected-pixel fill/clear now edit intersecting 256-pixel tiles. Feathering blurs
only a conservatively padded selection region while retaining the complete mask
API and the original coverage bytes. Simple integer-position image stacks use
retained padded pieces for patched-image previews and source thumbnails. Ordinary
images retain the existing compositor, whose composed pixels seed unchanged
pieces on the first patch edit. Eligible cached pieces survive undo to ordinary
images; their recorded document drives damage checking on the next edit.
Deleted/replaced layer sets and unsupported scenes release them. Patched-image
moves invalidate old/new bounds
rather than the whole cache. Source thumbnails cache layer-local pixels and
apply canvas placement afterward, retaining source pixels during moves and
antialiasing the full source rectangle. Neighboring samples are retained at seams; caches have a 24 MP budget. Transformed/effected stacks,
masks and clipping keep the established conservative rendering path. Native mask
strokes still publish contiguous grayscale pixels.

The worker yields to pending presentation after eight milliseconds **between
requests**, or 4,096 requests. It preserves command order and all samples; it
cannot interrupt a single expensive command. The latest-frame mailbox still
bounds pending presentation. This is CPU engine work with the existing GPUI GPU
upload/presentation path, not a new GPU brush or compositor implementation.

Source attribution, adaptations and tests are in the
[performance port map](compositor-port.md#immutable-raster-and-bounded-pixel-work-performance-pass).

### Repeated CPU command and complete output comparisons

The unchanged fifth-pass fixtures, setup, history, sampling and output checks
are used: 0.96 MP and 8.64 MP, three alternating launches per app, five warm-ups
and eight measured samples per variant/size/launch. Each cell below summarizes
24 measured samples. The final cohort has nine variants, including the original
13-pixel nearest control: **864 measured CPU edits** across both applications.
No build, test or additional editor instance ran alongside a timed cohort.

Picsie complete output includes its full one-layer render and RGBA readback.
GIMP uses the public API and reads the same canvas region of its single layer;
its command time includes IPC. This remains a comparison of command/output
availability, not visible latency or identical rendering work.

| Complete RGBA output, median | 0.96 MP before / now / GIMP | 8.64 MP before / now / GIMP |
| --- | ---: | ---: |
| Hard fill | 79.1 / 27.5 / 16.3 ms | 704.0 / 203.3 / 128.7 ms |
| Hard clear | 37.0 / 26.9 / 16.3 ms | 316.1 / 196.9 / 128.2 ms |
| Fill, Gaussian sigma 10 | 108.1 / 40.4 / 33.9 ms | 921.1 / 273.6 / 235.2 ms |
| Clear, Gaussian sigma 10 | 65.3 / 36.9 / 33.8 ms | 592.0 / 260.7 / 233.5 ms |
| Retained canvas crop | 11.1 / 11.1 / 3.1 ms | 107.9 / 107.4 / 23.2 ms |
| Brush, 61 control points | 662.4 / 61.5 / 46.2 ms | 3681.4 / 248.1 / 207.4 ms |
| Eraser, 61 control points | 619.7 / 62.0 / 33.3 ms | 3609.5 / 248.5 / 166.1 ms |
| Half-size linear resize, differing output | 10.6 / 10.6 / 74.2 ms | 104.8 / 103.7 / 594.0 ms |

| 8.64 MP command only, median | Before | Now | GIMP |
| --- | ---: | ---: | ---: |
| Hard fill | 543.40 ms | 46.04 ms | 34.09 ms |
| Hard clear | 149.04 ms | 40.25 ms | 34.45 ms |
| Fill, Gaussian sigma 10 | 762.59 ms | 116.75 ms | 144.29 ms |
| Clear, Gaussian sigma 10 | 426.40 ms | 102.11 ms | 142.85 ms |
| Retained canvas crop | 0.01 ms | 0.01 ms | 2.43 ms |
| Brush, 61 control points | 3503.03 ms | 92.09 ms | 122.74 ms |
| Eraser, 61 control points | 3454.50 ms | 93.09 ms | 86.53 ms |
| Half-size linear resize, differing output | 71.31 ms | 70.76 ms | 584.15 ms |

The large brush command is now 38.0× faster, and the eraser command 37.1×
faster. Including complete output, the improvements are 14.8× and 14.5×. Hard
fill improves 3.5× and hard clear 1.6× including output. Feathered fill/clear
improve 3.4×/2.3×. These are before/after results on the same local workloads;
they do not establish equality with GIMP.

Complete-output rendering/readback remains a major cost. GIMP still leads on
these totals. Retained crop and image resizing were not optimized and remain
roughly unchanged in Picsie. Linear resize and the 13-pixel nearest checker
still produce different pixels from GIMP; their timings cannot establish an
equivalent-quality win. The fifth-pass aligned 16-pixel nearest control remains
the restricted exact-output resize comparison.

### Repeated native brush and eraser observations

The maintained X11 driver uses the same fixed one-second, 60 Hz trajectory,
100-document-pixel hard tip, fit zoom, endpoint calibration and Undo checks.
One entire warm-up launch per app/size and the first stroke per tool in every
launch are excluded. Three measured launches and two measured strokes per tool
give **six observations per app/size/tool**, 48 measured strokes total. These
are untraced app launches on AMD RADV RENOIR with real Vulkan rendering into
Xvfb. Physical compositor/scanout remains excluded. All sixteen completed launch
startup extents and all final endpoints pass the maintained geometry assertions.

| Native observation | 0.96 MP before / now / GIMP | 8.64 MP before / now / GIMP |
| --- | ---: | ---: |
| Brush: First visible, median | 56.15 / 30.50 / 25.85 ms | 5630.74 / 45.62 / 25.87 ms |
| Brush: First visible, p95 | 65.45 / 38.98 / 26.16 ms | 5894.06 / 56.05 / 30.41 ms |
| Brush: Endpoint after release, median | 91.15 / 58.42 / 27.35 ms | 4634.99 / 72.79 / 30.71 ms |
| Brush: Updates during input, median | 21.49 / 38.99 / 53.47 /s | 0.00 / 33.00 / 55.48 /s |
| Brush: CPU rate during input + endpoint wait, median | 162.90 / 142.03 / 26.29 % | 114.23 / 157.24 / 40.18 % |
| Eraser: First visible, median | 54.10 / 32.34 / 25.89 ms | 4866.39 / 47.55 / 25.85 ms |
| Eraser: First visible, p95 | 61.29 / 47.97 / 25.94 ms | 5707.31 / 48.21 / 26.39 ms |
| Eraser: Endpoint after release, median | 83.14 / 55.69 / 28.68 ms | 3866.95 / 68.03 / 29.19 ms |
| Eraser: Updates during input, median | 8.98 / 40.47 / 51.45 /s | 0.00 / 25.45 / 51.98 /s |
| Eraser: CPU rate during input + endpoint wait, median | 164.83 / 142.45 / 26.72 % | 116.30 / 156.86 / 32.10 % |

Large painting now produces intermediate visible updates during the input,
instead of waiting for the input queue to empty. Endpoint backlog drops from
seconds to tens of milliseconds. It still updates at roughly 25–33/s against
GIMP’s roughly 52–55/s on this control, and has higher process CPU rates. This is
a substantial recovery in usability, **not 60 FPS or a 20 ms p95 result**.
The p95 values above summarize only six stroke starts in each case; they are
not percentiles over every pointer event or a universal interaction guarantee.

Process CPU percentages use one logical core as 100%, measured over input plus
endpoint wait. The old multi-second wait and new shorter wait differ; compare
their absolute CPU work separately rather than interpreting the percentages
as identical-duration utilization. Output quality differences from the fifth
pass still apply. Actual median input/poll spacing and host pressure are
retained in the cohort records. Native brush/eraser screenshots in both apps
at both sizes were inspected.

| Visible interval during complete strokes | 0.96 MP Picsie / GIMP | 8.64 MP Picsie / GIMP |
| --- | ---: | ---: |
| Brush, median | 25.81 / 17.23 ms | 30.83 / 17.24 ms |
| Brush, p95 | 34.22 / 34.47 ms | 47.27 / 30.53 ms |
| Eraser, median | 25.89 / 17.34 ms | 38.98 / 17.34 ms |
| Eraser, p95 | 30.77 / 34.69 ms | 51.96 / 35.02 ms |

These intervals pool actual observed advancing stroke edges, including endpoint
completion. Large Picsie interval p95 values are 47.27 / 51.96 ms for brush/eraser, compared
to 30.53 / 35.02 ms in GIMP. Median input spacing is 16.66–16.69 ms across cases and
framebuffer polling is 4.31–4.36 ms; observation adds quantization and delay.
These are visible stroke updates, not every window redraw or physical frames.

### Movement controls and the regression they caught

The initial paired movement control uses the built-in six-layer editable demo,
whose Screen blend routes through the existing compositor. It showed roughly
unchanged nudge/drag behavior, but did not exercise the new simple-stack cache.
A further control uses the same rasterized six-layer assets and geometry with
all blend modes set to source-over. This is a local workload adaptation, not a
change to editor defaults or a translated upstream fixture. The full adapted
driver and fixture are retained in the artifacts.

That control exposed a nudge regression: **44.46 → 57.81 ms** median. Reusing
old/new bounds for tile damage reduced it to about 50 ms. Seeding patched
previews from the existing ordinary-image compositor left about 49 ms because
source thumbnails still repainted their moved pieces. Source-local thumbnail
pieces with canvas placement afterward removed that remaining cost. The
source-edge antialiasing regression found during that change is covered by
the contiguous-renderer comparison; early failed test logs remain retained.

Movement regression confirmation: three measured launches per executable, 24 measured nudges
per launch, identical seeded idle spacing and two out-and-back drag trajectories,
plus a whole warm-up launch. The faster drag is used below. No slow measured
launch was excluded from any candidate cohort.

| Simple raster movement regression confirmation | Before | After fix |
| --- | ---: | ---: |
| Nudge, median | 44.76 ms | 44.86 ms |
| Nudge, p95 | 46.30 ms | 46.40 ms |
| Faster drag updates, median | 40.25 /s | 40.24 /s |
| Faster drag interval, p95 | 30.99 ms | 31.08 ms |
| Idle resident memory, median | 225.61 MiB | 227.95 MiB |

This control precedes the final undo-cache-retention update; its exact
executables are recorded in its own manifest. Fresh ordinary scenes have no
patched cache to retain. The nudge difference is 0.10 ms, below the observer
granularity; drag
rates and their interval tails are comparable. This control does not establish
large-document, masked, transformed or post-paint movement parity.

A fresh old-executable large-stroke control also reproduces the delay: one
warm-up launch and one measured launch, two measured strokes per tool. Brush
first response/endpoint medians are 5023.0 / 4024.1 ms, and eraser is
5054.9 / 4055.3 ms. Both show zero advancing paint updates during
input. This small control corroborates the historical baseline and is not
pooled into the six-observation before/after table.

### Output conformance and verification

Every byte of all **18** final Picsie RGBA probes equals the corresponding
fifth-pass baseline probe, including arbitrary checker pixels and transparency.
Existing GIMP brush edges, feathered coverage and resampling differences remain;
this pass preserves Picsie output rather than changing its raster semantics to
match GIMP. All 18 final GIMP probes also exactly equal their fifth-pass baseline probes.
The existing cross-app full-probe differences therefore remain unchanged at
both sizes. Final GIMP pair comparisons and all per-sample geometry/content
checks are retained separately.

New local regressions compare bounded fill/clear against the original contiguous
algorithm with translucent pixels and transformed layers; hard tips against an
independent four-sample distance reference at arbitrary phases; and bounded
feather against the original full-canvas blur in **70 complete-mask comparisons**.
A preview regression additionally compares complete translucent pixels after
cache-boundary moves and restoration while verifying remote unchanged pieces
retain their identity. Thumbnail comparisons include translated and partially
off-canvas translucent sources, transparent patches and source-edge antialiasing;
they match the original contiguous renderer within one 8-bit channel step. The preview comparison also exercises a second patch edit after undo to an
ordinary source, validating retained pieces against complete rendering. Snapshot
tests exercise replacement transparency, shifted crops, immutable older
snapshots, shared-allocation accounting, persistence and one-step stroke undo.
A queued-pointer desktop test verifies an intermediate frame and exact final
pixels without dropping or reordering samples.

Retained preview pieces match the independent complete renderer exactly at unit
zoom. At fractional zoom, their local Skia sampling origins can produce at most
**1/255 channel rounding** in fewer than 1% of pixels in the test fixtures;
there are no visible seams. Full-output probes remain byte-exact. This is an
explicit presentation adaptation, not a claim of exact fractional-preview
raster equivalence.

Required checks passed: `npm run check:architecture`, `npm run check`, `npm test`
and `npm run test:bun`. They cover **177 core Rust tests, 17 desktop tests,
26 actual Node addon tests, 23 Bun addon tests and seven architecture guard tests**.
Both release builds passed. The final running native application passed **226
interaction checks** covering painting, selections, fill/clear, masks, transforms,
undo/redo, clipboard, native file dialogs, projects, reopening and quitting.
Native screenshots were inspected. Passing these checks does not promote
remaining Compositor polish gaps to complete.

### Rejected hypotheses and retained intermediate experiments

An isolated source-decoding experiment compared identical clipped tile draws
and RGBA reads using a lazy decoded image and an owned raster of the same 8.64 MP
PNG. After five discarded draws and 16 alternating measurements, medians were
**0.971 / 0.959 ms**, with every output byte equal. This does **not** establish a
useful isolated speedup. Skia’s nominal 32 MiB decoder budget is a soft limit;
the observed cache retained the 34.56 MB image. Memoizing owned decoded pixels
makes ownership explicit, but the total speedup is not attributed to that alone.

Early image-publishing candidates reduced first visible large-stroke response
to about 65–70 ms while leaving roughly 2.9 seconds of endpoint backlog. They
were insufficient. An untraced follow-up reduced that backlog to roughly 660 ms.
A subsequent diagnostic traced run, using cropped replacement patches, reached
about 48 ms first response and 77–80 ms endpoint delay. That traced run is kept
separate from the final untraced cohort; its timings are not pooled. Source
snapshots and failed/intermediate results remain available.

The final cache-retention ablation addresses a further first-response cost.
Clearing eligible viewport pieces on undo forced their recreation from the
ordinary compositor on each subsequent stroke. Keeping their recorded document
and validating damage before reuse reduced an untraced large-image pilot from
about 61 ms to 46 ms first response, with two measured strokes per tool.
The final repeated cohort uses this implementation; the pilot is separate.
Deleted/replaced layer sets and unsupported scenes release retained pieces.

### Remaining performance work and wider audit

Further performance testing is **required**, as requested. Six newly audited
behaviors revealed costs absent from the original six. The current twelve-family
inventory is not comprehensive, and correctness checks are not performance
measurements. Continue repeated, output-checked comparisons for:

- feathered, inverted, combined and raster selections, including clipped painting;
- image/mask painting with transforms, linked masks and multiple strokes;
- floating-selection movement, scaling and rotation;
- undo/redo after painting, transforms and selected-pixel edits;
- gradients, filters and large visible layer stacks;
- clipboard copy/cut/paste and project open/save/export;
- workflow combinations, cold/warm caches, long sessions and memory pressure.

Keep live response, visible update intervals, command processing and complete
output availability distinct. Prioritize the remaining visible-paint throughput
and full-output costs, then measured mask/transform bottlenecks. Retained crop
and equivalent-output resize remain explicit gaps. Test the conservative paths
before assuming the image-paint improvements cover them. A GPU engine rewrite,
physical 60 FPS, cross-platform performance parity or complete Compositor polish
is not established by this pass.

### Requested stress matrix: layer ordering, navigation and resize

Added at the user's request on 2026-10-01. The following is the coverage target,
not a claim that every combination has been measured. The
[seventh pass](#seventh-pass-layer-ordering-navigation-and-resize-stress) records
the completed CPU breadth, repeated resize controls, diagnostic experiments and
native navigation/order observations. Its remaining gaps retain the untested
variants below. Existing interaction assertions do not establish performance.

Use 50, 100, 300 and 1,000 layers with a moderate canvas and an 8.64 MP canvas;
add a near-24 MP fixture where the asset budget permits. Separate small scattered
assets from overlapping assets, and simple source-over stacks from stacks with
folders, masks, clipping, opacity, blend modes and supported adjustments. Record
total source/mask pixels, visible layer count and visible folder-row count: layer
count alone does not describe rendering or memory cost. Keep resize fixtures
within the separate aggregate resized-source and mask budgets. Test validation
at the budget boundary separately from successful operations.

| Requested case | Variants and correctness checks |
| --- | --- |
| Layer ordering | Adjacent swaps and top-to-bottom/bottom-to-top moves; single and multiple selections; into/out of folders; collapsed/expanded folders; row drag with autoscroll; Undo/Redo. Verify the exact stack/hierarchy, active selection and changed composite, including clipping/mask dependencies. |
| Layers panel | Scroll through hundreds of rows before and after reorder, with image/mask thumbnails visible. Compare collapsed and expanded folders and first-time versus revisited rows. Separate list/thumbnail response from canvas recomposition. |
| Zoom | Fit, 25%, 50%, 100%, 200% and 400%, including fractional zoom and repeated in/out sweeps. Exercise both scroll-at-pointer and existing buttons/shortcuts. Verify anchor preservation for scroll zoom, correct final viewport, retained detail and seam-free previews. |
| Pan | Slow/fast diagonal and horizontal/vertical drags at Fit, 100% and 400%; first visit to new regions versus revisiting cached regions; cross tile/image boundaries; repeat after paint and Undo. Verify the final offset and rendered region, including masks/transforms and off-canvas content. |

Resize is several distinct workloads in the current engine:

| Resize family | Required variants |
| --- | --- |
| Image Size / pixel resampling | Downsample to 25%/50% and upscale to 200% where budgets permit; noninteger ratios and odd dimensions; width-only/height-only and proportional changes; Nearest, Smooth and High. Include transformed/hidden layers, attached and independent masks, live text/shapes/gradients, and many layers. Measure commit, final output and Undo/Redo independently. |
| Canvas Size / crop | Shrink and expand, center and corner anchors, transparent and colored extension, mixed shrink/expand axes, retained off-canvas artwork and guides. Include many layers and Undo/Redo; verify retained source pixels and the extension layer rather than treating this as image resampling. |
| Layer scaling | Continuous corner/edge drag and typed dimensions/percent; free, Shift-proportional and Alt-centered scaling; already rotated/flipped layers; Nearest/Smooth/High; sparse and fully overlapping stacks. Measure live draft, Apply, Cancel and Undo separately. |
| Mask scaling | Linked masks following layer scaling and independent mask handles/numeric fields. Include dense painted masks, uniform masks and off-canvas placement; verify image/mask alignment and Cancel/Undo. |
| Floating-selection scaling | Small and canvas-sized floating pixels, hard/feathered coverage, rotation and source growth. Measure preview, Apply, Cancel and Undo, checking both extracted pixels and the remaining source. |
| Perspective/distortion | Image, floating-pixel and independent-mask corner/edge drafts; convex and folded shapes. Measure capped preview and full-resolution Apply separately, then Cancel/Undo. Group/text distortion remains unsupported and must not be recorded as a successful benchmark. |
| Text-box resizing | Long paragraphs and multiple styled runs; narrow/wide boxes and repeated handle drags. Measure text reflow and live preview, preserving glyph size and the final box. |
| Resolution-only | Change DPI/physical size with resampling disabled. Verify identical pixel resources and transforms; isolate metadata/history/preview cost. |
| Window / viewport resizing | Repeated native window resizing on the same complex document, with rulers, overlays and Layers panel visible. Verify layout and viewport behavior; measure preview/upload and UI layout cost separately from document edits. |

Current image sampling is Nearest, Smooth (linear) and High (Skia Catmull-Rom
cubic), not a promise of identical GIMP or Core Graphics kernels. Independent
mask placement still shares the bilinear adapter for Smooth/High. Use matching
output controls where available and report remaining pixel differences alongside
timings. Group/multi-layer transform boxes remain a feature gap, distinct from
supported multi-layer reordering and whole-document Image Size.

For every new family, retain repeated launch/input samples, cold/first-use and
warm/revisited results separately, and enough measured interactions to assess
tails beyond a few gesture starts. Record first visible response, advancing
visible intervals, endpoint/commit delay, command time, full-output time, process
CPU, resident memory and host pressure. Include paint → pan/zoom → reorder →
Undo/Redo workflows to exercise cache invalidation. Validate final document
state and pixels as well as screenshots; a fast stale frame is a failed result.
Use the same underlying raster assets and comparable geometry for GIMP controls,
retaining an editable Picsie fixture separately when rasterization changes work.
Compositor remains the UI/workflow reference; GIMP is a functionality/performance
control. Compare apps sequentially and keep tracing/builds/tests outside timing.

### Reproduction and retained evidence

Use the fifth-pass reproduction commands for the CPU and native stroke cohorts,
with fresh output directories. The final release executables and sources are
captured under ignored `artifacts/perf-fix-2026-10-01/final-source/`; the SHA256
manifest covers 106 source/configuration files and both executables. HEAD is
`6a10df36e7dcf1418734f0773d8bb7fb8899f6fb`, with the captured uncommitted changes;
HEAD alone does not identify these binaries. Final SHA256 values are
`b308091028296fa8017c882947cfba71302cf56aabf75edca8867ee50b6f2a74`
(native desktop) and
`faf37e29c18881287e65233d5acc37dcbdceadfda9e70f3f4ed9312d2dc6373a`
(CPU workload). Reference pins remain Compositor
`609dbeae2ef68ef4fc82d67e4981a49852eb6e13` and GIMP
`e101dd19b165f927d3ba0a74658a71537c5661b9`. GIMP contributes no code or UX.

The artifacts retain final CPU/native summaries, all samples, source/binary
hashes, host-pressure records, RGBA probes, output comparisons and screenshots.
`verification-final-retained/` contains required check/build results. The native
verification report contains all 226 actual interaction checks. Intermediate
`stage1-brush/`, `pilot/`, `profile/`, `profile-cropped/`, `cpu-cropped/`,
`cpu-final-before-movement-fix/`, `strokes-final-before-movement-fix/`, `cpu-final-before-retention/`,
`strokes-final-before-retention/`, `native-verification-final-before-retention/`,
`retention-pilot/` and saved
source/executable stages are separate from final cohorts. `profile/` has no trace
records and is an untraced pilot; `profile-cropped/` is diagnostic traced evidence.
The isolated decoder ablation is retained as `source-decode.json`. No slow
measured sample was discarded.

This is the same shared Ryzen 5 5600U, 6 cores / 12 logical CPUs, about 7.1 GiB
RAM and swap. No unrelated workloads were stopped. Final CPU phase-boundary 10-second pressure maxima are 2.20% CPU
some-stall and 4.16% memory full-stall; final native-stroke maxima are
0.12% and 0.36%. These records
limit exact ratio claims on an uncontended machine. Builds, tests, diagnostics
and additional editor instances ran separately from timed cohorts. The native
window observations use real AMD Vulkan through Xvfb, and exclude physical
compositor/scanout, macOS/Windows, Wayland and physical HiDPI validation.

### Seventh pass: layer ordering, navigation and resize stress

2026-10-01. Local generated stress fixtures and public-API controls, **not
translated upstream fixtures**. This pass measures existing behavior; it changes
no production algorithm, sampling default or UI workflow. Compositor remains the
UI/semantics reference, and GIMP remains a functionality/performance control.

The repeated resize, CPU ordering/navigation breadth and diagnostic cohorts have
completed. Native interaction pilots and final cohorts are recorded below;
raw checkpoints are retained under ignored `artifacts/perf-stress-2026-10-01/`.
This measures the main requested families, not every combination in the target
matrix.

#### Fixtures and measurement boundaries

There are 36 stack documents: 1200×800 and 3600×2400 at 50/100/300/1,000
layers, plus 6000×4000 at 300 layers; every size/count has simple/complex and
scattered/overlapping variants. The background is a full-resolution generated
raster. Other sources are 96×96. Overlapping variants also scale those sources
4×, so the scattered/overlap contrast changes **both placement and sampling
work**. Complex variants add folders, opacity, rotation, Multiply, supported
brightness/saturation/blur, raster masks and live clipping links. Layer counts
include folders. Source/mask pixel counts are in `fixtures/fixtures.json`.
A 24 MP canvas can have more than 24 MP aggregate input sources; the separate
aggregate resampling budget is checked before successful Image Size operations.

GIMP ORA assets bake affine placement into raster pixels in **all** fixtures,
including scaled simple stacks. Complex assets also bake masks and adjustments;
folders/opacity/Multiply remain, and live clipping links are omitted. Therefore
source pixel populations and live work differ, and complex controls are not
identical-output performance comparisons. Imported GIMP groups use Normal group
compositing, another difference from Picsie's pass-through group behavior.
The sampling/rasterization diagnostic
controls separate these adaptations from layer count. GIMP Layer Scale performs
raster resampling; Picsie retains its non-destructive transform metadata.

The first CPU breadth launch loads ORA directly. Later GIMP controls use a native
XCF cache to avoid repeated per-PNG importer process startup; this changes setup,
not the timed command. Four small pilot fixtures match their ORA imports in
complete merged RGBA bytes, thumbnail bytes and hierarchy/metadata signatures.
All 36 XCF fixtures also pass save/reload checks of complete merged RGBA bytes,
thumbnails and signatures. XCF opacity comparisons canonicalize to normalized
float32, matching serialization precision. Fixture preparation alone uses a
256 MiB tile cache; measured GIMP sessions retain the package's default cache.

`performance_stress` times existing commands, CPU preview/readback, Undo/Redo
previews and, for resize cases, a complete RGBA output after Redo. Command plus
preview is measured from the same start, rather than added from separate runs.
An editor is fresh each sample, while immutable source assets and the renderer
are retained across cases within each fixture. `first_preview_ms` is the setup
preview cost, which may be cold, warm or a recomposition after the preceding
case; it is not uniformly cold-load latency. A separate pre-transaction preview
and selection are used for Cancel assertions. Full output after Redo is a warm
output consumer, not commit or first-visible latency.

GIMP public-API command time includes libgimp IPC. Its merged thumbnail time is
reported separately; it is **not** a matched viewport, a complete output read or
screen latency. The single-image/mask controls use the same generated source
and a real dense GIMP layer mask. High uses Skia Catmull-Rom versus GIMP's Cubic
setting; other kernel/color-processing differences remain. No quality-equivalent
speedup is established merely by comparing command times.

The native cohorts observe XTest input and XGetImage canvas changes, with
untimed endpoint calibration and retained poll/input timestamps. Both apps have
a 936×734 canvas, resized to 836×674. Fit margins differ; 100% pan shares the
same screen trajectory. Native window resizing observes both canvas and window
boundary. Pan intervals include partial updates and are not a physical FPS
measurement. Picsie endpoints require complete byte equality with an untimed
reference. Accepted GIMP partial redraw/Undo calibrations exhibited at most one
channel step of variation, including at 1,000 layers; its endpoint matcher checks every
byte within that bound. Raw CRC changes are retained, while GIMP pan progression
excludes changes contained entirely within that bound. A compiled external
observer performs these comparisons, without changing either editor. CPU Redo
preview comparisons use complete byte equality. Client-window origins are
queried rather than assumed; layer-list wheel input lands on row names to avoid
mask-thumbnail tooltips overlapping the canvas. GPUI uses the existing
RADV hardware device, while presentation stops at Xvfb's virtual framebuffer.
No physical scanout, macOS/Windows/Wayland/HiDPI parity or universal 60 FPS claim
follows from these measurements.

Pan begins only after the held-button starting frame is quiet and matches the
reference, and validates that the endpoint remains restored after draining
pending work. First pan response is measured from the first posted motion;
held-button setup and readiness waits are untimed. Sparse/zero-update gestures
remain results, with absent first-response/interval values rather than invented
zero latency. Window resizing includes server boundary/crop changes; GIMP keeps
its image origin while Picsie recenters, so those cases do not perform identical
viewport work. A first boundary change alone is not an editor paint.

The corrected `navigation-scene` cohort recenters the 100% view on generated
overlapping artwork at `(width/3 + 216, height/3 + 216)` using an untimed native
pan before recording the shared trajectory. Shortcut zoom retains its normal
native center behavior. Layer-list/drop controls use Fit, ensuring the edited
artwork is visible. The original 24 MP default-center view showed background
only: its zoom/pan/window/list measurements remain background-view controls,
while its row-drop visibility timeout is an invalid observer setup, not proof
of an editor failure. The completed original 1,000-layer ordering data remains
separate; a navigation-only centered-scene cohort extends its pan coverage.

Application exits and failed endpoint checks retain the completed partial cases,
screen/host records and exit codes where available. The driver completes other
fixture launches and lists failed launches separately. Timing summaries use
completed, validated launches by default. The casewise analysis also retains
completed, validated rows from a later-failed or explicitly interrupted launch,
with counts and status reported; a later failure does not erase earlier slow
measurements. Failures are not silently retried or substituted.

Apps run sequentially, with alternating launch order. No Picsie build/test,
profiler or second tested editor runs alongside a timed cohort. This is a shared
host; pressure/load/memory snapshots accompany every run, and no measured sample
is removed for contention. CPU run RSS is sampled at 250 ms intervals across the app process group,
including live GIMP plug-ins and setup. CPU counters sum currently live group
members; exited plug-ins are not a cumulative process-tree CPU accounting.

The completed CPU/command inventory retains 955 case observations: 429 resize,
432 stack breadth, 64 moderate-stack repeats, nine clipping controls, six
sampling controls and 15 corrected GIMP DPI controls. Six original complex GIMP
DPI observations were no-ops and are excluded from true-change comparisons;
the other nine original DPI observations remain a separate thumbnail protocol.
These are case observations, not 955 distinct editor behaviors. Validation,
fixture preparation and driver pilots are separate from that count.

#### Repeated resize results

Three independent app launches, one measured edit per fixture/case per launch,
zero edit warmups, with setup previews excluded from command timing. All 77
Picsie and 66 GIMP cases passed, giving **429 measured case runs**. The following
are medians in milliseconds (`n=3` each); these are not reliable tail estimates.

| Fixture / operation | Picsie command | Picsie command + CPU preview | Picsie full output after Redo | GIMP command | GIMP thumbnail |
| --- | ---: | ---: | ---: | ---: | ---: |
| 8.64 MP, half-size Nearest | 25.3 | 71.7 | 29.6 | 13.6 | 21.1 |
| 8.64 MP, half-size Smooth/Linear | 43.4 | 86.3 | 31.5 | 713.6 | 21.5 |
| 8.64 MP, half-size High/Cubic | 103.3 | 144.2 | 31.4 | 773.1 | 21.0 |
| 8.64 MP + dense mask, half-size High/Cubic | 273.7 | 327.0 | 28.8 | 1,441.6 | 43.3 |
| 2.16 → 8.64 MP, High/Cubic | 363.9 | 499.4 | 154.2 | 405.8 | 45.0 |
| 8.64 MP layer, 150% scale | 1.1 | 423.7 | 468.1 | 1,022.9 | 51.0 |
| 8.64 MP, hard floating-selection scale | 121.3 | 248.1 | 140.4 | 254.1 | 56.6 |
| 8.64 MP, feathered floating-selection scale | 153.8 | 279.4 | 129.1 | 303.6 | 99.2 |
| 8.64 MP, convex distortion | 589.3 | 707.5 | 128.1 | 675.7 | 74.9 |
| 8.64 MP, independent-mask scale | 1.1 | 666.6 | 121.4 | — | — |
| 8.64 MP, independent-mask distortion | 364.1 | 541.0 | 117.1 | — | — |
| 8.64 MP, folded distortion | 566.4 | 686.0 | 127.7 | — | — |
| Paragraph box 800×600 → 620×650 | 0.08 | 57.8 | 10.1 | 14.6 | 14.4 |
| 8.64 MP, DPI-only | 0.53 | 11.5 | 154.4 | 0.21 | 16.3 |
| 8.64 MP / 100 complex layers, transparent canvas expansion | 1.8 | 4,165.0 | 4,560.7 | 20.6 | 341.8 |
| Same complex stack, colored canvas expansion | 785.0 | 5,742.9 | 4,810.0 | 128.4 | 391.4 |
| Same complex stack, half-size High/Cubic | 115.0 | 634.6 | 511.4 | 1,058.4 | 169.1 |
| 24 MP, half-size High/Cubic | 352.4 | 485.7 | 106.1 | 2,156.5 | 46.7 |

Additional cases cover quarter-size/odd/width-only resampling, centered shrink
and expansion, nonuniform layer scale, linked mask scale, Apply/Cancel, and
Undo/Redo. Four limit checks per Picsie launch reject oversized sides, over-24 MP
canvas area, aggregate resized sources and aggregate resized masks before
mutation; rejected operations keep the original document and zero history
entries. All committed edits add one entry; Undo restores the document signature,
Redo restores the committed signature and exact preview bytes. Cancel restores
pre-transaction pixels, selection and history. GIMP checks successful operations,
item positions/parents, dimensions, leaf counts and actual thumbnail bytes; the
text control verifies nonempty glyph alpha after resizing.

The GIMP DPI column uses the separate `resize-dpi-fixed` cohort: untimed explicit
72-DPI setup and a pre-edit thumbnail, timed 300-DPI change, final DPI/dimension
assertions and byte-identical thumbnail validation, repeated three times for all
five fixtures. PNG imports actually start at 72 DPI; the complex ORA/XCF images
start at 300 DPI. Consequently, the original complex GIMP DPI calls were no-ops;
those six samples remain retained and are excluded from true-change comparisons.
Other original DPI samples used a different thumbnail setup and are kept separate.

Peak process-group RSS across each entire resize launch (including setup) was
602–616 MiB for Picsie and 445–675 MiB for GIMP. These are group maxima sampled
at 250 ms, not a per-operation allocation comparison.

#### CPU ordering and navigation breadth

All 324 bounded Picsie cases passed. This is one measured sample per case, so
these figures identify expensive workloads rather than reliable tail latency.
Adjacent ordering changes only one layer position, while command-plus-preview
includes CPU compositing and readback. Navigation uses the retained render cache.

| Fixture | Adjacent command | Command + CPU preview | Cached 400% zoom | Pan into another region |
| --- | ---: | ---: | ---: | ---: |
| `1200-50-simple-scattered` | 0.6 | 41.9 | 25.7 | 21.2 |
| `1200-50-simple-overlap` | 0.6 | 427.2 | 22.6 | 20.8 |
| `1200-1000-simple-scattered` | 4.5 | 118.9 | 16.3 | 15.4 |
| `1200-1000-simple-overlap` | 4.4 | 6796.3 | 22.1 | 18.3 |
| `1200-1000-complex-overlap` | 8.4 | 10175.7 | 22.0 | 15.1 |
| `3600-1000-complex-scattered` | 15.5 | 21166.2 | 16.1 | 14.6 |
| `3600-1000-complex-overlap` | 16.1 | 27516.8 | 16.0 | 14.6 |
| `6000-300-complex-overlap` | 11.9 | 24349.1 | 38.7 | 15.4 |

All values are milliseconds. “Overlap” also means 4× scaling, while scattered
sources retain their original size. The contrast is therefore not an isolated
occlusion or placement experiment. Long moves, five-layer moves and folder moves
were also measured at every count on the 1200×800 canvas. On complex 300/1,000-layer
large canvases, the bounded run retains adjacent reorder and all six navigation
cases; additional long/multiple/folder moves remain a coverage gap there.

The large-stack command is small compared with recomposition. The existing
`render.rs::draw_document` clipping-stack branch creates full-document surfaces,
extracts/restores alpha and reads full RGBA buffers for each stack. In the pinned
Compositor source, `LiveMaskRenderer.swift::drawComposite` allocates at its `bounds`,
and `Rendering/EditorCanvas.swift:917` supplies `context.boundingBoxOfClipPath`.
GIMP's pinned `gimpprojection.c` aligns damaged regions to an update grid, validates
them in chunks and prioritizes the visible rectangle supplied by
`gimpdisplayshell.c:2169`. These are source observations, not measured attribution
of the entire slowdown. The scaled **simple** stack is also slow, so clipping
alone cannot explain the results. Diagnostic clipping and sampling controls are
recorded separately before choosing a production fix.

#### Clipping and sampling diagnostics

Each diagnostic has three measured launches, one adjacent reorder per launch,
with command and CPU preview measured together. Only the stated property changes;
these are causal controls with intentionally different pixels, not product fixes.

| Control | Original command + preview | Diagnostic median | Samples |
| --- | ---: | ---: | --- |
| 1200×800 / 1,000 complex layers; clear clipping links | 9,495.7 ms | 7,499.7 ms | Original 3, control 3 |
| 3600×2400 / 1,000 complex layers; clear clipping links | 27,516.8 ms | 6,879.5 ms | Original 1, control 3 |
| 6000×4000 / 300 complex layers; clear clipping links | 24,349.1 ms | 2,701.2 ms | Original 1, control 3 |
| 1200×800 / 1,000 simple scaled layers; Nearest instead of High | 6,745.6 ms | 1,924.7 ms | Original 3, control 3 |
| Same simple scene; baked world rasters, identity transforms | 6,745.6 ms | 1,802.5 ms | Original 3, control 3 |

The original moderate-canvas medians combine the breadth launch and two repeated
launches with the same CPU executable, assets and timed commands. The large
originals remain single samples; their ratios are diagnostic, not paired estimates
or confidence intervals. The clipping controls preserve masks, effects, folders,
opacity, placement and assets while clearing only `maskSourceId`. The result
supports the full-document clipping branch as a substantial large-canvas cost.

Nearest retains the same placement, scaling and source images. It establishes
that sampling matters, while the remaining 1.9-second redraw shows that reducing
sampling cost alone is insufficient. The baked control also increases source
pixel count from 10,166,784 to 148,652,160 and changes memory work; it is not a
memory-equivalent test. Complete native-addon PNG exports were decoded to RGBA8
and compared: 19,713 of 960,000 pixels differ, with maximum channel delta 2 and
mean absolute channel delta 0.006853125. This is small measured variation, not
exact output equality. The hashes and comparison are retained in
`sampling-quality/comparison.json`. A production optimization must retain the
requested High sampling and clipping behavior. No default or production code
changed in this pass.

#### Native ordering and navigation

These are input-to-validated-canvas medians from three fresh launches, not CPU
preview times. Normal adjacent ordering and zoom cases have four measured inputs
per direction per launch after two warmup cycles (`n=12`). Complex stacks with
300 or more layers have one measured adjacent reorder/Undo per launch and no
edit warmups (`n=3`); endpoint calibration remains untimed. Revisited panning
has two two-second gestures per launch (`n=6`); the table reports the median
observed canvas updates per second, not physical display FPS.

The count-only rows use 3600×2400 (8.64 MP); the 24 MP rows use 6000×4000.

| Overlapping fixture | Picsie adjacent reorder | GIMP adjacent reorder | Picsie Fit → 100% | GIMP Fit → 100% | Picsie revisited pan | GIMP revisited pan |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 50 simple layers | 490.4 ms | 57.7 ms | 65.5 ms | 18.4 ms | 33.5/s | 53.2/s |
| 50 complex layers | 1,698.3 ms | 65.9 ms | 66.7 ms | 16.6 ms | 32.0/s | 53.0/s |
| 100 simple layers | 850.3 ms | 88.9 ms | 91.6 ms | 17.9 ms | 24.5/s | 54.2/s |
| 100 complex layers | 3,402.9 ms | 68.9 ms | 101.4 ms | 17.2 ms | 23.0/s | 51.2/s |
| 300 simple layers | 2,337.9 ms | 245.9 ms | 221.3 ms | 17.7 ms | 11.0/s | 52.8/s |
| 300 complex layers | 9,386.3 ms | 106.0 ms | 244.3 ms | 16.9 ms | 10.0/s | 51.0/s |
| 1,000 simple layers | 8,037.8 ms | 990.0 ms | 1,157.8 ms | 17.1 ms | 1.0/s | 52.2/s |
| 1,000 complex layers | 44,507.1 ms | 153.0 ms | 1,281.8 ms | 16.7 ms | 1.0/s | 54.5/s |
| 24 MP / 300 simple layers | 2,724.7 ms | 242.4 ms | 226.4 ms | 17.1 ms | 11.0/s | 51.5/s |
| 24 MP / 300 complex layers | 32,276.1 ms | 113.8 ms | 247.2 ms | 16.8 ms | 9.5/s | 49.2/s |

GIMP's third 300-layer simple launch failed a later pan starting-frame check.
All twelve zoom/reorder measurements precede that failure and remain in the
table. Its revisited-pan count is four gestures across two launches, rather
than six; the failed launch's valid first pan also remains recorded separately.
GIMP's second 24 MP simple launch similarly failed a later starting-frame check;
its completed zoom/reorder and two pan gestures remain, giving five revisited
gestures and eight window-resize inputs per direction. Picsie's third 24 MP
complex launch ended after nine validated measurements: zoom has twelve inputs,
adjacent reorder three, Undo two, and revisited pan four. Those unequal counts
are retained rather than filling gaps with retries.

The 1,000-layer complex row uses the original Fit ordering controls and the
completed corrected navigation-only cohort for zoom/pan. All three original
ordering launches contribute validated rows: Picsie trial two ended after 26
checks; GIMP trial two was explicitly interrupted during finalization after all
21 checks. Neither is relabeled as passed. Picsie's adjacent Undo median is
44,817.2 ms, with a retained maximum of 132,447.9 ms. Both Picsie exits coincided
with substantial shared-host CPU/memory pressure, but their exit codes were not
available before cleanup; the causes remain unconfirmed.

| Retained native cohort | Launches | Passed | Other status | Validated observations |
| --- | ---: | ---: | --- | ---: |
| Nine fixtures, centered-scene pan and Fit row-drop | 54 | 51 | Two GIMP pan starting-frame failures; one Picsie exit | 1,511 |
| 1,000 complex layers, centered-scene navigation only | 6 | 6 | None | 114 |
| Original 1,000 complex layers, including ordering | 6 | 4 | One Picsie exit; one GIMP finalization interruption | 143 |
| Original 24 MP default-center background controls | 3 | 1 | One invalid row-drop observer setup; one incomplete launch | 59 |

The primary matrix retains 1,768 validated observations across 66 launches.
The casewise file also preserves the 59 separate background-view observations,
giving 1,827 retained rows overall. These are repetitions, not distinct behaviors.

All GIMP fixture adaptations described above still apply, including baked scaled
sources even in simple stacks. These are matched interactions and viewport
sizes, not a proof of equal source work or exact pixel output. The actual native
driver, compiled observer, poll/input timestamps, starting/endpoint pixels and
per-launch status are retained. `scripts/perf/summarize-stress-navigation.py`
summarizes each protocol independently and retains validated case rows from
later-failed/interrupted launches; it does not silently discard their slow cases.

#### Metadata diagnostics and checkpoint

At the user's 2026-10-01 checkpoint, all timed cohorts above are finished.
The new Rust diagnostic examples compile. Three fresh process launches measure
the actual `Editor::snapshot` and desktop `Snapshot::capture` paths, ten captures
per path/phase after two warmups. These are CPU metadata creation timings with
drop cost excluded, not UI or rendering latency. Desktop capture includes the
engine snapshot internally; do not add these columns or subtract unrelated
sample medians to estimate overhead.

| 8.64 MP fixture / phase | Rows | Engine snapshot median | Desktop capture median |
| --- | ---: | ---: | ---: |
| 50 simple, expanded | 50 | 2.0 ms | 2.1 ms |
| 300 simple, expanded | 300 | 54.7 ms | 55.1 ms |
| 1,000 simple, expanded | 1,000 | 609.5 ms | 613.7 ms |
| 1,000 complex, expanded | 1,000 | 676.8 ms | 675.7 ms |
| Same complex document, collapsed | 41 | 370.4 ms | 370.9 ms |

The same top folder stays selected in both complex phases; document signatures,
selection and history remain unchanged. Simple files have no folder-collapse
phase. The independently timed public-query components use the same fixture and
selection setup, three launches and ten captures after two warmups (`n=30` per
component). Their medians are not a decomposition whose sum must equal a snapshot.

| 1,000-layer fixture | Document metadata | Clipping candidate queries | Expanded-row clipping checks |
| --- | ---: | ---: | ---: |
| Simple | 10.0 ms | 263.4 ms | 270.5 ms |
| Complex | 9.6 ms | 320.6 ms | 270.2 ms |

This directly measures two expensive engine query loops, rather than attributing
all navigation delay to UI rows. `live_mask.rs::can_link` clones the entire
document and validates its graph for each candidate; `clipping_source` invokes
that query again from row eligibility checks. The pinned
`Document/LiveLayerMask.swift:23` rejects self-links and group/adjustment sources
and group targets before creating compact hierarchy records for validation.
Those source differences guide the next optimization; no query algorithm was
changed in this pass. Repeated query work, all-expanded-row UI construction and
large-document recomposition are separate costs to address.

Raw diagnostics are in `snapshot-controls/` and `snapshot-parts/`. The earlier
traced `list-diagnostic/` remains a pilot: its active child changed to a folder
between phases, so it is not a clean expansion control. The prepared
`picsie-list-diagnostic.py` selects the same folder before both phases and adds
traced leaf reorder/Undo, but **that updated native diagnostic has not run yet**.
The checkpoint source/binary freeze is complete in `final-source-v8/`.
Final full repository checks and final screenshot review remain follow-up work.
Architecture/check/test passed before
the native cohorts; that is not a claim of a final rerun.

#### Remaining stress coverage

The main requested families have CPU coverage; the native cohort concentrates
on shortcut zoom, 100% diagonal pan, adjacent ordering/Undo, list scrolling,
row-drag autoscroll and window resizing. It does not cover the full Cartesian
product of the earlier target matrix. Further performance testing remains
required, particularly:

- Pointer-wheel/pinch zoom, Fit/400% pan trajectories, off-canvas content and
  paint → navigate → reorder → Undo/Redo cache invalidation.
- Collapsed versus expanded folders, first/revisited thumbnail rows, and
  long/multiple/folder moves on the bounded complex large-canvas cases.
- Continuous native resize drafts, Shift/Alt, rotated/flipped layers, uniform
  and off-canvas masks, and canvas-sized floating selections.
- Height-only/mixed-axis resizing, anchor/guide combinations, retained crop,
  heterogeneous live text/shapes/gradients, styled text runs and long sessions.
- Quality-equivalent GIMP output controls, physical display cadence, Wayland,
  macOS, Windows and physical HiDPI behavior.

The registry retains these gaps without promoting implementation or polish
status based solely on performance tests.

#### Reproduction and retained evidence

The checkpoint snapshot is
`artifacts/perf-stress-2026-10-01/final-source-v8/` (158 files, including both
compiled metadata diagnostic examples). The corrected native protocol snapshot
is `final-source-v7/`; the original center-view
cohort uses `final-source-v6/`, and earlier CPU/protocol stages remain in
`final-source-v2/` through `final-source-v5/`.
The v7 manifest covers 154 source/configuration/binary files at HEAD
`6a10df36e7dcf1418734f0773d8bb7fb8899f6fb` plus the captured working tree.
The CPU executable SHA256 is
`e35f8c9cae311c65c2f49ef7ffe141ebbb7c9614c0a59b09ce168228f15c4dad`;
the unchanged native executable is
`b308091028296fa8017c882947cfba71302cf56aabf75edca8867ee50b6f2a74`.
Every cohort also records source, helper, fixture and relevant executable hashes.
HEAD alone does not reproduce the dirty working-tree binaries.

```sh
cargo build --release -p picsie-core --example performance_stress
python3 scripts/compare-stress-performance.py --prepare --output artifacts/stress-resize-new --mode resize --trials 3 --samples 1 --warmups 0
python3 scripts/compare-stress-performance.py --output artifacts/stress-stacks-new --mode stacks --bounded --trials 1 --samples 1 --warmups 0
python3 scripts/compare-stress-performance.py --output artifacts/stress-repeat-new --mode stacks --filter 1200- --cases reorder-adjacent --trials 2 --samples 1 --warmups 0
python3 scripts/prepare-stress-gimp.py --output artifacts/stress-xcf-new
cc -O3 -shared -fPIC scripts/perf/screen-match.c -o artifacts/perf-stress-2026-10-01/screen-match.so
python3 scripts/compare-stress-navigation.py --gimp-format xcf --pan-focus scene --row-drop-view fit --output artifacts/stress-native-new --trials 3 --samples 4
```

Use fresh output directories; `--resume` on the native driver requires identical
source, executable, fixtures and protocol. Failed incomplete launches are retained
with a suffix rather than overwritten. Preparation/import/correctness assertions
are outside the timed commands. The driver pilots, empty-filter error, incorrect
draft comparison, shortcut/ROI setup failures and font-control failures remain
retained as pilots; none are substituted for final measured samples.

## Eighth pass: layer-query and retained-projection rewrite (2026-10-01)

The user requested an architectural rewrite before another stress matrix run.
The seventh pass remains the baseline; its failures/partial runs are retained.
Pinned Compositor `LayerGroups.swift`, `LiveLayerMask.swift`,
`Rendering/LiveMaskRenderer.swift`, `UI/NativeLayerList.swift` and `LiveMaskTests`
were inspected. GIMP `gimpprojection.c` and `gimptilehandlervalidate.c` supplied
architecture evidence for retained damage/validation; no GPL code was copied.

The concrete faults were repeated raster-bearing document clones and whole graph
validation per menu candidate/row, position-based full invalidation on reorder,
full-document scratch/readback for every shared-alpha clipping stack, and eager
construction/layout of every expanded layer row. The rewrite adds a borrowed
ID/sibling/live-mask index, indexed source LRU, bounded retained clipping nodes,
dependency-aware reorder damage, fixed 256-pixel projection chunks, and sparse
premultiplied backdrop checkpoints every 64 atomic nodes plus a short final tail.
A later edit reuses only a prefix proved unchanged by old/new ordering and mask/
folder dependencies. Projection source indexing is prepared once, and node
supports cull work outside the current chunk. GPUI `uniform_list` creates visible
52-point rows using the existing interaction handlers and base scroll handle.

The first post-rewrite diagnostic (three samples of just the adjacent CPU case)
found 5,815.0 ms after bounded clipping alone: replaying hundreds of unchanged
scaled layers remained dominant. This evidence motivated the projection-prefix
part of the rewrite, before the native benchmark was run. Keep that intermediate
result at `artifacts/projection-rewrite-2026-10-01/cpu/results.json`; it is not the
final implementation. Metadata at that stage measured 13.0 ms desktop capture
expanded / 12.0 ms collapsed, versus 675.7 / 370.9 ms in the three-launch baseline.
Only the final version below is used for native interaction conclusions.

Verification includes translated Compositor soft-alpha, hidden-source, mask-chain,
release/adoption and Undo fixtures; local compact-record graph oracle/depth tests;
frozen pre-rewrite renderer comparisons allowing one premultiplied channel step
for transformed/blurred bounded scratch origins; and byte-exact fresh/retained
projection comparisons across chunks, reordered layers, masks, visibility,
deletion and restored state. A prefix-reuse test checks that a later hidden source
invalidates an earlier consumer. Larger straight-alpha RGB differences at very
low alpha can result from the documented one-step premultiplied rounding.
Sampling remains High; no source raster baking or quality reduction was used.
The complete projection still needs a cold full composition on first use, and
edits below the retained prefix can require replaying the whole lower stack.
Visible-priority/multiresolution projection remains future work.

Final evidence, source/binary hashes, logs and screenshots are stored under
`artifacts/projection-rewrite-2026-10-01/`. This is a focused acceptance check on
the existing 8.64 MP / 1,000-complex-layer fixture, not a repeat of the large matrix.
Broader behavior/performance testing remains required, including other reorder
positions, large masked folders, 24 MP and post-paint workflows. Native results
stop at Xvfb/RADV framebuffer observation; they are not physical display FPS.
GIMP's complex control has baked masks/effects/transforms and no live clipping,
so its performance is a functionality reference with the previous scope limits.

The single-prefix acceptance run passed all 17 native observations and reached
168.3 ms adjacent reorder, but also exposed 7,463.9 ms row-autoscroll drop and
5,896.5 ms Undo when the drop invalidated the late checkpoint. The final cache
therefore keeps earlier sparse checkpoints too. Empty ranges share immutable
images; unique bitmap pixels are counted once and capped at 24 million, evicting
older distinct images before the newest per chunk. Pixel budget/reuse tests
supplement the dependency tests. The intermediate native run remains under
`native-complete/`; only `native-sparse-final/` describes the final implementation.

Final acceptance results use `source-manifest-sparse-final.json`,
`cpu-sparse-final/`, `native-sparse-final/`, and the unchanged metadata path measured
in `metadata-complete.json`. One native launch passed 17 observations across 11
case labels. Zoom/window/list wheel cases have two measured observations; adjacent
reorder/Undo, first pan, list sweep and row drop have one, and revisited pan has two.
This is acceptance evidence, not a new three-launch median or a p95/FPS claim.

| 8.64 MP / 1,000 complex layers | Seventh-pass Picsie baseline | Final rewritten Picsie | Prior GIMP control |
| --- | ---: | ---: | ---: |
| Adjacent reorder, final observed frame | 44,507.1 ms | 158.3 ms (n=1) | 153.0 ms |
| Fit → 100%, final observed frame | 1,281.8 ms | 64.6 ms (n=2) | 16.7 ms |
| Revisited pan, observed updates/s | 1.0 | 21.5 (n=2) | 54.5 |
| Desktop metadata capture, expanded rows | 675.7 ms | 12.6 ms (10 captures) | Not measured |
| CPU adjacent command + viewport preview/readback | 27,516.8 ms (n=1) | 117.1 ms (n=3) | Not equivalent API |

Adjacent native reorder improves about 281× in this specific observation; CPU
reorder about 235×. The GIMP complex control remains baked, not an identical live
pipeline, and the baseline has more launches. Final native Undo is 148.9 ms, and
row-autoscroll drop is 901.6 ms with 740.7 ms exact Undo, down from 7,463.9/5,896.5 ms
in the intermediate single-checkpoint run. Autoscroll progression, list traversal
to bottom and exact return to top were validated. Wheel response is 22.0 ms;
traversal duration includes the driver's deliberate pauses and is not a pure
rendering time. Source/mask thumbnail slots, row alignment, footer and selected
row were inspected in final `fit.png` and `layer-row-autoscroll-dropped.png`.

Pan/zoom remain slower than GIMP. The first CPU setup render in the final sample
still requires a full composition; the JSON records that separately from warm
edit timings. Prefix storage remains capped at 24 million unique pixels; sparse
checkpoints increased final native RSS to roughly 576–581 MiB on this fixture.
The run began/ended with about 3 GiB available host memory and low measured CPU/
memory pressure. This does not establish behavior under the seventh pass's severe
memory-pressure failures. Earlier-stack, large masked-folder, scattered-layer,
24 MP, changed sampling, post-paint and repeated-launch coverage remain required.

Final validation: `npm test` passes 185 core Rust tests, 17 desktop Rust tests,
26 actual Node addon/application tests and 7 architecture guard tests; Bun's
23 actual addon tests pass. Local prefix/pixel/index tests are supplemental tests,
not additional translated upstream fixtures. Final repository checks and generated
registry validation are recorded alongside the benchmark logs.

`npm run check` (including architecture, source/reference/registry, TypeScript and
both Rust checks), `npm test`, `npm run test:bun`, and `git diff --check` pass on
the final sparse-checkpoint sources. The final source/binary hash manifest still
matches the workspace after verification; tested editor processes were stopped.

## Ninth pass: shared native viewport publication (2026-10-01)

The eighth pass made layer queries and clipping/reorder compositing practical,
but left the desktop rebuilding and deserializing every layer's JSON metadata
on each viewport frame. It also revisited all thumbnails, performed quadratic
membership checks, and rebuilt unchanged inspector choices. This pass profiles
those phases, replaces that publication path coherently, then measures the final
implementation. No rendering-quality or viewport-semantics changes are involved.

Pinned Compositor `CanvasViewport.swift`, `EditorCanvas.swift::synchronizeDisplay/draw`,
`NativeLayerList.swift::Coordinator.update` and applicable existing transform/guide
fixtures were inspected. Compositor keeps viewport state separate and reuses
unchanged cells/assets. Pinned GIMP display scroll/scale code was inspected as a
secondary functionality/performance reference, including its retained display
pixels and cached scale values. No GIMP implementation was copied or translated.
The new typed publisher is a Rust transport adaptation, not a literal Swift port.

### Implementation and compatibility

`editor/publication.rs` now constructs read-only native metadata directly in
Rust. A worker-owned `SnapshotPublisher` shares document metadata, the ID index,
rows, mask-source candidates and current text data across unchanged publications.
It always refreshes viewport, cursor, selection feedback and transient state.
The existing addon/QuickGUI serialized contract is unchanged.

Validation compares the actual document, including uncommitted edits, instead
of history revision or a list of commands. Every scalar participates; immutable
content/mask/stroke identities avoid walking source pixels or stroke samples.
Exhaustive model destructuring makes a newly added field require an explicit
cache decision. Rows also track collapse state; candidates track the selected
target independently. The thumbnail producer shares an unchanged collection,
indexed membership removes quadratic reconciliation, and the desktop skips
unchanged thumbnail and inspector updates. Rulers still update with the viewport.

Six supplemental local tests compare native metadata with the prior serialized
contract, including image/mask payload omission, shared viewport publication,
live edits, Undo/deletion/direct mutation, collapsed-target capabilities,
session controls and pixel-resource replacement. They are transport regressions,
not newly translated upstream fixtures. Existing translated rendering, viewport,
mask, transform and guide tests remain applicable.

### Worker phase experiment

`performance_frames.rs` drives the production worker without GPUI. It separates
cold composition from warm command/render/readback/thumbnail/snapshot phases.
Each case has two warmups and ten recorded samples. Document metadata/history
invariants are checked outside timing. Baseline and final executables are retained
with source/binary hashes in `artifacts/navigation-publication-2026-10-01/`.

| 8.64 MP / 1,000 complex layers, warm worker phase | Before publication rewrite | After |
| --- | ---: | ---: |
| Pan metadata capture | 15.84 ms | 0.150 ms |
| Pan thumbnail bookkeeping | 3.29 ms | 0.153 ms |
| Pan render | 5.75 ms | 5.22 ms |
| Pan frame availability, excluding GPUI/display | 25.42 ms | 6.06 ms |
| Fit → 100% frame availability | 25.16 ms | 5.21 ms |
| 100% → Fit frame availability | 30.85 ms | 11.31 ms |

The standalone metadata diagnostic selects the top folder, unlike the worker's
selected leaf. It measures a retained publication around 0.100 ms expanded /
0.109 ms collapsed, and one-off typed capture around 1.20 / 1.09 ms. Legacy JSON
snapshot capture remains around 11 ms. These separate phase medians must not be
added or subtracted as a decomposition. Cold first composition still takes
seconds; this pass does not implement visible-priority or multiresolution rendering.

### Native interaction cohorts

The focused cohort uses three fresh launches per app, three recorded observations
per zoom/resize/list-wheel case per launch, and the existing 100% two-second pan
trajectory. All three Picsie launches pass: **66 observations**, including exact
reorder Undo, list traversal and row-drag/autoscroll Undo. GIMP completes two full
launches and eight earlier validated observations in its remaining launch:
**42 observations** in total. That launch fails an **untimed** pan-start assertion;
its screenshot/status shows a Move gesture rather than pan. All completed rows
and the failure are retained. No failed result is silently removed or retried.

A first ORA setup was interrupted during untimed GIMP import of 1,000 separate
PNG layers, before any timed sample. Final GIMP controls use the already validated
native XCF fixtures containing the same baked assets. Initial loading is outside
interaction timing. The interrupted setup and final cohorts remain separate.
GIMP complex controls retain their prior limitations: masks/effects/transforms
are baked, live clipping is absent, and group behavior differs from Picsie.

| Focused 1,000-layer complex file | Eighth-pass Picsie (one launch) | New Picsie (three launches) | Fresh GIMP control |
| --- | ---: | ---: | ---: |
| Fit → 100%, observed final frame | 64.6 ms | 40.8 ms (n=9) | 57.4 ms (n=9; 16.6–123.1 ms range) |
| 100% → Fit, observed final frame | 70.3 ms | 42.7 ms (n=9) | 93.7 ms (n=9; 29.9–151.9 ms range) |
| Revisited pan, observed updates/s | 21.5 | 47.5 (n=6) | 50.5 (n=4; 26.4–55.0 range) |
| Adjacent reorder, observed final frame | 158.3 ms | 151.0 ms (n=3) | 1,140.3 ms (n=3; 149.3–1,637.0 ms range) |
| Row-autoscroll drop, first visible result | 901.6 ms | 910.9 ms (n=3) | Not matched |
| Row-autoscroll exact Undo | 740.7 ms | 732.4 ms (n=3) | Not matched |

GIMP's focused results vary substantially, with host load/memory pressure recorded
in its manifests (including approximately 1.34 GiB available and load 10.2 at the
failed launch's end). These measurements do **not** establish Picsie beating GIMP
at zoom or reorder. Historical cleaner GIMP controls remain approximately 17 ms
zoom and 153 ms adjacent reorder; later broad zoom controls below remain near
17 ms. All valid slow observations, ranges and failure states are preserved.

A separate broader navigation cohort uses one launch per app for four more files,
three zoom/resize observations each, and three pan trajectories (first visit plus
two revisits). GIMP uses its supported middle-button pan, which enters the same
scroll path directly without temporary Space-tool activation. The input protocol
is recorded in the driver manifest, and this cohort is not pooled with the focused
Space cohort. **All eight launches / 120 observations pass.**

| Broader file | Picsie Fit → 100% | GIMP Fit → 100% | Picsie revisited pan updates/s | GIMP revisited pan updates/s |
| --- | ---: | ---: | ---: | ---: |
| 8.64 MP / 100 simple overlapping layers | 48.1 ms | 16.7 ms | 47.9 | 48.7 |
| 8.64 MP / 1,000 complex scattered layers | 38.6 ms | 16.8 ms | 49.9 | 50.2 |
| 24 MP / 300 complex overlapping layers | 47.8 ms | 17.6 ms | 51.9 | 43.7 |
| 24 MP / 300 complex scattered layers | 38.2 ms | 16.6 ms | 54.4 | 44.7 |

All native timings stop at Xvfb framebuffer observation on RADV RENOIR with software
WSI presentation. They are observed changes, **not physical display FPS**, a 60 Hz
p95 guarantee, or a complete stress-matrix claim. The original complete-pixel
endpoint checks are retained: Picsie exact bytes, GIMP at most one channel step.
The broader runs cover window shrink/grow as well as navigation. They do not
cover every earlier-stack reorder, post-paint large scene, 400% trajectory,
off-canvas crop preview or physical display configuration.

### Native phase diagnostic and UI verification

A separate traced native zoom diagnostic on the 1,000-layer file records three
samples per direction after two warmups. Tracing serializes metadata before paint;
its times are not pooled with untraced comparisons. At 100%, median snapshot
capture is 0.157 ms, thumbnail UI reconciliation 0.0003 ms, state application
0.0196 ms and total frame acceptance 0.0457 ms. Queue-to-UI-paint is about 30.7 ms,
while CPU rendering is about 6.0 ms. Fit records similar acceptance costs and
about 11.8 ms rendering. This points to remaining delivery/frame scheduling
between the worker and UI paint; it does not isolate an exact GPUI subsystem or
measure physical presentation. Further work should investigate that boundary,
not assume more metadata caching will close the remaining zoom gap.

The full real-window suite passes **226 checks**, including layers/masks/colors,
text, selection, numeric transforms, image distortion, crop, history, clipboard,
file dialogs and minimum-window controls. Actual `32-transform-controls.png`,
`27-layers-masks-colors.png` and the focused 1,000-layer `fit.png` were inspected:
fields, thumbnails/mask slots, selected rows, panel/footer and transform handles
remain aligned. This workflow suite uses Linux X11/software Vulkan and is separate
from measured RADV performance launches. No broad cross-platform polish claim is made.

Reproduction tools: `performance_frames.rs`, `performance_snapshot.rs`,
`compare-stress-navigation.py` (including `--gimp-pan`), and
`perf/summarize-stress-navigation.py`. Raw samples, partial results, hashes,
commands/logs, screenshots, native diagnostic script and comparison summaries are
retained under `artifacts/navigation-publication-2026-10-01/`.

Final validation: `npm test` passes **191 core Rust tests, 17 desktop Rust tests,
26 actual Node addon/application tests and 7 architecture guards**. Bun passes
23 actual addon tests. `npm run check`, generated registry validation, the
standalone architecture guard and `git diff --check` pass. Tested production
source/binary hashes remain matched after verification. Benchmark/verification
applications are stopped; source quality and the legacy command contract remain
unchanged. Remaining performance work includes delivery/frame scheduling, cold
first composition, broader earlier-stack/post-paint workloads and physical-display
percentile measurement.
