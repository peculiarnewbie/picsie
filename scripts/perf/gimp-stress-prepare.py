"""Build native GIMP fixtures from the existing baked PNG assets, outside timing.

This is a local fixture adapter, not a production implementation or an upstream
algorithm port. Layer order, modes and group defaults follow the pinned ORA
importer. A small ORA control must match pixels and hierarchy before using it.
"""
import hashlib,json,os,struct,time
from pathlib import Path
from gi.repository import Gimp,Gegl,Gio,GdkPixbuf
fixtures=Path(os.environ['PICSIE_STRESS_FIXTURES'])
output=Path(os.environ['PICSIE_STRESS_OUTPUT'])
wanted=os.environ.get('PICSIE_STRESS_FILTER','')
output.mkdir(parents=True,exist_ok=True)
reports=[]
source_hash=hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
def signature(image):
    def opacity(layer):
        # XCF stores normalized opacity as float32. Compare the hierarchy at
        # that storage precision; pixel equivalence is checked independently.
        return struct.unpack('f',struct.pack('f',layer.get_opacity()/100.))[0]
    def walk(items):
        return [(l.get_name(),opacity(l),int(l.get_mode()),l.get_offsets(),l.get_width(),l.get_height(),l.has_alpha(),l.get_visible(),l.get_lock_content(),walk(l.get_children()) if l.is_group() else None) for l in items]
    return (image.get_width(),image.get_height(),int(image.get_precision()),walk(image.get_layers()),[l.get_name() for l in image.get_selected_layers()])
def thumbnail(image):
    data,w,h,bpp=image.get_thumbnail_data(936,734)
    return (w,h,bpp,data.get_data())
def full_pixels(image):
    duplicate=image.duplicate()
    try:
        merged=duplicate.merge_visible_layers(Gimp.MergeType.CLIP_TO_IMAGE)
        valid,x,y=merged.get_offsets();assert valid
        w,h=duplicate.get_width(),duplicate.get_height()
        data=merged.get_buffer().get(Gegl.Rectangle.new(-x,-y,w,h),1.,"R'G'B'A u8",Gegl.AbyssPolicy.NONE)
        assert len(data)==w*h*4
        return data
    finally:duplicate.delete()
for spec in json.loads((fixtures/'fixtures.json').read_text()):
    key=spec['key']
    if wanted and wanted not in key:continue
    dest=fixtures/(key+'.xcf')
    stamp=fixtures/(key+'.xcf.json')
    records=json.loads((fixtures/key/'layers.json').read_text())
    asset_hash=hashlib.sha256((fixtures/key/'layers.json').read_bytes()+b''.join(
        hashlib.sha256((fixtures/key/r['file']).read_bytes()).digest() for r in records if not r['group'])).hexdigest()
    compared_ora=os.environ.get('PICSIE_STRESS_COMPARE_ORA')=='1'
    if dest.exists() and stamp.exists():
        previous=json.loads(stamp.read_text())
        if previous['source_sha256']==source_hash and previous['assets_sha256']==asset_hash and previous['xcf_sha256']==hashlib.sha256(dest.read_bytes()).hexdigest() and (not compared_ora or previous['compared_ora_full_pixels']):
            reports.append(previous);continue
    started=time.perf_counter();image=Gimp.Image.new(spec['width'],spec['height'],Gimp.ImageBaseType.RGB)
    assert image.undo_disable()
    groups={};selected=None
    for r in records:
        parent=groups.get(r['parent'])
        if r['group']:
            layer=Gimp.GroupLayer.new(image,r['name']);groups[r['id']]=layer
        else:
            pixels=GdkPixbuf.Pixbuf.new_from_file(str(fixtures/key/r['file']))
            w,h=pixels.get_width(),pixels.get_height();channels=pixels.get_n_channels()
            assert channels in [3,4]
            data=pixels.get_pixels();stride=pixels.get_rowstride();size=w*channels
            if stride!=size:data=b''.join(data[y*stride:y*stride+size] for y in range(h))
            assert len(data)==w*h*channels
            layer=Gimp.Layer.new(image,r['name'],w,h,Gimp.ImageType.RGBA_IMAGE if channels==4 else Gimp.ImageType.RGB_IMAGE,100.,Gimp.LayerMode.NORMAL)
            assert image.insert_layer(layer,parent,0)
            layer.get_buffer().set(Gegl.Rectangle.new(0,0,w,h),"R'G'B'A u8" if channels==4 else "R'G'B' u8",data)
            layer.update(0,0,w,h)
            assert layer.set_offsets(round(r['x']),round(r['y']))
            assert layer.set_mode(Gimp.LayerMode.MULTIPLY if r['blend']=='svg:multiply' else Gimp.LayerMode.NORMAL)
            if r['id']==records[-1]['id']:selected=layer
        if r['group']:
            assert image.insert_layer(layer,parent,0)
            assert layer.set_mode(Gimp.LayerMode.NORMAL)
        assert layer.set_opacity(r['opacity']*100.)
    assert image.undo_enable()
    assert image.set_selected_layers([selected])
    if compared_ora:
        reference=Gimp.file_load(Gimp.RunMode.NONINTERACTIVE,Gio.File.new_for_path(str(fixtures/(key+'.ora'))))
        assert signature(image)==signature(reference),'Direct fixture hierarchy differs from ORA'
        assert thumbnail(image)==thumbnail(reference),'Direct fixture pixels differ from ORA'
        assert full_pixels(image)==full_pixels(reference),'Full direct fixture pixels differ from ORA'
        reference.delete()
    before=signature(image);pixels_before=thumbnail(image);full_before=full_pixels(image)
    assert Gimp.file_save(Gimp.RunMode.NONINTERACTIVE,image,Gio.File.new_for_path(str(dest)))
    reopened=Gimp.file_load(Gimp.RunMode.NONINTERACTIVE,Gio.File.new_for_path(str(dest)))
    after=signature(reopened)
    if after!=before:
        (output/(key+'-hierarchy-difference.json')).write_text(json.dumps(dict(before=before,after=after),indent=2)+'\n')
    assert after==before,'XCF hierarchy changed'
    assert thumbnail(reopened)==pixels_before,'XCF pixels changed'
    assert full_pixels(reopened)==full_before,'Full XCF pixels changed'
    reopened.delete();image.delete()
    report=dict(key=key,setup_seconds=time.perf_counter()-started,validated=True,
        compared_ora_full_pixels=compared_ora,roundtrip_full_pixels=True,opacity_precision='XCF float32 normalized opacity',xcf_sha256=hashlib.sha256(dest.read_bytes()).hexdigest(),
        source_sha256=source_hash,assets_sha256=asset_hash)
    stamp.write_text(json.dumps(report,indent=2)+'\n');reports.append(report)
    print('Prepared '+key,flush=True)
(output/'results.json').write_text(json.dumps(reports,indent=2)+'\n')
