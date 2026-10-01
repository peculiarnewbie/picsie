"""Compare six additional behaviors through existing native editor/public APIs.

CPU command and complete RGBA availability timings are separate from GUI latency.
Requires the extracted GIMP runtime described in docs/gimp-performance.md.
Build the Rust workload with cargo build --release -p picsie-core
--example performance_behaviors. All temporary fixtures/results stay in artifacts/.
"""
import argparse
import hashlib
import importlib.util
import json
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


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path,
                        default=Path('artifacts/perf-behaviors-2026-10-01/cpu'))
    parser.add_argument('--fixtures', type=Path,
                        default=Path('artifacts/perf-behaviors-2026-10-01/fixtures'))
    parser.add_argument('--runtime', type=Path,
                        default=Path('artifacts/gimp-performance/runtime'))
    parser.add_argument('--picsie', type=Path,
                        default=Path('target/release/examples/performance_behaviors'))
    parser.add_argument('--warmups', type=int, default=5)
    parser.add_argument('--samples', type=int, default=16)
    parser.add_argument('--trials', type=int, default=3)
    parser.add_argument('--case', help='Run just one named workload for setup/pilots')
    parser.add_argument('--grid', type=int, default=13,
                        help='Checker tile width; 16 aligns edges with half-size nearest sampling')
    args = parser.parse_args()
    if args.samples < 1 or args.trials < 1 or args.warmups < 0 or not 1 <= args.grid <= 1000:
        parser.error('Samples/trials must be positive, warmups nonnegative and grid between 1 and 1000')
    for name in ['output', 'fixtures', 'runtime', 'picsie']:
        setattr(args, name, getattr(args, name).resolve())
    args.output.mkdir(parents=True, exist_ok=True)
    if (args.output/'manifest.json').exists():
        parser.error('Output already contains a cohort; choose a new directory')
    workload = ROOT/'scripts/perf/gimp-behavior-workload.py'
    fixture_env = {**os.environ, 'PICSIE_PERF_GRID': str(args.grid)}
    subprocess.run([str(args.picsie), 'prepare', str(args.fixtures)], env=fixture_env, check=True)
    env = {**fixture_env, **comparison.runtime_environment(args.runtime)}
    env['PATH'] = str(args.runtime/'usr/bin') + os.pathsep + env.get('PATH', '')
    config = args.output/'gimp-config'
    config.mkdir(exist_ok=True)
    (config/'gimprc').write_text('(show-welcome-dialog no)\n(check-updates no)\n(use-opencl no)\n')
    env.update(GIMP3_DIRECTORY=str(config), GIMP3_CACHEDIR=str(args.output/'gimp-cache'),
               GEGL_USE_OPENCL='no', PICSIE_PERF_FIXTURES=str(args.fixtures),
               PICSIE_PERF_WARMUPS=str(args.warmups), PICSIE_PERF_SAMPLES=str(args.samples))
    if args.case:
        env['PICSIE_PERF_CASE'] = args.case
    else:
        env.pop('PICSIE_PERF_CASE', None)
    manifest = {
        'scope': 'CPU command plus complete RGBA availability, not visible input latency',
        'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'warmups_per_case': args.warmups, 'samples_per_case': args.samples,
        'trials': args.trials, 'case_filter': args.case,
        'checker_grid_doc_px': args.grid,
        'launch_order': 'Alternating applications each trial; sequential timed workloads',
        'source_sha256': {str(path.relative_to(ROOT)): digest(path) for path in [
            Path(__file__), workload, ROOT/'crates/picsie-core/examples/performance_behaviors.rs']},
        'fixture_sha256': {path.name: digest(path) for path in args.fixtures.iterdir() if path.is_file()},
        'binary_sha256': {'picsie': digest(args.picsie),
                          'gimp': digest((args.runtime/'usr/bin/gimp-console').resolve())},
    }
    (args.output/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    (args.output/'working-tree.patch').write_bytes(subprocess.check_output(['git', 'diff'], cwd=ROOT))
    runs = []
    for trial in range(1, args.trials+1):
        for app in (['picsie', 'gimp'] if trial % 2 else ['gimp', 'picsie']):
            folder = args.output/f'{app}-{trial}'
            folder.mkdir()
            launch_env = {**env, 'PICSIE_PERF_OUTPUT': str(folder)}
            if app == 'picsie':
                command = [str(args.picsie), 'run', str(folder),
                           str(args.warmups), str(args.samples)]
                if args.case:
                    command.append(args.case)
            else:
                command = [str(args.runtime/'usr/bin/gimp-console'), '--no-interface',
                           '--new-instance', '--batch-interpreter=python-fu-eval', '--batch',
                           f"exec(compile(open({str(workload)!r}).read(),'behavior-workload','exec'))",
                           '--quit']
            host_before = comparison.host_resources()
            started = time.monotonic()
            process = None
            try:
                with (folder/'app.log').open('w') as log:
                    process = subprocess.Popen(command, env=launch_env, stdout=log,
                                               stderr=subprocess.STDOUT, start_new_session=True)
                    returncode = process.wait(timeout=900)
                assert returncode == 0, f'{app} failed; see {folder}/app.log'
            finally:
                comparison.desktop.stop(process)
            lines = (folder/'app.log').read_text().splitlines()
            matches = [line.removeprefix('BEHAVIOR_RESULT ') for line in lines
                       if line.startswith(('BEHAVIOR_RESULT ', '{"app":'))]
            assert len(matches) == 1, f'No result payload; see {folder}/app.log'
            result = json.loads(matches[0])
            assert result['results'], 'No matching workload was executed'
            result.update(trial=trial, elapsed_s=time.monotonic()-started,
                          host_before=host_before, host_after=comparison.host_resources())
            (folder/'result.json').write_text(json.dumps(result, indent=2)+'\n')
            runs.append(result)
            print(app, trial, [(row['width'], row['case'],
                               round(comparison.desktop.stats([v['total_ms'] for v in row['samples']])['median'], 1))
                              for row in result['results']], flush=True)
    summary = {}
    for app in ['picsie', 'gimp']:
        summary[app] = {}
        for row in next(run for run in runs if run['app'] == app)['results']:
            key = f"{row['width']}-{row['case']}"
            summary[app][key] = {metric: comparison.desktop.stats([
                sample[metric] for run in runs if run['app'] == app
                for candidate in run['results']
                if (candidate['width'], candidate['case']) == (row['width'], row['case'])
                for sample in candidate['samples']]) for metric in ['command_ms', 'total_ms']}
    pixel_checks = []
    # Ignore RGB where both pixels are transparent; also report premultiplied
    # color separately, because raw RGB can exaggerate near-transparent errors.
    # Small probes keep comparison memory bounded. Both sizes also have per-sample assertions.
    for path in sorted((args.output/'picsie-1').glob('1200-*.rgba')):
        ours = path.read_bytes()
        theirs = (args.output/'gimp-1'/path.name).read_bytes()
        assert len(ours) == len(theirs)
        different = max_difference = visible_different = max_visible_difference = max_alpha_difference = 0
        for offset in range(0, len(ours), 4):
            a, b = ours[offset:offset+4], theirs[offset:offset+4]
            differences = [abs(a[3]-b[3])]
            if a[3] or b[3]:
                differences.extend(abs(x-y) for x, y in zip(a[:3], b[:3]))
            if max(differences):
                different += 1
                max_difference = max(max_difference, max(differences))
            alpha_difference = abs(a[3]-b[3])
            max_alpha_difference = max(max_alpha_difference, alpha_difference)
            visible = [alpha_difference, *[
                abs((x*a[3]+127)//255-(y*b[3]+127)//255)
                for x, y in zip(a[:3], b[:3])]]
            if max(visible):
                visible_different += 1
                max_visible_difference = max(max_visible_difference, max(visible))
        pixel_checks.append(dict(probe=path.name, differing_pixels=different,
                                 max_channel_difference=max_difference,
                                 max_alpha_difference=max_alpha_difference,
                                 differing_premultiplied_pixels=visible_different,
                                 max_premultiplied_channel_difference=max_visible_difference))
    report = dict(scope=manifest['scope'], summary=summary, pixel_checks=pixel_checks, runs=runs)
    (args.output/'results.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(dict(summary=summary, pixel_checks=pixel_checks), indent=2), flush=True)


if __name__ == '__main__':
    main()
