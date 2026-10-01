"""Actual untraced X11 stress interactions; all output under ignored artifacts/.

Measurements stop at the virtual framebuffer. No physical scanout or universal
FPS claim. GIMP complex controls use baked ORA assets (see fixture manifest).
Fit margins differ by application; 100% pan uses the same canvas and trajectory.
Expected final frames are calibrated untimed. Picsie checks exact bytes; GIMP
allows one channel step across every pixel for its partial-rerender rounding.
The external compiled matcher keeps those comparisons out of Python pixel loops.
"""
import argparse
import ctypes as C
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import zlib

sys.dont_write_bytecode=True
ROOT=Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('comparison',ROOT/'scripts/compare-gimp-performance.py')
comparison=importlib.util.module_from_spec(spec);spec.loader.exec_module(comparison)

class Screen(comparison.desktop.Screen):
    def button(self,down,button=1):
        self.test.XTestFakeButtonEvent(self.display,button,bool(down),0);self.flush()
    def chord(self,name,*modifiers):
        for modifier in modifiers:self.key(modifier,True)
        self.key(name,True);self.key(name,False)
        for modifier in reversed(modifiers):self.key(modifier,False)
        self.flush()
    def fingerprint(self,rect):
        return zlib.crc32(self.pixels(rect))
    def pixels(self,rect):
        if isinstance(rect[0],(list,tuple)):
            return b''.join(self.pixels(r) for r in rect)
        x,y,w,h=rect
        image=self.x.XGetImage(self.display,self.root,x,y,w,h,C.c_ulong(-1).value,2)
        assert image
        try:
            assert image.contents.bits_per_pixel==32
            return C.string_at(image.contents.data,image.contents.bytes_per_line*h)
        finally:self.x.XDestroyImage(image)
    def matches(self,a,b):
        assert len(a)==len(b)
        if self.tolerance==0:return a==b
        return bool(self.matcher(a,b,len(a),self.tolerance))
    def resize(self,window,w,h):
        self.x.XResizeWindow.argtypes=[C.c_void_p,C.c_ulong,C.c_uint,C.c_uint]
        self.x.XResizeWindow(self.display,int(window),w,h);self.flush()
    def origin(self,window):
        self.x.XTranslateCoordinates.argtypes=[C.c_void_p,C.c_ulong,C.c_ulong,C.c_int,C.c_int,C.POINTER(C.c_int),C.POINTER(C.c_int),C.POINTER(C.c_ulong)]
        x=C.c_int();y=C.c_int();child=C.c_ulong()
        assert self.x.XTranslateCoordinates(self.display,int(window),self.root,0,0,C.byref(x),C.byref(y),C.byref(child))
        return x.value,y.value
    def wheel(self,up):
        button=4 if up else 5
        self.test.XTestFakeButtonEvent(self.display,button,True,0)
        self.test.XTestFakeButtonEvent(self.display,button,False,0);self.flush()

def wait(callback,label,timeout=180):
    start=time.perf_counter()
    while time.perf_counter()-start<timeout:
        value=callback()
        if value:return value
        time.sleep(.01)
    raise RuntimeError('Timeout: '+label)

def idle(pid,timeout=240):
    # Calibration only: do not accept an unchanged old frame while the worker
    # is still composing a large clipping stack. Not used inside timed inputs.
    start=time.perf_counter();quiet=0
    previous=comparison.desktop.resources(pid)['cpu_seconds']
    while time.perf_counter()-start<timeout:
        time.sleep(.5)
        require_alive(pid)
        current=comparison.desktop.resources(pid)['cpu_seconds']
        quiet=quiet+1 if current-previous<.15 else 0
        if quiet>=2:return
        previous=current
    raise RuntimeError('Calibration worker did not become idle')

def require_alive(pid):
    try:
        state=Path(f'/proc/{pid}/stat').read_text().rsplit(') ',1)[1].split()[0]
        if state=='Z':raise RuntimeError('Application exited')
    except FileNotFoundError:
        raise RuntimeError('Application exited') from None

