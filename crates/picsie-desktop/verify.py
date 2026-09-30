"""Exercise the real GPUI window through X11 input and the real GTK file portal.

Requires Xvfb, xdotool, ImageMagick, dbus-run-session and xdg-desktop-portal-gtk.
--tools points to extracted bin/lib/share tools; --software selects their lavapipe.
All documents, traces and screenshots stay in the ignored output directory.
"""
import argparse
import json
import os
import re
from pathlib import Path
import signal
import shutil
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument('--binary', default='crates/picsie-desktop/target/release/picsie-desktop')
parser.add_argument('--output', default='artifacts/gpui-parity/verification')
parser.add_argument('--tools', type=Path)
parser.add_argument('--software', action='store_true')
parser.add_argument('--display', default=':95')
parser.add_argument('--polish-only', action='store_true')
args = parser.parse_args()
out = Path(args.output).resolve()
out.mkdir(parents=True, exist_ok=True)
trace = out / 'trace'
trace.mkdir(exist_ok=True)
for name in ['parity.picsie', 'parity-as.picsie', 'parity.png', 'parity.jpg', 'parity.comp', 'layout.picsie', 'layout.comp', 'report.json', 'final-state.json', 'failure.png']:
    file = out / name
    if file.is_dir(): shutil.rmtree(file)
    elif file.exists(): file.unlink()
for file in trace.glob('window-*.json'):
    file.unlink()
config = out / 'config/xdg-desktop-portal'
config.mkdir(parents=True, exist_ok=True)
preferences = out / 'config/picsie/ui.json'
if preferences.exists(): preferences.unlink() # Each verification run starts with source defaults.
(config / 'portals.conf').write_text('[preferred]\ndefault=gtk\n')
env = {**os.environ, 'DISPLAY': args.display, 'PICSIE_TRACE_DIR': str(trace),
       'XDG_CONFIG_HOME': str(out / 'config'), 'XDG_DATA_HOME': str(out / 'data'), 'XDG_CURRENT_DESKTOP': 'GPUIParity'}
env.pop('WAYLAND_DISPLAY', None)
if args.tools:
    tools = args.tools.resolve()
    env['PATH'] = str(tools / 'bin') + os.pathsep + env.get('PATH', '')
    env['LD_LIBRARY_PATH'] = str(tools / 'lib') + os.pathsep + env.get('LD_LIBRARY_PATH', '')
    if args.software:
        env['VK_DRIVER_FILES'] = str(tools / 'share/vulkan/icd.d/lvp_icd.json')
if args.software and not args.tools:
    drivers = sorted(Path('/usr/share/vulkan/icd.d').glob('lvp_icd*.json'))
    if not drivers: raise SystemExit('Mesa lavapipe ICD not found; install mesa-vulkan-drivers')
    env['VK_DRIVER_FILES'] = str(drivers[0])
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

def wait(predicate, label='state transition', timeout=20):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        current = state()
        if current and predicate(current):
            return current
        time.sleep(.05)
    raise AssertionError(f'{label}: {json.dumps(state())[:2500]}')

def settle():
    wait(lambda s: s['busy'] or s['sequence'] >= s.get('submittedSequence', 0), 'preview catches up')

