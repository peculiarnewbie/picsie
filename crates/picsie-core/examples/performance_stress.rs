//! Local performance workloads, not translated upstream fixtures or production code.
//! Exercises existing Rust commands; all generated assets/results belong in artifacts/.
use anyhow::{Result, ensure};
use picsie_core::{
    canvas_size::CanvasSizeOptions,
    editor::{Command, Editor, PaintTarget, SelectionMode, Side, TransformField},
    files,
    geometry::{self, Viewport},
    image_size::ImageSizeOptions,
    model::*,
    pixel_selection::PixelSelection,
    render::{self, Renderer},
    text::TextLayout,
};
use serde_json::{Value, json};
use skia_safe::{PathBuilder, Rect};
use std::{fs, hint::black_box, path::Path, sync::Arc, time::Instant};

fn image(w: u32, h: u32, seed: u32) -> Result<Content> {
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let v = ((x / 13 + y / 17 + seed) % 7) as u8;
            pixels.extend([
                30 + v * 25,
                50 + ((x / 7 + seed) % 6) as u8 * 30,
                70 + ((y / 11 + seed) % 5) as u8 * 35,
                if (x + y + seed) % 19 == 0 { 120 } else { 255 },
            ]);
        }
    }
    Ok(render::native_content(render::rgba_image(w, h, &pixels)?))
}
fn mask(w: u32, h: u32, independent: bool, layer: &Layer) -> Arc<LayerMask> {
    Arc::new(LayerMask {
        enabled: true,
        base: MaskMode::Reveal,
        linked: !independent,
        placement: independent.then(|| MaskPlacement::of(layer)),
        strokes: vec![],
        raster: Some(Arc::new(MaskRaster {
            width: w,
            height: h,
            pixels: Arc::new(
                (0..w * h)
                    .map(|i| {
                        if (i / w / 12 + i % w / 12) % 2 == 0 {
                            255
                        } else {
                            130
                        }
                    })
                    .collect(),
            ),
        })),
    })
}
fn stack(w: u32, h: u32, count: usize, complex: bool, overlap: bool) -> Result<Document> {
    let mut d = Document::new("Stress audit", w, h)?;
    let base = Layer::new("Background", w, h, image(w, h, 1)?);
    d.layers.push(base);
    let mut parent = None;
    while d.layers.len() < count {
        let i = d.layers.len();
        if complex && i % 25 == 1 {
            let g = Layer::new(&format!("Folder {i:04}"), w, h, Content::Group);
            parent = Some(g.id.clone());
            d.layers.push(g);
            continue;
        }
        let mut l = Layer::new(&format!("Tile {i:04}"), 96, 96, image(96, 96, i as u32)?);
        l.x = if overlap {
            w as f64 / 3. + (i % 9) as f64 * 7.
        } else {
            ((i * 173) % w as usize) as f64 - 24.
        };
        l.y = if overlap {
            h as f64 / 3. + (i % 7) as f64 * 7.
        } else {
            ((i * 137) % h as usize) as f64 - 24.
        };
        if overlap {
            l.scale_x = 4.;
            l.scale_y = 4.;
        }
        if complex {
            l.parent_id = parent.clone();
            l.opacity = 0.6 + (i % 4) as f64 * 0.1;
            if i % 5 == 0 {
                l.rotation = 13.;
            }
            if i % 7 == 0 {
                l.blend = Blend::Multiply;
            }
            if i % 11 == 0 {
                l.brightness = 1.15;
                l.saturation = 0.8;
            }
            if i % 13 == 0 {
                l.blur = 2.;
            }
            if i % 9 == 0 {
                l.mask = Some(mask(96, 96, i % 18 == 0, &l));
            }
            if i % 10 == 4 {
                l.mask_source_id = Some(d.layers.last().unwrap().id.clone());
            }
        }
        d.layers.push(l);
    }
    d.validate()?;
    Ok(d)
}
fn single(w: u32, h: u32, masked: bool) -> Result<Document> {
    let mut d = Document::new("Resize audit", w, h)?;
    let mut l = Layer::new("Raster", w, h, image(w, h, 1)?);
    if masked {
        l.mask = Some(mask(w, h, false, &l));
    }
    d.layers.push(l);
    Ok(d)
}
fn prepare(out: &Path) -> Result<()> {
    fs::create_dir_all(out)?;
    let mut specs = vec![];
    for (w, h) in [(1200, 800), (3600, 2400), (6000, 4000)] {
        let counts: &[usize] = if w == 6000 {
            &[300]
        } else {
            &[50, 100, 300, 1000]
        };
        for &count in counts {
            for complex in [false, true] {
                for overlap in [false, true] {
                    let key = format!(
                        "{w}-{count}-{}-{}",
                        if complex { "complex" } else { "simple" },
                        if overlap { "overlap" } else { "scattered" }
                    );
                    if out.join(format!("{key}.picsie")).exists()
                        && out.join(&key).join("layers.json").exists()
                    {
                        let mut d = files::open_project(&out.join(format!("{key}.picsie")))?;
                        if d.layers[0].width != w || d.layers[0].height != h {
                            let base = &mut d.layers[0];
                            base.width = w;
                            base.height = h;
                            base.scale_x = 1.;
                            base.scale_y = 1.;
                            base.content = Arc::new(image(w, h, 1)?);
                            let background = base.clone();
                            files::save_project(&out.join(format!("{key}.picsie")), &d)?;
                            let mut part = Document::new("Background", w, h)?;
                            part.layers.push(background);
                            let asset = out.join(&key);
                            fs::write(
                                asset.join("0.png"),
                                render::encode(
                                    &Renderer::default().render(&part)?.image_snapshot(),
                                    false,
                                )?,
                            )?;
                            fs::write(
                                asset.join("merged.png"),
                                render::encode(
                                    &Renderer::default().render(&d)?.image_snapshot(),
                                    false,
                                )?,
                            )?;
                            eprintln!("Upgraded full-resolution source {key}");
                        }
                        specs.push(json!({"key":key,"width":w,"height":h,"layers":count,"complex":complex,"overlap":overlap,"source_pixels":d.layers.iter().filter(|l|!matches!(l.content.as_ref(),Content::Group)).map(|l|l.width as u64*l.height as u64).sum::<u64>(),"mask_pixels":d.layers.iter().filter_map(|l|l.mask.as_ref()?.raster.as_ref()).map(|m|m.width as u64*m.height as u64).sum::<u64>()}));
                        continue;
                    }
                    let d = stack(w, h, count, complex, overlap)?;
                    files::save_project(&out.join(format!("{key}.picsie")), &d)?;
                    // Rasterize each isolated transformed layer into tight document-space bounds
                    // for OpenRaster controls. Masks/adjustments are baked, folders retained.
                    let assets = out.join(&key);
                    fs::create_dir_all(&assets)?;
                    let mut records = vec![];
                    for (i, l) in d.layers.iter().enumerate() {
                        if matches!(l.content.as_ref(), Content::Group) {
                            records.push(json!({"id":l.id,"parent":l.parent_id,"name":l.name,"group":true,"opacity":l.opacity}));
                            continue;
                        }
                        let corners = geometry::corners(l);
                        let left = corners
                            .iter()
                            .map(|p| p.x.floor())
                            .fold(f64::INFINITY, f64::min);
                        let top = corners
                            .iter()
                            .map(|p| p.y.floor())
                            .fold(f64::INFINITY, f64::min);
                        let right = corners
                            .iter()
                            .map(|p| p.x.ceil())
                            .fold(f64::NEG_INFINITY, f64::max);
                        let bottom = corners
                            .iter()
                            .map(|p| p.y.ceil())
                            .fold(f64::NEG_INFINITY, f64::max);
                        let mut part =
                            Document::new("Layer", (right - left) as u32, (bottom - top) as u32)?;
                        let mut bare = l.clone();
                        bare.x -= left;
                        bare.y -= top;
                        bare.parent_id = None;
                        bare.mask_source_id = None;
                        bare.opacity = 1.;
                        bare.blend = Blend::SourceOver;
                        if let Some(m) = &mut bare.mask {
                            if let Some(p) = &mut Arc::make_mut(m).placement {
                                p.x -= left;
                                p.y -= top;
                            }
                        }
                        part.layers.push(bare);
                        let file = format!("{i}.png");
                        fs::write(
                            assets.join(&file),
                            render::encode(
                                &Renderer::default().render(&part)?.image_snapshot(),
                                false,
                            )?,
                        )?;
                        records.push(json!({"id":l.id,"parent":l.parent_id,"name":l.name,"group":false,"file":file,"x":left,"y":top,"opacity":l.opacity,"blend":if l.blend==Blend::Multiply {"svg:multiply"} else {"svg:src-over"},"clip":l.mask_source_id}));
                    }
                    let merged = Renderer::default().render(&d)?.image_snapshot();
                    fs::write(assets.join("merged.png"), render::encode(&merged, false)?)?;
                    fs::write(
                        assets.join("layers.json"),
                        serde_json::to_vec_pretty(&records)?,
                    )?;
                    specs.push(json!({"key":key,"width":w,"height":h,"layers":count,"complex":complex,"overlap":overlap,"source_pixels":d.layers.iter().filter(|l|!matches!(l.content.as_ref(),Content::Group)).map(|l|l.width as u64*l.height as u64).sum::<u64>(),"mask_pixels":d.layers.iter().filter_map(|l|l.mask.as_ref()?.raster.as_ref()).map(|m|m.width as u64*m.height as u64).sum::<u64>()}));
                    eprintln!("Prepared {key}");
                }
            }
        }
    }
    for (key, w, h, masked) in [
        ("resize-single", 3600, 2400, false),
        ("resize-mask", 3600, 2400, true),
        ("resize-up", 1800, 1200, false),
        ("resize-near-limit", 6000, 4000, false),
    ] {
        let d = single(w, h, masked)?;
        files::save_project(&out.join(format!("{key}.picsie")), &d)?;
        fs::write(
            out.join(format!("{key}.png")),
            render::encode(&Renderer::default().render(&d)?.image_snapshot(), false)?,
        )?;
    }
    let mut text = Document::new("Text reflow", 1200, 800)?;
    let mut l = Layer::new(
        "Paragraph",
        800,
        600,
        Content::Text {
            text: "Performance audit paragraph with long wrapping lines and punctuation. "
                .repeat(100),
            font_size: 18.,
            font_family: FontFamily::SansSerif,
            color: "#e53935".into(),
        },
    );
    l.x = 100.;
    l.y = 80.;
    l.text_layout = Some(TextLayout {
        point: false,
        ..Default::default()
    });
    text.layers.push(l);
    files::save_project(&out.join("resize-text.picsie"), &text)?;
    fs::write(
        out.join("fixtures.json"),
        serde_json::to_vec_pretty(&specs)?,
    )?;
    Ok(())
}
fn preview(e: &Editor, r: &mut Renderer) -> Result<Vec<u8>> {
    let d = e.preview_document();
    let mut s = r.preview(
        &d,
        &e.viewport,
        &[],
        false,
        None,
        None,
        e.history.pixel_selection.as_ref(),
        None,
    )?;
    render::rgba_pixels(&s.image_snapshot())
}
fn signature(d: &Document) -> Value {
    json!({"w":d.width,"h":d.height,"dpi":d.resolution,"layers":d.layers.iter().map(|l|json!([l.id,l.parent_id,l.x,l.y,l.width,l.height,l.scale_x,l.scale_y,l.rotation,l.mask_source_id,l.sampling,l.opacity,l.blend,l.visible,l.flip_x,l.flip_y,l.brightness,l.saturation,l.blur,Arc::as_ptr(&l.content) as usize,l.mask.as_ref().map(|m|Arc::as_ptr(m) as usize),l.text_layout])).collect::<Vec<_>>()})
}
fn setup(d: &Document, case: &str) -> Result<Editor> {
    setup_with_draft(d, case, true)
}
fn setup_with_draft(d: &Document, case: &str, begin: bool) -> Result<Editor> {
    let mut e = Editor::new(d.clone())?;
    e.viewport = Viewport {
        width: 936.,
        height: 734.,
        zoom: 1.,
        pan: Point::default(),
    };
    e.fit();
    let selected = d.layers.last().unwrap().id.clone();
    e.select(Some(selected), SelectionMode::Replace)?;
    if case == "reorder-multi" {
        for l in d
            .layers
            .iter()
            .rev()
            .skip(1)
            .filter(|l| !matches!(l.content.as_ref(), Content::Group))
            .take(4)
        {
            e.select(Some(l.id.clone()), SelectionMode::Toggle)?;
        }
    }
    if case.starts_with("floating") {
        let mut p = PathBuilder::new();
        p.add_rect(Rect::from_xywh(300., 300., 1000., 800.), None, None);
        e.history.pixel_selection = Some(PixelSelection::from_path(
            d.width,
            d.height,
            p.detach(),
            if case.contains("feather") { 20. } else { 0. },
        )?);
        if begin {
            e.command(Command::BeginTransform)?;
        }
    }
    if case.starts_with("mask-scale") {
        e.command(Command::SetPaintTarget {
            target: PaintTarget::Mask,
        })?;
    }
    if case.starts_with("text-reflow") {
        e.command(Command::EditText {
            id: e.selected_id().map(str::to_string),
            point: None,
        })?;
    }
    if case.starts_with("pan-") {
        e.command(Command::Zoom {
            zoom: 2.,
            point: None,
        })?;
    }
    Ok(e)
}
fn perform(e: &mut Editor, case: &str) -> Result<(bool, Value)> {
    let mut phases = serde_json::Map::new();
    let d = &e.history.document;
    let (w, h) = (d.width, d.height);
    match case {
        "reorder-adjacent" => e.command(Command::Reorder { direction: -1 })?,
        "reorder-long" | "reorder-multi" => {
            let target = d.layers[0].id.clone();
            e.command(Command::ReorderTo {
                target_id: target,
                side: Side::Below,
            })?;
        }
        "folder-move" => {
            let target = d
                .layers
                .iter()
                .find(|l| matches!(l.content.as_ref(), Content::Group))
                .unwrap()
                .id
                .clone();
            e.command(Command::MoveToGroup {
                parent_id: Some(target),
            })?;
        }
        "zoom-in" | "zoom-out" | "zoom-fractional" => e.command(Command::Zoom {
            zoom: if case == "zoom-in" {
                4.
            } else if case == "zoom-out" {
                0.25
            } else {
                1.37
            },
            point: Some(Point::new(330., 270.)),
        })?,
        "pan-new" | "pan-revisit" => {
            e.command(Command::Pan {
                delta: Point::new(280., 170.),
            })?;
        }
        "viewport-resize" => e.command(Command::ResizeViewport {
            width: 1120.,
            height: 810.,
        })?,
        "canvas-shrink" | "canvas-expand-transparent" | "canvas-expand-color" => {
            e.command(Command::ResizeCanvas {
                options: CanvasSizeOptions {
                    width: if case == "canvas-shrink" {
                        w * 3 / 4
                    } else {
                        w + w / 4
                    },
                    height: if case == "canvas-shrink" {
                        h * 3 / 4
                    } else {
                        h + h / 4
                    },
                    anchor: 4,
                    fill: (case == "canvas-expand-color").then(|| "#e53935".into()),
                },
            })?
        }
        "dpi-only" => e.command(Command::ResizeImage {
            options: ImageSizeOptions {
                width: w,
                height: h,
                resolution: 300.,
                sampling: Sampling::High,
            },
        })?,
        c if c.starts_with("image-") => {
            let sampling = if c.ends_with("nearest") {
                Sampling::Nearest
            } else if c.ends_with("linear") {
                Sampling::Smooth
            } else {
                Sampling::High
            };
            let (nw, nh) = if c.contains("quarter") {
                (w / 4, h / 4)
            } else if c.contains("up2") {
                (w * 2, h * 2)
            } else if c.contains("width") {
                (w / 2, h)
            } else if c.contains("odd") {
                (w * 2 / 3 + 1, h * 3 / 5 + 1)
            } else {
                (w / 2, h / 2)
            };
            e.command(Command::ResizeImage {
                options: ImageSizeOptions {
                    width: nw,
                    height: nh,
                    resolution: 96.,
                    sampling,
                },
            })?;
        }
        "layer-scale"
        | "layer-scale-nonuniform"
        | "mask-scale-linked"
        | "mask-scale-independent"
        | "floating-scale"
        | "floating-feather-scale"
        | "layer-scale-cancel"
        | "floating-scale-cancel" => {
            let stage = Instant::now();
            e.command(Command::SetTransformField {
                field: if case == "layer-scale-nonuniform" {
                    TransformField::Width
                } else {
                    TransformField::ScalePercent
                },
                value: if case == "layer-scale-nonuniform" {
                    w as f64 / 2.
                } else {
                    150.
                },
            })?;
            phases.insert(
                "draft_command_ms".into(),
                json!(stage.elapsed().as_secs_f64() * 1000.),
            );
            let stage = Instant::now();
            e.command(if case.ends_with("cancel") {
                Command::CancelTransform
            } else {
                Command::CommitTransform
            })?;
            phases.insert(
                if case.ends_with("cancel") {
                    "cancel_command_ms"
                } else {
                    "apply_command_ms"
                }
                .into(),
                json!(stage.elapsed().as_secs_f64() * 1000.),
            );
        }
        "distort-convex" | "distort-folded" | "mask-distort" | "distort-cancel" => {
            let stage = Instant::now();
            let mut points: [Point; 4] =
                geometry::corners(e.selected().unwrap()).try_into().unwrap();
            points[0].x += 200.;
            points[0].y += 120.;
            if case == "distort-folded" {
                points.swap(1, 2);
            }
            if case == "mask-distort" {
                e.command(Command::SetPaintTarget {
                    target: PaintTarget::Mask,
                })?;
                e.command(Command::DistortMask { corners: points })?;
            } else {
                e.command(Command::DistortLayer { corners: points })?;
            }
            phases.insert(
                "draft_command_ms".into(),
                json!(stage.elapsed().as_secs_f64() * 1000.),
            );
            let stage = Instant::now();
            e.command(if case.ends_with("cancel") {
                Command::CancelTransform
            } else {
                Command::CommitTransform
            })?;
            phases.insert(
                if case.ends_with("cancel") {
                    "cancel_command_ms"
                } else {
                    "apply_command_ms"
                }
                .into(),
                json!(stage.elapsed().as_secs_f64() * 1000.),
            );
        }
        "text-reflow" => {
            let l = e.selected().unwrap().clone();
            let p = geometry::to_world(&l, Point::new(l.width as f64, l.height as f64));
            let o = geometry::canvas_origin(&e.history.document, &e.viewport);
            let a = Point::new(o.x + p.x * e.viewport.zoom, o.y + p.y * e.viewport.zoom);
            e.command(Command::Pointer {
                samples: vec![
                    picsie_core::editor::PointerSample {
                        phase: picsie_core::editor::Phase::Down,
                        point: a,
                        modifiers: Default::default(),
                    },
                    picsie_core::editor::PointerSample {
                        phase: picsie_core::editor::Phase::Move,
                        point: Point::new(
                            a.x - 180. * e.viewport.zoom,
                            a.y + 50. * e.viewport.zoom,
                        ),
                        modifiers: Default::default(),
                    },
                    picsie_core::editor::PointerSample {
                        phase: picsie_core::editor::Phase::Up,
                        point: Point::new(
                            a.x - 180. * e.viewport.zoom,
                            a.y + 50. * e.viewport.zoom,
                        ),
                        modifiers: Default::default(),
                    },
                ],
            })?;
            e.command(Command::CommitText)?;
        }
        _ => anyhow::bail!("Unknown case {case}"),
    }
    Ok((
        !case.starts_with("zoom")
            && !case.starts_with("pan")
            && case != "viewport-resize"
            && !case.ends_with("cancel"),
        Value::Object(phases),
    ))
}
fn run(fixtures: &Path, out: &Path, warmups: usize, samples: usize, mode: &str) -> Result<()> {
    fs::create_dir_all(out)?;
    let specs: Vec<Value> = serde_json::from_slice(&fs::read(fixtures.join("fixtures.json"))?)?;
    let mut tasks: Vec<(String, String)> = vec![];
    if mode == "stacks" {
        for spec in &specs {
            for case in [
                "reorder-adjacent",
                "reorder-long",
                "reorder-multi",
                "zoom-in",
                "zoom-out",
                "zoom-fractional",
                "pan-new",
                "pan-revisit",
                "viewport-resize",
            ] {
                tasks.push((spec["key"].as_str().unwrap().into(), case.into()));
            }
            if spec["complex"] == true {
                tasks.push((spec["key"].as_str().unwrap().into(), "folder-move".into()));
            }
        }
    } else {
        for key in [
            "resize-single",
            "resize-mask",
            "resize-up",
            "1200-100-complex-scattered",
            "3600-100-complex-scattered",
        ] {
            for case in [
                "image-half-nearest",
                "image-half-linear",
                "image-half-high",
                "image-quarter-high",
                "image-odd-high",
                "image-width-high",
                "dpi-only",
                "canvas-shrink",
                "canvas-expand-transparent",
                "canvas-expand-color",
                "layer-scale",
                "layer-scale-nonuniform",
                "layer-scale-cancel",
            ] {
                tasks.push((key.into(), case.into()));
            }
        }
        tasks.push(("resize-up".into(), "image-up2-high".into()));
        for case in [
            "mask-scale-linked",
            "mask-scale-independent",
            "mask-distort",
        ] {
            tasks.push(("resize-mask".into(), case.into()));
        }
        for case in [
            "floating-scale",
            "floating-feather-scale",
            "distort-convex",
            "distort-folded",
            "floating-scale-cancel",
            "distort-cancel",
        ] {
            tasks.push(("resize-single".into(), case.into()));
        }
        tasks.push(("resize-text".into(), "text-reflow".into()));
        tasks.push(("resize-near-limit".into(), "image-half-high".into()));
    }
    // Freeze this reduction before a cohort: large complex documents retain
    // adjacent reorder, all navigation and viewport cases. Other reorder forms
    // are covered across every layer count on the moderate canvas instead.
    if std::env::var("PICSIE_STRESS_BOUNDED").as_deref() == Ok("1") {
        tasks.retain(|(key, case)| {
            let heavy = (key.starts_with("3600-") || key.starts_with("6000-"))
                && key.contains("complex")
                && key
                    .split('-')
                    .nth(1)
                    .and_then(|n| n.parse::<usize>().ok())
                    .unwrap_or(0)
                    >= 300;
            !heavy
                || !matches!(
                    case.as_str(),
                    "reorder-long" | "reorder-multi" | "folder-move"
                )
        });
    }
    if let Ok(filter) = std::env::var("PICSIE_STRESS_FILTER") {
        tasks.retain(|(key, case)| format!("{key}/{case}").contains(&filter));
    }
    if let Ok(cases) = std::env::var("PICSIE_STRESS_CASES") {
        if !cases.is_empty() {
            tasks.retain(|(_, case)| cases.split(',').any(|wanted| wanted == case));
        }
    }
    let mut results = vec![];
    let mut previous_key = String::new();
    let mut retained_document = None;
    let mut retained_renderer = Renderer::default();
    for (key, case) in tasks {
        if key != previous_key {
            retained_document = Some(files::open_project(
                &fixtures.join(format!("{key}.picsie")),
            )?);
            retained_renderer = Renderer::default();
            previous_key = key.clone();
        }
        let mut d = retained_document.as_ref().unwrap().clone();
        if case == "mask-scale-independent" || case == "mask-distort" {
            let l = d.layers.last_mut().unwrap();
            let placement = MaskPlacement::of(l);
            let m = Arc::make_mut(l.mask.as_mut().unwrap());
            m.linked = false;
            m.placement = Some(placement);
        }
        let original = signature(&d);
        let mut rows = vec![];
        let r = &mut retained_renderer;
        for i in 0..warmups + samples {
            let cancel_reference = if case.ends_with("cancel") {
                let reference = setup_with_draft(&d, &case, false)?;
                Some((
                    preview(&reference, &mut Renderer::default())?,
                    reference.history.pixel_selection,
                ))
            } else {
                None
            };
            let mut e = setup(&d, &case)?;
            let first_started = Instant::now();
            let baseline = preview(&e, r)?;
            let first_preview_ms = first_started.elapsed().as_secs_f64() * 1000.;
            if case == "pan-revisit" {
                perform(&mut e, &case)?;
                preview(&e, r)?;
                e.viewport = setup(&d, &case)?.viewport;
                preview(&e, r)?;
            }
            let before = e.history.info().undo_count;
            let viewport_before = e.viewport.clone();
            let start = Instant::now();
            let (edit, phases) = perform(&mut e, &case)?;
            let command_ms = start.elapsed().as_secs_f64() * 1000.;
            let pixels = preview(&e, r)?;
            let preview_ms = start.elapsed().as_secs_f64() * 1000.;
            let state = signature(&e.history.document);
            e.history.document.validate()?;
            if case.starts_with("zoom-") {
                let o = geometry::canvas_origin(&d, &viewport_before);
                let world = Point::new(
                    (330. - o.x) / viewport_before.zoom,
                    (270. - o.y) / viewport_before.zoom,
                );
                let o = geometry::canvas_origin(&d, &e.viewport);
                ensure!(
                    (o.x + world.x * e.viewport.zoom - 330.).abs() < 1e-6
                        && (o.y + world.y * e.viewport.zoom - 270.).abs() < 1e-6,
                    "Zoom lost pointer anchor"
                );
            }
            if case.starts_with("pan-") {
                ensure!(
                    (e.viewport.pan.x - viewport_before.pan.x - 280.).abs() < 1e-6
                        && (e.viewport.pan.y - viewport_before.pan.y - 170.).abs() < 1e-6,
                    "Pan lost requested offset"
                );
            }
            if case == "text-reflow" {
                ensure!(
                    e.selected().unwrap().width == 620 && e.selected().unwrap().height == 650,
                    "Text box did not resize to matched dimensions"
                );
            }
            let mut undo_ms = None;
            let mut redo_ms = None;
            let mut output_ms = None;
            if edit {
                ensure!(
                    e.history.info().undo_count == before + 1,
                    "{key}/{case}: expected one committed edit"
                );
                let t = Instant::now();
                e.command(Command::Undo)?;
                preview(&e, r)?;
                undo_ms = Some(t.elapsed().as_secs_f64() * 1000.);
                ensure!(
                    signature(&e.history.document) == original,
                    "{key}/{case}: Undo did not restore document"
                );
                let t = Instant::now();
                e.command(Command::Redo)?;
                let redo_pixels = preview(&e, r)?;
                redo_ms = Some(t.elapsed().as_secs_f64() * 1000.);
                ensure!(
                    signature(&e.history.document) == state,
                    "{key}/{case}: Redo differs"
                );
                ensure!(
                    pixels == redo_pixels,
                    "{key}/{case}: Redo preview pixels differ"
                );
                if mode == "resize" {
                    let t = Instant::now();
                    let full =
                        render::rgba_pixels(&r.render(&e.history.document)?.image_snapshot())?;
                    output_ms = Some(t.elapsed().as_secs_f64() * 1000.);
                    black_box(&full);
                    if i == warmups && std::env::var("PICSIE_STRESS_PROBES").as_deref() != Ok("0") {
                        fs::write(out.join(format!("{key}-{case}.rgba")), &full)?;
                    }
                }
            } else {
                ensure!(
                    signature(&e.history.document) == original
                        && e.history.info().undo_count == before,
                    "Navigation edited document"
                );
                if case.ends_with("cancel") {
                    let (reference_pixels, reference_selection) =
                        cancel_reference.as_ref().unwrap();
                    ensure!(
                        pixels == *reference_pixels,
                        "Cancel changed pre-transaction pixels"
                    );
                    ensure!(
                        e.history.pixel_selection == *reference_selection,
                        "Cancel changed pre-transaction selection"
                    );
                } else {
                    ensure!(pixels != baseline, "Navigation produced stale output");
                }
            }
            if i >= warmups {
                let mut row = json!({"first_preview_ms":first_preview_ms,"command_ms":command_ms,"preview_ms":preview_ms,"undo_preview_ms":undo_ms,"redo_preview_ms":redo_ms,"full_output_after_redo_ms":output_ms});
                for (key, value) in phases.as_object().unwrap() {
                    row[key] = value.clone();
                }
                rows.push(row);
            }
            black_box(pixels);
        }
        let item = json!({"key":key,"case":case,"samples":rows,"validated":true});
        fs::write(
            out.join(format!("{key}-{case}.json")),
            serde_json::to_vec_pretty(&item)?,
        )?;
        eprintln!("Completed {key}/{case}");
        results.push(item);
    }
    let mut limit_checks = vec![];
    if mode == "resize" {
        let d = files::open_project(&fixtures.join("resize-near-limit.picsie"))?;
        let mut e = setup(&d, "image-up2-high")?;
        ensure!(
            perform(&mut e, "image-up2-high").is_err(),
            "Over-budget resize unexpectedly succeeded"
        );
        ensure!(
            signature(&e.history.document) == signature(&d) && e.history.info().undo_count == 0,
            "Rejected resize mutated document"
        );
        limit_checks.push("oversized side rejected atomically");
        let before = signature(&e.history.document);
        ensure!(
            e.command(Command::ResizeImage {
                options: ImageSizeOptions {
                    width: 6001,
                    height: 4000,
                    resolution: 96.,
                    sampling: Sampling::High
                }
            })
            .is_err()
        );
        ensure!(signature(&e.history.document) == before && e.history.info().undo_count == 0);
        limit_checks.push("24 MP canvas budget rejected atomically");
        let mut sources = files::open_project(&fixtures.join("resize-single.picsie"))?;
        let mut second = sources.layers[0].clone();
        second.id = id();
        sources.layers.push(second);
        let mut masks = Document::new("Mask budget", 3600, 2400)?;
        for i in 0..2 {
            let mut l = Layer::new(&format!("Folder {i}"), 3600, 2400, Content::Group);
            l.mask = Some(mask(96, 96, false, &l));
            masks.layers.push(l);
        }
        for (label, d) in [
            ("aggregate resized-source budget", sources),
            ("aggregate resized-mask budget", masks),
        ] {
            let mut e = setup(&d, "image-up2-high")?;
            let before = signature(&d);
            let result = e.command(Command::ResizeImage {
                options: ImageSizeOptions {
                    width: 4500,
                    height: 3000,
                    resolution: 96.,
                    sampling: Sampling::High,
                },
            });
            ensure!(result.is_err(), "{label} unexpectedly succeeded");
            ensure!(
                signature(&e.history.document) == before && e.history.info().undo_count == 0,
                "{label} changed the document"
            );
            limit_checks.push(label);
        }
    }
    fs::write(
        out.join("results.json"),
        serde_json::to_vec_pretty(
            &json!({"app":"picsie","mode":mode,"warmups":warmups,"samples":samples,"validation_checks":limit_checks,"scope":"Existing commands with history, CPU viewport preview/readback; full output and Undo/Redo separate. No UI/GPU/input latency. Per-sample fresh editor sharing source assets; retained renderer/source assets across cases within each fixture; cold/warm setup preview costs separated, one setup preview before timing. First-render cost is recorded separately.","results":results}),
        )?,
    )?;
    Ok(())
}
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().collect();
    ensure!(
        a.len() >= 3,
        "prepare FIXTURES | run FIXTURES OUTPUT WARMUPS SAMPLES stacks|resize"
    );
    if a[1] == "prepare" {
        prepare(Path::new(&a[2]))
    } else {
        ensure!(a.len() == 7);
        run(
            Path::new(&a[2]),
            Path::new(&a[3]),
            a[4].parse()?,
            a[5].parse()?,
            &a[6],
        )
    }
}
