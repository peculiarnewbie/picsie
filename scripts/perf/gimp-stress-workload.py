"""Local public-API stress controls; run by GIMP's Python interpreter.

Complex fixtures have affine transforms, masks and adjustments baked into
raster assets. Live clipping dependencies are omitted from the ORA control.
Command timing includes libgimp IPC. Thumbnail availability is separate and
is not Picsie's viewport preview, a full-output read, or screen latency.
"""
import json
import os
from pathlib import Path
import time
from gi.repository import Gimp, Gegl, Gio

fixtures = Path(os.environ['PICSIE_STRESS_FIXTURES'])
output = Path(os.environ['PICSIE_STRESS_OUTPUT'])
warmups = int(os.environ.get('PICSIE_STRESS_WARMUPS', '2'))
samples = int(os.environ.get('PICSIE_STRESS_SAMPLES', '4'))
mode = os.environ['PICSIE_STRESS_MODE']
wanted = os.environ.get('PICSIE_STRESS_FILTER', '')
output.mkdir(parents=True, exist_ok=True)
specs = json.loads((fixtures/'fixtures.json').read_text())
tasks = []
if mode == 'stacks':
    for spec in specs:
        for case in ['reorder-adjacent', 'reorder-long', 'reorder-multi']:
            tasks.append((spec['key'], case))
        if spec['complex']:
            tasks.append((spec['key'], 'folder-move'))
else:
    for key in ['resize-single', 'resize-mask', 'resize-up',
                '1200-100-complex-scattered', '3600-100-complex-scattered']:
        for case in ['image-half-nearest', 'image-half-linear', 'image-half-high',
                     'image-quarter-high', 'image-odd-high', 'image-width-high',
                     'dpi-only', 'canvas-shrink', 'canvas-expand-transparent',
                     'canvas-expand-color', 'layer-scale', 'layer-scale-nonuniform']:
            tasks.append((key, case))
    tasks.append(('resize-up', 'image-up2-high'))
    for case in ['floating-scale', 'floating-feather-scale', 'distort-convex']:
        tasks.append(('resize-single', case))
    tasks.append(('resize-near-limit', 'image-half-high'))
    tasks.append(('resize-text', 'text-reflow'))

def leaves(image):
    def walk(layers):
        for layer in layers:
            if layer.is_group():
                yield from walk(layer.get_children())
            else:
                yield layer
    return list(walk(image.get_layers()))

