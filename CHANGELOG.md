# Changelog

Release notes live here under each `## x.y.z` heading (a ` - YYYY-MM-DD` date may follow).
Native packaging copies the version section into the draft GitHub release.
Keep each section under 16 KiB.

## Unreleased

- Make Rust / GPUI Kit the default application for development, builds, checks, and releases.
- Keep the QuickGUI UI behind explicit parity-reference commands.
- Package native Linux Debian/portable archives and Windows NSIS/portable archives with checksums, icons, project associations, and notices.
- Accept multiple project paths from desktop file associations, preserve `--open`, and expose native `--help`/`--version`.
- Verify extracted native packages and exercise the Linux installer application in CI.
- Retain preview pixels and wake completed X11 frames on demand; measured performance is recorded in docs/desktop-gimp-experiments.md.

## 0.1.0 - 2026-09-23

First release: an experimental port of Robbie Tilton's Compositor, with a TypeScript/QuickGUI
interface and a Rust/Skia editing engine.

- Layers with nested folders, multi-selection, drag reordering, 16 blend modes, and Compositor-style undo/redo.
- Brush and eraser strokes, non-destructive grayscale layer masks with Hide/Reveal painting, and linked or independent mask placement.
- Rectangular/elliptical marquee and freehand lasso selections with New/Add/Subtract modes, Select → Modify → Feather, and clearing selected pixels.
- Eight-handle transforms with Shift-to-preserve proportions, 15° rotation snapping, and Alt/Option center resizing.
- Rectangles, ellipses, gradients, and editable text; brightness, saturation, and Gaussian blur per layer.
- Canvas Size with nine anchors and colored extensions, crop with ratio presets and edge snapping, PNG/JPEG export, and image import.
- Validated `.picsie` projects, legacy `.electropic` files, and the supported raster/folder/mask subset of Compositor `.comp` packages.

Known limits: Windows builds are unsigned, macOS builds are not part of this release, and the
remaining port gaps (polygonal lasso, wand/object selection, selection move/transform, folder
masks, clipping, and more) are tracked in docs/compositor-port.md.
