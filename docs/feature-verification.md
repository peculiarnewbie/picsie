# Feature verification record

The group-transform, editable-shape/gradient and Levels/Curves integration at
`359e844` passed the following Linux X11 runs on 2026-10-03. These are historical
verification results, not a claim of complete Compositor polish or other-platform
coverage. Compositor remains pinned at `609dbeae2ef68ef4fc82d67e4981a49852eb6e13`.

| Flow                                                                                  | Passed assertions | Tracked reproducer   |
| ------------------------------------------------------------------------------------- | ----------------: | -------------------- |
| Existing editor workflows plus shapes/gradients                                       |               250 | `native.baseline`    |
| Group numeric/pointer transforms, Shift, flip, locks, folders and package persistence |                16 | `native.groups`      |
| Grouped shapes, colored gradient, Levels and Curves in one session                    |                14 | `native.combined`    |
| Levels handles, sampling, cancel/reopen/preview/undo and Curves point editing         |                19 | `native.adjustments` |

The 16 group checks ran on its reviewed feature branch. The other three flows
ran on the combined binary. Their counts are assertions, not distinct feature
counts. Rust/Node/Bun behavior tests also passed; source-derived and additional
local fixtures remain labeled in their test modules.

Combined desktop binary SHA-256:
`5dbc75937e98e33c9ec143066928710ac29b774470ab61bea2b1b7a46ea57da1`.
It was built from the combined integration worktree before its final commits,
including integration glue; the later changes were verification scripts/docs.

The native runs used AMD RADV with Xvfb and software WSI, not physical display
scanout. Screenshots were inspected. Raw reports/screenshots/logs remain under
ignored `artifacts/integration/` and the feature artifact directories; they are
not required to validate a fresh clone. New runs use a fresh output directory and
record their own hashes. See [reproducibility](reproducibility.md).

Harness findings: shape commits round dimensions to whole pixels, so Shift ratio
checks allow the exact half-pixel-per-axis rounding bound. The shapes helper must
close/discard its owned window before testing application Quit. A one-off XTest
typing failure was retained and the dialog sequence reproduced independently.

Remaining gaps include group distortion/sampling, adjustment kinds beyond
Levels/Curves, destructive Levels/Curves, procedural shape storage versus upstream
asset identity, and Windows/macOS/Wayland/HiDPI verification. The feature registry
remains authoritative for status and gaps. Reproducing a flow does not close those
gaps automatically.

## Tracked runner validation, 2026-10-03

The new `scripts/reproduce.py` native profile replayed all four tracked drivers
against the combined binary above: **250 + 16 + 19 + 14 = 299 assertions passed**.
This includes the group driver on the combined application. Gradient and Curves
screenshots were inspected, alongside the baseline layout. The run recorded
RADV/Xvfb/software WSI, source/binary hashes, executed commands and evidence
hashes in `artifacts/reproduction/native-2026-10-03-corrected/run.json`.

An initial harness edit incorrectly waited for `busy` to clear before completing
a file chooser. The failed run and its screenshot/logs were retained in
`artifacts/reproduction/native-2026-10-03/`; the existing synchronization behavior
was restored. `busy` covers file/clipboard UI, while engine rendering uses the
sequence barrier. This was a driver error, not a measured editor regression.

Architecture checks, `npm run check`, `npm test` (275 core Rust, 18 desktop and
29 Node tests), 26 Bun addon tests and the runner's ten orchestration tests passed.
A source-only copy without local artifacts passed registry and coverage
validation. The targeted `core.cross_features` scenario also passed through the
runner. These checks establish runnable flows; feature and performance gaps in
the coverage map remain open.

A `performance.behaviors --case crop-retained` pilot rebuilt the Rust workload
and completed both Picsie/GIMP workloads at 1200×800 and 3600×2400 using GIMP
3.2.6. The smaller output probe had zero differing pixels. One trial, one sample
per size and zero warmups validate orchestration/output checks only; they do not
support a timing conclusion. Evidence is under
`artifacts/reproduction/performance-pilot-2026-10-03/`.