results = []
previous_key=None
original=None
for key, case in tasks:
    if os.environ.get('PICSIE_STRESS_BOUNDED')=='1' and key.startswith(('3600-','6000-')) and 'complex' in key and int(key.split('-')[1])>=300 and case in ['reorder-long','reorder-multi','folder-move']:
        continue
    if wanted and wanted not in key+'/'+case:
        continue
    cases=os.environ.get('PICSIE_STRESS_CASES','')
    if cases and case not in cases.split(','):
        continue
    if key!=previous_key:
        if original is not None:original.delete()
        previous_key=key
        suffix=os.environ.get('PICSIE_STRESS_GIMP_FORMAT','ora')
        path = fixtures/(key+('.'+suffix if (fixtures/(key+'.ora')).exists() else '.png'))
        if suffix=='xcf' and path.suffix=='.xcf':
            assert (fixtures/(key+'.xcf.json')).exists(),'Native fixture lacks validation evidence'
        if key=='resize-mask':
            path=fixtures/'resize-single.png'
        if key=='resize-text':
            original=Gimp.Image.new(1200,800,Gimp.ImageBaseType.RGB)
            assert Gimp.fonts_refresh()
            fonts=Gimp.fonts_get_list('DejaVu Sans')
            names=[f.get_name() for f in fonts]
            print('Text control font candidates: '+repr(names),flush=True)
            font=next((f for f in fonts if f.get_name() in ['DejaVu Sans','DejaVu Sans Book','DejaVu Sans Regular']),None)
            assert font is not None,names
            layer=Gimp.TextLayer.new(original,
                'Paragraph',
                font,18.,Gimp.Unit.pixel())
            assert original.insert_layer(layer,None,0)
            assert layer.resize(800.,600.)
            assert layer.set_text('Performance audit paragraph with long wrapping lines and punctuation. '*100)
            assert layer.set_offsets(100,80)
            assert layer.set_color(Gegl.Color.new('#e53935'))
        else:
            original = Gimp.file_load(Gimp.RunMode.NONINTERACTIVE, Gio.File.new_for_path(str(path)))
        assert original is not None, path
        if key=='resize-mask':
            layer=original.get_layers()[0]
            coverage=layer.create_mask(Gimp.AddMaskType.WHITE)
            assert layer.add_mask(coverage)
            w,h=original.get_width(),original.get_height()
            pixels=bytes(255 if (y//12+x//12)%2==0 else 130 for y in range(h) for x in range(w))
            coverage.get_buffer().set(Gegl.Rectangle.new(0,0,w,h),'Y u8',pixels)
            coverage.update(0,0,w,h)
    rows = []
    for iteration in range(warmups+samples):
        image = original.duplicate()
        assert image.undo_is_enabled()
        layers = leaves(image)
        layer = layers[0]
        w, h = image.get_width(), image.get_height()
        count = len(layers)
        parent = layer.get_parent()
        position = image.get_item_position(layer)
        Gimp.context_set_interpolation(Gimp.InterpolationType.NONE if case.endswith('nearest')
            else Gimp.InterpolationType.LINEAR if case.endswith('linear')
            else Gimp.InterpolationType.CUBIC)
        Gimp.context_set_transform_resize(Gimp.TransformResize.ADJUST)
        if case.startswith('floating'):
            assert image.select_rectangle(Gimp.ChannelOps.REPLACE, 300, 300, 1000, 800)
            if 'feather' in case:
                assert Gimp.Selection.feather(image, 35.)
        if case=='dpi-only':
            initial_resolution=image.get_resolution()
            assert image.set_resolution(72.,72.)
            assert image.get_resolution()==(True,72.,72.)
            before_dpi_thumbnail=image.get_thumbnail_data(936,734)
            before_dpi_pixels=before_dpi_thumbnail[0].get_data()
        started = time.perf_counter()
        if case.startswith('reorder'):
            selected = layers[:5] if case=='reorder-multi' else [layer]
            assert image.undo_group_start()
            if case=='reorder-adjacent':
                assert image.reorder_item(layer, parent, position+1)
            else:
                for chosen in selected:
                    assert image.reorder_item(chosen, None, len(image.get_layers())-1)
            assert image.undo_group_end()
        elif case=='folder-move':
            groups=[l for l in image.get_layers() if l.is_group()]
            assert groups
            assert image.reorder_item(layer, groups[-1], 0)
        elif case.startswith('image-'):
            if 'quarter' in case: nw, nh = w//4, h//4
            elif 'up2' in case: nw, nh = w*2, h*2
            elif 'width' in case: nw, nh = w//2, h
            elif 'odd' in case: nw, nh = w*2//3+1, h*3//5+1
            else: nw, nh = w//2, h//2
            assert image.scale(nw, nh)
        elif case=='dpi-only':
            assert image.set_resolution(300., 300.)
        elif case.startswith('canvas-'):
            nw,nh=(w*3//4,h*3//4) if case=='canvas-shrink' else (w+w//4,h+h//4)
            dx,dy=(nw-w)//2,(nh-h)//2
            assert image.undo_group_start()
            assert image.resize(nw,nh,dx,dy)
            if case=='canvas-expand-color':
                extension=Gimp.Layer.new(image,'Canvas Extension',nw,nh,
                    Gimp.ImageType.RGBA_IMAGE,100.,Gimp.LayerMode.NORMAL)
                assert image.insert_layer(extension,None,len(image.get_layers()))
                assert Gimp.context_set_foreground(Gegl.Color.new('#e53935'))
                assert extension.edit_fill(Gimp.FillType.FOREGROUND)
                assert image.select_rectangle(Gimp.ChannelOps.REPLACE,dx,dy,w,h)
                assert extension.edit_clear()
                assert Gimp.Selection.none(image)
            assert image.undo_group_end()
        elif case in ['layer-scale','layer-scale-nonuniform'] or case.startswith('floating'):
            ok,x,y=layer.get_offsets(); assert ok
            lw,lh=layer.get_width(),layer.get_height()
            if case.startswith('floating'): x,y,lw,lh=300,300,1000,800
            nw,nh=(w/2,lh) if case=='layer-scale-nonuniform' else (lw*1.5,lh*1.5)
            transformed=layer.transform_scale(x-(nw-lw)/2,y-(nh-lh)/2,x+(nw+lw)/2,y+(nh+lh)/2)
            assert transformed is not None
            if case.startswith('floating'):
                assert Gimp.floating_sel_anchor(transformed)
        elif case=='distort-convex':
            ok,x,y=layer.get_offsets(); assert ok
            lw,lh=layer.get_width(),layer.get_height()
            assert layer.transform_perspective(x+200,y+120,x+lw,y,x,y+lh,x+lw,y+lh) is not None
        elif case=='text-reflow':
            assert layer.resize(620.,650.)
        else:
            raise AssertionError(case)
        command_ms=(time.perf_counter()-started)*1000
        if case=='reorder-adjacent':
            assert image.get_item_position(layer)==position+1
        elif case=='folder-move':
            assert layer.get_parent().get_id()==groups[-1].get_id()
        elif case.startswith(('image-','canvas-')):
            assert (image.get_width(),image.get_height())==(nw,nh)
        elif case=='dpi-only':
            assert (image.get_width(),image.get_height())==(w,h)
            assert image.get_resolution()==(True,300.,300.)
        # GIMP exposes the merged thumbnail publicly, not arbitrary zoom/pan
        # of a display. Do not treat this as a matched viewport render.
        thumbnail_started=time.perf_counter()
        thumb=image.get_thumbnail_data(936,734)
        thumbnail_ms=(time.perf_counter()-thumbnail_started)*1000
        assert thumb is not None and thumb[0] is not None
        data=thumb[0].get_data();tw,th,bpp=thumb[1:]
        assert tw>0 and th>0 and bpp in [3,4] and len(data)==tw*th*bpp
        if case=='text-reflow':
            assert (layer.get_width(),layer.get_height())==(620,650)
            assert bpp==4 and any(data[3::4]),'Text output is blank'
        if case=='dpi-only':
            assert data==before_dpi_pixels and thumb[1:]==before_dpi_thumbnail[1:],'DPI-only changed pixels'
        assert len(leaves(image))==count+(case=='canvas-expand-color')
        if iteration>=warmups:
            rows.append(dict(command_ms=command_ms,thumbnail_ms=thumbnail_ms))
        image.delete()
    item=dict(key=key,case=case,samples=rows,validated=True)
    if case=='dpi-only':item.update(initial_resolution=initial_resolution,normalized_initial_resolution=[72.,72.],target_resolution=[300.,300.],pixels_unchanged=True)
    (output/f'{key}-{case}.json').write_text(json.dumps(item,indent=2)+'\n')
    results.append(item)
    print('Completed '+key+'/'+case,flush=True)
if original is not None:original.delete()
(output/'results.json').write_text(json.dumps(dict(app='gimp',mode=mode,results=results,
    scope=__doc__,warmups=warmups,samples=samples),indent=2)+'\n')
