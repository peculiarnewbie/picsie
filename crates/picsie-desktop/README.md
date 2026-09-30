# Picsie desktop

The default Picsie application, with a Rust / GPUI Kit UI following the pinned
Compositor shell and tool controls. GPUI Kit is pinned to
**0.7.0**, with its matching GPUI 0.3.7 snapshot in this crate's Cargo.lock. The
crate has its own workspace so toolkit dependencies do not affect the Node addon.
The pinned Linux backend has a small local X11 frame-wakeup adaptation; see
[its source record and patch](vendor/gpui-pre-linux/PICSIE.md). The monitor timer,
presentation mode and queue depth keep their original settings.

From the repository root, in a graphical desktop session:

```sh
npm run dev
# Or: bash crates/picsie-desktop/run.sh
# Open a project directly:
npm run dev -- --open /path/to/project.picsie
# Desktop file associations pass positional paths:
npm run dev -- first.picsie second.electropic
```

Compositor’s compact header, grouped tool rail, 42-pixel tool bars, resizable
Layers panel, native thumbnails and status bar replace the original scrolling
inspector. Color, Canvas Size and Image Size forms, file dialogs and the
unsaved-document workflow remain available. The
QuickGUI application remains available through `npm run dev:quickgui`. See the
[parity record](../../docs/gpui-ui-parity.md) for scope and verification.
See the [performance comparison](../../docs/desktop-performance.md) for measured
startup, input latency, memory, and frame transport against QuickGUI.
The [optimization follow-up](../../docs/desktop-optimization.md) measures the
worker notification change and the renderer's opaque-fill fast path.
The [GIMP comparison experiments](../../docs/desktop-gimp-experiments.md) record
retained rendering, hardware-adapter controls and the X11 wakeup measurements.

## Ownership

The UI holds transient form state and sanitized metadata. One worker per window
owns the existing `picsie-core::Editor` and `Renderer`, including all commands,
history, pixel buffers, imports, project saves, and exports. Requests execute in
order. Pointer samples and file completions are reliable; only preview delivery
is coalesced into a bounded latest-frame mailbox.
The worker wakes the UI through a bounded asynchronous notification channel after
publishing a frame or completion. There is no idle mailbox polling; redundant
wake notifications can coalesce without dropping pointer input or file results.

```text
Rust editor → CPU Skia preview → BGRA memory → GPUI RenderImage → GPU upload
```

GPUI takes the pixel vector and the previous image is explicitly evicted from its
atlas. There are no preview files, encoded image loops, JavaScript buffers, or
Node-API calls. Compositing and brushes still use CPU Skia; full viewport copies
and uploads remain. This is not a GPU image-processing implementation.

The native layout follows Compositor: a 56 px tool rail, resizable 252 px Layers
panel, 42 px tool header and 30 px footer, with an 800×520 minimum window. Type edits directly on the canvas, with installed-font search, alignment,
tracking/leading and point/paragraph boxes. Enter commits; Shift+Enter inserts
a newline; Escape cancels. Optional rulers, guides, grid and snapping are in View. Inputs retain their own text editing and clipboard behavior. Shift
preserves proportions when resizing. Ctrl/Cmd-wheel zooms around the cursor;
plain scrolling pans. File actions work while a text field is focused.

Native menus are installed on platforms that expose them. Linux uses the visible
controls and keyboard shortcuts, as the QuickGUI version did. Native file pickers
use GPUI; on Linux, install a working `xdg-desktop-portal` file chooser, such as
`xdg-desktop-portal-gtk`. The current GPUI path prompt has no extension-filter API;
file contents are validated by the engine.

## Build and verify

Use a current Rust toolchain and the GPUI Kit platform build dependencies. On Linux
these include a C/C++ toolchain, pkg-config, fontconfig, FreeType, OpenSSL,
libxkbcommon, X11/Wayland development libraries, and Vulkan support. First builds
also obtain the Skia binaries. The initial dependency build is substantial.

Ubuntu build dependencies:

```sh
sudo apt-get install build-essential clang pkg-config libssl-dev libfontconfig-dev libfreetype-dev \
  libx11-dev libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libvulkan-dev
sudo apt-get install mesa-vulkan-drivers xdg-desktop-portal xdg-desktop-portal-gtk
```

```sh
npm run build:desktop
npm run setup:packager # Install pinned cargo-packager once
npm run build         # Native installers, portable archives and checksums
npm run check:desktop
npm run test:desktop
python3 crates/picsie-desktop/verify.py --software
python3 crates/picsie-desktop/verify-frame-wakeup.py
python3 scripts/verify-desktop-package.py --target linux-x64
```

The verification script uses a real application, Xvfb, xdotool, ImageMagick, and a
private D-Bus session with the real GTK portal file picker. It writes documents,
engine traces, reports, and screenshots under ignored `artifacts/gpui-parity/`.
It cleans up only its own processes. Use `--display` for an unused X display,
`--binary` for another build, and `--tools` for an extracted tools directory.
The frame-wakeup script additionally checks hidden/inactive CPU use and verifies
that pending edits and input recover after remapping/refocusing a real window.
For system-installed tools use `--tools /usr`. Install `xvfb`, `xdotool`,
`imagemagick`, and `dbus-x11` alongside the GTK portal to run the Linux harness.
Install `xclip` to also exercise image paste from an external clipboard owner.

The default build stages a standalone `picsie` (`picsie.exe` on Windows) and
packages it with cargo-packager 0.11.8. Linux generates a `.deb` and `.tar.gz`;
Windows generates an NSIS installer and `.zip`. Icons, project file associations,
Compositor notices, and available Cargo dependency licenses are included. Package
verification extracts these artifacts and runs the binary outside the checkout.
The executable embeds its UI assets and does not require Node, Bun, or QuickGUI.
Default `check`/`test` also retain engine and reference-addon coverage.

`package.json` and this crate's package version must match; packaging enforces
this, and `picsie --version` reports the crate version. Native releases use manual
upgrades from download packages. The QuickGUI updater is not linked into this app.

`PICSIE_TRACE_DIR=/absolute/path` enables atomic per-window JSON snapshots and
control geometry for inspection. Production runs do not write these traces.
Reported paint timings stop at GPUI's paint callback, before GPU completion or
screen presentation. The original isolated transport measurement is retained as
`bash crates/picsie-desktop/run.sh --measure`; see the
[experiment report](../../docs/gpui-kit-experiment.md) for its limits.

Linux X11 and extracted Linux packages have been exercised using software Vulkan;
the desktop has also been verified with AMD hardware rendering under Xvfb.
Windows CI has passed compilation, the native Rust tests, NSIS/portable packaging,
and extracted executable version/help/error checks. Physical display presentation,
macOS/Windows editor interaction, Wayland, and HiDPI behavior still need platform
validation. macOS app/DMG packaging is available locally but is
unverified, registers no project associations, and is outside the release matrix.
