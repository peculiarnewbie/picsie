"""Select tracked editor scenarios; run sequentially and preserve provenance/failures.

This is test orchestration, never an editor engine. Scenario-owned measurements
are authoritative; this runner's wall time includes setup and is NOT a benchmark.
"""
import argparse
import ast
from contextlib import contextmanager
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import string
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent.parent
CATALOG = ROOT / 'scripts/reproduction-scenarios.json'
COVERAGE = ROOT / 'docs/reproducibility-coverage.json'
KINDS = {'correctness', 'native', 'engine-performance', 'visible-performance'}
DEFAULT_BINARY = ROOT / 'crates/picsie-desktop/target/release/picsie-desktop'
COMMAND_VALUES = {'python', 'tsx', 'desktop', 'addon', 'runtime', 'tools', 'display',
                  'gpu_icd', 'trials', 'samples', 'warmups', 'behavior_binary',
                  'feature_binary', 'stress_binary', 'matcher', 'fixtures', 'case_output'}


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def load_catalog(root=ROOT):
    catalog = json.loads((root / CATALOG.relative_to(ROOT)).read_text())
    registry = json.loads((root / 'docs/compositor-registry.json').read_text())
    rows = {item['id']: item for area in registry['areas'] for item in area['items']}
    if catalog.get('schemaVersion') != 1:
        raise ValueError('Unsupported scenario schema')
    ids = set()
    for scenario in catalog['scenarios']:
        sid = scenario['id']
        if not re.fullmatch(r'[a-z][a-z0-9._-]+', sid) or sid in ids:
            raise ValueError(f'Invalid/duplicate scenario: {sid}')
        ids.add(sid)
        if scenario['kind'] not in KINDS or not scenario['command']:
            raise ValueError(f'Invalid kind/command: {sid}')
        for token in scenario['command']:
            if not isinstance(token, str):
                raise ValueError(f'Command arguments must be strings: {sid}')
            for _, field, spec, conversion in string.Formatter().parse(token):
                if field is not None and (field not in COMMAND_VALUES or spec or conversion):
                    raise ValueError(f'Unknown/unsupported command placeholder: {sid}: {field}')
        requirements = set(scenario.get('requires', []))
        if requirements - {'desktop', 'addon', 'gimp', 'x11', 'behavior-fixtures', 'stress-fixtures', 'matcher'}:
            raise ValueError(f'Unknown prerequisites: {sid}')
        if set(scenario.get('build_examples', [])) - {'performance_behaviors', 'performance_features',
                                                        'performance_stress'}:
            raise ValueError(f'Unknown benchmark example: {sid}')
        for key in ('fixture', 'steps', 'assertions', 'sources'):
            if not scenario.get(key):
                raise ValueError(f'Missing {key}: {sid}')
        for source in scenario['sources']:
            path = root / source
            if not path.resolve().is_relative_to(root.resolve()) or source.startswith('artifacts/') or not path.is_file():
                raise ValueError(f'Scenario needs tracked source: {sid}: {source}')
        for feature in scenario['features']:
            if feature not in rows:
                raise ValueError(f'Unknown feature: {sid}: {feature}')
        driver = scenario.get('native_driver')
        scenario['named_checks'] = []
        if scenario['kind'] == 'native' and driver not in scenario['sources']:
            raise ValueError(f'Native driver must be a tracked scenario source: {sid}')
        if driver:
            tree = ast.parse((root / driver).read_text())
            scenario['named_checks'] = sorted({node.args[0].value for node in ast.walk(tree)
                if isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                and node.func.id == 'check' and node.args
                and isinstance(node.args[0], ast.Constant) and isinstance(node.args[0].value, str)})
            if not scenario['named_checks']:
                raise ValueError(f'Native scenario has no named assertions: {sid}')
    for name, selected in catalog['profiles'].items():
        if not selected or any(sid not in ids for sid in selected):
            raise ValueError(f'Invalid profile: {name}')
    return catalog, rows