def window_exists(id):
    return subprocess.run(['xdotool','getwindowname',id],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode == 0

def check(label, assertion):
    if callable(assertion): wait(lambda _: assertion(), label)
    else: assert assertion, label
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
            if time.monotonic() >= end: raise
            time.sleep(.05)
    x('windowmove', window_id, 0, 0, 'windowraise', window_id, 'windowfocus', window_id)
    time.sleep(.25)

def prepare_control(control):
    if control in {'tool-eraser','tool-ellipse'}:
        click('tool-brush' if control=='tool-eraser' else 'tool-rectangle')
        return
    menus = {'new':'File','open':'File','open-comp':'File','import':'File','save':'File','save-as':'File','save-comp':'File','export-png':'File','export-jpeg':'File',
             'duplicate':'Layer','group':'Layer','out-of-folder':'Layer','add-gradient':'Layer','raise':'Layer','lower':'Layer','layers-merge':'Layer','pixels-via-copy':'Layer',
             'pixels-copy':'Edit','pixels-cut':'Edit','pixels-paste':'Edit','pixels-copy-merged':'Edit','pixels-transform':'Edit','pixels-fill-foreground':'Edit','pixels-fill-background':'Edit','undo':'Edit','redo':'Edit',
             'rulers':'View','guides':'View','grid':'View','snap':'View','lock-guides':'View','snap-guides':'View','snap-grid':'View','snap-layers':'View','snap-bounds':'View','clear-guides':'View','pixels-all':'Select','pixels-invert':'Select','pixels-fill':'Select','pixels-clear':'Select','image-size':'Image'}
    mask_controls={'mask-add','mask-link','mask-enabled','mask-remove','mask-reset-reveal','mask-reset-hide','paint-content','paint-mask','clipping','select-mask-source'}
    adjustment_controls={'field-brightness','field-saturation','field-blur','slider-brightness','slider-saturation','slider-blur'}
    properties_controls={'field-name','lock','use-foreground','gradient-end'}
    palette='properties-menu' if control in mask_controls else 'adjustments-menu' if control in adjustment_controls else 'properties-menu' if control in properties_controls else None
    current=state()
    if control in {'mask-color-black','mask-color-white'}:
        palette=current.get('palette')
    if current.get('textPalette') and control!='text-editor':
        key('Escape')
    if current.get('palette') and current['palette']!=palette and control!=current['palette']:
        key('Escape')
    if palette and state().get('palette')!=palette:
        click(palette,scroll=False)
    if control=='text-editor' and not state()['state'].get('textEditing'):
        click('edit-text',scroll=False)
    if control in {'field-x','field-y','field-width','field-height','field-rotation','flip-x','flip-y'} and control not in state()['controls']:
        click('tool-move')
    if control in {'field-font-size','select-font'} and control not in state()['controls']:
        click('tool-text')
    if control in menus and control not in state()['controls']:
        click('menu-'+menus[control],scroll=False)

def bounds(control, scroll=True):
    prepare_control(control)
    control={"tool-eraser":"brush-erase","tool-ellipse":"shape-ellipse"}.get(control,control)
    if control=='foreground-picker' and control not in state()['controls']:
        control='foreground-picker-rail'
    rect=None
    for _ in range(70):
        current=state();rect=current['controls'].get(control)
        if rect:
            left,top,width,height=rect
            canvas=current['controls']['canvas']; bottom=canvas[1]+canvas[3]
            if not scroll or current['modal'] or (not control.startswith('tool-') and not control.endswith('picker-rail')) or (84<=top and top+height<=bottom):
                return rect
            x('mousemove',28,round(canvas[1]+canvas[3]/2),'click',5 if top+height>bottom else 4)
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
    click('field-' + name)
    key('ctrl+a')
    if str(value): x('type', '--clearmodifiers', str(value))
    else: x('key', '--clearmodifiers', 'BackSpace')
    if enter:
        key('Return')
    time.sleep(.1)

def choose(control, index):
    if control=='font':
        click('select-font');key('ctrl+a');x('type','--clearmodifiers',['sans-serif','serif','monospace'][index]);key('Return');return
    values = {'font':['sans-serif','serif','monospace'], 'blend':['source-over','darken','multiply','color-burn','linear-burn','lighten','screen','color-dodge','linear-dodge','overlay','soft-light','hard-light','vivid-light','linear-light','pin-light','hard-mix','difference','exclusion','subtract','divide','hue','saturation','color','luminosity']}
    if control in values:
        value = selected()['content']['fontFamily'] if control=='font' else selected()['blend']
        current = values[control].index(value)
    elif control == 'mask-source':
        value = selected()['maskSourceId']
        current = state()['state']['maskSourceIds'].index(value) if value else -1
    elif control == 'crop-ratio':
        current = ['free','original','square','fourThree','sixteenNine'].index(state()['state']['cropRatio'])
    else:
        current = state()['state']['wand']['radius'] if control == 'wand-sample' else 0 # Fresh size dialogs start at their first unit.
    position = (selected()['x'], selected()['y']) if layers() else None
    click('select-'+control)
    for _ in range(abs(index-current)):key('Down' if index>current else 'Up')
    key('Return')
    if position is not None: assert (selected()['x'], selected()['y']) == position, 'Selector navigation must not nudge the layer'

def drag(x0, y0, x1, y1, modifier=None):
    if modifier:
        x('keydown', *modifier.split())
    x('mousemove', round(x0), round(y0), 'mousedown', 1)
    for step in range(1, 21):
        x('mousemove', round(x0 + (x1-x0)*step/20), round(y0 + (y1-y0)*step/20))
        time.sleep(.015)
    x('mouseup', 1)
    if modifier:
        x('keyup', *modifier.split())
    time.sleep(.25)
    settle()

def canvas_point(old_x, old_y):
    current=state(); snapshot=current['state']; viewport=snapshot['viewport']; doc=snapshot['document']
    left,top,width,height=current['controls']['canvas']; zoom=viewport['zoom']
    return (left+(width-doc['width']*zoom)/2+viewport['pan']['x']+(old_x-332)*zoom,
            top+(height-doc['height']*zoom)/2+viewport['pan']['y']+(old_y-315)*zoom)

def canvas_mouse(old_x,old_y,*arguments):
    px,py=canvas_point(old_x,old_y)
    return x('mousemove',round(px),round(py),*arguments)

def doc_drag(x0,y0,x1,y1,modifier=None):
    start=canvas_point(x0,y0);end=canvas_point(x1,y1)
    drag(*start,*end,modifier)

def shot(name):
    time.sleep(.3)
    subprocess.run(['import', '-window', window_id or 'root', str(out / name)], env=env, check=True, timeout=15)

def layers(): return state()['state']['document']['layers']
def selected():
    selected_id = state()['state']['selection']['ids'][-1]
    return next(layer for layer in layers() if layer['id'] == selected_id)

def portal(path, save=True):
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
    key('ctrl+a' if save else 'ctrl+l')
    x('type', '--clearmodifiers', str(path))
    key('Return')
    # GTK opens a parent directory before submitting a new absolute filename.
    time.sleep(.4)
    try:
        visible = x('search', '--onlyvisible', '--class', 'Xdg-desktop-portal-gtk')
        if dialog in visible.splitlines(): key('Return')
    except subprocess.CalledProcessError:
        pass
    wait(lambda s: not s['busy'], 'file operation', timeout=30)
    focus(re.escape(state()['state']['document']['name'])+'.*Picsie')

def layers_masks_colors(number):
    global window_number
    previous_window=window_number;previous_name=state()['state']['document']['name']
    click('new',scroll=False);field('new-name','Layers masks colors',False);field('new-width',256,False);field('new-height',192,False);click('modal-apply')
    window_number=number;wait(lambda s:s.get('state') is not None);focus('Layers masks colors.*Picsie');x('windowsize',window_id,1281,860);time.sleep(.3);click('fit')
    def pt(dx,dy):
        s=state();v=s['state']['viewport'];d=s['state']['document'];b=s['controls']['canvas'];z=v['zoom']
        return (b[0]+(b[2]-d['width']*z)/2+v['pan']['x']+dx*z,b[1]+(b[3]-d['height']*z)/2+v['pan']['y']+dy*z)
    def center(name):
        b=bounds(name,False);return b[0]+b[2]/2,b[1]+b[3]/2
    def color(value,background=False):
        # The foreground occludes the overlapping part of the background swatch.
        name='background-picker-rail' if background else 'foreground-picker-rail';b=bounds(name,False)
        x('mousemove',round(b[0]+b[2]-4 if background else b[0]+5),round(b[1]+b[3]-4 if background else b[1]+5),'click',1)
        wait(lambda s:s['modal']=='color');field('color-hex',value,False);click('modal-apply')
    color('E53B42');color('F1D5A2',True)
    check('overlapping palette swatches edit separate foreground and background',lambda: (state()['state']['color'].lower()=='#e53b42' and state()['state']['backgroundColor'].lower()=='#f1d5a2'))
    key('x');check('X swaps foreground and background',lambda: (state()['state']['color'].lower()=='#f1d5a2' and state()['state']['backgroundColor'].lower()=='#e53b42'));key('x')
    click('tool-rectangle');drag(*pt(20,20),*pt(200,170));red=selected()['id']
    color('3276D2');drag(*pt(120,60),*pt(235,150));blue=selected()['id'];click('tool-move')
    click('tool-marquee');drag(*pt(190,80),*pt(220,120));count=state()['state']['history']['undoCount'];key('ctrl+BackSpace');check('background fill is one undo and retains pixel selection',lambda: (state()['state']['history']['undoCount']==count+1 and state()['state']['hasPixelSelection']))
    click('foreground-picker-rail');x('mousemove',*map(round,pt(210,100)),'click',1);wait(lambda s:s.get('colorWorking','').lower()=='#f1d5a2');check('background fill uses the separate palette color',lambda: (state()['state']['color'].lower()=='#3276d2'));click('modal-cancel');key('ctrl+z');key('ctrl+d');click('tool-move')
    r=bounds('layer-name-'+blue,False);x('mousemove',round(r[0]+25),round(r[1]+r[3]/2),'click','--repeat',2,'--delay',90,1)
    wait(lambda s:s['renameLayer']==blue);key('ctrl+a');x('type','--clearmodifiers','Blue overlay');key('Return')
    check('inline layer rename commits with Return',lambda: (selected()['name']=='Blue overlay' and state()['renameLayer'] is None))
    r=bounds('layer-name-'+blue,False);x('mousemove',round(r[0]+25),round(r[1]+r[3]/2),'click','--repeat',2,'--delay',90,1);wait(lambda s:s['renameLayer']==blue);key('ctrl+a');x('type','--clearmodifiers','Cancelled');key('Escape')
    check('inline layer rename Escape restores the name',lambda: (selected()['name']=='Blue overlay' and state()['renameLayer'] is None))
    # Kit owns context-menu focus, navigation and dismissal.
    x('mousemove',*map(round,center('layer-name-'+blue)),'click',3);time.sleep(.2);key('Down');key('Return');wait(lambda s:s['renameLayer']==blue);key('Escape')
    check('layer context menu opens inline Rename',lambda: (selected()['name']=='Blue overlay'))
    count=state()['state']['history']['undoCount'];selection=state()['state']['selection'].copy()
    drag(*center('visibility-'+blue),*center('visibility-'+red))
    check('visibility swipe applies one state with one undo and retains selection',lambda: (all(not l['visible'] for l in layers()) and state()['state']['history']['undoCount']==count+1 and state()['state']['selection']==selection));key('ctrl+z')
    count=state()['state']['history']['undoCount'];drag(*center('layer-name-'+blue),*center('layer-name-'+red),modifier='Alt_L')
    check('Alt drag duplicates and reorders in one undo',lambda: (len(layers())==3 and state()['state']['history']['undoCount']==count+1 and selected()['id']!=blue));key('ctrl+z');click('thumbnail-'+red);wait(lambda s:s['state']['selection']['ids']==[red])
    r=bounds('layer-'+blue,False);x('keydown','Alt_L','mousemove',round(r[0]+r[2]/2),round(r[1]+r[3]-4),'click',1,'keyup','Alt_L');time.sleep(.25);settle()
    check('Alt clipping boundary creates a link without changing selection',lambda: (next(l for l in layers() if l['id']==blue)['maskSourceId']==red and selected()['id']==red));key('ctrl+z');click('layer-'+blue)
    click('select-blend');key('Down');check('blend menu previews without history mutation',lambda: (state()['state']['blendPreview'] is not None and selected()['blend']=='source-over'));key('Escape')
    check('blend menu dismissal restores the committed mode',lambda: (state()['state']['blendPreview'] is None and selected()['blend']=='source-over'))
    choose('blend',23);check('all 24 source blend modes are selectable',lambda: (selected()['blend']=='luminosity'));key('shift+plus');check('Shift plus wraps the source blend order',lambda: (selected()['blend']=='source-over'));key('shift+minus');check('Shift minus wraps backward',lambda: (selected()['blend']=='luminosity'));choose('blend',0)
    click('foreground-picker-rail');field('color-hex','123456',False)
    check('picker working color does not mutate palette before OK',lambda: (state()['colorWorking'].lower()=='#123456' and state()['state']['color'].lower()=='#3276d2'))
    r=bounds('color-panel-title',False);old=state()['colorPickerPosition'];drag(r[0]+120,r[1]+r[3]/2,r[0]+70,r[1]+r[3]/2+15)
    check('nonmodal color panel moves and persists its position',lambda: (state()['colorPickerPosition']!=old and all(abs(a-b)<.01 for a,b in zip(json.loads((out/'config/picsie/ui.json').read_text())['color_picker_position'],state()['colorPickerPosition']))))
    field('color-r',100,False);key('Up');check('RGB Up steps one without committing palette',lambda: (state()['colorWorking'].lower().startswith('#65') and state()['state']['color'].lower()=='#3276d2'));key('shift+Down');check('RGB Shift Down steps ten',lambda: (state()['colorWorking'].lower().startswith('#5b')))
    x('mousemove',*map(round,pt(210,100)),'mousedown',1);wait(lambda s:s['samplingColor'] and s['sampleCurrent'].lower()=='#3276d2');shot('24-picker-sampling.png');x('mouseup',1);time.sleep(.2)
    check('open picker samples composited canvas into working color only',lambda: (state()['colorWorking'].lower()=='#3276d2' and state()['state']['color'].lower()=='#3276d2'));click('modal-cancel')
    color('E53B42');click('tool-brush');count=state()['state']['history']['undoCount'];x('keydown','Alt_L','mousemove',*map(round,pt(210,100)),'mousedown',1);wait(lambda s:s['sampleCurrent'].lower()=='#3276d2');x('mouseup',1,'keyup','Alt_L');time.sleep(.2)
    check('Alt Brush temporarily samples displayed color without painting or undo',lambda: (state()['state']['color'].lower()=='#3276d2' and state()['state']['history']['undoCount']==count and state()['state']['tool']=='brush'))
    click('tool-eyedropper');click('sample-ring-toggle');check('Sample Ring toggle persists',lambda: (not state()['showsSampleRing'] and not json.loads((out/'config/picsie/ui.json').read_text())['shows_sample_ring']));click('sample-ring-toggle')
    x('mousemove',*map(round,pt(70,110)),'mousedown',1);wait(lambda s:s['sampleCurrent'].lower()=='#e53b42');shot('25-sample-ring.png');x('mouseup',1);time.sleep(.2)
    check('eyedropper comparison ring retains old and new colors',lambda: (state()['sampleOriginal'].lower()=='#3276d2' and state()['sampleCurrent'].lower()=='#e53b42'))
    x('mousemove',*map(round,pt(2,2)),'click',1);time.sleep(.2);x('mousemove',*map(round,pt(-10,-10)),'click',1);time.sleep(.2);check('eyedropper ignores outside and transparent canvas',lambda: (state()['state']['color'].lower()=='#e53b42'));key('d');check('D restores black and white palette defaults',lambda: (state()['state']['color']=='#000000' and state()['state']['backgroundColor']=='#ffffff'))
    click('layer-'+red);click('tool-marquee');drag(*pt(40,40),*pt(80,80));count=state()['state']['history']['undoCount'];click('foreground-picker-rail');click('mask-menu')
    check('mask targeting cancels an open palette picker',lambda: (state()['modal'] is None));check('one click Add Mask consumes selection in one undo and keeps tool',lambda: (selected()['mask'] is not None and not state()['state']['hasPixelSelection'] and state()['state']['paintTarget']=='mask' and state()['state']['tool']=='marquee' and state()['state']['history']['undoCount']==count+1))
    click('mask-row-'+red,modifier='Shift_L');check('Shift mask thumbnail disables without changing tool',lambda: (selected()['mask']['enabled']==False and state()['state']['tool']=='marquee'));click('mask-row-'+red,modifier='Shift_L');click('layer-'+blue);click('mask-chain-'+red);check('thumbnail chain unlinks without changing active layer',lambda: (selected()['id']==blue and next(l for l in layers() if l['id']==red)['mask']['linked']==False));key('ctrl+z');click('mask-row-'+red,modifier='Control_L');check('Command mask thumbnail selects black coverage',lambda: (state()['state']['pixelSelectionBounds']=={'x':40.,'y':40.,'width':40.,'height':40.}))
    click('thumbnail-'+blue,modifier='Control_L Shift_L');check('Shift Command thumbnail adds coverage',lambda: (state()['state']['pixelSelectionBounds']['width']>40));click('thumbnail-'+blue,modifier='Control_L Alt_L');check('Alt Command thumbnail subtracts coverage',lambda: (state()['state']['pixelSelectionBounds']=={'x':40.,'y':40.,'width':40.,'height':40.}));key('ctrl+d')
    click('mask-row-'+red);key('d');check('mask palette defaults are independent of image palette',lambda: (state()['state']['maskMode']=='hide' and state()['state']['color']=='#000000'));key('x');check('X swaps mask foreground and background',lambda: (state()['state']['maskMode']=='reveal'))
    click('foreground-picker-rail');click('mask-color-black');check('mask palette chooses black and dismisses',lambda: (state()['state']['maskMode']=='hide' and state()['palette'] is None))
    click('tool-brush');count=state()['state']['history']['undoCount'];x('keydown','Alt_L','mousemove',*map(round,pt(210,100)),'click',1,'keyup','Alt_L');time.sleep(.2);check('Alt mask Brush samples image foreground and preserves mask palette',lambda: (state()['state']['color'].lower()=='#3276d2' and state()['state']['maskMode']=='hide' and state()['state']['history']['undoCount']==count));click('tool-move')
    count=state()['state']['history']['undoCount'];drag(*center('mask-row-'+red),*center('layer-name-'+blue),modifier='Alt_L')
    check('Alt mask drag copies coverage and document placement in one undo',lambda: (selected()['id']==blue and selected()['mask'] is not None and selected()['mask'].get('placement') is not None and state()['state']['history']['undoCount']==count+1));key('ctrl+z');click('mask-row-'+red)
    click('mask-link');key('Escape');click('tool-move');base=selected().copy();count=state()['state']['history']['undoCount'];field('x',30);field('rotation',12);click('transform-cancel')
    check('independent mask numeric transform cancels without moving layer',lambda: (selected()==base))
    field('x',30);field('rotation',12);click('transform-apply');check('independent mask numeric transform commits one undo',lambda: (selected()['x']==base['x'] and selected()['mask']['placement']['x']==30 and selected()['mask']['placement']['rotation']==12 and state()['state']['history']['undoCount']==count+1));key('ctrl+z')
    # A source Cmd/Control corner drag is a non-destructive distortion draft.
    drag(*pt(20,20),*pt(30,35),modifier='Control_L');check('mask corner drag creates perspective draft',lambda: (state()['state']['maskDistortion'] is not None and state()['state']['transformActive']));shot('26-mask-distortion.png');click('transform-cancel')
    check('mask distortion Cancel restores raster and placement',lambda: (selected()==base));drag(*pt(20,20),*pt(30,35),modifier='Control_L');click('transform-apply');check('mask distortion Apply leaves layer geometry intact',lambda: (selected()['x']==base['x'] and selected()['scaleX']==base['scaleX'] and state()['state']['maskDistortion'] is None));key('ctrl+z')
    click('thumbnail-'+red);key('ctrl+d');click('zoom-in');click('zoom-in');shot('27-layers-masks-colors.png');click('fit')
    key('5');key('4');check('paired opacity digits apply the exact percent',lambda: (abs(selected()['opacity']-.54)<.001));key('ctrl+z')
    click('layer-'+blue,modifier='Control_L');click('group');folder=selected()['id'];click('tool-marquee');drag(*pt(40,40),*pt(80,80));click('mask-menu');click('mask-link');key('Escape');click('tool-move');count=state()['state']['history']['undoCount'];field('x',12);click('transform-apply')
    check('folder mask can transform independently in one undo',lambda: (selected()['id']==folder and selected()['x']==0 and selected()['mask'].get('placement',{}).get('x')==12 and state()['state']['history']['undoCount']==count+1));key('ctrl+z')
    click('mask-row-'+folder);click('delete');check('trash deletes targeted mask and preserves folder and children',lambda: (selected()['id']==folder and selected()['mask'] is None and len(layers())==3));click('collapse-'+folder);click('add-paint');outside=selected()['id'];click('layer-'+folder,modifier='Shift_L')
    check('Shift range excludes children of a collapsed folder',lambda: (set(state()['state']['selection']['ids'])=={folder,outside}))
    click('layer-'+outside)
    color('F1D5A2',True);click('canvas-size');field('canvas-width',276,False);choose('canvas-fill',2);click('modal-apply');check('Canvas Size uses background palette for extension',lambda: (state()['state']['document']['width']==276));click('tool-eyedropper');x('mousemove',*map(round,pt(2,100)),'click',1);time.sleep(.2);check('background extension has the chosen color',lambda: (state()['state']['color'].lower()=='#f1d5a2'));key('ctrl+z');click('tool-move')
    # Enough rows to test scrolling beyond the list, including fixed footer visibility.
    for _ in range(18): key('ctrl+j')
    top=selected()['id'];r=bounds('layer-name-'+top,False);lst=state()['controls']['canvas'];bottom=lst[1]+lst[3]-55
    x('mousemove',round(r[0]+25),round(r[1]+r[3]/2),'mousedown',1,'mousemove',round(r[0]+25),round(bottom));time.sleep(.8)
    check('layer drag autoscrolls the list and keeps footer visible',lambda: (state()['listScroll']< -100 and state()['controls']['delete'][1]+state()['controls']['delete'][3]<830));x('mouseup',1);time.sleep(.3);settle()
    x('windowsize',window_id,800,520);time.sleep(.3);click('foreground-picker-rail');wait(lambda s:s['modal']=='color');check('picker stays inside minimum window',lambda: (state()['controls']['color-panel'][0]+state()['controls']['color-panel'][2]<=800 and state()['controls']['color-panel'][1]+state()['controls']['color-panel'][3]<=520));shot('28-color-minimum.png');click('modal-cancel')
    key('ctrl+w');wait(lambda s:s['modal']=='close');click('modal-discard');wait(lambda s:not window_exists(window_id),'close polish window')
    window_number=previous_window;focus(re.escape(previous_name)+'.*Picsie')

def feature_workflows():
    global window_number
    click('new',scroll=False);field('new-name','Feature workflows',False);field('new-width','400',False);field('new-height','300',False);click('modal-apply')
    window_number=3;wait(lambda s:s.get('state') is not None);focus('Feature workflows.*Picsie');x('windowsize',window_id,1281,860);time.sleep(.3)
    click('tool-rectangle');doc_drag(352,335,452,435);click('tool-marquee');doc_drag(372,355,392,375)
    original=layers().copy();selection=state()['state']['pixelSelectionBounds'].copy();count=state()['state']['history']['undoCount']
    key('ctrl+c');wait(lambda s:not s['busy'] and s['notice']=='Copied pixels')
    check('system image copy creates no undo',lambda: (state()['state']['history']['undoCount']==count))
    window_number=2;focus('GPUI parity.*Picsie');click('tool-move');base_count=len(layers());key('ctrl+v');wait(lambda s:not s['busy'] and len(s['state']['document']['layers'])==base_count+1)
    check('copied image pastes in place across native windows',lambda: (selected()['x']==40 and selected()['y']==40));key('ctrl+z')
    window_number=3;focus('Feature workflows.*Picsie')
    key('ctrl+v');wait(lambda s:not s['busy'] and len(s['state']['document']['layers'])==2)
    check('system image paste retains internal origin and clears selection',lambda: (selected()['x']==40 and selected()['y']==40 and not state()['state']['hasPixelSelection']))
    key('ctrl+z');check('paste undo restores selection',lambda: (state()['state']['pixelSelectionBounds']==selection))
    key('ctrl+shift+c');wait(lambda s:not s['busy'] and s['notice']=='Copied pixels');key('ctrl+v');wait(lambda s:not s['busy'] and len(s['state']['document']['layers'])==2)
    check('Copy Merged exchanges native image pixels',lambda: (selected()['width']==selection['width'] and selected()['height']==selection['height']));key('ctrl+z')
    key('ctrl+x');wait(lambda s:not s['busy'] and s['state']['history']['undoLabel']=='Clear selected pixels')
    check('Cut is one undo and retains selection',lambda: (state()['state']['history']['undoCount']==count+1 and state()['state']['pixelSelectionBounds']==selection))
    key('ctrl+z');check('Cut undo restores original content',lambda: (layers()==original))
    key('ctrl+j');wait(lambda s:len(s['state']['document']['layers'])==2)
    check('Layer via Copy creates layer in place',lambda: (selected()['x']==40 and selected()['y']==40));key('ctrl+z')
    if shutil.which('xclip',path=env['PATH']):
        image=out/'external-clipboard.png'
        image_command = 'magick' if shutil.which('magick',path=env['PATH']) else 'convert'
        subprocess.run([image_command,'-size','12x8','xc:#ff0000',str(image)],env=env,check=True,timeout=15)
        subprocess.run(['xclip','-selection','clipboard','-t','image/png','-i',str(image)],env=env,check=True,timeout=15)
        key('ctrl+v');wait(lambda s:not s['busy'] and len(s['state']['document']['layers'])==2)
        check('external system image paste is centered',lambda: (selected()['x']==194 and selected()['y']==146 and selected()['width']==12 and selected()['height']==8));key('ctrl+z')
    doc_drag(380,363,390,373);moved=state()['state']['pixelSelectionBounds']
    check('drag inside selection moves outline by whole pixels',lambda: (moved['x']==50 and moved['y']==50));key('ctrl+z')
    key('Right');check('selection arrow nudge',lambda: (state()['state']['pixelSelectionBounds']['x']==41));key('ctrl+z')
    doc_drag(380,363,410,363,'Control_L')
    check('Control drag moves pixels in one undo',lambda: (state()['state']['pixelSelectionBounds']['x']==70 and state()['state']['history']['undoLabel']=='Move Pixels'));key('ctrl+z')
    x('keydown','Alt_L');doc_drag(380,363,410,363,'Control_L');x('keyup','Alt_L')
    check('Control Alt drag duplicates pixels',lambda: (state()['state']['pixelSelectionBounds']['x']==70 and state()['state']['history']['undoLabel']=='Duplicate Pixels'));key('ctrl+z')
    key('ctrl+t');wait(lambda s:s['state']['transformActive']);field('x','70');shot('10-selection-transform.png');click('transform-cancel',scroll=False)
    check('transform Cancel restores document and selection',lambda: (layers()==original and state()['state']['pixelSelectionBounds']==selection and not state()['state']['transformActive']))
    count=state()['state']['history']['undoCount'];key('ctrl+t');wait(lambda s:s['state']['transformActive']);field('x','70');click('transform-apply',scroll=False)
    check('transform Apply merges into source as one undo',lambda: (len(layers())==1 and state()['state']['pixelSelectionBounds']['x']==70 and state()['state']['history']['undoCount']==count+1));key('ctrl+z')
    key('ctrl+d');click('tool-lasso');click('lasso-polygonal',scroll=False)
    for px,py in [(372,355),(432,355),(432,415),(372,415)]:canvas_mouse(px,py,'click',1);time.sleep(.18)
    wait(lambda s:s['state']['selectionDraft']);key('BackSpace');canvas_mouse(372,415,'click',1);time.sleep(.18);key('Return')
    check('polygonal lasso supports corners, Backspace and Enter',lambda: (state()['state']['hasPixelSelection'] and not state()['state']['selectionDraft']));shot('11-polygonal-lasso.png');key('ctrl+d')
    canvas_mouse(372,355,'click',1);time.sleep(.2);key('Escape')
    check('Escape cancels polygon draft',lambda: (not state()['state']['selectionDraft'] and not state()['state']['hasPixelSelection']))
    click('tool-wand');field('wand-tolerance','0');click('tool-wand');canvas_mouse(382,365,'click',1);time.sleep(.25);settle()
    check('magic wand selects matching connected pixels',lambda: (state()['state']['pixelSelectionBounds']=={'x':20,'y':20,'width':100,'height':100}))
    doc_drag(382,365,412,365,'Control_L');check('Control drag also moves wand-selected pixels',lambda: (state()['state']['history']['undoLabel']=='Move Pixels'));key('ctrl+z')
    choose('wand-sample',1);check('wand 3 by 3 sampling control',lambda: (state()['state']['wand']['radius']==1))
    click('wand-contiguous');click('wand-all-layers');shot('12-magic-wand.png')
    check('wand contiguous and all-layer controls',lambda: (not state()['state']['wand']['contiguous'] and state()['state']['wand']['sampleAllLayers']));key('ctrl+d')
    click('tool-move');click('duplicate');key('ctrl+e');wait(lambda s:len(s['state']['document']['layers'])==1)
    check('Merge Down is one command and undo restores layers',lambda: (state()['state']['history']['undoLabel']=='Merge Down'));key('ctrl+z')
    key('ctrl+a');key('ctrl+e');wait(lambda s:len(s['state']['document']['layers'])==1)
    check('merge selected layers',lambda: (state()['state']['history']['undoLabel']=='Merge Layers'));key('ctrl+z');click('group');key('ctrl+e');wait(lambda s:len(s['state']['document']['layers'])==1)
    check('merge group removes folder',lambda: (state()['state']['history']['undoLabel']=='Merge Group'));key('ctrl+z');key('ctrl+z');key('ctrl+z')
    key('ctrl+alt+i');wait(lambda s:s['modal']=='image-size');field('image-width','800',False);field('image-resolution','300',False);shot('13-image-size.png');click('modal-apply');wait(lambda s:s['modal'] is None and not s['busy'])
    check('Image Size scales pixels and resolution',lambda: (state()['state']['document']['width']==800 and state()['state']['document']['height']==600 and state()['state']['document']['resolution']==300))
    key('ctrl+z');check('Image Size undo restores pixels and resolution',lambda: (state()['state']['document']['width']==400 and state()['state']['document']['resolution']==72))
    key('ctrl+alt+i');click('image-resample');choose('image-unit',1);field('image-resolution','150',False);shot('14-resolution-only.png');click('modal-apply');wait(lambda s:s['modal'] is None and not s['busy'])
    check('resolution-only change retains original pixel geometry',lambda: (state()['state']['document']['width']==400 and state()['state']['document']['height']==300 and state()['state']['document']['resolution']==150));key('ctrl+z')
    key('ctrl+alt+i');field('image-width','9000',False);click('modal-apply')
    check('Image Size rejects invalid allocation without applying',lambda: (state()['modal']=='image-size' and state()['state']['document']['width']==400));click('modal-cancel')
    click('tool-wand');x('windowsize',window_id,960,640);time.sleep(.4);click('tool-wand');check('last tool stays accessible by scrolling at minimum window',lambda: (state()['controls']['tool-wand'][1]+state()['controls']['tool-wand'][3]<=610));shot('15-new-tools-minimum-size.png');x('windowsize',window_id,1280,860);time.sleep(.3)
    key('ctrl+w');wait(lambda s:s['modal']=='close');click('modal-discard');time.sleep(.3)
    window_number=2;focus('GPUI parity.*Picsie')

def close(a,b): return abs(a-b)<.05
def pt(xp,yp):
    s=state();v=s['state']['viewport'];d=s['state']['document'];r=s['controls']['canvas'];z=v['zoom']
    return (r[0]+(r[2]-d['width']*z)/2+v['pan']['x']+xp*z,r[1]+(r[3]-d['height']*z)/2+v['pan']['y']+yp*z)
def new_placement_window(name,w=400,h=300,number=4):
    global window_number
    click('new-canvas');field('new-name',name,False);field('new-width',w,False);field('new-height',h,False);click('modal-apply');window_number=number;wait(lambda s:s.get('state') is not None);focus(name+'.*Picsie');x('windowsize',window_id,1281,860);time.sleep(.4);click('actual')

def placement_text_workflows():
    global window_number
    previous_window=window_number;previous_name=state()['state']['document']['name']
    new_placement_window('Placement and type')
    click('rulers')
    r=bounds('ruler-vertical',False);drag(r[0]+9,r[1]+120,*pt(60,70));check('vertical ruler creates document guide',lambda: (close(state()['state']['document']['guides'][0]['position'],60)))
    r=bounds('ruler-horizontal',False);drag(r[0]+120,r[1]+9,*pt(90,45));check('horizontal ruler creates document guide',lambda: (close(state()['state']['document']['guides'][1]['position'],45)))
    count=state()['state']['history']['undoCount'];drag(*pt(60,20),*pt(80,20));check('guide movement is one undo',lambda: (state()['state']['history']['undoCount']==count+1 and close(state()['state']['document']['guides'][0]['position'],80)))
    key('ctrl+z');check('guide movement undo',lambda: (close(state()['state']['document']['guides'][0]['position'],60)))
    r=bounds('ruler-vertical',False);drag(*pt(60,20),r[0]+8,r[1]+120);check('ruler drop deletes guide',lambda: (len(state()['state']['document']['guides'])==1));key('ctrl+z')
    click('lock-guides');drag(*pt(60,20),*pt(85,20));check('locked guide does not move',lambda: (close(state()['state']['document']['guides'][0]['position'],60)));click('lock-guides')
    click('grid');shot('17-guides-grid.png');click('grid')
    click('tool-rectangle');drag(*pt(130,90),*pt(170,130));click('tool-move');check('Auto Select default follows source',lambda: (state()['state']['viewOptions']['autoSelect']==False))
    old=(selected()['x'],selected()['y']);drag(*pt(280,230),*pt(285,237),modifier='Control_L');check('Control drag outside bounds moves active layer freely',lambda: (close(selected()['x'],old[0]+5) and close(selected()['y'],old[1]+7)))
    field('x',100);click('transform-apply');field('y',90);click('transform-apply');drag(*pt(115,105),*pt(77,105));check('move snaps left edge to guide',lambda: (close(selected()['x'],60)))
    click('show-controls');check('Show Controls toggle',lambda: (not state()['state']['viewOptions']['showControls']));click('show-controls')
    first=selected()['id'];click('tool-rectangle');drag(*pt(260,110),*pt(280,125),modifier='Shift_L Alt_L');check('shape Shift Alt draws from center with equal sides',lambda: (selected()['width']==40 and selected()['height']==40 and close(selected()['x'],240) and close(selected()['y'],90)));second=selected()['id'];click('tool-move');click('layer-'+first);click('auto-select');x('mousemove',*map(round,pt(260,110)),'click',1);time.sleep(.2);settle();check('Auto Select picks the layer under the click',lambda: (selected()['id']==second));click('auto-select');click('delete');click('visibility-'+first)
    click('tool-text');x('mousemove',*map(round,pt(25,150)),'click',1);time.sleep(.35);settle();wait(lambda s:s['state']['textEditing'],'point text draft')
    wait(lambda s:not s['state']['textCaretVisible'],'caret blink hidden');check('idle text caret blinks without committing draft',lambda: (state()['state']['textEditing']));
    rect=bounds('text-editor',False);caret=state()['state']['textCaret'];expected=pt(caret['x'],caret['y']);check('native text input follows the rendered caret',lambda: (abs(rect[0]-expected[0])<.05 and abs(rect[1]-expected[1])<.05 and rect[2:]==[1.,18.]))
    x('type','--clearmodifiers','Native layout');wait(lambda s:s['state']['currentText']['content'].get('text')=='Native layout' and s['state']['textCaretVisible'],'typed text and reset caret');check('typing resets the visible text caret',lambda: (True));time.sleep(.35);shot('18-point-text.png');check('canvas typing preserves spaces',lambda: (selected()['content']['text']=='Native layout'));check('point text grows',lambda: (selected()['textLayout']['point'] and selected()['width']>200))
    key('ctrl+a');key('ctrl+c');copied=subprocess.check_output(['xclip','-selection','clipboard','-o'],env=env,text=True,timeout=10);check('text selection uses native clipboard',lambda: (copied=='Native layout'));shot('19-text-selection.png')
    key('Right');key('shift+Return');x('type','--clearmodifiers','editable');key('Return');check('Enter commits one text history entry',lambda: (not state()['state']['textEditing'] and selected()['content']['text']=='Native layout\neditable'))
    id=selected()['id'];old=selected().copy();click('edit-text');key('ctrl+a');x('type','--clearmodifiers','Cancelled change');key('Escape');check('Escape restores text and geometry',lambda: (selected()['content']==old['content'] and selected()['width']==old['width']))
    click('edit-text');field('font-size',32);field('tracking',3);field('leading',44);click('text-center');choose('font',2);wait(lambda s:s['state']['currentText'].get('textLayout',{}).get('fontName')=='monospace','font choice')
    click('text-color');field('color-hex','CEE1FF',False);click('modal-apply');check('Type color updates edited text and foreground together',lambda: (selected()['content']['color'].lower()=='#cee1ff' and state()['state']['color'].lower()=='#cee1ff'))
    r=bounds('field-font-size',False);x('mousemove',round(r[0]+r[2]/2),round(r[1]+r[3]/2),'click',1);key('Up');check('Type numeric Up steps one',lambda: (selected()['content']['fontSize']==33));key('shift+Down');check('Type numeric Shift Down steps ten',lambda: (selected()['content']['fontSize']==23));field('font-size',32);
    field('leading','',False);r=bounds('field-leading',False);x('mousemove',round(r[0]+r[2]/2),round(r[1]+r[3]/2),'click',1);key('Up');check('Auto leading steps from computed line height',lambda: (abs(selected()['textLayout']['leading']-39.4)<.01));field('leading',44);
    check('typography controls edit draft',lambda: (selected()['textLayout']['tracking']==3 and selected()['textLayout']['leading']==44 and selected()['textLayout']['alignment']=='center'));shot('20-typography.png');click('text-done');click('tool-move');field('y',190);click('transform-apply')
    click('tool-text');drag(*pt(30,25),*pt(250,125));wait(lambda s:s['state']['textEditing'],'paragraph draft');field('font-size',20);field('tracking',0);field('leading',28);click('text-left');x('type','--clearmodifiers','Paragraph text wraps inside the box. Resize the handles to reflow the words.');time.sleep(.35);settle();x('mousemove',*map(round,pt(110,55)));wait(lambda s:s['cursorHint']=='text','canvas text cursor');check('canvas text uses I-beam feedback',lambda: (state()['cursorHint']=='text'));x('mousemove',*map(round,pt(30,70)));wait(lambda s:s['cursorHint']=='resizeHorizontal','text edge cursor');check('text edge offers directional resize cursor',lambda: (state()['cursorHint']=='resizeHorizontal'));
    check('drag makes paragraph text',lambda: (selected()['width']==220 and selected()['height']==100 and not selected()['textLayout']['point']));shot('21-paragraph.png')
    key('ctrl+Home');top=state()['state']['textCaret']['y'];key('shift+Down');check('Shift Down follows the rendered paragraph row',lambda: (state()['state']['textSelection']['anchor']==0 and 0<state()['state']['textSelection']['head']<len(selected()['content']['text'])/2 and 10<state()['state']['textCaret']['y']-top<40));key('Up');check('Up returns to the previous wrapped row',lambda: (state()['state']['textSelection']['head']==0));key('End');check('End stays on the current rendered row',lambda: (state()['state']['textSelection']['head']<len(selected()['content']['text']) and abs(state()['state']['textCaret']['y']-top)<8));key('ctrl+End')
    key('alt+Right');key('alt+shift+Down');check('Alt arrows adjust tracking and leading',lambda: (selected()['textLayout']['tracking']==1 and selected()['textLayout']['leading']==38));key('alt+Left');key('alt+shift+Up')
    paragraph=selected()['content']['text'];key('ctrl+a');subprocess.run(['xclip','-selection','clipboard','-i'],env=env,input='Café 🙂 editable',text=True,check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL);key('ctrl+v');check('native paste retains Unicode',lambda: (selected()['content']['text']=='Café 🙂 editable'));key('ctrl+z');check('local text undo restores paragraph',lambda: (selected()['content']['text']==paragraph));key('Right')
    x('mousemove',*map(round,pt(55,40)),'click','--repeat',2,'--delay',80,1);time.sleep(.2);settle();sel=state()['state']['textSelection'];check('double click selects a whole text word',lambda: (selected()['content']['text'][min(sel['anchor'],sel['head']):max(sel['anchor'],sel['head'])]=='Paragraph'));x('mousemove',*map(round,pt(55,40)),'click','--repeat',3,'--delay',80,1);time.sleep(.2);settle();sel=state()['state']['textSelection'];check('triple click selects the text paragraph',lambda: (abs(sel['head']-sel['anchor'])==len(paragraph)));key('Right');
    before_font=selected()['content']['fontSize'];layer=selected();drag(*pt(layer['x']+layer['width'],layer['y']+layer['height']),*pt(layer['x']+layer['width']+70,layer['y']+layer['height']+60));check('text box handles resize without scaling glyphs',lambda: (selected()['width']==290 and selected()['height']==160 and selected()['content']['fontSize']==before_font and selected()['scaleX']==1));shot('22-resized-paragraph.png');click('text-done')
    click('tool-move');x('mousemove',*map(round,pt(85,selected()['y'])));wait(lambda s:s['cursorHint']=='resizeVertical','whole-edge transform cursor');check('whole transform edge offers resize feedback',lambda: (state()['cursorHint']=='resizeVertical'));click('save-as');portal(out/'layout.picsie');check('saved project contains guides and text layout',lambda: (len(json.loads((out/'layout.picsie').read_text())['guides'])==2))
    click('save-comp');portal(out/'layout.comp');manifest=json.loads((out/'layout.comp/manifest.json').read_text());check('Compositor package retains editable text and guides',lambda: (len(manifest['guides'])==2 and sum(l.get('text') is not None for l in manifest['layers'])==2))
    x('windowsize',window_id,800,520);time.sleep(.4);click('fit');shot('23-minimum.png');check('minimum layout has rulers and canvas',lambda: (state()['controls']['canvas'][2]>200 and state()['controls']['canvas'][3]>300));prefs=json.loads((out/'config/picsie/ui.json').read_text());check('panel width and view settings persist',lambda: (prefs['layers_width']==252 and prefs['view_options']['rulers']))
    key('ctrl+w');wait(lambda s:not window_exists(window_id),'close layout window')
    window_number=previous_window;focus(re.escape(previous_name)+'.*Picsie')

try:
    server = subprocess.Popen(['Xvfb', args.display, '-screen', '0', '1280x900x24', '-nolisten', 'tcp'], env=env,
                              stdout=open(out/'xvfb.log', 'w'), stderr=subprocess.STDOUT, start_new_session=True)
    processes.append(server)
    time.sleep(.5)
    app = subprocess.Popen(['dbus-run-session', '--', str(Path(args.binary).resolve())], env=env,
                           stdout=open(out/'app.log', 'w'), stderr=subprocess.STDOUT, start_new_session=True)
    processes.append(app)
    wait(lambda s: s.get('state') is not None, 'startup')
    focus('Color studies.*Picsie')
    if args.polish_only:
        layers_masks_colors(2)
        key('ctrl+q');time.sleep(.3)
        (out/'report.json').write_text(json.dumps({'checks':checks,'count':len(checks),'backend':'Linux X11 software Vulkan'},indent=2)+'\n')
        print(f'All {len(checks)} polish checks passed. Screenshots: {out}',flush=True)
        raise SystemExit(0)
    check('native window matches installed Picsie desktop identity', lambda: (window_id in x('search', '--onlyvisible', '--class', '^picsie$').splitlines()))
    check('Compositor chrome, 971×746 canvas and ten grouped tool buttons', lambda: (state()['controls']['canvas'] == [56.,84.,971.,746.] and len([k for k in state()['controls'] if k.startswith('tool-')]) == 10))
    edge=bounds('layers-resize',scroll=False);drag(edge[0],300,edge[0]-150,300)
    check('Layers panel clamps at 352 pixels',lambda: (state()['controls']['canvas'][2]==871))
    edge=bounds('layers-resize',scroll=False);drag(edge[0],300,edge[0]+200,300)
    check('Layers panel clamps at 202 pixels',lambda: (state()['controls']['canvas'][2]==1021))
    edge=bounds('layers-resize',scroll=False);drag(edge[0],300,edge[0]-50,300)
    check('Layers panel returns to 252 pixels',lambda: (state()['controls']['canvas'][2]==971))
    x('windowsize',window_id,800,520);time.sleep(.4)
    check('800×520 minimum keeps layer footer visible',lambda: (state()['controls']['canvas']==[56.,84.,491.,406.] and state()['controls']['delete'][1]+state()['controls']['delete'][3]<=490))
    click('tool-hand');check('last grouped tool remains accessible in a short window',lambda: (state()['controls']['tool-hand'][1]+state()['controls']['tool-hand'][3]<=490))
    x('windowsize',window_id,1280,860);time.sleep(.4);click('tool-move');click('fit')
    shot('01-layout.png')
    click('new')
    field('new-name', 'GPUI parity', False)
    field('new-width', '400', False)
    field('new-height', '300', False)
    click('modal-apply')
    window_number = 2
    wait(lambda s: s.get('state') is not None, 'new window')
    focus('GPUI parity.*Picsie')
    # Pixel fixtures use an even canvas width so integer document coordinates hit integer X11 pixels.
    x('windowsize',window_id,1281,860);time.sleep(.3)
    check('new document opens a separate window', lambda: (state()['state']['document']['width'] == 400 and state()['state']['document']['height'] == 300 and not layers()))
    click('tool-rectangle')
    doc_drag(350,335,620,500)
    wait(lambda s: len(s['state']['document']['layers']) == 1)
    check('rectangle gesture creates layer', lambda: (selected()['content']['kind'] == 'shape'))
    rectangle = selected()['id']
    count = state()['state']['history']['undoCount']
    click('tool-move')
    old_x = selected()['x']
    doc_drag(450,410,480,425)
    check('move gesture is one undo', lambda: (selected()['x'] > old_x and state()['state']['history']['undoCount'] == count+1))
    key('ctrl+z'); check('undo restores transform', lambda: (abs(selected()['x']-old_x)<.01))
    key('ctrl+shift+z'); check('redo reapplies transform', lambda: (selected()['x']>old_x))
    original = selected().copy()
    ox,oy=canvas_point(332+original['x'],315+original['y'])
    ow,oh=original['width']*original['scaleX'],original['height']*original['scaleY']
    drag(ox+ow,oy+oh,ox+ow+55,oy+oh+12,'Shift_L')
    check('Shift resize preserves proportions', lambda: (abs((selected()['width']*selected()['scaleX'])/(selected()['height']*selected()['scaleY'])-ow/oh)<.001))
    key('ctrl+z')
    click('duplicate'); check('duplicate selects new layer', lambda: (len(layers()) == 2 and selected()['id'] != rectangle))
    click('layer-'+rectangle, 'Control_L')
    check('control toggle layer selection', lambda: (len(state()['state']['selection']['ids']) == 2))
    click('group'); check('group selected layers', lambda: (len(layers()) == 3 and selected()['content']['kind'] == 'group'))
    group = selected()['id']
    click('add-paint'); paint = selected()['id']
    click('visibility-'+paint);check('layer visibility toggle', lambda: (not selected()['visible']));click('visibility-'+paint)
    click('layer-'+rectangle,'Shift_L');check('Shift range selection',lambda: (len(state()['state']['selection']['ids'])>1));click('layer-'+paint)
    source=bounds('layer-'+paint);target=bounds('layer-'+group)
    drag(source[0]+160,source[1]+20,target[0]+160,target[1]+20)
    check('drag layer into folder',lambda: (selected()['parentId']==group))
    click('out-of-folder');check('move layer out of folder',lambda: (selected()['parentId'] is None))
    rows=len(state()['state']['layerRows']);click('collapse-'+group);check('folder collapse',lambda: (len(state()['state']['layerRows'])<rows));click('collapse-'+group)
    click('tool-brush'); field('brush-size','25'); field('brush-opacity','70'); field('hardness','45'); field('smoothing','25')
    check('brush controls', lambda: (all(abs(state()['state'][k]-v)<.001 for k,v in [('brushSize',25),('brushOpacity',.7),('brushHardness',.45),('brushSmoothing',25)])))
    key('4');key('5');check('two opacity digits set exact brush percentage',lambda: (abs(state()['state']['brushOpacity']-.45)<.001));key('shift+bracketright');check('Shift bracket steps hardness to next quarter',lambda: (abs(state()['state']['brushHardness']-.5)<.001));field('brush-opacity','70');field('hardness','45');
    count=state()['state']['history']['undoCount']; doc_drag(360,340,620,540)
    check('brush stroke is one undo', lambda: (state()['state']['history']['undoCount']==count+1))
    count=state()['state']['history']['undoCount'];doc_drag(450,410,1100,540)
    check('pointer release beyond canvas commits once',lambda: (state()['state']['history']['undoCount']==count+1));key('ctrl+z')
    click('tool-eraser'); doc_drag(490,400,510,445)
    click('mask-add'); check('mask added and targeted', lambda: (selected()['mask'] is not None and state()['state']['paintTarget']=='mask'))
    click('tool-brush'); doc_drag(390,330,560,470)
    click('mask-reveal'); check('mask reveal control', lambda: (state()['state']['maskMode']=='reveal'))
    click('mask-link'); check('mask unlink control', lambda: (selected()['mask']['linked'] is False))
    x('mousemove',*map(round,canvas_point(490,400)));time.sleep(.1);check('brush outline metadata matches displayed diameter',lambda: (abs(state()['state']['cursorMap']['brush_diameter']-state()['state']['brushSize']*state()['state']['viewport']['zoom'])<.01));check('mask row contains grayscale thumbnail resource',lambda: (state()['controls']['mask-row-'+paint][2]==30));shot('02-mask-inspector.png')
    click('mask-enabled');check('mask disable',lambda: (not selected()['mask']['enabled']));click('mask-enabled')
    click('paint-content'); check('return to layer content', lambda: (state()['state']['paintTarget']=='content'))
    choose('mask-source',0);check('live clipping source picker',lambda: (selected()['maskSourceId'] is not None));click('clipping')
    click('tool-marquee'); doc_drag(350,335,600,500)
    check('marquee creates selection', lambda: (state()['state']['hasPixelSelection']))
    field('selection-amount','9',False); before=state()['state']['pixelSelectionBounds']; click('pixels-expand')
    check('selection expansion uses current form value', lambda: (state()['state']['pixelSelectionBounds'] != before))
    field('feather','4',False); click('pixels-feather'); click('pixels-fill')
    shot('03-selection.png'); click('pixels-deselect')
    check('deselect pixels', lambda: (not state()['state']['hasPixelSelection']))
    click('tool-lasso');canvas_mouse(380,360,'mousedown',1)
    for px,py in [(570,360),(570,470),(380,470),(380,360)]: canvas_mouse(px,py);time.sleep(.04)
    x('mouseup',1);time.sleep(.2);check('lasso polygon selection',lambda: (state()['state']['hasPixelSelection']));key('ctrl+d')
    click('tool-crop');check('Crop default frame is visible without a pending edit',lambda: (state()['state']['cursorMap']['crop'] is not None and not state()['state']['cursorMap']['crop_active']));choose('crop-ratio',2);check('Crop ratio starts an editable frame',lambda: (state()['state']['cursorMap']['crop_active']));shot('24-crop-controls.png');click('crop-cancel');check('Crop Cancel restores the default nonediting frame',lambda: (not state()['state']['cursorMap']['crop_active']));choose('crop-ratio',1);choose('crop-ratio',2);click('crop-apply');time.sleep(.3)
    check('crop ratio and apply',lambda: (state()['state']['document']['width']==state()['state']['document']['height']));key('ctrl+z');click('tool-move')
    click('foreground-picker'); field('color-hex','12abef',False); click('modal-cancel')
    check('color cancel preserves foreground', lambda: (state()['state']['color'].lower()=='#000000'))
    click('foreground-picker'); field('color-hex','12abef',False); shot('04-color-picker.png'); click('modal-apply')
    wait(lambda s:s['state']['color'].lower()=='#12abef')
    check('color picker commits hex', lambda: (True))
    click('canvas-size'); click('canvas-locked'); field('canvas-width','480',False)
    shot('05-canvas-size.png'); click('modal-apply'); wait(lambda s:s['modal'] is None)
    check('canvas resize keeps original aspect ratio', lambda: (state()['state']['document']['width']==480 and state()['state']['document']['height']==360))
    key('ctrl+z'); check('canvas resize undo', lambda: (state()['state']['document']['width']==400))
    click('canvas-size');choose('canvas-unit',1);click('canvas-relative');click('canvas-locked');field('canvas-width','50',False);click('anchor-0');choose('canvas-fill',5);field('canvas-custom','#123456',False);click('modal-apply');wait(lambda s:s['modal'] is None and not s['busy'])
    check('relative percent canvas resize with custom fill',lambda: (state()['state']['document']['width']==600 and state()['state']['document']['height']==450));key('ctrl+z')
    click('tool-text'); canvas_mouse(400,350,'click',1);time.sleep(.25)
    wait(lambda s:any(l['id'] in s['state']['selection']['ids'] and l['content']['kind']=='text' for l in s['state']['document']['layers']), 'text creation')
    check('text tool creates text layer', lambda: (selected()['content']['kind']=='text'))
    click('text-editor');key('ctrl+a');x('type','--clearmodifiers','Native text');key('shift+Return');x('type','--clearmodifiers','parity');key('Return')
    check('Shift Enter adds newline and Enter commits text',lambda: (selected()['content']['text']=='Native text\nparity'))
    click('text-editor');key('ctrl+a');x('type','--clearmodifiers','Native text parity');time.sleep(.2)
    click('field-font-size'); key('ctrl+a');x('type','32');key('Return')
    wait(lambda s:any(l['id'] in s['state']['selection']['ids'] and l['content'].get('fontSize')==32 and l['content'].get('text')=='Native text parity' for l in s['state']['document']['layers']),'text and font field worker completion')
    check('text editor and font size preserve each other', lambda: (selected()['content']['text']=='Native text parity' and selected()['content']['fontSize']==32))
    choose('font',2);check('font selector',lambda: (selected()['content']['fontFamily']=='monospace'));choose('font',0)
    choose('blend',2);check('blend selector',lambda: (selected()['blend']=='multiply'));choose('blend',0)
    field('x','48');field('rotation','12');click('flip-x')
    check('transform fields and flip', lambda: (selected()['x']==48 and selected()['rotation']==12 and selected()['flipX']));click('transform-apply')
    count=state()['state']['history']['undoCount'];left,top,width,height=bounds('slider-opacity');drag(left+width*.95,top+height/2,left+width*.4,top+height/2)
    check('opacity slider transaction', lambda: (selected()['opacity']<.6 and state()['state']['history']['undoCount']==count+1))
    field('brightness','125');field('saturation','85');field('blur','2.5')
    check('typed adjustment values',lambda: (selected()['brightness']==1.25 and selected()['saturation']==.85 and selected()['blur']==2.5))
    field('blur','0');click('lock');locked_x=selected()['x'];key('Right');check('locked layer rejects nudge',lambda: (selected()['x']==locked_x));click('lock')
    click('text-editor');key('ctrl+a');key('ctrl+c');key('End');key('Return')
    shot('06-text-inspector.png')
    feature_workflows()
    placement_text_workflows()
    # Native file dialogs and real disk writes.
    project=out/'parity.picsie'
    click('save',scroll=False);portal(project)
    check('native save writes project and clears dirty',lambda: (project.exists() and not state()['state']['history']['dirty']))
    click('text-editor');key('ctrl+shift+s');portal(out/'parity-as.picsie')
    check('Save As shortcut works with text focused',lambda: ((out/'parity-as.picsie').exists()))
    click('tool-move');click('export-png',scroll=False);portal(out/'parity.png')
    check('PNG export', lambda: ((out/'parity.png').read_bytes()[:8]==b'\x89PNG\r\n\x1a\n'))
    key('ctrl+alt+shift+s');portal(out/'parity.jpg')
    check('JPEG export shortcut', lambda: ((out/'parity.jpg').read_bytes()[:2]==b'\xff\xd8'))
    click('save-comp',scroll=False);portal(out/'parity.comp')
    check('Compositor package save', lambda: ((out/'parity.comp').is_dir() and 'rasterized' in state()['notice']))
    click('import',scroll=False);portal(out/'parity.png',save=False)
    check('native image import',lambda: (selected()['content']['kind']=='image'))
    click('tool-move');key('ctrl+w');wait(lambda s:s['modal']=='close');shot('07-unsaved-close.png');click('modal-cancel')
    check('cancel close retains dirty document',lambda: (state()['modal'] is None and state()['state']['history']['dirty']))
    x('windowsize',window_id,960,640);time.sleep(.4)
    check('minimum window canvas layout',lambda: (state()['controls']['canvas']==[56.,84.,651.,526.]))
    shot('08-minimum-size.png')
    x('windowsize',window_id,1280,860);time.sleep(.3)
    key('ctrl+w');wait(lambda s:s['modal']=='close');click('modal-apply');time.sleep(.6)
    end=time.monotonic()+10
    while window_exists(window_id) and time.monotonic()<end:time.sleep(.05)
    check('save on close writes and closes window',lambda: (app.poll() is None and not window_exists(window_id)))
    window_number=1;focus('Color studies.*Picsie');click('open')
    # The open dialog uses the original window; switch trace only after successful loading.
    portal(project,save=False)
    window_number=5;wait(lambda s:s.get('state') is not None,'reopen window');focus('GPUI parity.*Picsie')
    check('new windows load saved view and panel preferences',lambda: (state()['state']['viewOptions']['rulers'] and abs(state()['controls']['layers-resize'][0]-1027)<.1))
    check('saved project reopens with text and layer structure',lambda: (len(layers())==5 and any(l['content'].get('text')=='Native text parity' for l in layers())))
    shot('09-reopened.png')
    layers_masks_colors(6)
    # The .comp directory picker is verified separately with mouse navigation;
    # GTK's location-entry keyboard behavior is unreliable without a window manager.
    click('add-paint');key('ctrl+q');wait(lambda s:s['modal']=='close');click('modal-cancel')
    check('cancel quit keeps application running',lambda: (app.poll() is None and state()['modal'] is None))
    key('ctrl+q');wait(lambda s:s['modal']=='close');click('modal-discard')
    end=time.monotonic()+10
    while app.poll() is None and time.monotonic()<end:time.sleep(.05)
    check('discard and quit closes remaining clean windows',lambda: (app.poll()==0))
    (out/'report.json').write_text(json.dumps({'checks':checks,'count':len(checks),'backend':'Linux X11 software Vulkan' if args.software else 'Linux X11','binary':args.binary},indent=2)+'\n')
    print(f'All {len(checks)} checks passed. Screenshots: {out}',flush=True)
except BaseException as error:
    if not isinstance(error,SystemExit) or error.code != 0:
        try: shot('failure.png')
        except (OSError, subprocess.SubprocessError): pass
    raise
finally:
    if state():
        (out/'final-state.json').write_text(json.dumps(state(),indent=2)+'\n')
    for process in reversed(processes):
        try:os.killpg(process.pid,signal.SIGTERM)
        except ProcessLookupError:pass
    for process in processes:
        try:process.wait(timeout=5)
        except subprocess.TimeoutExpired:process.kill()
