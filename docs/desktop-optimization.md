# Bounded GPUI optimization pass

Measured 2026-09-29 against the saved release binary from `ea53d64`, using the
same Linux X11/software Vulkan environment and demo as the
[initial comparison](desktop-performance.md). This pass changes worker wakeups
and a narrowly guarded renderer fast path. It adds no editor features or GPU
image-processing implementation.

## Results

Fresh paired runs of the GPUI application before and after this pass, with one
discarded warm-up and three measured launches per version in alternating order.
Input latency pools 72 key events per version; other application values are
medians of the three runs. These are independent reruns, so the before values
differ slightly from the earlier QuickGUI/GPUI report.

| Measurement | Before | After | Change |
| --- | ---: | ---: | ---: |
| Idle CPU, 100% = one logical core | 3.64% | 0.66% | 82% less |
| Input → visible change, median | 88.7 ms | 75.4 ms | 15% less latency |
| Input → visible change, p95 | 94.2 ms | 78.9 ms | 16% less latency |
| Visible drag updates | 19.25/s | 20.25/s | 5% more |
| Warm-cache launch → first artwork | 418.8 ms | 401.9 ms | 4% less time |
| Idle RSS | 198.5 MiB | 199.2 MiB | Essentially unchanged |
| RSS after nudges and drag | 218.2 MiB | 215.0 MiB | Essentially unchanged |
| CPU during drag, 100% = one logical core | 574% | 577% | Essentially unchanged |

The three after runs used 0.33–0.66% idle CPU, compared with 3.31–3.64% before.
Drag rates were 19.74–20.50/s after versus 19.00–20.00/s before. The drag gain is
modest and the ranges overlap; this pass does not establish a substantial
continuous-drag throughput improvement. These short runs are not battery tests.

The isolated preview benchmark also ran three paired rounds, each with five
warm-up iterations and 40 measured samples per viewport. Values below are
medians of run medians:

| CPU stage | Before | After |
| --- | ---: | ---: |
| Preview at 936×734 | 23.63 ms | 15.82 ms (**33% less**) |
| Preview at 1920×1080 | 19.04 ms | 11.46 ms (**40% less**) |
| BGRA/RenderImage transport at 936×734 | 2.38 ms | 2.38 ms |
| BGRA/RenderImage transport at 1920×1080 | 7.74 ms | 7.81 ms |

The demo is fitted independently to each viewport. These figures do not describe
resolution scaling. Transport still extracts and uploads a full viewport; the
transport implementation was not changed in this pass.

## Changes and correctness

The UI now awaits a bounded asynchronous notification channel instead of waking
every 8 ms to poll the worker. Notifications follow publication of frames,
completions, and errors. A pending wake covers all unread results; pointer commands
and file outcomes retain their existing reliable queues. Startup failure wakes
the UI too, and closing the worker ends the notification stream.

Release profiling separated document composition from the full preview. The
1200×800 gradient background alone cost about 8.8 ms to composite, despite neutral
brightness and saturation. Skia's identity color matrix was sending this simple
copy through its floating-point color pipeline.

The renderer omits that matrix only for opaque rectangle/gradient fills with
neutral adjustments, full opacity, normal blending, integer placement, unit
scale, and no rotation, flips, masks, strokes, parent, or preexisting canvas
transform. Every other draw retains the previous path. In the stage profiler,
the background fell to about 0.53 ms and document composition fell from about
15.94 ms to 7.64 ms.

A broad neutral-filter bypass was rejected because it changed rounding on
translucent pixels. Two local regression tests exercise **448 exact-pixel
comparisons** against the previous identity-matrix path, including alpha ramps,
all sixteen blends, sampling modes, blur, transforms, and opaque/translucent
fills. They are backend regression cases, not additional upstream fixture ports.

Verification passed:

- `npm run check`, `npm test`, and `npm run test:bun`.
- `npm run check:desktop`, all **9 desktop tests**, and the release build.
- All **55 real-window workflow checks**, including painting, masks, selection,
  text/controls, file operations, dirty close, and quit.
- Before/after full-window startup and post-drag captures have **zero differing
  pixels**. The native verification startup canvas also matches the prior parity
  capture exactly. Screenshots were inspected.

## Reproduction and evidence

Save the pre-change release binary before rebuilding. Then run without concurrent
builds or other editor instances:

```sh
cargo run --release -p picsie-core --example profile_preview
npm run build:desktop

artifacts/optimization/before/picsie-desktop --measure
crates/picsie-desktop/target/release/picsie-desktop --measure

python3 scripts/compare-desktop-performance.py \
  --baseline artifacts/optimization/before/picsie-desktop \
  --baseline-name gpui-before --candidate-name gpui-after \
  --tools artifacts/selection-history/tools/usr \
  --output artifacts/optimization/comparison

python3 crates/picsie-desktop/verify.py \
  --tools artifacts/selection-history/tools/usr --software \
  --output artifacts/optimization/verification
```

The benchmark's measurement loop is unchanged; its CLI now accepts named baseline
and candidate binaries. The original `--quickgui` and `--gpui` arguments remain
aliases, and its default comparison remains QuickGUI against GPUI.

Raw results, screenshots, build/test logs, and stage profiles are under ignored
`artifacts/optimization/`. `comparison/application-results.json` contains every
input/visible transition and resource measurement. `transport-before-*.json` and
`transport-after-*.json` retain the three paired transport runs; `environment.json`
records binary hashes and environment details.

These are software-renderer measurements ending at Xvfb's framebuffer, not
physical-GPU or display-scanout timings. The CPU preview gains apply to documents
that contain eligible fills; they do not establish a general brush/filter speedup.
Larger renderer changes should follow representative document workloads and
physical-GPU profiling. This completes the bounded pass; feature work can resume.