def coverage(catalog, rows):
    result = []
    for fid, row in rows.items():
        matches = {'correctness': [], 'native': [], 'performance': []}
        for s in catalog['scenarios']:
            if s.get('umbrella'):
                continue  # A whole-suite pass is not per-feature coverage.
            explicit = fid in s['features']
            tests = s['kind'] == 'correctness' and bool(set(s['sources']) & set(row['evidence']))
            named = s['kind'] == 'native' and bool(set(s['named_checks']) & set(row.get('nativeChecks', [])))
            if row['implementation'] != 'missing' and (explicit or tests or named):
                key = s['kind'] if s['kind'] in ('correctness', 'native') else 'performance'
                matches[key].append(s['id'])
        gaps = []
        if row['implementation'] == 'missing':
            gaps.append('Feature missing; no executable behavior claim.')
        else:
            for key, values in matches.items():
                if not values:
                    gaps.append(f'No tracked {key} scenario mapped.')
        result.append({'feature': fid, 'implementation': row['implementation'], **matches,
                       'gaps': gaps, 'scope': 'Suite/check association only; not proof of every sub-behavior or full polish.'})
    return {'schemaVersion': 1, 'features': result}


def select(catalog, mapped, args):
    scenarios = {s['id']: s for s in catalog['scenarios']}
    requested = set(args.scenario or [])
    for feature in args.feature or []:
        row = next((r for r in mapped['features'] if r['feature'] == feature), None)
        if row is None:
            raise ValueError(f'Unknown feature: {feature}')
        keys = ['native'] if args.profile == 'native' else ['performance'] if args.profile == 'performance' else ['correctness']
        for key in keys:
            if not row[key]:
                raise ValueError(f'Feature {feature} has no mapped {key} scenario; inspect coverage gaps.')
            requested.update(row[key])
    if not requested and (args.feature or args.scenario):
        raise ValueError('Selected features have no mapped scenarios for this profile; inspect coverage gaps.')
    if not requested:
        if args.profile == 'performance':
            raise ValueError('Performance runs require explicit --scenario or --feature; no broad campaign by default.')
        requested.update(catalog['profiles'][args.profile])
    unknown = requested - scenarios.keys()
    if unknown:
        raise ValueError(f'Unknown scenarios: {", ".join(sorted(unknown))}')
    chosen = [s for s in catalog['scenarios'] if s['id'] in requested]
    if any('performance' in s['kind'] for s in chosen) and args.profile != 'performance':
        raise ValueError('Timing scenarios require --profile performance explicitly.')
    return chosen


def output_directory(path, root=ROOT):
    path = path.resolve()
    artifacts = (root / 'artifacts').resolve()
    if not path.is_relative_to(artifacts) or path == artifacts:
        raise ValueError('Outputs must use a fresh subdirectory under ignored artifacts/.')
    if path.exists():
        raise ValueError(f'Output already exists; choose a new directory: {path}')
    return path


def command(scenario, values, args):
    argv = [token.format_map(values) for token in scenario['command']]
    if args.case:
        if args.case not in scenario.get('cases', []):
            raise ValueError(f'Scenario {scenario["id"]} does not support case {args.case}.')
        argv += ['--case', args.case]
    wanted = getattr(args, 'cases', None)
    if args.case and wanted:
        raise ValueError('Choose --case or --cases, not both.')
    if wanted:
        subset = [value for value in wanted.split(',') if value]
        unknown = [value for value in subset if value not in scenario.get('cases', [])]
        if unknown:
            raise ValueError(f'Scenario {scenario["id"]} does not support cases {", ".join(unknown)}.')
        argv += ['--cases', ','.join(subset)]
    if scenario['kind'] == 'native':
        if args.tools:
            argv += ['--tools', str(args.tools.resolve())]
        if args.software:
            argv.append('--software')
        # Baseline reads explicit ICD from env; focused drivers also do so.
    return argv


def run_process(argv, log_path, env, cwd=ROOT, timeout=1800):
    started = time.monotonic()
    with log_path.open('w') as log:
        process = subprocess.Popen(argv, cwd=cwd, env=env, stdout=log,
                                   stderr=subprocess.STDOUT, start_new_session=os.name != 'nt')
        try:
            code = process.wait(timeout=timeout)
            status = 'passed' if code == 0 else 'failed'
        except (subprocess.TimeoutExpired, KeyboardInterrupt) as error:
            if os.name == 'nt':
                process.terminate()
            else:
                try:
                    # Python drivers own app/Xvfb sessions. SIGINT lets their
                    # finally blocks close those sessions before forced cleanup.
                    os.killpg(process.pid, signal.SIGINT)
                except ProcessLookupError:
                    pass
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                if os.name == 'nt':
                    process.kill()
                else:
                    os.killpg(process.pid, signal.SIGKILL)
                process.wait()
            if os.name != 'nt':
                # The direct child may exit before a descendant that ignores SIGINT.
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            code, status = process.returncode, 'interrupted' if isinstance(error, KeyboardInterrupt) else 'timeout'
    return {'command': argv, 'status': status, 'exitCode': code,
            'orchestrationWallSeconds': time.monotonic() - started, 'log': str(log_path)}


