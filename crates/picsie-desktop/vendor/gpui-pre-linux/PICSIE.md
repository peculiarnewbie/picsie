# Pinned GPUI Linux backend

This is the published `gpui-pre-linux` 0.3.7 source, from the desktop lockfile.
Its metadata identifies Zed revision `1a28cff4b409169bac058bca40dfbfeb7621d19b`.
The Apache-2.0 license is retained in `LICENSE-APACHE`. The normalized published
manifest is retained; there are no dependency/version changes.
`upstream-sha256.json` records all original source/manifest/license hashes.

## Local adaptation

Only `src/linux/x11/client.rs` and `src/linux/x11/window.rs` differ.
`picsie-x11-frame-wakeup.patch` records those differences against the published
crate (apply with `git apply --unidiff-zero`). GPUI already defines the `PlatformWindow::frame_waker` contract; X11
previously returned its default `None`. We implement it with a coalescing
calloop ping, requesting the standard GPUI frame callback for visible windows.
The existing monitor refresh timer, surface presentation mode and queue depth
are unchanged. Hidden windows ignore pings and repaint through the existing
map/visibility timer. Each window removes its ping registration when closed.

Frame callbacks suppress their own demand pings, so a throttled animation
retries on the existing timer instead of spinning the event loop. Requests from
input or a completed engine frame can wake normally after the callback ends.
The Wayland and other backend source files are unchanged.

This is a local platform integration, not a Compositor or GIMP algorithm port.
See `docs/desktop-gimp-experiments.md` for paired latency/CPU/pixel evidence,
native workflow verification and the virtual-display limits. Measurements do
not establish performance on a physical display, Windows or macOS.

Keep this patch small. When updating GPUI, check whether upstream implements
X11 frame waking, remove this copy if possible, and rerun native interactions
and an untraced before/after comparison before retaining another adaptation.
