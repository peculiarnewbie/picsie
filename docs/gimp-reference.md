# GIMP functionality reference

Accepted reference scope, 2026-09-29: use GIMP to investigate functionality and edge cases. **Never use GIMP as a UX reference.** Compositor remains the primary source for editor semantics, defaults, workflows, and UI, subject to the user's explicit choices. The TypeScript/QuickGUI UI and Rust engine boundary remains as defined in [architecture.md](architecture.md).

## Pinned checkout

- Repository: <https://gitlab.gnome.org/GNOME/gimp>
- Release tag: `GIMP_3_2_6`
- Commit: `e101dd19b165f927d3ba0a74658a71537c5661b9`
- Local reference: `/home/bolt/git/reference/gimp`, outside the Picsie repository.
- The reference checkout has not been built and its tests have not been executed.
  Packaged GIMP 3.2.6 was separately extracted and run for the
  [interaction measurements](gimp-performance.md). It is not a Picsie dependency.
  Asset submodules are not initialized.

To recreate the reference in an unused directory:

```sh
git clone --depth 1 --single-branch --branch GIMP_3_2_6 https://gitlab.gnome.org/GNOME/gimp.git /home/bolt/git/reference/gimp
git -C /home/bolt/git/reference/gimp rev-parse HEAD
```

Verify the printed commit against the pin above before using it. Do not silently advance the reference.

## Useful source entry points

Paths below are relative to the pinned GIMP checkout. These are investigation starting points, not claims of equivalence to Picsie or a completed algorithm audit.

| Functionality | GIMP source |
| --- | --- |
| Pixel selections, coverage, combining regions | `app/core/gimpselection.c`, `app/core/gimpchannel.c`, `app/core/gimpchannel-select.c` |
| Selection/channel undo | `app/core/gimpmaskundo.c`, `app/core/gimpchannelundo.c` |
| Selection behavior tests | `plug-ins/script-fu/test/tests/PDB/selection/` |
| Brush spacing, stroke processing, paint transactions | `app/paint/gimpbrushcore.c`, `app/paint/gimppaintcore.c`, `app/paint/gimppaintbrush.c` |
| Layer masks | `app/core/gimplayermask.c`, `app/core/gimplayer.c` |
| Blend modes and image operations | `app/operations/`, `app/gegl/` |
| Image import/export | `plug-ins/common/file-png.c`, `plug-ins/file-jpeg/`, `plug-ins/file-webp/` |
| Core test entry points | `app/tests/` |
| Interactive projection and damage scheduling | `app/core/gimpprojection.c`, `app/core/gimpchunkiterator.c`, `app/gegl/gimptilehandlervalidate.c` |
| Retained display rendering | `app/display/gimpdisplayshell-render.c`, `app/display/gimpdisplayshell-draw.c`, `app/display/gimpdisplay.c` |

See the [layer-move rendering analysis](gimp-rendering-analysis.md) for the pinned
source path, its differences from Picsie, and an incremental optimization sequence.

GIMP delegates substantial image processing to GEGL and pixel format/color conversion to babl. If an investigation reaches those calls, consult and pin the relevant dependency source separately; the GIMP checkout alone does not contain every algorithm. See the [GIMP API reference index](https://developer.gimp.org/resource/api/).

Do not adopt GIMP's panels, menus, dialogs, shortcuts, or tool interaction model. Where its behavior differs from Compositor, record the difference rather than silently changing the port's target. Label any new local regression scenarios separately from translated Compositor fixtures.

The inspected GIMP core source headers identify GPL-3.0-or-later. This reference setup copies or translates no GIMP implementation or fixtures into Picsie; any future source reuse must track its own license and attribution, separately from Compositor's MIT attribution.

## Four-item implementation pass

1. **Selection history — implemented.** Rust history now captures coverage and feather for Select All, Deselect, Feather, and completed marquee/lasso edits, following Compositor's transaction behavior. See the [port and fixture notes](compositor-port.md#pixel-selection-history-port) for coverage, adaptations, and verification.
2. **Basic selection operations — implemented.** Rust now supports Fill, Invert, Expand, and Contract, including feathered coverage, transformed layers, editable text recoloring, mask targets, and explicit empty selections.
3. **Brush fidelity — implemented for the software paint/erase path.** The port follows Compositor's spacing, curves, provisional tail, hardness, smoothing, per-stroke opacity, and native raster ownership. The subsequent immutable image snapshot and bounded preview pass is documented in the [performance port](compositor-port.md#immutable-raster-and-bounded-pixel-work-performance-pass); GPU coverage and contiguous mask-stroke publication remain adaptations.
4. **Folder masks and clipping relationships — implemented.** Nested pass-through masks, live alpha sources, shared clipping stacks, graph validation, undo, dependency-preserving deletion, and `.picsie`/`.comp` round-trips are covered.

Implementation and fixture attribution, plus remaining differences, are in the [source map](compositor-port.md#selection-operations-software-brush-and-clipping-port). Completion of this pass is not a claim of complete Compositor or GIMP parity.

During this pass, `gimpchannel.c::gimp_channel_real_grow`/`gimp_channel_real_shrink` were inspected for empty/full coverage and image-edge cases, and `gimpbrushcore.c` was inspected for spacing entry points. GIMP uses channel coverage and GEGL morphology, with separate X/Y radii and shrink edge-lock behavior; Picsie keeps Compositor's single-amount vector-outline band semantics. The added edge/empty fixtures are adapted from Compositor or explicitly labeled local regressions. No GIMP implementation or fixture was copied or translated.
