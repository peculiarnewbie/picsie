"""Compare two release desktop applications through their actual X11 output.

Defaults compare QuickGUI with GPUI; named baselines support before/after runs.

No app tracing or benchmark-only editor code is enabled. Requires Xvfb, xdotool,
ImageMagick, libX11 and libXtst. Results stay under ignored artifacts/. The visible
latencies end at Xvfb's framebuffer, not GPU fences, a compositor, or scanout.
"""
import argparse
import ctypes as C
import ctypes.util
import json
import math
import os
from pathlib import Path
import re
import signal
import statistics
import subprocess
import time


class XImage(C.Structure):
    _fields_ = [(name, kind) for name, kind in [
        ('width', C.c_int), ('height', C.c_int), ('xoffset', C.c_int), ('format', C.c_int),
        ('data', C.c_void_p), ('byte_order', C.c_int), ('bitmap_unit', C.c_int),
        ('bitmap_bit_order', C.c_int), ('bitmap_pad', C.c_int), ('depth', C.c_int),
        ('bytes_per_line', C.c_int), ('bits_per_pixel', C.c_int),
        ('red_mask', C.c_ulong), ('green_mask', C.c_ulong), ('blue_mask', C.c_ulong),
    ]]


class Screen:
    def __init__(self, display):
        self.x = C.CDLL(ctypes.util.find_library('X11'))
        self.test = C.CDLL(ctypes.util.find_library('Xtst'))
        signatures = {
            'XOpenDisplay': ([C.c_char_p], C.c_void_p),
            'XDefaultRootWindow': ([C.c_void_p], C.c_ulong),
            'XGetImage': ([C.c_void_p, C.c_ulong, C.c_int, C.c_int, C.c_uint, C.c_uint, C.c_ulong, C.c_int], C.POINTER(XImage)),
            'XDestroyImage': ([C.POINTER(XImage)], C.c_int),
            'XFlush': ([C.c_void_p], C.c_int),
            'XCloseDisplay': ([C.c_void_p], C.c_int),
            'XStringToKeysym': ([C.c_char_p], C.c_ulong),
            'XKeysymToKeycode': ([C.c_void_p, C.c_ulong], C.c_uint),
        }
        for name, (arguments, result) in signatures.items():
            function = getattr(self.x, name)
            function.argtypes, function.restype = arguments, result
        self.test.XTestFakeKeyEvent.argtypes = [C.c_void_p, C.c_uint, C.c_int, C.c_ulong]
        self.test.XTestFakeButtonEvent.argtypes = [C.c_void_p, C.c_uint, C.c_int, C.c_ulong]
        self.test.XTestFakeMotionEvent.argtypes = [C.c_void_p, C.c_int, C.c_int, C.c_int, C.c_ulong]
        self.display = self.x.XOpenDisplay(display.encode())
        if not self.display:
            raise RuntimeError('Cannot connect to test X server')
        self.root = self.x.XDefaultRootWindow(self.display)

    def edge(self):
        # First fully covered #6587ff pixel in the demo's blue ellipse at y=550.
        # A one-row read is cheap and excludes the hardware cursor and inspector.
        image = self.x.XGetImage(self.display, self.root, 350, 550, 640, 1, C.c_ulong(-1).value, 2)
        if not image:
            raise RuntimeError('XGetImage failed')
        try:
            assert image.contents.bits_per_pixel == 32 and image.contents.byte_order == 0
            assert image.contents.red_mask == 0xff0000
            data = C.string_at(image.contents.data, image.contents.bytes_per_line)
            offset = data.find(b'\xff\x87\x65')
            return 350 + offset // 4 if offset >= 0 and offset % 4 == 0 else None
        finally:
            self.x.XDestroyImage(image)

    def key(self, name, down):
        code = self.x.XKeysymToKeycode(self.display, self.x.XStringToKeysym(name.encode()))
        self.test.XTestFakeKeyEvent(self.display, code, down, 0)

    def nudge(self, direction):
        self.key('Shift_L', True)
        self.key(direction, True)
        self.key(direction, False)
        self.key('Shift_L', False)
        self.flush()

    def move(self, x, y):
        self.test.XTestFakeMotionEvent(self.display, -1, round(x), round(y), 0)
        self.flush()

    def button(self, down):
        self.test.XTestFakeButtonEvent(self.display, 1, down, 0)
        self.flush()

    def flush(self):
        self.x.XFlush(self.display)

    def close(self):
        self.x.XCloseDisplay(self.display)


def resources(group):
    cpu_ticks = rss_kib = pss_kib = 0
    members = []
    for directory in Path('/proc').iterdir():
        if not directory.name.isdigit():
            continue
        try:
            fields = (directory / 'stat').read_text().rsplit(')', 1)[1].split()
            if int(fields[2]) != group:
                continue
            members.append(int(directory.name))
            cpu_ticks += int(fields[11]) + int(fields[12])
            for line in (directory / 'smaps_rollup').read_text().splitlines():
                if line.startswith('Rss:'):
                    rss_kib += int(line.split()[1])
                elif line.startswith('Pss:'):
                    pss_kib += int(line.split()[1])
        except (FileNotFoundError, ProcessLookupError):
            pass
    return {'cpu_seconds': cpu_ticks / os.sysconf('SC_CLK_TCK'),
            'rss_mib': rss_kib / 1024, 'pss_mib': pss_kib / 1024, 'pids': members}