def settled(screen,roi,timeout=240):
    deadline=time.perf_counter()+timeout;last=None;since=time.perf_counter()
    while time.perf_counter()<deadline:
        pixels=screen.pixels(roi);current=zlib.crc32(pixels)
        if current!=last:last=current;since=time.perf_counter()
        elif time.perf_counter()-since>.35:
            screen.references[repr(roi),current]=pixels
            return current
        time.sleep(.015)
    raise RuntimeError('Frame did not settle')

def measure(screen,action,roi,target,baseline,pid):
    assert target!=baseline,'Control must visibly change'
    target_pixels=screen.references[repr(roi),target]
    baseline_pixels=screen.pixels(roi)
    assert screen.matches(baseline_pixels,screen.references[repr(roi),baseline]),'Unexpected starting frame'
    assert not screen.matches(target_pixels,baseline_pixels),'Control must exceed observer tolerance'
    before=comparison.desktop.resources(pid)
    start=time.perf_counter();action();first=None;changes=[];polls=[];previous=baseline
    checked_alive=start
    while time.perf_counter()-start<240:
        pixels=screen.pixels(roi);value=zlib.crc32(pixels);elapsed=time.perf_counter()-start;polls.append(elapsed)
        if value!=previous:
            changes.append([elapsed,value]);previous=value
            if first is None and not screen.matches(pixels,baseline_pixels):first=elapsed
        if screen.matches(pixels,target_pixels):break
        if time.perf_counter()-checked_alive>1:
            require_alive(pid);checked_alive=time.perf_counter()
        time.sleep(.004)
    else:
        (screen.output/'failed-endpoint.json').write_text(json.dumps(dict(
            roi=roi,target=target,baseline=baseline,last=value,changes=changes,polls_s=polls),indent=2)+'\n')
        raise RuntimeError('Measured endpoint does not match untimed reference')
    require_alive(pid);after=comparison.desktop.resources(pid)
    return dict(first_visible_ms=first*1000,final_visible_ms=elapsed*1000,
        cpu_seconds=after['cpu_seconds']-before['cpu_seconds'],rss_mib=after['rss_mib'],
        changes=changes,polls_s=polls,exact_endpoint=value==target,
        endpoint_channel_tolerance=screen.tolerance,matches_reference=True)

class CheckpointedResults(list):
    def __init__(self,folder):
        super().__init__();self.folder=folder
    def append(self,row):
        super().append(row)
        # Outside input timing; preserve completed measured cases even if a
        # later interaction fails its endpoint/correctness assertion.
        (self.folder/'samples.partial.json').write_text(json.dumps(self,indent=2)+'\n')

