"""Compare layer-appearance and mask features through existing public APIs.

CPU command, render/projection, and RGBA-read stages are separate from GUI
latency. Requires the extracted GIMP runtime described in
docs/gimp-performance.md. Build the Rust workload with cargo build --release
-p picsie-core --example performance_features. All temporary
fixtures/results stay in artifacts/.
"""
import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('movement_comparison',
                                             ROOT/'scripts/compare-gimp-performance.py')
comparison = importlib.util.module_from_spec(spec)
spec.loader.exec_module(comparison)

FEATURE_CASES = [
    'opacity-preview', 'opacity-commit', 'visibility-toggle',
    'blend-multiply', 'blend-screen', 'mask-disabled', 'mask-paint',
    'select-inverse', 'select-expand5', 'select-contract5',
    'wand-contiguous', 'wand-noncontiguous', 'select-layer-alpha',
    'group-resize', 'group-rotate',
    'gradient-image-linear', 'gradient-mask-linear',
    'levels-update', 'curves-update',
]
LAYER_MASK_CASES = [
    'opacity-preview', 'opacity-commit', 'visibility-toggle',
    'blend-multiply', 'blend-screen', 'mask-disabled', 'mask-paint',
]
SELECTION_CASES = [
    'select-inverse', 'select-expand5', 'select-contract5',
    'wand-contiguous', 'wand-noncontiguous', 'select-layer-alpha',
]
RECENT_CASES = [
    'group-resize', 'group-rotate',
    'gradient-image-linear', 'gradient-mask-linear',
    'levels-update', 'curves-update',
]
# Cases whose primary paired output is a real coverage channel (compared
# with the single-channel comparator), not just the RGBA composite.
MASKCOVERAGE_CASES = ['gradient-mask-linear']
SUITES = {'layers-masks': LAYER_MASK_CASES, 'selections': SELECTION_CASES,
          'recent-features': RECENT_CASES}
GIMP_CASES = [case for case in FEATURE_CASES if case != 'opacity-preview']
PICSIE_ONLY_CASES = ['opacity-preview']
SIZES = [(1200, 800), (3600, 2400)]
METRICS = ['command_ms', 'coverage_ms', 'render_ms', 'read_ms', 'total_ms', 'metadata_ms']
LEGACY_METRICS = ['command_ms', 'render_ms', 'read_ms', 'total_ms']


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate_timings(result):
    """Reject invalid or double-counted stages; retain logs on failure.

    Small positive wall-clock gaps between timer reads are allowed. Excluded
    metadata must never make the measured stages exceed the CPU total.
    """
    for row in result['results']:
        for sample in row['samples']:
            stages = [sample[name] for name in LEGACY_METRICS[:-1]]
            stages.append(sample.get('coverage_ms', 0.))
            total = sample['total_ms']
            values = stages + [total, sample.get('metadata_ms', 0.)]
            if any(not math.isfinite(value) or value < 0 for value in values):
                raise ValueError(f"Invalid timing in {result['app']} {row['case']}")
            if sum(stages) > total + 0.05:
                raise ValueError(f"Timing stages exceed total in {result['app']} {row['case']}: "
                                 f"{sum(stages):.3f} > {total:.3f} ms")


def resolve_case(value, suite=None):
    """Validate an exact-case filter before any fixture work. Unknown cases
    fail here, never after launching applications. A suite narrows the
    accepted catalog; the default accepts every tracked feature workload."""
    allowed = SUITES[suite] if suite else FEATURE_CASES
    if value is None:
        return None
    if value not in allowed:
        raise ValueError(f'Unknown feature workload: {value}; '
                         f'supported cases: {", ".join(allowed)}')
    return value


def resolve_cases(values, suite=None):
    """Validate an explicit multi-case selection for suite cohorts."""
    allowed = SUITES[suite] if suite else FEATURE_CASES
    selected = list(values) if values else list(allowed)
    unknown = [value for value in selected if value not in allowed]
    if unknown:
        raise ValueError(f'Unknown feature workload: {", ".join(unknown)}; '
                         f'supported cases: {", ".join(allowed)}')
    return selected


