# GPUI Kit experiment

Date: 2026-09-29. User authorized evaluating Rust for the UI and running this experiment.

The prototype demonstrates that GPUI Kit can host Picsie's existing engine and
present its frames from native memory without modifying the toolkit. It is a
working Linux application, not a component gallery. Source and reproducible
commands are in [experiments/gpui-kit](../experiments/gpui-kit/README.md).

The experiment has since been promoted to [the desktop application](../crates/picsie-desktop/README.md).
This report preserves the initial experiment results; current UI coverage is tracked
in [the parity record](gpui-ui-parity.md).

## Scope and versions

- `gpui-kit = 0.7.0`, `gpui-base = 0.7.0`, `gpui-component = 0.7.0`, and the kit's
  matching `gpui-pre = 0.3.7`; dependencies are recorded in a separate Cargo.lock.
- Existing `picsie-core`, including the unchanged Compositor-derived brush,
  transform, history, and rendering behavior. No GIMP UX or code is introduced.
- GPUI Kit provides buttons, the text field, sliders, and resizable panels.
- The experiment adds application layout, event-to-command mapping, a worker/mailbox,
  native image presentation, and measurement/verification tools.
- No toolkit forks, renderer patches, JavaScript buffers, or per-frame image files.

## What was exercised

The real application was launched on Linux under Xvfb with Mesa lavapipe. This is
**software Vulkan**, so the run establishes integration and interaction correctness,
not physical-GPU performance. The automated script drives xdotool and checks engine
state after real UI events. Screenshots were inspected at 1280×860 and 960×640.

Passed checks:

1. GPUI Kit Input: select-all, typing, Enter commit, focus return, keyboard undo.
2. Layer selection, movement, Shift-proportional resizing, undo.
3. Brush stroke input, one history entry per gesture, keyboard undo/redo.
4. Opacity-slider live changes and one history entry on release.
5. Pan displacement, wheel-up zoom direction, and cursor-anchor preservation.
6. Resizing the inspector and the native window updates the canvas dimensions.

Evidence: `artifacts/gpui-kit/verification/verification.json`, its screenshots, and
`frames.jsonl`. The script cleans up the application and X server it launches.

## Native presentation

The engine worker renders the same CPU Skia preview as QuickGUI. Skia reads straight
BGRA into a Rust vector, which is moved into GPUI's `RenderImage`. GPUI uploads the
image through its native renderer. Old images are explicitly dropped from its atlas.
The last completed image stays visible until a replacement is available.

This removes TIFF encoding, file publication/read, decoding, and the JavaScript/Node
bridge from this prototype's preview path. It **still** extracts and uploads a full
viewport buffer. Compositing still rebuilds the engine preview; brush snapshots still
have their existing full-image assembly costs. There is no shared GPU texture or
dirty-tile display update in this experiment.

The UI polls a single-frame mailbox every 8 ms. The worker drains commands in order;
coalescing presentation never discards pointer samples or release/cancel events.
This is prototype scheduling, not a finalized frame scheduler.

## Measurement method

`--measure` uses the same demo document and native renderer for both paths. Five
warm-up iterations precede 40 samples, and transport order alternates. It measures:

- Common engine preview rendering, separately from transport.
- Native BGRA extraction plus constructing `RenderImage`.
- The production `frame_bytes` TIFF encoder and `Frames::publish`, reading the file,
  and decoding through Rust's `image` library.

The final decoder is a proxy for QuickGUI's native load step; the running QuickGUI
host is not instrumented. GPU uploads, UI scheduling, Node/FFI, display presentation,
and scanout are excluded. Ratios describe this CPU transport work only. Full-frame
pixel equivalence is checked after BGRA/RGBA conversion.

## Results

Release build on Linux x86-64, AMD Ryzen 5 5600U, Rust
`1.99.0-nightly (73dc9167f 2026-08-01)`, default release optimization without LTO.
The benchmark ran after compilation finished and the interactive test application
was closed. File publication used `/dev/shm`. These are measurements of the demo
document fitted independently to each viewport, not a resolution-scaling benchmark.

Times are **median / p95 in milliseconds**, from 40 measured samples:

| Viewport | Common engine preview | Native BGRA + RenderImage | TIFF encode + publish + read + decode |
| --- | ---: | ---: | ---: |
| 936×734 | 24.22 / 24.63 | 2.47 / 2.51 | 8.64 / 9.18 |
| 1920×1080 | 19.66 / 19.94 | 8.08 / 8.31 | 26.92 / 27.96 |

The native path reduced median CPU transport time by **6.17 ms (71%)** at 936×734
and **18.85 ms (70%)** at 1920×1080. That is approximately 3.5× and 3.3× faster for
this stage. It is not an application-wide or GPU speedup. The unchanged engine
preview remains a substantial cost; changing the UI does not itself deliver a
60 fps editor for this workload on this machine.

All pixels matched between the two paths. Raw frames contain 2,748,096 and
8,294,400 bytes respectively; TIFF frames contain 2,748,334 and 8,294,686 bytes.
The benefit comes from removing work and handoffs, not compressing the frame.
Raw output: `artifacts/gpui-kit/measurements.json`.

The first release build took 14m 51s with two Cargo workers and a cold release
target directory (dependencies had already been downloaded for the debug build).
This is setup cost, not application startup latency.

Production checks also passed: `npm run check`, `npm test` (73 Rust tests,
7 architecture tests, 22 Node integration tests), and `npm run test:bun`
(19 integration tests). Experiment formatting and real-window verification are
separate from those production checks.

## Assessment

GPUI Kit is a viable candidate for a fuller Rust UI port. The library supplied the
ordinary control behavior needed in this slice, and the existing engine integrated
without changes. Native frame ownership is simpler than the current path-based
QuickGUI integration.

This experiment does not establish complete application parity. Before a migration,
exercise file dialogs, menus, color controls, layer drag/drop, accessibility, IME,
HiDPI, packaging, and Windows/macOS. Existing engine tests remain useful regardless
of UI choice. Full GPU brush/filter work remains a separate project.

## Sources

- [GPUI Kit setup](https://gpui-kit.com/docs/installation/) and the exact downloaded
  0.7.0 crate source, especially `open_window`, Input, Slider, and ResizablePanel.
- The kit's matching GPUI `RenderImage`, `Window::paint_image`, and `drop_image` APIs.
- Pinned Compositor `CanvasViewport.swift` and `TransformTests.swift`; the production
  source map remains [compositor-port.md](compositor-port.md).
