# QuickGUI and GPUI Kit performance comparison

Measured 2026-09-29 after the GPUI UI parity implementation. These are fresh release
builds of both complete applications using the same current `picsie-core`, not the
small initial GPUI experiment. No editor behavior or application scheduling was
changed for this comparison.

A subsequent [bounded optimization pass](desktop-optimization.md) records fresh
GPUI before/after measurements and addresses the idle CPU regression identified here.

On this workload, GPUI reduces visible input latency and memory use, starts faster
with warm caches, and presents more updates during a drag. Idle CPU is higher.
The isolated transport improvement is larger than the application-wide gain.

## Complete application results

AMD Ryzen 5 5600U, Linux x86-64, X11/Xvfb, Mesa **software Vulkan**, 1280×860 windows,
936×734 canvas, the six-layer 1200×800 demo document fitted identically in each app.
The startup canvas crops were pixel-identical: ImageMagick absolute error **0**.

There were three measured launches per application, alternating order, after one
discarded complete warm-up run per application. Values below are medians across
runs except input latency, which pools 72 measured key events per application.

| Measurement | QuickGUI 0.1.6 | GPUI Kit 0.7.0 | Change |
| --- | ---: | ---: | ---: |
| Warm-cache launch → first artwork | 786.8 ms | 422.8 ms | 46% less time |
| Nudge → visible change, median | 123.0 ms | 90.8 ms | 26% less latency |
| Nudge → visible change, p95 | 126.0 ms | 95.3 ms | 24% less latency |
| Visible updates during a layer drag | 15.75/s | 19.25/s | 22% more updates |
| Idle resident memory (RSS) | 297.1 MiB | 198.7 MiB | 33% less |
| Resident memory after nudges and drag | 627.4 MiB | 219.1 MiB | 65% less |
| Idle proportional memory (PSS) | 282.6 MiB | 184.7 MiB | 35% less |
| CPU during drag, 100% = one logical core | 688% | 570% | 17% less CPU time |
| Idle CPU, 100% = one logical core | 0.33% | 3.31% | GPUI uses more |

Measured run ranges: startup 779–799 ms versus 417–425 ms; drag updates
15.75–16.00/s versus 18.00–19.50/s; memory after interaction 625–629 MiB versus
213–220 MiB. This is a short repeatability check, not a statistical confidence interval.

Startup numbers are specifically **warm-cache** results. The initial discarded
runs took 785 ms for QuickGUI and 929 ms for GPUI. OS caches were not purged, so
neither is a controlled cold-start measurement.

The memory figures are process-group totals and exclude the X server, benchmark
driver, and unmapped temporary frame files. Memory after interaction was sampled
one second after the final drag settled. The difference is observed residency,
not proof of a leak or a long-session memory ceiling.

## CPU transport results

The existing release `--measure` harness was run three times independently of the
application tests. Each round uses five warm-up iterations and 40 measured samples
per viewport, alternating raw/TIFF order. Table values are medians of the three
run medians. All decoded pixels matched in every round.

| Viewport | Shared engine preview | Old TIFF encode/publish/read/decode | Native BGRA + RenderImage | Transport saving |
| --- | ---: | ---: | ---: | ---: |
| 936×734 | 23.67 ms | 7.95 ms | 2.38 ms | 5.57 ms / 70% |
| 1920×1080 | 19.06 ms | 24.98 ms | 7.79 ms | 17.19 ms / 69% |

The demo is fitted independently at each viewport, so this is not a resolution
scaling test. TIFF decoding in this isolated harness uses Rust's `image` library
as a proxy for the QuickGUI loader. The full-application results above exercise
the actual QuickGUI host. Transport timings exclude Node/FFI, UI scheduling,
GPU upload and presentation. The [initial experiment](gpui-kit-experiment.md)
describes the transport path in more detail.

## Method and reproduction

```sh
npm run build
npm run build:desktop

# Run sequentially, with other app instances and builds closed.
crates/picsie-desktop/target/release/picsie-desktop --measure
python3 scripts/compare-desktop-performance.py

# This workspace uses locally extracted X11/Mesa tools:
python3 scripts/compare-desktop-performance.py \
  --tools artifacts/selection-history/tools/usr
```

The application harness uses XTest keyboard and pointer input. It detects the
left edge of the blue ellipse through `XGetImage` on a single scanline, polling
every approximately 4 ms. Timing begins before dispatching a Shift+Arrow nudge
and ends when the changed pixels are observed. After four discarded nudge
warm-ups, each run records 24 alternating right/left nudges. Expected movement and
return to the initial position are asserted.
The detector compares against each application's own initial edge position. GPUI
opens 2 px to the right in this Xvfb session; that constant window offset does not
affect the relative nudge or drag distance. Canvas pixel equality was checked in
window-relative screenshot crops.

The drag supplies 480 pointer motions over four seconds: a 120 px move out and
back at a requested 120 samples/s. Recorded input times confirm all six measured
runs took 4.0002–4.0005 seconds; median motion spacing was 8.37–8.43 ms. The
reported update rate counts distinct visible ellipse-edge positions during that
interval. It does not count unchanged screen refreshes or internal render calls.

Startup ends on the first visible blue artwork pixel, before the later screenshot
and focus operations. Idle CPU is sampled over three seconds after startup has
settled. CPU and memory come from `/proc` for the application process group,
including worker threads. Native tracing is disabled in both apps. App and Xvfb
processes are stopped after each run. No user project is opened or saved.

The repeatable harness is [compare-desktop-performance.py](../scripts/compare-desktop-performance.py).
Evidence is under ignored `artifacts/performance-comparison/`:

- `application-results.json`: summary, all latency samples, visible transitions,
  input times, CPU and memory observations.
- `transport-1.json` through `transport-3.json`: isolated transport results.
- `environment.json`: compiler/runtime versions, binary sizes and SHA-256 hashes.
- Per-run directories: screenshots, application logs, and complete measurements.

The QuickGUI executable was rebuilt with Bun 1.4.0 and its release native addon;
the GPUI executable uses the pinned desktop lockfile and Rust
`1.99.0-nightly (73dc9167f 2026-08-01)`. The working tree contains the UI parity
implementation; the recorded Git base alone does not identify the tested binaries.

## Interpretation and limits

The Rust UI removes the TIFF/file/decoder handoff and shows a measurable benefit
in the actual application. Engine rendering still costs about 24 ms in the normal
viewport transport test. The UI migration does not accelerate brushes, filters,
or compositing by itself.

GPUI's 8 ms mailbox polling is a plausible contributor to the higher idle CPU;
attributing the cost requires profiling. That is a useful next optimization target.

These timings end at Xvfb's framebuffer. There is no physical GPU, desktop
compositor, display scanout, or human input-device delay in the measurement.
The software renderer also consumes multiple CPU cores during interaction.
The results do not establish hardware-GPU frame rates, 60 fps behavior, brush or
large-document throughput, battery usage, or macOS/Windows performance.