def launch(app,key,trial,a,base_env):
    folder=a.output/f'{app}-{key}-{trial}';folder.mkdir(parents=True)
    env={**base_env,'XDG_CONFIG_HOME':str(folder/'config'),'XDG_CACHE_HOME':str(folder/'cache')}
    env.pop('PICSIE_TRACE_DIR',None)
    server=process=screen=None;results=CheckpointedResults(folder);host_before=comparison.host_resources()
    try:
        server=subprocess.Popen(['Xvfb',a.display,'-screen','0','1280x900x24','-nolisten','tcp'],env=env,
            stdout=(folder/'xvfb.log').open('w'),stderr=subprocess.STDOUT,start_new_session=True)
        time.sleep(.5);assert server.poll() is None
        if app=='gimp':
            env.update(comparison.runtime_environment(a.runtime));env['LD_LIBRARY_PATH']+=os.pathsep+base_env['LD_LIBRARY_PATH']
            env['PATH']=str(a.runtime/'usr/bin')+os.pathsep+env['PATH']
            config=folder/'gimp-config';config.mkdir()
            (config/'gimprc').write_text('(show-welcome-dialog no)\n(check-updates no)\n(config-version "3.2.6")\n(initial-zoom-to-fit yes)\n(devices-share-tool yes)\n(use-opencl no)\n')
            session=(a.runtime/'etc/gimp/3.0/sessionrc').read_text().replace('(size 800 600)','(size 1280 860)')
            (config/'sessionrc.performance').write_text(session)
            (config/'tool-options').mkdir()
            (config/'tool-options/gimp-move-tool').write_text('(tool "gimp-move-tool")\n(move-current yes)\n')
            (config/'shortcutsrc').write_text('(file-version 1)\n(action "layers-lower" "<Primary>bracketleft")\n(action "layers-raise" "<Primary>bracketright")\n')
            env['GIMP3_DIRECTORY']=str(config);env['GIMP3_CACHEDIR']=str(folder/'gimp-cache')
            if a.gimp_format=='xcf':assert (a.fixtures/(key+'.xcf.json')).exists(),'Native fixture lacks validation evidence'
            command=[str(a.runtime/'usr/bin/gimp'),'--new-instance','--console-messages','--session=performance',str(a.fixtures/(key+'.'+a.gimp_format))]
        else:command=[str(a.binary),'--open',str(a.fixtures/(key+'.picsie'))]
        process=subprocess.Popen(command,env=env,stdout=(folder/'app.log').open('w'),stderr=subprocess.STDOUT,start_new_session=True)
        focused=set()
        def window():
            assert process.poll() is None,'Application exited'
            if app=='gimp':
                early=subprocess.run(['xdotool','search','--onlyvisible','--name','GIMP'],env=env,capture_output=True,text=True).stdout.splitlines()
                for candidate in early:
                    if candidate not in focused:
                        comparison.xdo(env,'windowfocus',candidate);focused.add(candidate)
            matches=subprocess.run(['xdotool','search','--onlyvisible','--name',key+'.*GIMP' if app=='gimp' else 'Picsie'],env=env,capture_output=True,text=True).stdout.splitlines()
            return matches[-1] if matches else None
        wid=wait(window,'main window');comparison.xdo(env,'windowfocus',wid)
        time.sleep(3);screen=Screen(a.display);screen.output=folder
        matcher=C.CDLL(str(a.matcher));screen.matcher=matcher.picsie_screen_matches
        screen.matcher.argtypes=[C.c_char_p,C.c_char_p,C.c_size_t,C.c_uint];screen.matcher.restype=C.c_int
        screen.tolerance=1 if app=='gimp' else 0;screen.references={}
        if app=='gimp':
            screen.chord('Escape');screen.chord('m')
            comparison.xdo(env,'windowfocus',str(screen.root));time.sleep(.1)
            comparison.xdo(env,'windowfocus',wid);time.sleep(.2);screen.chord('Tab')
            screen.resize(wid,970,834);time.sleep(1)
            canvas=[20,46,936,734]
            def fit():screen.chord('J','Control_L','Shift_L')
            def actual():screen.chord('1')
        else:
            screen.resize(wid,1245,848);time.sleep(1);screen.chord('h')
            canvas=[56,84,936,734]
            def fit():screen.chord('0','Control_L')
            def actual():screen.chord('1','Control_L')
        comparison.xdo(env,'windowmove',wid,'0','0');time.sleep(.2)
        ox,oy=screen.origin(wid)
        canvas[0]+=ox;canvas[1]+=oy
        (folder/'window-origin.json').write_text(json.dumps(dict(x=ox,y=oy,canvas=canvas))+'\n')
        screen.move(1100,875)
        roi=[canvas[0]+10,canvas[1]+10,canvas[2]-20,canvas[3]-20]
        # The Picsie window exists before its first engine frame. Shortcuts sent
        # during that load can be ignored by the UI; establish readiness first.
        idle(process.pid);settled(screen,roi)
        if app=='picsie':screen.chord('h');time.sleep(.25)
        fit();idle(process.pid);base=settled(screen,roi)
        comparison.screenshot(wid,folder/'fit.png',env)
        fit_pixels=screen.pixels(roi)
        actual();idle(process.pid);actual_ref=settled(screen,roi)
        fit();idle(process.pid);returned_fit=settled(screen,roi)
        if returned_fit!=base:
            (folder/'fit-calibration-before.bgra').write_bytes(fit_pixels)
            (folder/'fit-calibration-after.bgra').write_bytes(screen.pixels(roi))
            (folder/'fit-calibration.json').write_text(json.dumps(dict(roi=roi,before=base,after=returned_fit))+'\n')
        assert screen.matches(screen.references[repr(roi),returned_fit],fit_pixels),'Fit calibration did not restore'
        host_before=comparison.host_resources()
        for i in range(a.samples+2):
            for case,action,target,old in [('zoom-fit-to-100',actual,actual_ref,base),('zoom-100-to-fit',fit,base,actual_ref)]:
                row=measure(screen,action,roi,target,old,process.pid)
                if i>=2:results.append(dict(case=case,**row))
                time.sleep(.15)
        reorder_samples=reorder_warmups=0
        if not a.navigation_only:
            # Same adjacent stack move in both apps; GIMP's accelerator is assigned
            # only in the isolated benchmark config. Undo must restore exact pixels.
            initial_base=base;initial_pixels=screen.pixels(roi);calibration=[]
            for _ in range(3 if app=='gimp' else 1):
                screen.chord('bracketleft','Control_L');idle(process.pid)
                lowered=settled(screen,roi)
                assert lowered!=base,'Adjacent reorder shortcut did not visibly reorder'
                screen.chord('z','Control_L');idle(process.pid)
                returned=settled(screen,roi)
                if app=='gimp':
                    # GIMP's first partial rerender can differ from its initial
                    # display by one channel step. Validate that bounded difference
                    # untimed, then use the same all-pixel bound for timed inputs.
                    delta=max(abs(a-b) for a,b in zip(initial_pixels,screen.pixels(roi)))
                    assert delta<=1,'Reorder Undo exceeded one channel step'
                else:
                    delta=0;assert returned==initial_base,'Reorder Undo failed to restore exact frame'
                calibration.append(dict(lowered=lowered,returned=returned,max_initial_channel_delta=delta))
                base=returned
            (folder/'reorder-calibration.json').write_text(json.dumps(calibration,indent=2)+'\n')
            heavy='complex' in key and int(key.split('-')[1])>=300
            reorder_samples=1 if heavy else a.samples
            reorder_warmups=0 if heavy else 2
            for i in range(reorder_samples+reorder_warmups):
                for case,action,target,old in [('layer-reorder-adjacent',lambda:screen.chord('bracketleft','Control_L'),lowered,base),('layer-reorder-undo',lambda:screen.chord('z','Control_L'),base,lowered)]:
                    row=measure(screen,action,roi,target,old,process.pid)
                    if i>=reorder_warmups:results.append(dict(case=case,**row))
                    time.sleep(.15)
            comparison.screenshot(wid,folder/'reorder-restored.png',env)
        actual();idle(process.pid)
        def pan_press():
            if app=='gimp' and a.gimp_pan=='space':screen.key('space',True);screen.flush()
            screen.button(True,2 if app=='gimp' and a.gimp_pan=='middle' else 1)
        def pan_release():
            screen.button(False,2 if app=='gimp' and a.gimp_pan=='middle' else 1)
            if app=='gimp' and a.gimp_pan=='space':screen.key('space',False);screen.flush()
        if a.pan_focus=='scene':
            fixture=next(f for f in json.loads((a.fixtures/'fixtures.json').read_text()) if f['key']==key)
            # Generated 4x overlapping sources sit near (width/3,height/3),
            # rather than document center. Recenter untimed before pan; zoom
            # measurements above retain their actual native shortcut semantics.
            dx=fixture['width']/6-216;dy=fixture['height']/6-216
            assert 0<=dx<canvas[2]-140 and 0<=dy<canvas[3]-140
            sx,sy=canvas[0]+70,canvas[1]+70
            screen.move(sx,sy)
            pan_press()
            for step in range(1,33):screen.move(sx+dx*step/32,sy+dy*step/32);time.sleep(.01)
            pan_release()
            screen.move(1100,875);idle(process.pid)
        initial=settled(screen,roi)
        comparison.screenshot(wid,folder/'pan-start.png',env)
        initial_pixels=screen.references[repr(roi),initial]
        for iteration in range(3):
            screen.move(canvas[0]+130,canvas[1]+130)
            pan_press();time.sleep(.03)
            idle(process.pid)
            ready=settled(screen,roi)
            assert screen.matches(screen.references[repr(roi),ready],initial_pixels),'Pan did not begin at restored reference'
            before=comparison.desktop.resources(process.pid);start=time.perf_counter();changes=[];raw_changes=[];polls=[];inputs=[];previous=initial;previous_pixels=initial_pixels;next_poll=0.
            for step in range(1,241):
                target_time=start+step/120
                while time.perf_counter()<target_time:
                    now=time.perf_counter()-start
                    if now>=next_poll:
                        pixels=screen.pixels(roi);value=zlib.crc32(pixels);observed=time.perf_counter()-start;polls.append(observed)
                        if value!=previous:
                            raw_changes.append([observed,value])
                            if not screen.matches(previous_pixels,pixels):changes.append([observed,value])
                            previous=value;previous_pixels=pixels
                        next_poll=now+.004
                    time.sleep(.0004)
                delta=(step if step<=120 else 240-step)*2
                screen.move(canvas[0]+130+delta,canvas[1]+130+delta/4);inputs.append(time.perf_counter()-start)
            pan_release()
            released=time.perf_counter();screen.move(1100,875)
            while True:
                endpoint_pixels=screen.pixels(roi)
                if screen.matches(endpoint_pixels,initial_pixels):break
                if time.perf_counter()-released>120:raise RuntimeError('Pan endpoint did not restore')
                time.sleep(.004)
            endpoint=(time.perf_counter()-released)*1000;after=comparison.desktop.resources(process.pid)
            intervals=[(b[0]-aa[0])*1000 for aa,b in zip(changes,changes[1:])]
            # Sparse or absent progression is a performance result, not a
            # reason to discard a slow gesture. Endpoint correctness still
            # has to pass; intervals exist only when two updates were seen.
            assert not changes or changes[0][0]>=inputs[0],'Visible pan change preceded first motion'
            results.append(dict(case='pan-100-first' if iteration==0 else 'pan-100-revisit',first_visible_ms=(changes[0][0]-inputs[0])*1000 if changes else None,
                progression_observed=bool(changes),visible_updates=len(changes),
                endpoint_ms=endpoint,updates_per_second=len(changes)/(released-start),
                intervals_ms=intervals,input_s=inputs,poll_s=polls,changes=changes,
                cpu_seconds=after['cpu_seconds']-before['cpu_seconds'],rss_mib=after['rss_mib'],exact_endpoint=zlib.crc32(endpoint_pixels)==initial,
                endpoint_channel_tolerance=screen.tolerance,matches_reference=True,raw_changes=raw_changes))
            idle(process.pid)
            ready=settled(screen,roi)
            assert screen.matches(screen.references[repr(roi),ready],initial_pixels),'Pan did not remain restored after draining inputs'
            time.sleep(.4)
        comparison.screenshot(wid,folder/'pan-returned.png',env)
        # Native window size changes exercise UI layout and preview independently.
        small=[870,774] if app=='gimp' else [1145,788];large=[970,834] if app=='gimp' else [1245,848]
        resize_roi=[[canvas[0]+80,canvas[1]+80,640,450],[ox+small[0]-20,oy+100,large[0]-small[0]+10,450]]
        original=settled(screen,resize_roi);screen.resize(wid,*small);time.sleep(.8);small_ref=settled(screen,resize_roi)
        screen.resize(wid,*large);time.sleep(.8);large_ref=settled(screen,resize_roi)
        for i in range(a.samples+2):
            for case,size,target,old in [('window-smaller',small,small_ref,large_ref),('window-larger',large,large_ref,small_ref)]:
                row=measure(screen,lambda size=size:screen.resize(wid,*size),resize_roi,target,old,process.pid)
                if i>=2:results.append(dict(case=case,**row))
                time.sleep(.15)
        comparison.screenshot(wid,folder/'window-restored.png',env)
        if app=='picsie' and not a.navigation_only:
            if a.row_drop_view=='fit':
                fit();idle(process.pid);settled(screen,roi)
            # List response is measured independently of canvas navigation.
            list_roi=[ox+1002,oy+115,225,550];screen.move(ox+1190,oy+360)
            for _ in range(4):screen.wheel(True)
            time.sleep(.5)
            for i in range(a.samples+2):
                previous=screen.fingerprint(list_roi);start=time.perf_counter();screen.wheel(i%2==1)
                wait(lambda:screen.fingerprint(list_roi)!=previous,'list scrolling',timeout=10)
                elapsed=(time.perf_counter()-start)*1000
                if i>=2:results.append(dict(case='layer-list-scroll',first_visible_ms=elapsed,rss_mib=comparison.desktop.resources(process.pid)['rss_mib']))
                time.sleep(.2)
            comparison.screenshot(wid,folder/'layer-list-scrolled.png',env)
            # Traverse to the list boundary and back, separately from one-wheel
            # response. Folder children use the application's default expansion.
            for _ in range(8):screen.wheel(True)
            idle(process.pid);top=settled(screen,list_roi);canvas_before=screen.fingerprint(roi)
            started=time.perf_counter();before=comparison.desktop.resources(process.pid)
            previous=top;events=0;unchanged=0;first=None
            while events<int(key.split('-')[1])*4+40:
                for _ in range(10):screen.wheel(False);events+=1
                time.sleep(.12);current=screen.fingerprint(list_roi)
                if current!=previous:
                    if first is None:first=(time.perf_counter()-started)*1000
                    unchanged=0
                else:unchanged+=1
                previous=current
                if unchanged>=4:
                    # A busy large-list repaint can outlast four driver polls.
                    # Verify quiet/stable rows before declaring a boundary.
                    idle(process.pid);current=settled(screen,list_roi)
                    if current==previous:break
                    previous=current;unchanged=0
            idle(process.pid);bottom=settled(screen,list_roi)
            assert bottom!=top,'List sweep did not leave top'
            assert screen.fingerprint(roi)==canvas_before,'List scrolling changed canvas'
            after=comparison.desktop.resources(process.pid)
            results.append(dict(case='layer-list-sweep-down',first_visible_ms=first,
                endpoint_ms=(time.perf_counter()-started)*1000,wheel_events=events,
                cpu_seconds=after['cpu_seconds']-before['cpu_seconds'],rss_mib=after['rss_mib'],
                boundary_stable=True,canvas_unchanged=True))
            comparison.screenshot(wid,folder/'layer-list-bottom.png',env)
            for _ in range(events):screen.wheel(True);time.sleep(.001)
            idle(process.pid)
            assert settled(screen,list_roi)==top,'List sweep did not return to original top'
            comparison.screenshot(wid,folder/'layer-list-top-restored.png',env)
            # Row dragging uses the actual edge-autoscroll loop, then commits
            # at the current drop row. Check the canvas response and exact Undo.
            # The destination depends on actual autoscroll progress, so this is
            # a within-Picsie workload rather than a matched GIMP interaction.
            screen.move(ox+1140,oy+(298 if 'complex' in key else 246));screen.button(True)
            time.sleep(.05);list_before=screen.fingerprint(list_roi)
            start=time.perf_counter();screen.move(ox+1140,oy+770)
            scroll_changes=[];previous=list_before;polls=[]
            while time.perf_counter()-start<2.:
                current=screen.fingerprint(list_roi);elapsed=time.perf_counter()-start;polls.append(elapsed)
                if current!=previous:scroll_changes.append(elapsed);previous=current
                time.sleep(.004)
            comparison.screenshot(wid,folder/'layer-row-autoscroll-draft.png',env)
            before=comparison.desktop.resources(process.pid);canvas_before=screen.fingerprint(roi)
            started=time.perf_counter();screen.button(False);screen.move(1100,875)
            wait(lambda:screen.fingerprint(roi)!=canvas_before,'row drop composite',timeout=240)
            first=(time.perf_counter()-started)*1000
            idle(process.pid);dropped=settled(screen,roi)
            assert dropped!=canvas_before,'Row drop did not change composite'
            after=comparison.desktop.resources(process.pid)
            comparison.screenshot(wid,folder/'layer-row-autoscroll-dropped.png',env)
            row=measure(screen,lambda:screen.chord('z','Control_L'),roi,canvas_before,dropped,process.pid)
            results.append(dict(case='layer-row-autoscroll-drop',first_visible_ms=first,
                cpu_seconds=after['cpu_seconds']-before['cpu_seconds'],rss_mib=after['rss_mib'],
                autoscroll_changes_s=scroll_changes,autoscroll_polls_s=polls,
                autoscroll_progression_observed=bool(scroll_changes),
                undo_final_visible_ms=row['final_visible_ms'],exact_undo_endpoint=True))
            comparison.screenshot(wid,folder/'layer-row-autoscroll-undo.png',env)
        require_alive(process.pid)
        report=dict(status='passed',app=app,key=key,trial=trial,canvas=canvas,results=results,host_before=host_before,host_after=comparison.host_resources(),scope=__doc__,reorder_samples=reorder_samples,reorder_warmups=reorder_warmups)
        (folder/'result.json').write_text(json.dumps(report,indent=2)+'\n');return report
    except Exception as error:
        report=dict(status='failed',app=app,key=key,trial=trial,error=str(error),
            application_exit_code=process.poll() if process else None,results=results,
            host_before=host_before,host_after=comparison.host_resources(),scope=__doc__)
        (folder/'failure.json').write_text(json.dumps(report,indent=2)+'\n')
        if screen and server and server.poll() is None:
            comparison.screenshot(str(screen.root),folder/'failure.png',env)
        (folder/'result.json').write_text(json.dumps(report,indent=2)+'\n');return report
    finally:
        if screen:screen.close()
        comparison.desktop.stop(process);comparison.desktop.stop(server)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output',type=Path,required=True);p.add_argument('--fixtures',type=Path,default=Path('artifacts/perf-stress-2026-10-01/fixtures'))
    p.add_argument('--binary',type=Path,default=Path('crates/picsie-desktop/target/release/picsie-desktop'))
    p.add_argument('--runtime',type=Path,default=Path('artifacts/gimp-performance/runtime'))
    p.add_argument('--display',default=':97');p.add_argument('--trials',type=int,default=3);p.add_argument('--samples',type=int,default=4)
    p.add_argument('--warmup-launches',type=int,default=0);p.add_argument('--resume',action='store_true')
    p.add_argument('--gimp-format',choices=['ora','xcf'],default='ora')
    p.add_argument('--gimp-pan',choices=['space','middle'],default='space',
                   help='Untimed pan activation; middle enters GIMP scrolling directly')
    p.add_argument('--matcher',type=Path,default=Path('artifacts/perf-stress-2026-10-01/screen-match.so'))
    p.add_argument('--apps',nargs='+',choices=['picsie','gimp'],default=['picsie','gimp'])
    p.add_argument('--pan-focus',choices=['centre','scene'],default='centre')
    p.add_argument('--row-drop-view',choices=['actual','fit'],default='actual')
    p.add_argument('--navigation-only',action='store_true')
    p.add_argument('--keys',nargs='+',default=[f'3600-{count}-{kind}-overlap' for count in [50,100,300,1000] for kind in ['simple','complex']]+[f'6000-300-{kind}-overlap' for kind in ['simple','complex']])
    a=p.parse_args()
    for key in ['output','fixtures','binary','runtime','matcher']:setattr(a,key,getattr(a,key).resolve())
    if a.output.exists() and not a.resume:p.error('Choose a fresh output directory or --resume')
    a.output.mkdir(parents=True,exist_ok=True)
    tools=ROOT/'artifacts/selection-history/tools/usr'
    env={**os.environ,'DISPLAY':a.display,'WINIT_UNIX_BACKEND':'x11','VK_DRIVER_FILES':'/usr/share/vulkan/icd.d/radeon_icd.json','MESA_VK_WSI_DEBUG':'sw','PICSIE_GPU_DIAGNOSTICS':'1','GEGL_USE_OPENCL':'no','PATH':str(tools/'bin')+os.pathsep+os.environ.get('PATH',''),'LD_LIBRARY_PATH':str(tools/'lib')}
    env.pop('WAYLAND_DISPLAY',None)
    manifest=dict(keys=a.keys,trials=a.trials,apps=a.apps,pan_focus=a.pan_focus,row_drop_view=a.row_drop_view,navigation_only=a.navigation_only,fixture_index_sha256=hashlib.sha256((a.fixtures/'fixtures.json').read_bytes()).hexdigest(),pixel_tolerance={'picsie':0,'gimp':1},matcher_source_sha256=hashlib.sha256((ROOT/'scripts/perf/screen-match.c').read_bytes()).hexdigest(),matcher_binary_sha256=hashlib.sha256(a.matcher.read_bytes()).hexdigest(),gimp_format=a.gimp_format,gimp_pan=a.gimp_pan,prepare_source_sha256=hashlib.sha256((ROOT/'scripts/perf/gimp-stress-prepare.py').read_bytes()).hexdigest(),measured_samples_per_zoom_resize_case=a.samples,warmup=f'{a.warmup_launches} whole launch(es) and two zoom/resize cycles. Two reorder warmups normally; complex 300+ layers: zero reorder warmups, one measured reorder/Undo per launch. Each launch calibrates reference frames untimed; first-visit and revisited pans separate, with quiet starting/ending frames and latency from the first posted motion',source_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),binary_sha256=hashlib.sha256(a.binary.read_bytes()).hexdigest(),gimp_binary_sha256=hashlib.sha256((a.runtime/'usr/bin/gimp').read_bytes()).hexdigest(),helper_sha256={name:hashlib.sha256((ROOT/'scripts'/name).read_bytes()).hexdigest() for name in ['compare-gimp-performance.py','compare-desktop-performance.py']},fixture_sha256={key:{suffix:hashlib.sha256((a.fixtures/(key+suffix)).read_bytes()).hexdigest() for suffix in ['.picsie','.'+a.gimp_format]+(['.xcf.json'] if a.gimp_format=='xcf' else [])} for key in a.keys},head=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),scope=__doc__)
    if a.resume:
        assert json.loads((a.output/'manifest.json').read_text())==manifest,'Resume requires identical protocol, source, binary and fixtures'
    else:(a.output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    runs=[]
    for key in a.keys:
        for trial in range(1-a.warmup_launches,a.trials+1):
            for app in (a.apps if trial%2==0 else list(reversed(a.apps))):
                folder=a.output/f'{app}-{key}-{trial}'
                if (folder/'result.json').exists():
                    result=json.loads((folder/'result.json').read_text())
                else:
                    if folder.exists():
                        suffix=1
                        while folder.with_name(folder.name+f'.failed-{suffix}').exists():suffix+=1
                        folder.rename(folder.with_name(folder.name+f'.failed-{suffix}'))
                    result=launch(app,key,trial,a,env)
                if trial>=1:runs.append(result)
                print(f'{app}/{key}/{trial}: {result["status"]} {len(result["results"])} interactions'+(' '+result['error'] if result['status']=='failed' else ''),flush=True)
    summary={}
    for app in a.apps:
        summary[app]={}
        for key in a.keys:
            rows=[row for run in runs if run['status']=='passed' and run['app']==app and run['key']==key for row in run['results']]
            summary[app][key]={}
            for case in sorted({r['case'] for r in rows}):
                selected=[r for r in rows if r['case']==case];metrics={}
                for metric in ['first_visible_ms','final_visible_ms','endpoint_ms','updates_per_second','cpu_seconds','rss_mib']:
                    values=[r[metric] for r in selected if r.get(metric) is not None]
                    if values:metrics[metric]=comparison.desktop.stats(values)
                intervals=[v for r in selected for v in r.get('intervals_ms',[])]
                if intervals:metrics['visible_interval_ms']=comparison.desktop.stats(intervals)
                summary[app][key][case]=metrics
    (a.output/'results.json').write_text(json.dumps(dict(manifest=manifest,summary=summary,runs=runs,
        failed_launches=[dict(app=r['app'],key=r['key'],trial=r['trial'],error=r['error'],application_exit_code=r['application_exit_code']) for r in runs if r['status']=='failed']),indent=2)+'\n')

if __name__=='__main__':main()