@contextmanager
def run_lock(root=ROOT):
    path = root / 'artifacts/reproduction.lock'
    path.parent.mkdir(exist_ok=True)
    with path.open('a+b') as lock:
        try:
            if os.name == 'nt':
                import msvcrt
                lock.write(b'0'); lock.flush(); lock.seek(0)
                msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError as error:
            raise ValueError('Another reproduction run holds the lock; run sequentially.') from error
        yield


def provenance(env, values):
    tracked = subprocess.check_output(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=ROOT).decode().split('\0')
    files = {name: digest(ROOT / name) for name in sorted(set(tracked)) if name and (ROOT / name).is_file()}
    status = subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True)
    binaries = {key: {'path': str(path), 'sha256': digest(path)} for key in
                ['desktop', 'addon', 'behavior_binary', 'feature_binary', 'stress_binary', 'matcher']
                if (path := Path(values[key])).is_file()}
    for name in ['gimp', 'gimp-console']:
        path = Path(values['runtime']) / 'usr/bin' / name
        if path.is_file():
            binaries[name] = {'path': str(path), 'sha256': digest(path)}
    return {'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
            'workingTreeStatus': status, 'sourceSha256': files, 'binaries': binaries,
            'platform': platform.platform(), 'machine': platform.machine(), 'cpuCount': os.cpu_count(),
            'cpuInfo': Path('/proc/cpuinfo').read_text() if Path('/proc/cpuinfo').exists() else None,
            'python': sys.version, 'backendEnvironment': {key: env.get(key) for key in
                ['VK_DRIVER_FILES', 'VK_ICD_FILENAMES', 'MESA_VK_WSI_DEBUG', 'WINIT_UNIX_BACKEND']},
            'gimpVersion': values.get('gimp_version'),
            'physicalDisplayLatency': False,
            'binaryFreshness': 'Rebuild requested; see build logs.' if values['build'] else 'Provided binary; source-to-binary freshness unverified.'}


def preflight(chosen, values, args, env):
    requirements = {r for s in chosen for r in s.get('requires', [])}
    problems = []
    if args.build and 'desktop' in requirements and args.binary.resolve() != DEFAULT_BINARY.resolve():
        problems.append('--build creates the default desktop binary; omit --build when testing a supplied binary.')
    for name in ('desktop', 'addon'):
        if name in requirements and not args.build and not Path(values[name]).is_file():
            problems.append(f'Missing {name}: {values[name]}; build it or use --build.')
    if 'gimp' in requirements:
        for name in ('gimp', 'gimp-console'):
            if not (Path(values['runtime']) / 'usr/bin' / name).is_file():
                problems.append(f'Missing pinned GIMP runtime: {name}; see docs/reproducibility.md.')
        executable = Path(values['runtime']) / 'usr/bin/gimp-console'
        if executable.is_file():
            version_env = {**env, 'LD_LIBRARY_PATH': str(Path(values['runtime']) / 'usr/lib')
                           + os.pathsep + env.get('LD_LIBRARY_PATH', '')}
            try:
                version = subprocess.check_output([str(executable), '--version'], env=version_env,
                                                  text=True, stderr=subprocess.STDOUT, timeout=15).strip()
                values['gimp_version'] = version
                if not version.endswith('version 3.2.6'):
                    problems.append(f'Pinned scenarios require GIMP 3.2.6; found {version}.')
            except (OSError, subprocess.SubprocessError) as error:
                problems.append(f'GIMP runtime version probe failed: {error}')
    for example in {e for s in chosen for e in s.get('build_examples', [])}:
        key = {'performance_behaviors': 'behavior_binary',
               'performance_features': 'feature_binary'}.get(example, 'stress_binary')
        if not args.build and not Path(values[key]).is_file():
            problems.append(f'Missing benchmark example: {values[key]}; use --build.')
    if 'x11' in requirements:
        if sys.platform != 'linux':
            problems.append('Native X11 flows currently require Linux.')
        for name in ('Xvfb', 'xdotool', 'convert', 'import', 'dbus-run-session', 'xclip'):
            if not shutil.which(name, path=env.get('PATH')):
                problems.append(f'Missing native test dependency: {name}')
        if Path('/tmp/.X11-unix/X' + args.display.removeprefix(':')).exists():
            problems.append(f'Display {args.display} already in use; choose --display.')
    if any(s['kind'] == 'visible-performance' for s in chosen) and not (args.gpu_icd or args.software):
        problems.append('Visible performance requires explicit --gpu-icd or --software to label the rendering backend.')
    for name in ('behavior-fixtures', 'stress-fixtures'):
        if name in requirements and not Path(values['fixtures']).is_dir():
            problems.append(f'Missing {name}: prepare via the documented CPU scenario or provide --fixtures.')
    if 'matcher' in requirements and not Path(values['matcher']).is_file():
        problems.append('Missing screen matcher; compile scripts/perf/screen-match.c as documented.')
    if problems:
        raise ValueError('\n'.join(problems))


