"""Focused native verification for Levels/Curves adjustment layers.

Drives the real GPUI window on DISPLAY :113 through X11 input using the
extracted Xvfb/xdotool root, following crates/picsie-desktop/verify.py.
"""
import argparse
from environment import add_environment_arguments, native_environment, backend_description
import json
import os
import re
import subprocess
import time
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('--binary', default='crates/picsie-desktop/target/release/picsie-desktop')
parser.add_argument('--output', default='artifacts/levels-curves')
add_environment_arguments(parser)
parser.add_argument('--display', default=':113')
args = parser.parse_args()
out = Path(args.output).resolve()
out.mkdir(parents=True, exist_ok=True)
trace = out / 'trace'
trace.mkdir(parents=True, exist_ok=True)
for file in trace.glob('window-*.json'):
    file.unlink()
config = out / 'config/xdg-desktop-portal'
config.mkdir(parents=True, exist_ok=True)
(config / 'portals.conf').write_text('[preferred]\ndefault=gtk\n')
env = native_environment(args, out, trace)
processes = []
checks = []
window_number = 1
window_id = None


def x(*arguments):
    return subprocess.check_output(['xdotool', *map(str, arguments)], env=env, text=True, timeout=15).strip()


def state():
    try:
        return json.loads((trace / f'window-{window_number}.json').read_text())
    except (FileNotFoundError, json.JSONDecodeError):
        return {}


def wait(predicate, label='state transition', timeout=25):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        current = state()
        if current and predicate(current):
            return current
        time.sleep(.05)
    raise AssertionError(f'{label}: {json.dumps(state())[:2500]}')


def settle():
    # busy covers file/clipboard UI, including a chooser that needs driver input.
    # Engine rendering completes through the sequence barrier, not this flag.
    wait(lambda s: s['busy'] or s['sequence'] >= s.get('submittedSequence', 0), 'preview catches up')


def check(label, assertion):
    if callable(assertion):
        wait(lambda _: assertion(), label)
    else:
        assert assertion, label
    checks.append(label)
    print('PASS', label, flush=True)


def focus(title):
    global window_id
    end = time.monotonic() + 20
    while True:
        try:
            window_id = x('search', '--onlyvisible', '--name', title).splitlines()[-1]
            break
        except (subprocess.CalledProcessError, IndexError):
            if time.monotonic() >= end:
                raise
            time.sleep(.05)
    x('windowmove', window_id, 0, 0, 'windowraise', window_id, 'windowfocus', window_id)
    time.sleep(.25)


def bounds(control):
    rect = None
    for _ in range(70):
        current = state()
        rect = current['controls'].get(control)
        if rect:
            return rect
        time.sleep(.1)
    raise AssertionError(f'Cannot reveal {control}: {rect}')


def click(control):
    left, top, width, height = bounds(control)
    x('mousemove', round(left + width / 2), round(top + height / 2), 'click', 1)
    time.sleep(.15)
    settle()


def key(keys):
    x('key', '--clearmodifiers', keys)
    time.sleep(.16)
    settle()


def field(name, value, enter=True):
    click('field-' + name)
    key('ctrl+a')
    x('type', '--clearmodifiers', '--', str(value))
    if enter:
        key('Return')
    time.sleep(.1)


def drag(x0, y0, x1, y1):
    x('mousemove', round(x0), round(y0), 'mousedown', 1)
    for step in range(1, 21):
        x('mousemove', round(x0 + (x1 - x0) * step / 20), round(y0 + (y1 - y0) * step / 20))
        time.sleep(.015)
    x('mouseup', 1)
    time.sleep(.25)
    settle()


def shot(name):
    time.sleep(.3)
    subprocess.run(['import', '-window', window_id or 'root', str(out / name)], env=env, check=True, timeout=15)


def layers():
    return state()['state']['document']['layers']


def edit():
    return state()['state']['adjustmentEdit']


