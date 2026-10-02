# Reproducing editor behavior and performance

Run tracked scenarios through `scripts/reproduce.py`. The catalog is
[reproduction-scenarios.json](../scripts/reproduction-scenarios.json); every entry
records its fixture, steps, assertions, driver and measurement scope. The
[coverage map](reproducibility-coverage.json) accounts for every stable feature ID
in the Compositor registry, including missing coverage. Suite/check associations
do not prove every sub-behavior or full polish; feature statuses remain separate.

## Correctness

```sh
npm ci
npm run reproduce:quick

# A small feature-specific Rust/bridge selection; no broad suite or benchmarks.
npm run reproduce -- run --feature transform.group-box
npm run reproduce -- run --scenario core.cross_features
npm run reproduce -- list
```

The quick profile runs the existing `npm test`: it builds the actual addon and
runs architecture, Rust, Node and desktop tests. A targeted bridge run needs a
fresh addon (`npm run build:native`, or runner `--build`). Rust fixtures and
commands stay in Rust; Python only launches drivers and validates reports.
`npm run test:bun` separately exercises the real addon under Bun, using an explicit
test path so copied test sources inside artifacts cannot enter discovery.

## Native interactions and screenshots

Linux native flows need Xvfb, xdotool, xclip, ImageMagick (`convert`),
dbus-run-session, xdg-desktop-portal/GTK, fonts and Vulkan. CI installs these on
normal GitHub runners. Use system tools by default; an extracted prefix can be
supplied with `--tools /path/to/usr` (bin/lib/share). No user-specific path is
embedded in the tracked feature drivers.

```sh
# Build and replay baseline + groups + adjustments + combined feature session.
npm run reproduce -- run --profile native --build --software

# One feature, using an already built binary and explicit renderer.
npm run reproduce -- run --profile native --feature adjustments.curves \
  --gpu-icd /usr/share/vulkan/icd.d/radeon_icd.json

# Current workspace: extracted tools and RADV rendering into Xvfb.
MESA_VK_WSI_DEBUG=sw npm run reproduce -- run --profile native \
  --scenario native.combined --tools artifacts/selection-history/tools/usr \
  --gpu-icd /usr/share/vulkan/icd.d/radeon_icd.json
```

`--software` explicitly selects lavapipe; hardware runs choose `--gpu-icd`.
Correctness may also use the system's default driver, which is recorded. The
default display is `:119`; occupied displays fail preflight. Native flows use
isolated config/data paths, typed input, state assertions and actual screenshots.
They never certify polish merely from an exit code. Inspect screenshots after
UI changes. The previous [feature verification record](feature-verification.md)
documents the original integration runs.

## Performance: explicit opt-in

Performance never runs as part of quick/native correctness or implicitly as a
large matrix. Select a scenario explicitly. Keep builds and other editor instances
closed while measuring. The runner serializes its runs with an advisory lock;
unrelated programs/build commands do not participate in that lock.

```sh
# Engine command/render/RGBA availability; includes Rust fixture generation.
npm run reproduce -- run --profile performance --scenario performance.behaviors \
  --build --runtime artifacts/gimp-performance/runtime \
  --trials 3 --samples 16 --warmups 5

# A setup pilot, not enough samples for a performance conclusion.
npm run reproduce -- run --profile performance --scenario performance.behaviors \
  --build --case crop-retained --trials 1 --samples 1 --warmups 0

# Layer appearance and mask features; exact case filter plus optional batch use.
# opacity-preview is Picsie-only: the driver skips the GIMP launch cleanly.
npm run reproduce -- run --profile performance --scenario performance.features \
  --build --trials 3 --samples 4 --warmups 2
npm run reproduce -- run --profile performance --scenario performance.features \
  --build --case mask-paint --trials 1 --samples 1 --warmups 0

# Each new batch can run separately; --case or --cases narrows it further.
npm run reproduce -- run --profile performance --scenario performance.selections \
  --build --trials 3 --samples 4 --warmups 2
npm run reproduce -- run --profile performance --scenario performance.recent-features \
  --build --trials 3 --samples 4 --warmups 2

# Nudge/drag to observed framebuffer; fixed external-input protocol.
MESA_VK_WSI_DEBUG=sw npm run reproduce -- run --profile performance \
  --scenario performance.layer-move --build --software \
  --runtime artifacts/gimp-performance/runtime

# Large CPU ordering/navigation or resize campaigns are separate explicit choices.
npm run reproduce -- run --profile performance --scenario performance.stress-stacks \
  --build --trials 3 --samples 4 --warmups 2
```

