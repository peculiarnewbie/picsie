# Preview transport: the update-in-place deep dive

Status: design record, 2026-09-23. Question: how should canvas frames reach the screen in the
long term, replacing the per-frame TIFF file handoff? Short answer: keep the shipped
stacked-frame mitigation now, and pursue a small **core change in QuickGUI's open-source host**
(image-resource reload with retain-while-decoding), delivered as an upstream pull request or a
vendored core built from a pinned fork. Everything below is measured or read from source, not
assumed.

## What ships today, and what it costs

`docs/architecture.md` documents the path: Skia renders in Rust (`crates/picsie-core`), reads
pixels back, writes an uncompressed TIFF (~2.7 MB at 936×734) to a private directory, and the UI
sets the `Image` node's `source` to that path. Roughly: one GPU readback, one encode, one file
write, one file read, one decode, one texture upload — per frame. Files are uniquely named
because the core caches decoded images by path; up to eight are retained per window.

The visible failure of this path was canvas flicker: QuickGUI decodes `source` paths
asynchronously ("A path decodes on the core's bounded worker pool") and paints nothing while
loading, so every source change blanked the canvas. 30 fps captures of a scripted drag showed
near-perfect alternation between artwork and bare background (global luminance 60.9 ↔ 41.1).
The shipped fix (`src/ui/shell.tsx`) stacks the last few frame resources as overlapping `Image`
nodes: the last decoded frame shows through until the next finishes loading. Same measurement
afterward: stable 61.15–61.18. That fix is presentation-only; the copy cost above remains.

## Hard constraints (unchanged)

- Pixels stay out of JavaScript: no per-pixel JS calls, no JSON/base64 image transport, no JS
  renderer. Rust owns buffers and lifetimes; JS sees metadata and opaque resource paths.
  (`AGENTS.md`, `docs/architecture.md`.)
- Any solution must keep rendering in `crates/picsie-core` and present through QuickGUI's
  native compositor.

## What QuickGUI 0.1.6 can and cannot do (evidence)

Read from the installed packages and cross-checked against quickgui.dev and
[github.com/egoist/quickgui](https://github.com/egoist/quickgui) (public, MIT OR Apache-2.0 —
the shipped `libquickgui_host.so` is built from `crates/quickgui-host` in that repo):

| Capability | Verdict | Evidence |
| --- | --- | --- |
| `Image.source: string` (path, `file://`, base64 `data:` URL) | The only node-level pixel input | `JSX.ImageProps`, quickgui.dev Image reference |
| Same source string after in-place file overwrite | **Stale** — no reload | probe, phase 2 |
| `file://…#rev` or `?rev` cache-busting | **Fails to load** — falls back blank | probe, phases 3–4 |
| base64 `data:` URL updates | Works (probe phase 5), but ships encoded bytes through the JS protocol — violates the boundary and is larger than the file path | probe |
| `NativeImageSource` raw RGBA8 (`data` + width/height) | Exists, but only for window/tray/dock icons (`performNativeWindowImageAction`), not for nodes | `binding-types.ts`, `system.ts` |
| `Shader` node | Procedural WGSL with one fixed 16-float uniform; no texture binding | `ShaderProps`, `normalizeShaderParameters` |
| Load/decode completion event | None on `Image` (only `AvatarImage.onLoadingStatusChange`) | `solid/src/index.ts` |
| Shared textures, fds, shared memory in the ABI | None | full read of `ffi.ts`, `protocol.ts`, `binding.ts` |
| `quickgui_create_embedded_view` | A platform sub-window, not a texture slot | `binding.ts` `createHostedEmbeddedView` |
| Extension components | JSON subtree/IPC rendering, not pixel injection | `extension.ts`, extension templates |
| Newer upstream version with a fix | None — 0.1.6 is the latest published | npm registry |

The probe (interactive app + button-driven strategies + screen capture) lives in
`artifacts/inplace-test/`; it is the acceptance harness for whichever option below ships.

## Options