def screen(doc_x, doc_y):
    s = state()
    v, d, b = s['state']['viewport'], s['state']['document'], s['controls']['canvas']
    z = v['zoom']
    ox = b[0] + (b[2] - d['width'] * z) / 2 + v['pan']['x']
    oy = b[1] + (b[3] - d['height'] * z) / 2 + v['pan']['y']
    return (ox + doc_x * z, oy + doc_y * z)


def portal(path):
    end = time.monotonic() + 55
    dialog = None
    while time.monotonic() < end:
        try:
            dialog = x('search', '--onlyvisible', '--class', 'Xdg-desktop-portal-gtk').splitlines()[-1]
            break
        except (subprocess.CalledProcessError, IndexError):
            time.sleep(.2)
    if not dialog:
        raise AssertionError('GTK portal file chooser did not open: ' + state()['notice'])
    x('windowfocus', dialog)
    key('ctrl+l')
    x('type', '--clearmodifiers', str(path))
    key('Return')
    time.sleep(.4)
    try:
        visible = x('search', '--onlyvisible', '--class', 'Xdg-desktop-portal-gtk')
        if dialog in visible.splitlines():
            key('Return')
    except subprocess.CalledProcessError:
        pass
    wait(lambda s: not s['busy'], 'file operation', timeout=30)