def stats(values):
    ordered = sorted(values)
    return {'median': statistics.median(ordered), 'p95': ordered[math.ceil(len(ordered)*.95)-1],
            'min': min(ordered), 'max': max(ordered), 'count': len(ordered)}


def stop(process):
    if process:
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()


def run_one(name, binary, trial, args, env, out):
    folder = out / f'{name}-{trial}'
    folder.mkdir(exist_ok=True)
    app = server = screen = None
    try:
        server = subprocess.Popen(['Xvfb', args.display, '-screen', '0', '1280x860x24', '-nolisten', 'tcp'],
                                  env=env, stdout=open(folder/'xvfb.log', 'w'), stderr=subprocess.STDOUT,
                                  start_new_session=True)
        time.sleep(.5)
        if server.poll() is not None:
            raise RuntimeError('Test display unavailable; choose an unused --display')
        screen = Screen(args.display)
        assert screen.edge() is None, 'Display must start empty'
        started = time.perf_counter()
        app = subprocess.Popen([str(binary)], env=env, stdout=open(folder/'app.log', 'w'),
                               stderr=subprocess.STDOUT, start_new_session=True)
        while screen.edge() is None:
            if app.poll() is not None or time.perf_counter()-started > 30:
                raise RuntimeError(f'{name} did not display the demo; see {folder}/app.log')
            time.sleep(.004)
        startup_ms = (time.perf_counter()-started)*1000
        window = subprocess.check_output(['xdotool', 'search', '--onlyvisible', '--name', 'Picsie'], env=env, text=True).splitlines()[-1]
        subprocess.run(['xdotool', 'windowfocus', window], env=env, check=True)
        geometry = subprocess.check_output(['xdotool', 'getwindowgeometry', '--shell', window], env=env, text=True)
        assert 'WIDTH=1280\n' in geometry and 'HEIGHT=860\n' in geometry, geometry
        time.sleep(3)
        subprocess.run(['import', '-window', window, str(folder/'startup.png')], env=env, check=True)
        before = resources(app.pid)
        idle_start = time.perf_counter()
        time.sleep(3)
        idle = resources(app.pid)
        idle_cpu = (idle['cpu_seconds']-before['cpu_seconds'])/(time.perf_counter()-idle_start)*100
        initial_edge = screen.edge()
        assert initial_edge is not None
        # Below the headline's rectangular hit area and above the caption.
        screen.move(750, 610)
        screen.button(True)
        screen.button(False)
        time.sleep(.6)
        latencies = []
        for i in range(28):
            previous = screen.edge()
            direction = 'Right' if i % 2 == 0 else 'Left'
            started = time.perf_counter()
            screen.nudge(direction)
            while True:
                edge = screen.edge()
                if edge is not None and abs(edge-previous) >= 5:
                    break
                if time.perf_counter()-started > 5:
                    raise RuntimeError(f'{name}: nudge did not become visible, previous={previous}, edge={edge}')
                time.sleep(.004)
            duration = (time.perf_counter()-started)*1000
            assert 6 <= abs(edge-previous) <= 9, f'Unexpected nudge distance: {previous} → {edge}'
            if i >= 4:
                latencies.append(duration)
            time.sleep(.10)
        assert abs(screen.edge()-initial_edge) <= 1, 'Nudges must return to start'
        time.sleep(.5)
        # 120 motion samples/s, four-second triangle with a 120 px excursion.
        # Observe distinct visible edge positions at 250 Hz, without app tracing.
        screen.move(750, 610)
        screen.button(True)
        before = resources(app.pid)
        started = time.perf_counter()
        next_poll = 0.
        previous = screen.edge()
        changes, input_samples = [], []
        for step in range(1, 481):
            target = started + step/120
            while time.perf_counter() < target:
                now = time.perf_counter()-started
                if now >= next_poll:
                    edge = screen.edge()
                    if edge is not None and edge != previous:
                        changes.append([now, edge])
                        previous = edge
                    next_poll = now + .004
                time.sleep(.0005)
            offset = step/2 if step <= 240 else (480-step)/2
            screen.move(750+offset, 610)
            input_samples.append(time.perf_counter()-started)
        screen.button(False)
        duration = time.perf_counter()-started
        after = resources(app.pid)
        drag_cpu = (after['cpu_seconds']-before['cpu_seconds'])/duration*100
        catchup = time.perf_counter()
        while screen.edge() != initial_edge:
            if time.perf_counter()-catchup > 10:
                raise RuntimeError(f'{name}: drag did not return to its initial visible position')
            time.sleep(.004)
        catchup_ms = (time.perf_counter()-catchup)*1000
        time.sleep(1)
        settled = resources(app.pid)
        subprocess.run(['import', '-window', window, str(folder/'after-drag.png')], env=env, check=True)
        intervals = [(b[0]-a[0])*1000 for a, b in zip(changes, changes[1:])]
        assert len(changes) > 5
        result = {'app': name, 'trial': trial, 'window_geometry': geometry,
                  'startup_first_artwork_ms': startup_ms,
                  'idle': idle, 'idle_cpu_percent_one_core': idle_cpu,
                  'nudge_latency_ms': stats(latencies), 'nudge_samples_ms': latencies,
                  'initial_edge_x': initial_edge, 'drag_duration_s': duration,
                  'drag_visible_updates_per_second': len(changes)/duration,
                  'drag_visible_interval_ms': stats(intervals),
                  'drag_cpu_percent_one_core': drag_cpu, 'drag_end_catchup_ms': catchup_ms,
                  'after_drag': after, 'settled_after_drag': settled,
                  'drag_visible_changes': changes, 'drag_input_times_s': input_samples}
        (folder/'result.json').write_text(json.dumps(result, indent=2)+'\n')
        print(f'{name} {trial}: startup={startup_ms:.1f}ms, idle RSS={idle["rss_mib"]:.1f}MiB, '
              f'nudge median={statistics.median(latencies):.1f}ms, '
              f'drag={len(changes)/duration:.1f} updates/s, CPU={drag_cpu:.0f}%', flush=True)
        return result
    finally:
        stop(app)
        if screen:
            screen.close()
        stop(server)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', '--quickgui', type=Path, default=Path('dist/linux-x64/Picsie'))
    parser.add_argument('--candidate', '--gpui', type=Path, default=Path('crates/picsie-desktop/target/release/picsie-desktop'))
    parser.add_argument('--baseline-name', default='quickgui')
    parser.add_argument('--candidate-name', default='gpui')
    parser.add_argument('--tools', type=Path)
    parser.add_argument('--output', type=Path, default=Path('artifacts/performance-comparison'))
    parser.add_argument('--display', default=':94')
    parser.add_argument('--trials', type=int, default=3)
    args = parser.parse_args()
    if args.trials < 1:
        parser.error('--trials must be at least 1')
    if args.baseline_name == args.candidate_name or not all(
        re.fullmatch(r'[a-zA-Z0-9_-]+', name) for name in [args.baseline_name, args.candidate_name]
    ):
        parser.error('application names must be distinct and use letters, digits, underscores or hyphens')
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    env = {**os.environ, 'DISPLAY': args.display, 'WINIT_UNIX_BACKEND': 'x11',
           'XDG_CACHE_HOME': str(out/'cache')}
    for name in ['WAYLAND_DISPLAY', 'PICSIE_TRACE_DIR']:
        env.pop(name, None)
    if args.tools:
        tools = args.tools.resolve()
        env['PATH'] = str(tools/'bin') + os.pathsep + env.get('PATH', '')
        env['LD_LIBRARY_PATH'] = str(tools/'lib') + os.pathsep + env.get('LD_LIBRARY_PATH', '')
        env['VK_DRIVER_FILES'] = str(tools/'share/vulkan/icd.d/lvp_icd.json')
    else:
        env['VK_DRIVER_FILES'] = str(next(Path('/usr/share/vulkan/icd.d').glob('lvp_icd*.json')))
    applications = [(args.baseline_name, args.baseline.resolve()), (args.candidate_name, args.candidate.resolve())]
    results = []
    for trial in range(args.trials+1):
        for name, binary in applications if trial % 2 == 0 else reversed(applications):
            result = run_one(name, binary, trial, args, env, out)
            if trial > 0:
                results.append(result)
    summary = {}
    for name, _ in applications:
        runs = [r for r in results if r['app'] == name]
        summary[name] = {
            'startup_first_artwork_ms': stats([r['startup_first_artwork_ms'] for r in runs]),
            'idle_rss_mib': stats([r['idle']['rss_mib'] for r in runs]),
            'idle_pss_mib': stats([r['idle']['pss_mib'] for r in runs]),
            'idle_cpu_percent_one_core': stats([r['idle_cpu_percent_one_core'] for r in runs]),
            'nudge_latency_ms': stats([v for r in runs for v in r['nudge_samples_ms']]),
            'drag_updates_per_second': stats([r['drag_visible_updates_per_second'] for r in runs]),
            'drag_cpu_percent_one_core': stats([r['drag_cpu_percent_one_core'] for r in runs]),
            'settled_after_drag_rss_mib': stats([r['settled_after_drag']['rss_mib'] for r in runs]),
        }
    report = {'backend': 'Linux X11 / Xvfb / Mesa software Vulkan',
              'scope': 'External input to changed X11 framebuffer; excludes physical GPU and screen scanout',
              'window': [1280, 860], 'viewport': [936, 734], 'trials': args.trials,
              'warmup': 'One complete discarded run per app; alternating measured launch order; warm caches',
              'binaries': {name: str(binary) for name, binary in applications},
              'poll_interval_ms': 4, 'summary': summary, 'runs': results}
    (out/'application-results.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