def select_apps(case):
    """Picsie-only cases skip the GIMP launch cleanly; every other selection
    runs both applications in alternating order."""
    if case in PICSIE_ONLY_CASES:
        return ['picsie']
    return ['picsie', 'gimp']


def gimp_version(runtime):
    """Report the pinned runtime version; reject anything but GIMP 3.2.6."""
    executable = runtime/'usr/bin/gimp-console'
    env = {**os.environ,
           'LD_LIBRARY_PATH': str(runtime/'usr/lib') + os.pathsep + os.environ.get('LD_LIBRARY_PATH', '')}
    version = subprocess.check_output([str(executable), '--version'], env=env,
                                      text=True, stderr=subprocess.STDOUT, timeout=60).strip()
    if not version.endswith('version 3.2.6'):
        raise ValueError(f'Pinned scenarios require GIMP 3.2.6; found {version}.')
    return version


def run_app(command, launch_env, folder, timeout=900):
    process = None
    try:
        with (folder/'app.log').open('w') as log:
            process = subprocess.Popen(command, env=launch_env, stdout=log,
                                       stderr=subprocess.STDOUT, start_new_session=True)
            returncode = process.wait(timeout=timeout)
        assert returncode == 0, f'{folder.name} failed; see {folder}/app.log'
    finally:
        comparison.desktop.stop(process)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path,
                        default=Path('artifacts/feature-performance/layers-masks'))
    parser.add_argument('--fixtures', type=Path,
                        default=Path('artifacts/feature-performance/layers-masks-fixtures'))
    parser.add_argument('--runtime', type=Path,
                        default=Path('artifacts/gimp-performance/runtime'))
    parser.add_argument('--picsie', type=Path,
                        default=Path('target/release/examples/performance_features'))
    parser.add_argument('--warmups', type=int, default=2)
    parser.add_argument('--samples', type=int, default=4)
    parser.add_argument('--trials', type=int, default=3)
    parser.add_argument('--case', help='Run just one named workload for setup/pilots')
    parser.add_argument('--cases', help='Run a comma-separated subset of named workloads (must all belong to the suite, if given); never launches unlisted batches')
    parser.add_argument('--suite', choices=sorted(SUITES),
                        help='Restrict the cohort to one tracked suite so selection '
                             'cohorts never rerun layer/mask workloads and vice versa')
    args = parser.parse_args()
    if args.samples < 1 or args.trials < 1 or args.warmups < 0:
        parser.error('Samples/trials must be positive and warmups nonnegative')
    case = resolve_case(args.case, args.suite)
    if args.case and args.cases:
        parser.error('Choose --case or --cases, not both')
    subset = [value for value in (args.cases.split(',') if args.cases else []) if value]
    cases = resolve_cases(subset if subset else ([case] if case else []), args.suite)
    for name in ['output', 'fixtures', 'runtime', 'picsie']:
        setattr(args, name, getattr(args, name).resolve())
    args.output.mkdir(parents=True, exist_ok=True)
    if (args.output/'manifest.json').exists():
        parser.error('Output already contains a cohort; choose a new directory')
    workload = ROOT/'scripts/perf/gimp-feature-workload.py'
    version = gimp_version(args.runtime)
    subprocess.run([str(args.picsie), 'prepare', str(args.fixtures)], check=True)
    env = {**os.environ, **comparison.runtime_environment(args.runtime)}
    env['PATH'] = str(args.runtime/'usr/bin') + os.pathsep + env.get('PATH', '')
    config = args.output/'gimp-config'
    config.mkdir(exist_ok=True)
    (config/'gimprc').write_text('(show-welcome-dialog no)\n(check-updates no)\n(use-opencl no)\n')
    env.update(GIMP3_DIRECTORY=str(config), GIMP3_CACHEDIR=str(args.output/'gimp-cache'),
               GEGL_USE_OPENCL='no', PICSIE_PERF_WARMUPS=str(args.warmups),
               PICSIE_PERF_SAMPLES=str(args.samples))
    if case:
        env['PICSIE_PERF_CASE'] = case
    elif args.suite or subset:
        # Suite cohorts and explicit subsets run only their own cases on
        # both sides, never other suites' or unlisted workloads.
        env['PICSIE_PERF_CASE'] = ','.join(cases)
    else:
        env.pop('PICSIE_PERF_CASE', None)
    manifest = {
        'scope': 'CPU command/render/read stages plus end-to-end availability, not visible input latency',
        'gimp_version': version,
        'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'warmups_per_case': args.warmups, 'samples_per_case': args.samples,
        'trials': args.trials, 'case_filter': case,
        'launch_order': 'Alternating applications each trial; sequential timed workloads',
        'picsie_only_cases': PICSIE_ONLY_CASES,
        'suite': args.suite, 'suite_cases': cases, 'cases_filter': subset or None,
        'source_sha256': {str(path.relative_to(ROOT)): digest(path) for path in [
            Path(__file__), workload, ROOT/'crates/picsie-core/examples/performance_features.rs',
            ROOT/'scripts/compare-gimp-performance.py', ROOT/'scripts/compare-desktop-performance.py']},
        'fixture_sha256': {path.name: digest(path) for path in args.fixtures.iterdir() if path.is_file()},
        'binary_sha256': {'picsie': digest(args.picsie),
                          'gimp': digest((args.runtime/'usr/bin/gimp-console').resolve())},
    }
    (args.output/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    (args.output/'working-tree.patch').write_bytes(subprocess.check_output(['git', 'diff'], cwd=ROOT))
    if case:
        apps = select_apps(case)
    elif subset and all(item in PICSIE_ONLY_CASES for item in subset):
        apps = ['picsie']
    else:
        apps = ['picsie', 'gimp']
    runs = []
    for trial in range(1, args.trials+1):
        order = apps if trial % 2 else list(reversed(apps))
        for app in order:
            folder = args.output/f'{app}-{trial}'
            folder.mkdir()
            launch_env = {**env, 'PICSIE_PERF_OUTPUT': str(folder)}
            if app == 'picsie':
                command = [str(args.picsie), 'run', str(folder),
                           str(args.warmups), str(args.samples)]
                if case:
                    command.append(case)
                elif args.suite or subset:
                    command.extend(cases)
            else:
                command = [str(args.runtime/'usr/bin/gimp-console'), '--no-interface',
                           '--new-instance', '--batch-interpreter=python-fu-eval', '--batch',
                           f"exec(compile(open({str(workload)!r}).read(),'feature-workload','exec'))",
                           '--quit']
                prefix = 'FEATURE_RESULT '
            host_before = comparison.host_resources()
            started = time.monotonic()
            run_app(command, launch_env, folder)
            lines = (folder/'app.log').read_text().splitlines()
            matches = [line.removeprefix('FEATURE_RESULT ') for line in lines
                       if line.startswith(('FEATURE_RESULT ', '{"app":'))]
            assert len(matches) == 1, f'No result payload; see {folder}/app.log'
            result = json.loads(matches[0])
            assert result['results'], 'No matching workload was executed'
            validate_timings(result)
            if app == 'gimp':
                assert all(check['restoration']['verified'] and
                           check['restoration']['actual_undo_verified'] is False
                           for check in result.get('checks', [])), \
                    f'GIMP restoration controls failed; see {folder}/app.log'
                unexpected = {row['case'] for row in result['results']} - set(GIMP_CASES)
                assert not unexpected, f'GIMP ran unexpected cases: {unexpected}'
                ran = {row['case'] for row in result['results']}
                assert set(cases) - set(PICSIE_ONLY_CASES) <= ran, \
                    f'GIMP skipped suite cases: {sorted(set(cases) - set(PICSIE_ONLY_CASES) - ran)}'
            result.update(trial=trial, elapsed_s=time.monotonic()-started,
                          host_before=host_before, host_after=comparison.host_resources())
            (folder/'result.json').write_text(json.dumps(result, indent=2)+'\n')
            runs.append(result)
            print(app, trial, [(row['width'], row['case'],
                               round(comparison.desktop.stats([v['total_ms'] for v in row['samples']])['median'], 1))
                              for row in result['results']], flush=True)
    summary = {}
    for app in apps:
        summary[app] = {}
        seen = {(row['width'], row['case']) for run in runs if run['app'] == app
                for row in run['results']}
        for width, row_case in sorted(seen):
            key = f"{width}-{row_case}"
            available = [metric for metric in METRICS
                         if all(metric in sample
                                for run in runs if run['app'] == app
                                for candidate in run['results']
                                if (candidate['width'], candidate['case']) == (width, row_case)
                                for sample in candidate['samples'])]
            # Layer/mask samples predate the coverage stage and report the
            # legacy four metrics; selection samples report all five.
            summary[app][key] = {metric: comparison.desktop.stats([
                sample[metric] for run in runs if run['app'] == app
                for candidate in run['results']
                if (candidate['width'], candidate['case']) == (width, row_case)
                for sample in candidate['samples']]) for metric in available}
    if apps == ['picsie']:
        summary['gimp'] = {'status': 'skipped: Picsie-only case has no GIMP counterpart'}
    pixel_checks = []
    # Paired probes at BOTH sizes via the test-only Rust comparators
    # (bounded 256-row streaming, straight and premultiplied metrics for
    # RGBA; single-channel coverage metrics for .selcov channel dumps).
    for width, height in SIZES:
        probes = sorted((args.output/'picsie-1').glob(f'{width}-*.rgba'))
        if case:
            probes = [probe for probe in probes if probe.stem == f'{width}-{case}']
        elif args.suite or subset:
            probes = [probe for probe in probes if probe.stem.split(f'{width}-', 1)[1] in cases]
        for probe in probes:
            if probe.stem == f'{width}-opacity-preview':
                pixel_checks.append(dict(probe=probe.name, status='picsie-only, no GIMP counterpart'))
                continue
            counterpart = args.output/'gimp-1'/probe.name
            assert counterpart.is_file(), f'Missing GIMP probe for paired case {probe.name}'
            short = probe.stem.split(f'{width}-', 1)[1]
            if short in SELECTION_CASES:
                sel = probe.with_suffix('.selcov').name
                counterpart_sel = args.output/'gimp-1'/sel
                assert (args.output/'picsie-1'/sel).is_file(), f'Missing Picsie coverage dump {sel}'
                assert counterpart_sel.is_file(), f'Missing GIMP coverage dump for paired case {sel}'
                compared = subprocess.check_output(
                    [str(args.picsie), 'compare-sel', str(args.output/'picsie-1'/sel),
                     str(counterpart_sel), str(width), str(height)],
                    text=True)
                pixel_checks.append(dict(probe=sel, **json.loads(compared)))
                continue
            if short in MASKCOVERAGE_CASES:
                sel = probe.with_suffix('.maskcov').name
                counterpart_sel = args.output/'gimp-1'/sel
                assert (args.output/'picsie-1'/sel).is_file(), f'Missing Picsie mask coverage dump {sel}'
                assert counterpart_sel.is_file(), f'Missing GIMP mask coverage dump for paired case {sel}'
                compared = subprocess.check_output(
                    [str(args.picsie), 'compare-sel', str(args.output/'picsie-1'/sel),
                     str(counterpart_sel), str(width), str(height)],
                    text=True)
                pixel_checks.append(dict(probe=sel, **json.loads(compared)))
                # The RGBA composite below is compared as well: coverage and
                # composite are distinct paired outputs for this case.
            compared = subprocess.check_output(
                [str(args.picsie), 'compare', str(probe), str(counterpart), str(width), str(height)],
                text=True)
            pixel_checks.append(dict(probe=probe.name, **json.loads(compared)))
    report = dict(scope=manifest['scope'], gimp_version=version,
                  picsie_only_cases=PICSIE_ONLY_CASES,
                  summary=summary, pixel_checks=pixel_checks, runs=runs)
    (args.output/'results.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(dict(summary=summary, pixel_checks=pixel_checks), indent=2), flush=True)


if __name__ == '__main__':
    main()