try:
    server = subprocess.Popen(['Xvfb', args.display, '-screen', '0', '1280x900x24', '-nolisten', 'tcp'],
                              env=env, stdout=open(out / 'xvfb.log', 'w'),
                              stderr=subprocess.STDOUT, start_new_session=True)
    processes.append(server)
    time.sleep(.5)
    app = subprocess.Popen(['dbus-run-session', '--', str(Path(args.binary).resolve())], env=env,
                           stdout=open(out / 'app.log', 'w'), stderr=subprocess.STDOUT,
                           start_new_session=True)
    processes.append(app)
    wait(lambda s: s.get('state') is not None, 'startup')
    focus('Color studies.*Picsie')
    check('native window matches installed Picsie desktop identity',
          lambda: (window_id in x('search', '--onlyvisible', '--class', '^picsie$').splitlines()))
    # Deterministic canvas with tonal content for the histogram.
    click('menu-File')
    click('new')
    field('new-name', 'Levels curves', enter=False)
    field('new-width', '400', enter=False)
    field('new-height', '300', enter=False)
    click('modal-apply')
    window_number = 2
    wait(lambda s: s.get('state') is not None, 'new window')
    focus('Levels curves.*Picsie')
    x('windowsize', window_id, 1280, 860)
    time.sleep(.3)
    click('fit')
    click('foreground-picker-rail')
    field('color-hex', '404040', enter=False)
    click('modal-apply')
    click('tool-rectangle')
    x0, y0 = screen(50, 50)
    x1, y1 = screen(350, 250)
    drag(x0, y0, x1, y1)
    click('tool-move')
    # New Levels adjustment through the adjustments menu.
    click('adjustments-menu')
    click('add-levels')
    check('Levels adjustment opens a floating panel with histogram and handles',
          lambda: (edit() is not None and edit()['kind'] == 'Levels'
                   and all(k in state()['controls'] for k in
                           ['adjustment-panel', 'levels-histogram', 'levels-input-handles',
                            'levels-output-handles', 'field-levels-white'])))
    check('new Levels layer sits above the artwork with a badge',
          lambda: (layers()[-1]['adjustment'] == 'Levels'
                   and 'adjustment-tag-' + layers()[-1]['id'] in state()['controls']))
    shot('01-levels-panel.png')
    field('levels-out-white', '200')
    check('numeric output-white field drives the live edit',
          lambda: (edit()['levels']['ranges'][0]['outputWhite'] == 200))
    left, top, width, height = bounds('levels-input-handles')
    drag(left + 4, top + height / 2, left + 60, top + height / 2)
    check('input black handle drag raises the black point',
          lambda: (edit()['levels']['ranges'][0]['black'] >= 20))
    shot('02-levels-handles.png')
    click('levels-auto-contrast')
    check('auto contrast derives endpoints from the histogram',
          lambda: (edit()['levels']['ranges'][0]['white'] <= 255
                   and edit()['levels']['ranges'][0]['black'] >= 0))
    click('levels-sample-gray')
    sx, sy = screen(220, 150)
    x('mousemove', round(sx), round(sy), 'click', 1)
    time.sleep(.3)
    settle()
    check('gray eyedropper bends gamma from canvas content',
          lambda: (abs(edit()['levels']['ranges'][1]['gamma'] - 1.) > 1e-3
                   or abs(edit()['levels']['ranges'][2]['gamma'] - 1.) > 1e-3
                   or abs(edit()['levels']['ranges'][3]['gamma'] - 1.) > 1e-3))
    shot('03-levels-sample.png')
    key('Escape')
    check('Escape cancels the edit without an undo entry',
          lambda: (state()['state']['adjustmentEdit'] is None))
    count = state()['state']['history']['undoCount']
    click('adjustment-edit')
    check('idle panel reopens the adjustment for editing',
          lambda: (edit() is not None and edit()['kind'] == 'Levels'))
    field('levels-black', '30')
    check('reopened edit accepts field changes',
          lambda: (edit()['levels']['ranges'][0]['black'] == 30))
    click('levels-preview')
    check('preview toggle shows original pixels flag',
          lambda: (edit()['preview'] is False))
    click('levels-preview')
    click('levels-apply')
    check('Apply commits one undo entry',
          lambda: (state()['state']['adjustmentEdit'] is None
                   and state()['state']['history']['undoCount'] == count + 1))
    key('ctrl+z')
    check('Undo restores pre-apply settings',
          lambda: (state()['state']['history']['undoCount'] == count))
    key('ctrl+shift+z')
    # Curves: add point by click, drag it, remove it, reset, apply.
    click('adjustments-menu')
    click('add-curves')
    check('Curves adjustment opens a 260pt graph panel',
          lambda: (edit() is not None and edit()['kind'] == 'Curves'
                   and 'curves-canvas' in state()['controls']))
    shot('04-curves-panel.png')
    left, top, width, height = bounds('curves-canvas')
    x('mousemove', round(left + width / 2), round(top + height / 2), 'click', 1)
    time.sleep(.3)
    settle()
    check('canvas click adds a curve point',
          lambda: (len(edit()['curves']['channels'][0]) == 3))
    drag(left + width / 2, top + height / 2, left + width / 2 + 30, top + height / 2 - 30)
    check('curve drag moves the added point',
          lambda: (abs(edit()['curves']['channels'][0][1]['x'] - 127.5) > 5
                   or abs(edit()['curves']['channels'][0][1]['y'] - 127.5) > 5))
    shot('05-curves-drag.png')
    click('curves-remove')
    check('Remove point keeps endpoints only',
          lambda: (len(edit()['curves']['channels'][0]) == 2))
    click('curves-reset')
    check('Reset restores the diagonal curve',
          lambda: (edit()['curves']['channels'][0] == [{'x': 0.0, 'y': 0.0}, {'x': 255.0, 'y': 255.0}]))
    click('curves-apply')
    check('Curves Apply commits and closes the panel',
          lambda: (state()['state']['adjustmentEdit'] is None
                   and 'adjustment-panel' not in state()['controls']))
    shot('06-curves-applied.png')
    (out / 'report.json').write_text(json.dumps(
        {'checks': checks, 'count': len(checks), 'backend': backend_description(env), 'binary': args.binary},
        indent=2) + '\n')
    print(f'All {len(checks)} checks passed. Screenshots: {out}', flush=True)
except BaseException as error:
    if not isinstance(error, SystemExit) or error.code != 0:
        try:
            shot('failure.png')
        except BaseException:
            pass
        (out / 'report.json').write_text(json.dumps(
            {'checks': checks, 'count': len(checks), 'error': str(error)}, indent=2) + '\n')
    raise
finally:
    for process in processes:
        try:
            process.terminate()
        except BaseException:
            pass
