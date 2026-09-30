# Changelog

Release notes live here under each `## x.y.z` heading (a ` - YYYY-MM-DD` date may follow).
Native packaging copies the version section into the draft GitHub release.
Keep each section under 16 KiB.

## 0.2.0 - 2026-10-01

Native Rust / GPUI Kit desktop release, with Compositor guiding the interface and
editor workflows. QuickGUI remains available as the historical reference.

- Make Rust / GPUI Kit the default application for development, builds, checks, and releases.
- Keep the QuickGUI UI behind explicit parity-reference commands.
- Package native Linux Debian/portable archives and Windows NSIS/portable archives with checksums, icons, project associations, and notices.
- Accept multiple project paths from desktop file associations, preserve `--open`, and expose native `--help`/`--version`.
- Verify extracted native packages and exercise the Linux installer application in CI.
- Retain preview pixels and wake completed X11 frames on demand; measured performance is recorded in docs/desktop-gimp-experiments.md.
- Follow Compositor's compact shell, tool controls, layer list, dialogs and keyboard workflows; persist native view and panel preferences.
- Add polygonal lasso, magic wand, movable and transformable pixel selections, system image clipboard copy/cut/paste, layer merging, and Image Size.
- Add document rulers, guides, grid and snapping, plus live point/box text editing with caret/selection feedback, typography controls, alignment and overset indication.
- Complete the tracked layers, masks and color polish pass: inline rename, visibility swiping, drag autoscroll, Alt duplication/clipping, all 24 blend modes with transient previews and keyboard cycling, and the source 10,000-layer limit.
- Add adjacent mask thumbnails, thumbnail coverage selection, mask copying, independent affine and distortion transforms, folder-mask placement, selection-consuming mask creation and targeted deletion.
- Add independent foreground/background and mask palettes, swap/reset shortcuts, foreground/background fills, background-colored canvas extension, and a movable nonmodal color picker with canvas sampling and comparison ring.
- Retain editable text and guides in the supported Compositor package subset, preserve existing project readability, and generate the typed reference bridge from Rust.
- Introduce a pinned-source feature and polish registry recording implementation, verification, platform adaptations and remaining gaps.
- Verify 190 native Linux interaction checks, Rust behavior tests and actual-addon integration under Node and Bun.

Known limits: this is not full Compositor parity; remaining engine and UI gaps are
recorded in docs/compositor-registry.md. Linux downloads are a `.deb` and portable
`.tar.gz`; AppImage packaging is planned. Windows installers remain unsigned;
macOS is outside the release matrix. Native Windows/macOS/Wayland/physical HiDPI
editor interaction still needs verification.

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
