"""Combined cross-feature native verification.

Drives the combined group + shapes/gradients + levels/curves binary on X11:
a group box over two editable shapes commits with vector content intact, a
VISIBLE COLORED (red-to-blue) gradient applies as one undo, and Levels/Curves
adjustment layers open, edit and apply above the artwork. Distinct output under
artifacts/integration/combined; worker evidence is never overwritten.
Run from the repository root (the reproducibility runner serializes native runs):
  python3 scripts/verification/combined.py \
    --binary crates/picsie-desktop/target/release/picsie-desktop
"""
import argparse
from environment import add_environment_arguments, native_environment, backend_description
import json
import os
import re
import signal
import subprocess
import time
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('--binary', default='crates/picsie-desktop/target/release/picsie-desktop')
parser.add_argument('--output', default='artifacts/integration/combined')
add_environment_arguments(parser)
parser.add_argument('--display', default=':114')
args = parser.parse_args()
out = Path(args.output).resolve()
out.mkdir(parents=True, exist_ok=True)
trace = out / 'trace'
trace.mkdir(exist_ok=True)
for file in trace.glob('window-*.json'):
    file.unlink()
config = out / 'config/xdg-desktop-portal'
config.mkdir(parents=True, exist_ok=True)
preferences = out / 'config/picsie/ui.json'
if preferences.exists():
    preferences.unlink()
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

MENUS = {'new-canvas': 'File'}

def prepare_control(control):
    if control in MENUS and control not in state()['controls']:
        click('menu-' + MENUS[control], scroll=False)
        return

def bounds(control, scroll=True):
    prepare_control(control)
    rect = None
    for _ in range(70):
        current = state()
        rect = current['controls'].get(control)
        if rect:
            left, top, width, height = rect
            header_control = control in {'field-x', 'field-y', 'field-width', 'field-height',
                                         'field-rotation', 'field-transform-scale',
                                         'transform-ratio', 'select-transform-sampling',
                                         'flip-x', 'flip-y'}
            if scroll and not current['modal'] and header_control and current['state']['tool'] == 'move':
                clip = current['controls'].get('transform-fields', [112, top, 1048, height])
                start = clip[0]
                edge = start + clip[2]
                if left < start or left + width > edge:
                    x('mousemove', round((start + edge) / 2), round(top + height / 2),
                      'click', 7 if left + width > edge else 6)
                    time.sleep(.08)
                    continue
            return rect
        time.sleep(.1)
    raise AssertionError(f'Cannot reveal {control}: {rect}')

def click(control, modifier=None, scroll=True):
    left, top, width, height = bounds(control, scroll)
    if modifier:
        x('keydown', *modifier.split())
    x('mousemove', round(left + width / 2), round(top + height / 2), 'click', 1)
    if modifier:
        x('keyup', *modifier.split())
    time.sleep(.15)
    settle()

def key(keys):
    x('key', '--clearmodifiers', keys)
    time.sleep(.16)
    settle()

def field(name, value, enter=True):
    click('field-' + name if ('field-' + name) in state()['controls'] else name)
    key('ctrl+a')
    if str(value):
        x('type', '--clearmodifiers', '--', str(value))
    else:
        key('BackSpace')
    if enter:
        key('Return')
    time.sleep(.1)

def drag(x0, y0, x1, y1, modifier=None):
    if modifier:
        x('keydown', *modifier.split())
    x('mousemove', round(x0), round(y0), 'mousedown', 1)
    for step in range(1, 21):
        x('mousemove', round(x0 + (x1 - x0) * step / 20), round(y0 + (y1 - y0) * step / 20))
        time.sleep(.015)
    x('mouseup', 1)
    if modifier:
        x('keyup', *modifier.split())
    time.sleep(.25)
    settle()

def pt(xp, yp):
    s = state()
    v = s['state']['viewport']
    d = s['state']['document']
    r = s['controls']['canvas']
    z = v['zoom']
    return (r[0] + (r[2] - d['width'] * z) / 2 + v['pan']['x'] + xp * z,
            r[1] + (r[3] - d['height'] * z) / 2 + v['pan']['y'] + yp * z)

def shot(name):
    time.sleep(.3)
    subprocess.run(['import', '-window', window_id or 'root', str(out / name)],
                   env=env, check=True, timeout=15)

def layers():
    return state()['state']['document']['layers']

def selected():
    selected_id = state()['state']['selection']['ids'][-1]
    return next(layer for layer in layers() if layer['id'] == selected_id)

def group_box():
    return state()['state']['groupBox']

def edit():
    return state()['state']['adjustmentEdit']

def close(a, b):
    return abs(a - b) < .6

