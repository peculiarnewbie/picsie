# GPUI UI parity

Implemented in `crates/picsie-desktop`, following the user-authorized Rust UI
migration on 2026-09-29. The reference is the current QuickGUI application,
especially `src/ui/shell.tsx`, `layers.tsx`, `mask.tsx`, `selection.tsx`,
`canvas-size.tsx`, and `color-picker.tsx`. Compositor remains the engine semantics
reference; GIMP is not a UX reference.

This is the default application as of 2026-09-30. Run `npm run dev`; build native
packages with `npm run build`. The QuickGUI application remains available with
`npm run dev:quickgui`. See [desktop setup](../crates/picsie-desktop/README.md).

The complete applications have also been [benchmarked against each other](desktop-performance.md),
including visible input latency, startup, memory, and drag updates.

## Implemented coverage

| Area             | Desktop behavior                                                                                                                                                                                                               |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Layout           | Original palette and SVG icons; 52 px header, 46 px context bar, eleven-tool 64 px rail, native canvas, scrolling 280 px inspector, 28 px footer. Minimum window 960×640.                                                      |
| Documents        | New and open in separate windows; Picsie/legacy projects and `.comp` packages; native image import and file drop; Save, Save As, Save `.comp`, PNG/JPEG export; dirty titles; Save/Cancel/Discard on close and quit.           |
| Tools            | Move, resize, rotate, Shift proportions; brush/eraser size, opacity, hardness, smoothing; rectangle, ellipse, text, hand, eyedropper; crop ratios/apply/cancel; marquee rectangle/ellipse and lasso with replace/add/subtract. |
| Layers           | Range/toggle selection, visibility, lock, row reorder and folder drop, collapse, paint/gradient/folder creation, grouping, duplicate/delete, Into/Out of folder.                                                               |
| Properties       | Name, opacity slider and typed value, sixteen blend modes, transform fields and flips, multiline text, font size/family, fill picker, gradient end color, brightness/saturation/blur sliders and typed values.                 |
| Masks            | Add/remove/reset, layer or folder masks, content/mask target, hide/reveal, enable/link state, clipping and live alpha source picker.                                                                                           |
| Pixel selections | All, inverse, deselect, expand, contract, feather, fill, clear, and shared engine history.                                                                                                                                     |
| Color            | Foreground field and swatches, HSB square/hue strip, RGB/hex fields, preview, OK/Cancel, layer fill, eyedropper.                                                                                                               |
| Canvas Size      | Pixels/percent, relative sizing, original aspect lock, nine anchors, transparent/foreground/black/white/custom extension fills, engine limits and validation.                                                                  |
| Interaction      | Editor shortcuts, input focus isolation and clipboard, Enter commits text and Shift+Enter inserts a line, gesture capture/cancellation, zoom/fit, native application menus.                                                    |

Forms and event mapping are Rust UI code. All document mutations, geometry,
history, painting, masks, selections, compositing, codecs, and file operations use
`picsie-core`. Text and slider previews retain one undo entry per editing session
or drag. File requests run on the engine worker, after queued edits. Errors and
file completions use a reliable channel independent of coalesced preview frames.
Worker notifications wake the UI when those results arrive, without a periodic
mailbox timer.

The frame path is CPU Skia → BGRA memory → GPUI image upload. No JavaScript pixels,
TIFF preview files, new image algorithms, or toolkit patches were introduced.
This does not add GPU compositing, tiled rendering, or broader Compositor parity.

## Verification

`crates/picsie-desktop/verify.py` drives the actual release application using X11
mouse/keyboard events, a private D-Bus session, and the real GTK portal file picker.
It checks authoritative engine metadata and records screenshots at 1280×860 and
960×640. Opening a saved `.comp` through the GTK directory picker was additionally
verified with mouse navigation; the automated run does not depend on that picker's
location-entry keyboard behavior under Xvfb. Results and screenshots are under
`artifacts/gpui-parity/verification/`; the run log is
`artifacts/gpui-parity/verification-run.log`.

Verified on 2026-09-29:

- Release build and all **55 native workflow checks** pass, including dropdown
  navigation without accidental canvas nudges, save-on-close, and quit cancellation.
- `npm run check:desktop` and all **9 desktop Rust tests** pass, including worker
  notification delivery, idle quiescence, shutdown, and startup failure.
- `npm run check`, `npm test`, and `npm run test:bun` pass, covering the architecture
  guard, Rust engine, generated bridge contracts, and real Node/Bun addon integration.
- Screenshots were inspected for the shell, mask and text inspectors, color and
  Canvas Size dialogs, unsaved-close dialog, and minimum window size.

Desktop Rust tests cover ordered control updates, property history grouping,
pointer/file completion delivery, real project/package saves, PNG/JPEG exports,
image imports, failed-save recovery, and three selected upstream color-picker
fixture groups. The original architecture, Rust engine, Node, and Bun suites are
also retained. CI includes a separate desktop build/test/native interaction job.

## Adaptations and platform limits

Kit provides text editing, clipboard, selectors, sliders, modal focus, and native
path dialogs. Component text rendering and operating-system dialogs differ from
QuickGUI; the application layout and workflows follow the reference. The GPUI
path prompt currently has no extension-filter API, so the engine validates the
chosen files. Linux application menus follow the toolkit's platform support;
visible controls and shortcuts expose the same actions.

The original parity pass used Linux X11 at 1× with Mesa **software Vulkan**.
Subsequent [performance experiments](desktop-gimp-experiments.md) also exercised
an AMD hardware adapter under Xvfb. The default-application promotion built and
extracted the Debian installer and portable archive, checked their native binaries
and metadata, and passed all 56 workflow checks against the extracted installer.
Screenshots at both window sizes were inspected. CI repeats package and workflow
verification and checks hidden/inactive frame waking.

Windows CI passed native compilation, all 13 Rust tests, NSIS/portable packaging,
and extracted executable checks. macOS, Windows editor interaction, Wayland, HiDPI,
physical display presentation, and macOS package execution remain unverified. The [initial transport experiment](gpui-kit-experiment.md)
preserves the narrower historical measurements separately.