GIMP is a secondary functionality/performance reference, never a UX reference.
Paired drivers require the isolated GIMP 3.2.6 runtime described in
[gimp-performance.md](gimp-performance.md). The original package URLs/digests are
tracked in [benchmark-runtime-packages.json](benchmark-runtime-packages.json).
That historical runtime is Linux x86-64, includes x86-64-v3 packages, and depends
on compatible host libraries; it is not a universal cross-platform installer.
Missing runtime prerequisites fail clearly. Mirrors may retire historical URLs;
never silently substitute a different version or drop digest validation.

`performance.strokes` needs the raster fixtures generated by
`performance.behaviors` (`--fixtures path/to/that/run/fixtures`).
`performance.stress-navigation` needs the fixtures from `performance.stress-stacks`
and the native matcher:

```sh
cc -O3 -shared -fPIC scripts/perf/screen-match.c -o artifacts/screen-match.so
npm run reproduce -- run --profile performance \
  --scenario performance.stress-navigation --fixtures artifacts/your-stress-fixtures \
  --matcher artifacts/screen-match.so --gpu-icd /path/to/driver.json
```

CPU command/render time, input-to-visible latency, visible update intervals,
endpoint backlog, CPU and memory are distinct metrics. The runner's
`orchestrationWallSeconds` includes setup/build/I/O and **is not feature latency**.
Driver-owned samples and assertions remain authoritative. Polling granularity
adds uncertainty and cannot simply be subtracted from percentiles. Framebuffer
observations exclude physical compositor, scanout and input-device hardware.
Traced diagnostics remain separate from untraced measurements. Setup failures,
slow valid samples and quality differences must be retained/labeled.

The full historical methodology and cohort-specific adaptations are in
[desktop-gimp-experiments.md](desktop-gimp-experiments.md),
[gimp-performance.md](gimp-performance.md), and
[desktop-performance.md](desktop-performance.md). Underlying benchmark drivers
remain available for exact case filters, aligned-quality controls and separately
validated XCF cohorts. A performance association in the coverage map describes
its listed workload only, not every variant of that feature.

## Evidence, failures and maintaining coverage

Each run writes a fresh `artifacts/reproduction/<UTC timestamp>/run.json` plus
per-scenario logs, driver reports, screenshots and fixtures. `--output` may choose
another fresh subdirectory under ignored `artifacts/`; existing directories are
rejected. Reports retain source hashes (including uncommitted source files), HEAD,
working-tree status, binary hashes, CPU/OS, chosen backend and executed argv.
Fixtures/screenshots/reports receive hashes. Build logs establish whether a
rebuild was requested; supplied binary freshness is explicitly unverified.
Interrupted/failed commands retain their logs and failed/not-run status. The
runner does not auto-retry or discard failed samples.

```sh
npm run reproduce:coverage  # Regenerate all feature associations and gaps.
npm run check:reproduce     # Reject stale maps, unknown IDs or missing drivers.
npm run test:reproduce      # Real subprocess failures, locking, output safety, mapping.
```

When adding/changing a feature: update the stable feature row and its evidence;
add/update a tracked scenario's fixture, steps and assertions; map its stable ID
explicitly or through existing named native checks/test-file evidence; regenerate
coverage. Missing flows must stay visible. Add exact sub-behavior scenarios as
needed rather than labeling a family suite as exhaustive. Outputs belong in
artifacts, while reusable drivers/fixtures/specifications belong in tracked files.
