# Picsie desktop

Rust / GPUI Kit UI matching the existing QuickGUI editor. GPUI Kit is pinned to
**0.7.0**, with its matching GPUI 0.3.7 snapshot in this crate's Cargo.lock. The
crate has its own workspace so toolkit dependencies do not affect the Node addon.
The pinned Linux backend has a small local X11 frame-wakeup adaptation; see
[its source record and patch](vendor/gpui-pre-linux/PICSIE.md). The monitor timer,
presentation mode and queue depth keep their original settings.

From the repository root, in a graphical desktop session:

```sh
npm run dev:desktop
# Or: bash crates/picsie-desktop/run.sh
# Open a project directly:
bash crates/picsie-desktop/run.sh --open /path/to/project.picsie
```

The full header, tool rail, context controls, layers, inspector, color picker,
Canvas Size dialog, file dialogs, and unsaved-document workflow are ported. The
QuickGUI application remains available through `npm run dev`. See the
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

The layout keeps the old 64 px tool rail, 280 px scrolling inspector, 52 px header,
46 px context bar, and 28 px footer. Enter commits layer text; Shift+Enter inserts
a newline. Inputs retain their own text editing and clipboard behavior. Shift
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

```sh
npm run build:desktop
npm run check:desktop
npm run test:desktop
python3 crates/picsie-desktop/verify.py --software
python3 crates/picsie-desktop/verify-frame-wakeup.py
```

The verification script uses a real application, Xvfb, xdotool, ImageMagick, and a
private D-Bus session with the real GTK portal file picker. It writes documents,
engine traces, reports, and screenshots under ignored `artifacts/gpui-parity/`.
It cleans up only its own processes. Use `--display` for an unused X display,
`--binary` for another build, and `--tools` for an extracted tools directory.
The frame-wakeup script additionally checks hidden/inactive CPU use and verifies
that pending edits and input recover after remapping/refocusing a real window.

`PICSIE_TRACE_DIR=/absolute/path` enables atomic per-window JSON snapshots and
control geometry for inspection. Production runs do not write these traces.
Reported paint timings stop at GPUI's paint callback, before GPU completion or
screen presentation. The original isolated transport measurement is retained as
`bash crates/picsie-desktop/run.sh --measure`; see the
[experiment report](../../docs/gpui-kit-experiment.md) for its limits.

Linux X11 has been exercised using software Vulkan and verified AMD hardware
rendering under Xvfb. Physical display presentation, macOS, Windows, Wayland,
HiDPI behavior, and desktop packaging require platform validation.