def check(label, assertion):
    if callable(assertion):
        wait(lambda _: assertion(), label)
    else:
        assert assertion, label
    checks.append(label)
    print(f'ok {len(checks)}: {label}', flush=True)

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

    click('new-canvas')
    field('new-name', 'Combined cross', False)
    field('new-width', 800, False)
    field('new-height', 600, False)
    click('modal-apply')
    window_number = 2
    wait(lambda s: s.get('state') is not None, 'new window')
    focus('Combined cross.*Picsie')
    x('windowsize', window_id, 1281, 860)
    time.sleep(.4)
    click('fit')

    # Two editable shapes: red rounded rectangle + ellipse.
    click('foreground-picker-rail')
    field('color-hex', 'FF0000', False)
    click('modal-apply')
    click('tool-rectangle')
    field('shape-radius', 16)
    check('radius field commits to shape state',
          lambda: state()['state']['shapeCornerRadius'] == 16)
    drag(*pt(40, 50), *pt(140, 110))
    red = selected()['id']
    check('rectangle gesture creates an editable shape layer',
          lambda: (selected()['name'] == 'Rectangle 1'
                   and selected()['content']['kind'] == 'shape'
                   and selected()['content'].get('corner_radius') == 16))
    click('shape-ellipse')
    drag(*pt(200, 120), *pt(300, 210))
    blue = selected()['id']
    check('ellipse gesture creates a second shape layer',
          lambda: (selected()['name'] == 'Ellipse 1'
                   and selected()['content']['kind'] == 'shape'))
    shot('01-shapes.png')

    # Group box over both shapes commits with vector content intact.
    click('tool-move')
    click('layer-' + red, modifier='Control_L')
    check('multi selection publishes one shared group box',
          lambda: (sorted(state()['state']['selection']['ids']) == sorted([red, blue])
                   and group_box() is not None
                   and close(group_box()['x'], 40) and close(group_box()['y'], 50)))
    count = state()['state']['history']['undoCount']
    field('width', 390)
    check('numeric width opens a group draft', lambda: state()['state']['transformActive'])
    click('transform-apply')
    both = {layer['id']: layer for layer in layers()}
    check('group resize commits both shapes as one editable undo',
          lambda: (state()['state']['history']['undoCount'] == count + 1
                   and state()['state']['history']['undoLabel'] == 'Transform Layers'
                   and both[red]['content']['kind'] == 'shape'
                   and both[blue]['content']['kind'] == 'shape'
                   and both[red]['content'].get('corner_radius') == 16))
    shot('02-group-shapes.png')
    key('ctrl+z')
    click('layer-' + red)

    # Colored gradient: red foreground to blue background, applied as one undo.
    click('background-picker-rail')
    field('color-hex', '0000FF', False)
    click('modal-apply')
    click('tool-gradient')
    click('gradient-fg-bg')
    drag(*pt(40, 80), *pt(140, 80))
    check('gradient drag pends a line with Apply/Cancel',
          lambda: (state()['state']['gradientPending']
                   and 'gradient-apply' in state()['controls']))
    shot('03-gradient-pending.png')
    count = state()['state']['history']['undoCount']
    click('gradient-apply')
    check('colored gradient Apply commits as one undo',
          lambda: (not state()['state']['gradientPending']
                   and state()['state']['history']['undoCount'] == count + 1))
    shot('04-gradient-color.png')

    # Levels adjustment above the artwork: panel, field, apply.
    click('tool-move')
    click('adjustments-menu')
    click('add-levels')
    check('Levels opens a floating panel above the active art',
          lambda: (edit() is not None and edit()['kind'] == 'Levels'
                   and 'levels-histogram' in state()['controls']
                   and selected()['adjustment'] == 'Levels'))
    shot('05-levels-panel.png')
    field('levels-out-white', '200')
    check('output-white field drives the live edit',
          lambda: (edit()['levels']['ranges'][0]['outputWhite'] == 200))
    count = state()['state']['history']['undoCount']
    click('levels-apply')
    check('Levels Apply commits one undo entry',
          lambda: (state()['state']['adjustmentEdit'] is None
                   and state()['state']['history']['undoCount'] == count + 1))

    # Curves adjustment: click-add, drag, apply.
    click('adjustments-menu')
    click('add-curves')
    check('Curves opens a graph panel',
          lambda: (edit() is not None and edit()['kind'] == 'Curves'
                   and 'curves-canvas' in state()['controls']))
    shot('06-curves-panel.png')
    canvas = state()['controls']['curves-canvas']
    x('mousemove', round(canvas[0] + canvas[2] / 2), round(canvas[1] + canvas[3] / 2), 'click', 1)
    time.sleep(.3)
    settle()
    check('canvas click adds a curve point',
          lambda: (len(edit()['curves']['channels'][0]) == 3))
    count = state()['state']['history']['undoCount']
    click('curves-apply')
    check('Curves Apply commits and closes the panel',
          lambda: (state()['state']['adjustmentEdit'] is None
                   and 'adjustment-panel' not in state()['controls']
                   and state()['state']['history']['undoCount'] == count + 1))
    shot('07-curves-applied.png')
    (out / 'report.json').write_text(json.dumps(
        {'checks': checks, 'count': len(checks), 'backend': backend_description(env),
         'binary': args.binary}, indent=2) + '\n')
    print(f'All {len(checks)} combined checks passed. Screenshots: {out}', flush=True)
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
            os.killpg(process.pid, signal.SIGTERM)
        except (ProcessLookupError, PermissionError):
            pass
