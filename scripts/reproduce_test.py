"""Exercise real child processes and coverage/safety boundaries of the runner."""
import argparse
import ast
import copy
import importlib.util
import json
import os
import re
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

import reproduce

FEATURE_COMPARISON = importlib.util.spec_from_file_location(
    'compare_feature_performance', reproduce.ROOT / 'scripts/compare-feature-performance.py')
feature_comparison = importlib.util.module_from_spec(FEATURE_COMPARISON)
FEATURE_COMPARISON.loader.exec_module(feature_comparison)


class ReproductionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.catalog, cls.rows = reproduce.load_catalog()
        cls.mapped = reproduce.coverage(cls.catalog, cls.rows)

    def selection(self, **kwargs):
        args = argparse.Namespace(profile='quick', scenario=None, feature=None)
        for key, value in kwargs.items():
            setattr(args, key, value)
        return reproduce.select(self.catalog, self.mapped, args)

    def test_default_runs_correctness_without_timing(self):
        self.assertEqual([s['id'] for s in self.selection()], ['correctness.all'])
        with self.assertRaisesRegex(ValueError, 'explicit'):
            self.selection(profile='performance')
        with self.assertRaisesRegex(ValueError, 'explicitly'):
            self.selection(scenario=['performance.behaviors'])
        self.assertEqual([s['id'] for s in self.selection(profile='performance', scenario=['performance.behaviors'])],
                         ['performance.behaviors'])

    def test_every_registry_feature_accounted_for_without_missing_feature_claims(self):
        mapped = {row['feature']: row for row in self.mapped['features']}
        self.assertEqual(set(mapped), set(self.rows))
        self.assertEqual(len(mapped), len(self.mapped['features']))
        for fid, row in self.rows.items():
            self.assertNotIn('correctness.all', mapped[fid]['correctness'])
            if row['implementation'] == 'missing':
                self.assertEqual([mapped[fid][k] for k in ('correctness', 'native', 'performance')], [[], [], []])
                self.assertTrue(mapped[fid]['gaps'])
        self.assertIn('native.groups', mapped['transform.group-box']['native'])
        self.assertIn('native.adjustments', mapped['adjustments.curves']['native'])

    def test_no_silent_omission_of_uncovered_features(self):
        missing = next(row['feature'] for row in self.mapped['features'] if not row['native'])
        with self.assertRaisesRegex(ValueError, 'no mapped native'):
            self.selection(profile='native', feature=['transform.group-box', missing])
        with self.assertRaisesRegex(ValueError, 'Unknown feature'):
            self.selection(feature=['invented.feature'])
        with self.assertRaisesRegex(ValueError, 'Unknown scenarios'):
            self.selection(scenario=['invented.scenario'])

    def test_pilot_case_matches_both_real_workloads(self):
        scenario = next(s for s in self.catalog['scenarios'] if s['id'] == 'performance.behaviors')
        rust = (reproduce.ROOT / 'crates/picsie-core/examples/performance_behaviors.rs').read_text()
        rust_cases = re.findall(r'"([a-z0-9-]+)"', rust.split('const CASES:')[1].split('];')[0])
        tree = ast.parse((reproduce.ROOT / 'scripts/perf/gimp-behavior-workload.py').read_text())
        gimp_cases = next(ast.literal_eval(node.value) for node in tree.body if isinstance(node, ast.Assign)
                          and any(isinstance(target, ast.Name) and target.id == 'CASES' for target in node.targets))
        self.assertEqual(scenario['cases'], rust_cases)
        self.assertEqual(scenario['cases'], list(gimp_cases))
        args = argparse.Namespace(case='crop-retained', tools=None, software=False)
        values = {key: 'path with spaces' for key in reproduce.COMMAND_VALUES}
        argv = reproduce.command(scenario, values, args)
        self.assertEqual(argv[-2:], ['--case', 'crop-retained'])
        self.assertIn('path with spaces', argv)
        args.case = 'unknown'
        with self.assertRaises(ValueError):
            reproduce.command(scenario, values, args)

    def test_feature_case_catalog_consistency_and_launch_selection(self):
        scenario = next(s for s in self.catalog['scenarios'] if s['id'] == 'performance.features')
        selections = next(s for s in self.catalog['scenarios'] if s['id'] == 'performance.selections')
        recent = next(s for s in self.catalog['scenarios'] if s['id'] == 'performance.recent-features')
        rust = (reproduce.ROOT / 'crates/picsie-core/examples/performance_features.rs').read_text()
        rust_cases = re.findall(r'"([a-z0-9-]+)"', rust.split('const CASES:')[1].split('];')[0])
        rust_selection_cases = re.findall(r'"([a-z0-9-]+)"', rust.split('const SELECTION_CASES:')[1].split('];')[0])
        rust_recent_cases = re.findall(r'"([a-z0-9-]+)"', rust.split('const RECENT_CASES:')[1].split('];')[0])
        tree = ast.parse((reproduce.ROOT / 'scripts/perf/gimp-feature-workload.py').read_text())
        gimp_cases = next(ast.literal_eval(node.value) for node in tree.body if isinstance(node, ast.Assign)
                          and any(isinstance(target, ast.Name) and target.id == 'CASES' for target in node.targets))
        self.assertEqual(scenario['cases'], rust_cases)
        self.assertEqual(scenario['cases'], feature_comparison.LAYER_MASK_CASES)
        self.assertEqual(selections['cases'], rust_selection_cases)
        self.assertEqual(selections['cases'], feature_comparison.SELECTION_CASES)
        self.assertEqual(recent['cases'], rust_recent_cases)
        self.assertEqual(recent['cases'], feature_comparison.RECENT_CASES)
        self.assertEqual(scenario['cases'] + selections['cases'] + recent['cases'],
                         feature_comparison.FEATURE_CASES)
        self.assertEqual(list(gimp_cases), feature_comparison.GIMP_CASES)
        self.assertEqual(set(scenario['cases'] + selections['cases'] + recent['cases']) - set(gimp_cases),
                         set(feature_comparison.PICSIE_ONLY_CASES))
        self.assertEqual(feature_comparison.PICSIE_ONLY_CASES, ['opacity-preview'])
        # Suites keep cohorts separate: selection cases never match the
        # layer/mask suite and vice versa, and recent cases match only
        # their own suite.
        self.assertEqual(feature_comparison.SUITES,
                         {'layers-masks': feature_comparison.LAYER_MASK_CASES,
                          'selections': feature_comparison.SELECTION_CASES,
                          'recent-features': feature_comparison.RECENT_CASES})
        self.assertEqual(feature_comparison.resolve_cases([], 'selections'),
                         feature_comparison.SELECTION_CASES)
        self.assertEqual(feature_comparison.resolve_cases([], 'layers-masks'),
                         feature_comparison.LAYER_MASK_CASES)
        self.assertEqual(feature_comparison.resolve_cases([], 'recent-features'),
                         feature_comparison.RECENT_CASES)
        with self.assertRaisesRegex(ValueError, 'Unknown feature workload'):
            feature_comparison.resolve_case('mask-paint', 'selections')
        with self.assertRaisesRegex(ValueError, 'Unknown feature workload'):
            feature_comparison.resolve_case('wand-contiguous', 'layers-masks')
        with self.assertRaisesRegex(ValueError, 'Unknown feature workload'):
            feature_comparison.resolve_case('group-resize', 'selections')
        with self.assertRaisesRegex(ValueError, 'Unknown feature workload'):
            feature_comparison.resolve_case('mask-paint', 'recent-features')
        with self.assertRaisesRegex(ValueError, 'Unknown feature workload'):
            feature_comparison.resolve_case('levels-update', 'layers-masks')
        # Unknown cases fail before any fixture work.
        self.assertIsNone(feature_comparison.resolve_case(None))
        self.assertEqual(feature_comparison.resolve_case('mask-paint'), 'mask-paint')
        self.assertEqual(feature_comparison.resolve_case('wand-contiguous'), 'wand-contiguous')
        with self.assertRaisesRegex(ValueError, 'Unknown feature workload'):
            feature_comparison.resolve_case('invented-case')
        # Picsie-only cases skip the GIMP launch; everything else pairs.
        self.assertEqual(feature_comparison.select_apps('opacity-preview'), ['picsie'])
        self.assertEqual(feature_comparison.select_apps('mask-paint'), ['picsie', 'gimp'])
        self.assertEqual(feature_comparison.select_apps('wand-contiguous'), ['picsie', 'gimp'])
        self.assertEqual(feature_comparison.select_apps('group-resize'), ['picsie', 'gimp'])
        self.assertEqual(feature_comparison.select_apps('levels-update'), ['picsie', 'gimp'])
        self.assertEqual(feature_comparison.select_apps('select-layer-alpha'), ['picsie', 'gimp'])
        self.assertEqual(feature_comparison.select_apps(None), ['picsie', 'gimp'])
        args = argparse.Namespace(case='opacity-preview', tools=None, software=False)
        values = {key: 'path with spaces' for key in reproduce.COMMAND_VALUES}
        argv = reproduce.command(scenario, values, args)
        self.assertEqual(argv[-2:], ['--case', 'opacity-preview'])
        self.assertIn('--suite', argv)
        self.assertIn('layers-masks', argv)
        args = argparse.Namespace(case='wand-contiguous', tools=None, software=False)
        argv = reproduce.command(selections, values, args)
        self.assertEqual(argv[-2:], ['--case', 'wand-contiguous'])
        self.assertIn('selections', argv)
        args = argparse.Namespace(case='mask-paint', tools=None, software=False)
        with self.assertRaises(ValueError):
            reproduce.command(selections, values, args)
        args = argparse.Namespace(case='group-resize', tools=None, software=False)
        argv = reproduce.command(recent, values, args)
        self.assertEqual(argv[-2:], ['--case', 'group-resize'])
        self.assertIn('recent-features', argv)
        args = argparse.Namespace(case='mask-paint', tools=None, software=False)
        with self.assertRaises(ValueError):
            reproduce.command(recent, values, args)
        # Tracked multi-case subsets never launch unlisted batches.
        self.assertEqual(
            feature_comparison.resolve_cases(
                ['group-resize', 'gradient-mask-linear'], 'recent-features'),
            ['group-resize', 'gradient-mask-linear'])
        with self.assertRaisesRegex(ValueError, 'Unknown feature workload'):
            feature_comparison.resolve_cases(['group-resize', 'mask-paint'],
                                             'recent-features')
        args = argparse.Namespace(case=None, cases='group-resize,gradient-mask-linear',
                                  tools=None, software=False)
        argv = reproduce.command(recent, values, args)
        self.assertEqual(argv[-2:], ['--cases', 'group-resize,gradient-mask-linear'])
        self.assertIn('recent-features', argv)
        args = argparse.Namespace(case=None, cases='group-resize,mask-paint',
                                  tools=None, software=False)
        with self.assertRaises(ValueError):
            reproduce.command(recent, values, args)
        args = argparse.Namespace(case='group-resize', cases='group-rotate',
                                  tools=None, software=False)
        with self.assertRaises(ValueError):
            reproduce.command(recent, values, args)

    def test_measured_stage_accounting_rejects_metadata_double_counting(self):
        # Retained GIMP group-resize sample from the review cohort. The
        # render window accidentally included excluded hierarchy metadata.
        sample = {'command_ms': 38.593709003180265, 'coverage_ms': 0.,
                  'render_ms': 19.67036712449044, 'read_ms': 5.059094866737723,
                  'total_ms': 62.63765809126198, 'metadata_ms': 0.6895099068060517}
        result = {'app': 'gimp', 'results': [{'case': 'group-resize', 'samples': [sample]}]}
        with self.assertRaisesRegex(ValueError, 'stages exceed total'):
            feature_comparison.validate_timings(result)
        sample['render_ms'] -= sample['metadata_ms']
        feature_comparison.validate_timings(result)
        for invalid in (float('nan'), float('inf'), -1.):
            with self.subTest(invalid=invalid):
                changed = copy.deepcopy(result)
                changed['results'][0]['samples'][0]['command_ms'] = invalid
                with self.assertRaisesRegex(ValueError, 'Invalid timing'):
                    feature_comparison.validate_timings(changed)

    def test_output_requires_fresh_ignored_directory_and_rejects_symlink_escape(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            artifacts = root / 'artifacts'
            artifacts.mkdir()
            self.assertEqual(reproduce.output_directory(artifacts / 'fresh', root), artifacts / 'fresh')
            for path in (artifacts, root / 'outside', artifacts / 'existing'):
                if path.name == 'existing':
                    path.mkdir()
                with self.assertRaises(ValueError):
                    reproduce.output_directory(path, root)
            if os.name != 'nt':
                (artifacts / 'escape').symlink_to(root, target_is_directory=True)
                with self.assertRaises(ValueError):
                    reproduce.output_directory(artifacts / 'escape/new', root)

    def test_real_process_exit_and_logs_are_preserved(self):
        with tempfile.TemporaryDirectory() as temp:
            for code, status in ((0, 'passed'), (7, 'failed')):
                log = Path(temp) / f'{code}.log'
                result = reproduce.run_process(
                    [sys.executable, '-c', f'print("evidence"); raise SystemExit({code})'], log,
                    os.environ.copy(), cwd=Path(temp), timeout=5)
                self.assertEqual(result['status'], status)
                self.assertEqual(result['exitCode'], code)
                self.assertIn('evidence', log.read_text())

    @unittest.skipIf(os.name == 'nt', 'POSIX process-group cleanup assertion')
    def test_timeout_terminates_child_process_group(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            child = ('import signal,time,pathlib; signal.signal(signal.SIGTERM, signal.SIG_IGN); '
                     'signal.signal(signal.SIGINT, signal.SIG_IGN); '
                     'pathlib.Path("started").touch(); '
                     'time.sleep(30); pathlib.Path("leaked").touch()')
            parent = ('import subprocess,sys,time; '
                      f'p=subprocess.Popen([sys.executable,"-c",{child!r}]); '
                      'print(p.pid,flush=True); time.sleep(30)')
            result = reproduce.run_process([sys.executable, '-c', parent], root / 'timeout.log',
                                           os.environ.copy(), cwd=root, timeout=1)
            self.assertEqual(result['status'], 'timeout')
            self.assertTrue((root / 'started').is_file())
            pid = int((root / 'timeout.log').read_text().splitlines()[0])
            # A terminated orphan can briefly be a zombie until init reaps it.
            status = Path(f'/proc/{pid}/status')
            deadline = time.monotonic() + 2
            while status.exists() and 'State:\tZ' not in status.read_text() and time.monotonic() < deadline:
                time.sleep(0.01)
            if status.exists():
                self.assertIn('State:\tZ', status.read_text())
            self.assertFalse((root / 'leaked').exists())

    @unittest.skipIf(os.name == 'nt', 'POSIX native-driver session cleanup')
    def test_timeout_allows_driver_to_close_its_separate_app_session(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            parent = ('import subprocess,sys,time,pathlib; '
                      'p=subprocess.Popen([sys.executable,"-c","import time; time.sleep(30)"],start_new_session=True); '
                      '\ntry: time.sleep(30)'
                      '\nfinally: p.kill(); p.wait(); pathlib.Path("closed-session").touch()')
            result = reproduce.run_process([sys.executable, '-c', parent], root / 'timeout.log',
                                           os.environ.copy(), cwd=root, timeout=1)
            self.assertEqual(result['status'], 'timeout')
            self.assertTrue((root / 'closed-session').is_file())

    def test_lock_rejects_second_process_and_releases_after_run(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            script = ('import sys; from pathlib import Path; import reproduce; '
                      '\nwith reproduce.run_lock(Path(sys.argv[1])): print("acquired")')
            env = {**os.environ, 'PYTHONPATH': str(reproduce.ROOT / 'scripts')}
            with reproduce.run_lock(root):
                other = subprocess.run([sys.executable, '-c', script, temp], env=env, capture_output=True, text=True)
                self.assertNotEqual(other.returncode, 0)
                self.assertIn('Another reproduction run', other.stderr)
            other = subprocess.run([sys.executable, '-c', script, temp], env=env, capture_output=True, text=True)
            self.assertEqual(other.returncode, 0, other.stderr)

    def test_catalog_rejects_missing_source_unknown_features_and_placeholders(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / 'scripts').mkdir()
            (root / 'docs').mkdir()
            (root / 'driver.py').write_text('check("native assertion", True)\n')
            registry = {'areas': [{'items': [{'id': 'test.feature'}]}]}
            (root / 'docs/compositor-registry.json').write_text(json.dumps(registry))
            base = {'schemaVersion': 1, 'profiles': {'quick': ['test.scenario']}, 'scenarios': [{
                'id': 'test.scenario', 'kind': 'native', 'command': ['{python}', 'driver.py'],
                'fixture': 'fixed', 'steps': ['run'], 'assertions': ['check'],
                'features': ['test.feature'], 'sources': ['driver.py'], 'native_driver': 'driver.py'}]}
            mutations = [('sources', ['missing.py']), ('features', ['unknown.feature']),
                         ('command', ['{unknown}']), ('requires', ['unknown']), ('native_driver', 'missing.py')]
            for key, value in mutations:
                catalog = copy.deepcopy(base)
                catalog['scenarios'][0][key] = value
                (root / 'scripts/reproduction-scenarios.json').write_text(json.dumps(catalog))
                with self.subTest(key=key), self.assertRaises(ValueError):
                    reproduce.load_catalog(root)
            (root / 'scripts/reproduction-scenarios.json').write_text(json.dumps(base))
            catalog, _ = reproduce.load_catalog(root)
            self.assertEqual(catalog['scenarios'][0]['named_checks'], ['native assertion'])


if __name__ == '__main__':
    unittest.main()