def execute(chosen, args):
    if args.case and any(args.case not in s.get('cases', []) for s in chosen):
        raise ValueError('--case must be supported by every selected scenario; inspect the catalog.')
    output = output_directory(args.output or ROOT / 'artifacts/reproduction' /
        datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%S.%fZ'))
    tools = (args.tools or Path('/usr')).resolve()
    env = {**os.environ, 'CARGO_BUILD_JOBS': os.environ.get('CARGO_BUILD_JOBS', '1'), 'PYTHONDONTWRITEBYTECODE': '1'}
    env.pop('PICSIE_TRACE_DIR', None)  # Performance drivers own any untimed probes.
    if args.tools:
        env['PATH'] = str(tools / 'bin') + os.pathsep + env.get('PATH', '')
        env['LD_LIBRARY_PATH'] = str(tools / 'lib') + os.pathsep + env.get('LD_LIBRARY_PATH', '')
    driver = args.gpu_icd
    if args.software:
        drivers = sorted((tools / 'share/vulkan/icd.d').glob('lvp_icd*.json'))
        if not drivers:
            raise ValueError('Lavapipe ICD missing; install mesa-vulkan-drivers or supply --tools.')
        driver = drivers[0]
    if driver:
        if not driver.is_file():
            raise ValueError(f'ICD missing: {driver}')
        env['VK_DRIVER_FILES'] = str(driver.resolve())
    values = {'python': sys.executable, 'tsx': str(ROOT / 'node_modules/.bin/tsx'),
              'desktop': str(args.binary.resolve()), 'addon': str(ROOT / 'native/picsie.node'),
              'runtime': str(args.runtime.resolve()), 'tools': str(tools), 'display': args.display,
              'gpu_icd': str(driver.resolve()) if driver else '', 'build': args.build,
              'trials': str(args.trials), 'samples': str(args.samples), 'warmups': str(args.warmups),
              'behavior_binary': str(ROOT / 'target/release/examples/performance_behaviors'),
              'feature_binary': str(ROOT / 'target/release/examples/performance_features'),
              'stress_binary': str(ROOT / 'target/release/examples/performance_stress'),
              'matcher': str(args.matcher.resolve()), 'fixtures': str(args.fixtures.resolve()) if args.fixtures else ''}
    preflight(chosen, values, args, env)
    with run_lock():
        output.mkdir(parents=True)
        report = {'schemaVersion': 1, 'startedUtc': datetime.now(timezone.utc).isoformat(),
                  'requestedScenarios': [s['id'] for s in chosen], 'results': [], 'status': 'running',
                  'measurementWarning': 'Orchestration wall time includes setup; never use it as feature latency.'}
        def save():
            (output / 'run.json').write_text(json.dumps(report, indent=2) + '\n')
        save()
        try:
            report['preBuildProvenance'] = provenance(env, {**values, 'build': False}); save()
            builds = []
            if args.build:
                if any('desktop' in s.get('requires', []) for s in chosen):
                    builds.append(['npm', 'run', 'build:desktop'])
                if any('addon' in s.get('requires', []) for s in chosen):
                    builds.append(['npm', 'run', 'build:native'])
                examples = sorted({e for s in chosen for e in s.get('build_examples', [])})
                for example in examples:
                    builds.append(['cargo', 'build', '--locked', '--release', '-p', 'picsie-core', '--example', example])
            for n, argv in enumerate(builds):
                result = run_process(argv, output / f'build-{n}.log', env, timeout=args.timeout)
                report['results'].append({'scenario': 'build', **result}); save()
                if result['status'] != 'passed':
                    raise ValueError(f'Build failed; see {result["log"]}')
            report['provenance'] = provenance(env, values); save()
            for scenario in chosen:
                folder = output / scenario['id']
                folder.mkdir()
                case_output = folder / 'evidence'  # Drivers that reject existing cohorts create this themselves.
                run_values = {**values, 'case_output': str(case_output),
                              'fixtures': values['fixtures'] or str(folder / 'fixtures')}
                result = run_process(command(scenario, run_values, args), folder / 'run.log', env, timeout=args.timeout)
                evidence = case_output / 'report.json'
                if scenario['kind'] == 'native' and not evidence.is_file():
                    result['status'] = 'failed'
                    result['error'] = 'Native driver did not produce its assertion report.'
                if scenario['kind'] == 'native' and evidence.is_file():
                    result['nativeReport'] = json.loads(evidence.read_text())
                    if result['nativeReport'].get('error') or not result['nativeReport'].get('checks'):
                        result['status'] = 'failed'
                result.update(scenario=scenario['id'], kind=scenario['kind'])
                if scenario['id'] == 'correctness.all':
                    result['postBuildProvenance'] = provenance(env, {**values, 'build': True})
                report['results'].append(result); save()
                print(f'{scenario["id"]}: {result["status"]} — {folder}', flush=True)
                if result['status'] != 'passed':
                    raise ValueError(f'Scenario failed; see {result["log"]}')
            report['status'] = 'passed'
        except (ValueError, OSError, KeyboardInterrupt) as error:
            report['status'], report['error'] = 'failed', str(error)
        finally:
            report['finishedUtc'] = datetime.now(timezone.utc).isoformat()
            report['notRun'] = [s['id'] for s in chosen if s['id'] not in {r['scenario'] for r in report['results']}]
            report['evidenceSha256'] = {str(p.relative_to(output)): digest(p) for p in output.rglob('*')
                if p.is_file() and p.suffix in ('.json', '.png', '.picsie', '.ora', '.xcf', '.rgba', '.selcov', '.maskcov', '.log')
                and p != output / 'run.json'}
            save()
        print(f'Run {report["status"]}: {output / "run.json"}')
        return 0 if report['status'] == 'passed' else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['list', 'coverage', 'check', 'run'])
    parser.add_argument('--profile', choices=['quick', 'native', 'performance'], default='quick')
    parser.add_argument('--scenario', action='append')
    parser.add_argument('--feature', action='append')
    parser.add_argument('--case', help='Narrow a supporting scenario to one cataloged workload (performance.behaviors, performance.features, performance.selections, performance.recent-features).')
    parser.add_argument('--cases', help='Comma-separated subset of cataloged workloads for the chosen scenario; every entry must belong to it.')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--binary', type=Path, default=DEFAULT_BINARY)
    parser.add_argument('--tools', type=Path)
    parser.add_argument('--runtime', type=Path, default=ROOT / 'artifacts/gimp-performance/runtime')
    parser.add_argument('--fixtures', type=Path)
    parser.add_argument('--matcher', type=Path, default=ROOT / 'artifacts/perf-stress-2026-10-01/screen-match.so')
    parser.add_argument('--display', default=':119')
    parser.add_argument('--gpu-icd', type=Path)
    parser.add_argument('--software', action='store_true')
    parser.add_argument('--build', action='store_true')
    parser.add_argument('--trials', type=int, default=3)
    parser.add_argument('--samples', type=int, default=4)
    parser.add_argument('--warmups', type=int, default=2)
    parser.add_argument('--timeout', type=int, default=1800)
    args = parser.parse_args()
    if min(args.trials, args.samples, args.timeout) < 1 or args.warmups < 0:
        parser.error('Trials/samples/timeout must be positive; warmups nonnegative.')
    if args.gpu_icd and args.software:
        parser.error('Choose --gpu-icd or --software.')
    try:
        catalog, rows = load_catalog()
        mapped = coverage(catalog, rows)
        if args.action == 'coverage':
            COVERAGE.write_text(json.dumps(mapped, indent=2) + '\n')
            print(f'Mapped all {len(mapped["features"])} feature rows; gaps remain explicit.')
        elif args.action == 'check':
            if json.loads(COVERAGE.read_text()) != mapped:
                raise ValueError('Reproducibility coverage stale; run npm run reproduce:coverage.')
            print(f'{len(catalog["scenarios"])} scenarios validated; {len(rows)} feature rows accounted for.')
        elif args.action == 'list':
            for scenario in catalog['scenarios']:
                print(f'{scenario["id"]:36} {scenario["kind"]:20} {scenario["fixture"]}')
        else:
            return execute(select(catalog, mapped, args), args)
    except (ValueError, OSError) as error:
        print(f'Reproduction error: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