**A. Stacked frame resources — shipped.** Hides the decode gap with zero added latency and no
engine change. Leaves the copy cost, the 2.7 MB/frame file churn, and up to four retained
`Image` nodes. Good indefinitely as the compatibility layer.

**B. `data:` URL transport — works, rejected.** Removes file IO but pushes ~3.6 MB of base64
through the JS protocol per frame and still decodes asynchronously. Violates "pixels stay out
of JavaScript" and would not remove the stacking. No.

**C. Core change — the actual fix.** The core is open source and the host library is designed to
be swappable: `ffi.ts` honors `QUICKGUI_LIBRARY`, and the CLI's `native.libraryPath` config
("Use an existing Rust shared library instead of the installed native package") does the same.
So the long-term solution is implementable in `crates/quickgui-host` and deliverable either as
an upstream PR (preferred — every QuickGUI app showing live imagery benefits) or as a pinned
fork whose `libquickgui_host` we build in CI. Three shapes, in increasing payoff:

1. **`reloadRevision` on `Image` (minimum viable, ~small).** A numeric property that, when it
   changes, re-reads the current `source` path *while retaining the existing texture until the
   new one finishes decoding*. This is literally "update in place": one file overwritten in
   place, one node, no blank gap. It deletes the flicker class of bug for all apps and lets us
   drop the stacking hack and the unique-path churn.
2. **Raw RGBA8 `Image` source for nodes (medium).** Extend the existing icon path
   (`NativeImageSource { data, width, height }`) to the `Image` node. Skia's readback bytes
   would cross the FFI as an opaque buffer (not per-pixel JS work), removing the TIFF encode and
   file round-trip. Two copies remain (engine buffer → FFI → core texture).
3. **Shared buffer handle (the zero-copy end state, larger).** An fd/memfd or platform texture
   handle passed through the ABI (`include/` + protocol), with the engine rendering straight
   into memory the core samples, plus a fence/revision for tear-free swaps. This is the double-
   buffered canvas proper: CPU copies drop to ~zero and latency to one compositor frame.

**D. Rejected avenues,** with reasons: JS `<canvas>`/WebGL presentation (banned by the
architecture requirement), `background-image` style property (same path loader), `Shader` node
(procedural only), embedded views (platform windows), extension components (JSON subtrees),
waiting for a newer QuickGUI release (0.1.6 is current; the feature does not exist upstream).

## Recommendation

1. **Now:** keep A. Ship quality is good and measured stable.
2. **Next:** open an upstream issue/PR for C1 (`reloadRevision` + retain-while-decoding), with
   this document's probe as the acceptance test. Keep the request minimal and self-contained;
   C2/C3 can ride on the same discussion. If upstream is slow, build `crates/quickgui-host` from
   a fork in release CI and load it via `native.libraryPath` — the same pattern as our pinned
   `@quickgui/cli` NSIS patch, one layer down.
3. **Later:** if profiling after C1 still shows the copy cost mattering (large canvases, low-end
   GPUs), pursue C3. Do not claim wins without benchmarking.

Acceptance criteria for any core change: the probe's phase 2 shows live updates in place with no
fallback flash; the rapid-alternation capture shows content changes with no background frames
(the luminance test that caught the original flicker); `npm run check`, `npm test`, and a
packaged-app wiggle capture stay green.

## Appendix: measurement method

All display claims come from Xvfb + `ffmpeg -f x11grab` captures of the packaged app or the
probe, analyzed with ffmpeg `signalstats` per-frame luminance and ImageMagick pixel probes.
Before/after flicker numbers: alternating 60.93/41.11 (single `Image`) vs 61.15–61.18 (stacked).
Probe verdicts: same-path stale (stayed red), `#rev`/`?rev` fail-to-load (fallback color),
`data:` URL renders (distinct cyan test image). Frame size: 2,748,096 pixel bytes plus a small
TIFF header at 936×734. Per-frame end-to-end timing has not been benchmarked; none is claimed.
