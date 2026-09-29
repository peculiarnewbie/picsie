//! Selected Compositor fixtures, MIT © 2026 Wonder Assembly LLC.
//! Pinned sources and raster adaptations are identified in docs/compositor-port.md.
use picsie_core::{
    canvas_size::CanvasSizeOptions,
    crop::CropRect,
    editor::{Command, Editor, Modifiers, Phase, PointerSample, Tool},
    geometry::Viewport,
    model::{Document, Point},
    pixel_selection::{MarqueeKind, PixelSelectionMode},
    render::{Renderer, selection_coverage},
};
use std::sync::Arc;

fn session() -> Editor {
    let mut e = Editor::new(Document::new("Selection", 100, 100).unwrap()).unwrap();
    e.viewport = Viewport {
        width: 100.,
        height: 100.,
        zoom: 1.,
        pan: Point::default(),
    };
    e
}
fn command(e: &mut Editor, command: Command) {
    e.command(command).unwrap();
}
fn pointer(e: &mut Editor, phase: Phase, x: f64, y: f64) {
    e.pointer(PointerSample {
        phase,
        point: Point::new(x, y),
        modifiers: Modifiers::default(),
    })
    .unwrap();
}
fn square(e: &mut Editor, x: f64, y: f64, size: f64) {
    pointer(e, Phase::Down, x, y);
    pointer(e, Phase::Move, x + size, y);
    pointer(e, Phase::Move, x + size, y + size);
    pointer(e, Phase::Up, x, y + size);
}
fn marquee(e: &mut Editor, x: f64, y: f64, size: f64) {
    command(
        e,
        Command::SetTool {
            tool: Tool::Marquee,
        },
    );
    pointer(e, Phase::Down, x, y);
    pointer(e, Phase::Move, x + size / 2., y + size / 2.);
    pointer(e, Phase::Up, x + size, y + size);
}

/// Translated SelectionTests.clickDeselectsAndSelectionStepsUndo, retaining its inputs.
#[test]
fn compositor_click_deselects_and_selection_steps_undo() {
    let mut e = session();
    command(&mut e, Command::SetTool { tool: Tool::Lasso });
    let count = e.history.info().undo_count;
    square(&mut e, 10., 10., 40.);
    assert_eq!(e.history.info().undo_count, count + 1);
    assert_eq!(e.history.info().undo_label, "Lasso");
    pointer(&mut e, Phase::Down, 5., 5.);
    pointer(&mut e, Phase::Up, 5., 5.);
    assert!(e.history.pixel_selection.is_none());
    assert_eq!(e.history.info().undo_label, "Deselect");
    command(&mut e, Command::Undo);
    assert!(e.history.pixel_selection.as_ref().unwrap().bounds.is_some());
    command(&mut e, Command::Undo);
    assert!(e.history.pixel_selection.is_none());
    command(&mut e, Command::Redo);
    assert_eq!(e.history.pixel_selection.as_ref().unwrap().at(30, 30), 255);
}

/// Translated SelectionTests.emptySelectionIsDistinctFromNoSelection, with additional undo checks.
#[test]
fn compositor_empty_selection_is_distinct_from_no_selection() {
    let mut e = session();
    command(&mut e, Command::SetTool { tool: Tool::Lasso });
    command(
        &mut e,
        Command::SetSelectionMode {
            mode: PixelSelectionMode::Subtract,
        },
    );
    square(&mut e, 0., 0., 50.);
    assert!(e.history.pixel_selection.is_none());
    assert!(!e.history.info().can_undo);
    command(
        &mut e,
        Command::SetSelectionMode {
            mode: PixelSelectionMode::Replace,
        },
    );
    square(&mut e, 10., 10., 20.);
    command(
        &mut e,
        Command::SetSelectionMode {
            mode: PixelSelectionMode::Subtract,
        },
    );
    square(&mut e, 0., 0., 60.);
    assert!(e.history.pixel_selection.as_ref().unwrap().bounds.is_none());
    assert_eq!(e.history.pixel_selection.as_ref().unwrap().at(20, 20), 0);
    command(&mut e, Command::DeselectPixels);
    assert!(e.history.pixel_selection.is_none());
    command(&mut e, Command::Undo);
    assert!(e.history.pixel_selection.as_ref().unwrap().bounds.is_none());
    command(&mut e, Command::Undo);
    assert_eq!(e.history.pixel_selection.as_ref().unwrap().at(20, 20), 255);
}

/// Adapted SelectionTests.replaceAddAndSubtractCombineOutlines; the same four outlines and
/// coverage probes now also round-trip every state through history.
#[test]
fn compositor_combined_outlines_round_trip() {
    let mut e = session();
    command(&mut e, Command::SetTool { tool: Tool::Lasso });
    let mut states = vec![None];
    for (mode, x, y, size, probes) in [
        (
            PixelSelectionMode::Replace,
            10.,
            10.,
            40.,
            vec![(30, 30, 255), (70, 70, 0)],
        ),
        (
            PixelSelectionMode::Add,
            50.,
            50.,
            40.,
            vec![(30, 30, 255), (70, 70, 255)],
        ),
        (
            PixelSelectionMode::Subtract,
            20.,
            20.,
            20.,
            vec![(30, 30, 0), (15, 15, 255)],
        ),
        (
            PixelSelectionMode::Replace,
            60.,
            10.,
            20.,
            vec![(70, 20, 255), (70, 70, 0), (15, 15, 0)],
        ),
    ] {
        command(&mut e, Command::SetSelectionMode { mode });
        square(&mut e, x, y, size);
        for (x, y, value) in probes {
            assert_eq!(e.history.pixel_selection.as_ref().unwrap().at(x, y), value);
        }
        states.push(e.history.pixel_selection.clone());
    }
    assert_eq!(e.history.info().undo_count, 4);
    for state in states[..4].iter().rev() {
        command(&mut e, Command::Undo);
        assert_eq!(&e.history.pixel_selection, state);
    }
    for state in &states[1..] {
        command(&mut e, Command::Redo);
        assert_eq!(&e.history.pixel_selection, state);
    }
}

/// Additional local regression: one entry per drag, including ellipse metadata, plus
/// value-equivalent selections and capped feathering preserve redo and saved revisions.
#[test]
fn selection_noops_saved_revisions_and_branching() {
    let mut e = session();
    command(&mut e, Command::SelectAllPixels);
    let saved = e.history.revision.clone();
    e.history.mark_saved(saved.clone());
    command(&mut e, Command::FeatherSelection { amount: 6 });
    assert_eq!(e.history.info().undo_label, "Feather Selection");
    command(&mut e, Command::Undo);
    assert!(!e.history.info().dirty);
    command(&mut e, Command::SelectAllPixels);
    assert!(e.history.info().can_redo);
    assert_eq!(e.history.revision, saved);
    command(&mut e, Command::Redo);
    assert!(e.history.info().dirty);
    command(&mut e, Command::FeatherSelection { amount: 250 });
    command(&mut e, Command::DeselectPixels);
    command(&mut e, Command::Undo);
    let revision = e.history.revision.clone();
    command(&mut e, Command::FeatherSelection { amount: 250 });
    assert_eq!(e.history.revision, revision);
    assert!(e.history.info().can_redo);
    command(
        &mut e,
        Command::SetMarqueeKind {
            kind: MarqueeKind::Ellipse,
        },
    );
    let count = e.history.info().undo_count;
    marquee(&mut e, 10., 20., 40.);
    assert_eq!(e.history.info().undo_count, count + 1);
    assert_eq!(e.history.info().undo_label, "Elliptical Marquee");
    assert!(!e.history.info().can_redo);
    let ellipse = e.history.pixel_selection.clone();
    marquee(&mut e, 10., 20., 40.);
    assert_eq!(e.history.info().undo_count, count + 1);
    pointer(&mut e, Phase::Down, 5., 5.);
    pointer(&mut e, Phase::Up, 5., 5.);
    assert!(e.history.pixel_selection.is_none());
    command(&mut e, Command::Undo);
    assert_eq!(e.history.pixel_selection, ellipse);
}

/// Additional local regression for the retained raster adaptation: feather edits share
/// coverage, and undo restores the exact coverage and feather used by pixel clearing.
#[test]
fn feather_and_pixel_edits_restore_selection_and_layer_pixels() {
    let mut e = session();
    command(&mut e, Command::AddGradient);
    marquee(&mut e, 20., 20., 40.);
    let crisp = e.history.pixel_selection.clone().unwrap();
    command(&mut e, Command::FeatherSelection { amount: 6 });
    let feathered = e.history.pixel_selection.clone().unwrap();
    assert!(Arc::ptr_eq(&crisp.pixels, &feathered.pixels));
    let coverage = selection_coverage(&feathered).unwrap();
    command(&mut e, Command::ClearSelectedPixels);
    let cleared = e.history.document.clone();
    assert_eq!(
        Renderer::default()
            .sample(&cleared, Point::new(40., 40.))
            .unwrap()[3],
        0
    );
    command(&mut e, Command::DeselectPixels);
    command(&mut e, Command::Undo);
    assert_eq!(
        selection_coverage(e.history.pixel_selection.as_ref().unwrap()).unwrap(),
        coverage
    );
    command(&mut e, Command::Undo);
    assert_eq!(e.history.pixel_selection.as_ref().unwrap(), &feathered);
    assert_eq!(
        Renderer::default()
            .sample(&e.history.document, Point::new(40., 40.))
            .unwrap()[3],
        255
    );
    command(&mut e, Command::Undo);
    assert_eq!(e.history.pixel_selection.as_ref().unwrap(), &crisp);
    command(&mut e, Command::Redo);
    command(&mut e, Command::Redo);
    assert_eq!(e.history.document, cleared);
    assert_eq!(e.history.pixel_selection.as_ref().unwrap(), &feathered);
}

/// Additional local regression: canceled drafts, tool changes, invalid commands, and
/// canceled nested transactions do not destroy the previous selection or redo branch.
#[test]
fn canceled_drafts_and_nested_selection_transactions() {
    let mut e = session();
    marquee(&mut e, 10., 10., 30.);
    let original = e.history.pixel_selection.clone();
    command(&mut e, Command::FeatherSelection { amount: 6 });
    command(&mut e, Command::Undo);
    let count = e.history.info().undo_count;
    for cancel in [
        Command::CancelGesture,
        Command::SetTool { tool: Tool::Lasso },
    ] {
        pointer(&mut e, Phase::Down, 60., 60.);
        pointer(&mut e, Phase::Move, 90., 90.);
        command(&mut e, cancel);
        assert!(e.selection_draft().is_none());
        assert_eq!(e.history.pixel_selection, original);
        assert_eq!(e.history.info().undo_count, count);
        assert!(e.history.info().can_redo);
    }
    assert!(
        e.command(Command::FeatherSelection { amount: 251 })
            .is_err()
    );
    assert!(e.history.info().can_redo);
    e.begin_edit("Selection setup");
    command(&mut e, Command::SelectAllPixels);
    command(&mut e, Command::FeatherSelection { amount: 6 });
    assert!(!e.history.info().can_undo);
    command(&mut e, Command::CancelGesture);
    assert_eq!(e.history.pixel_selection, original);
    assert!(e.history.info().can_redo);
    e.begin_edit("Selection setup");
    command(&mut e, Command::SelectAllPixels);
    command(&mut e, Command::FeatherSelection { amount: 6 });
    e.end_edit();
    assert_eq!(e.history.info().undo_count, count + 1);
    assert_eq!(e.history.info().undo_label, "Selection setup");
    command(&mut e, Command::Undo);
    assert_eq!(e.history.pixel_selection, original);
    // Undo during a live outline cancels that draft and undoes the previous completed edit.
    pointer(&mut e, Phase::Down, 60., 60.);
    command(&mut e, Command::Undo);
    assert!(e.selection_draft().is_none());
    assert!(e.history.pixel_selection.is_none());
}

/// Additional local regression: crop/resize clear coverage atomically with geometry,
/// including a same-size crop that translates the canvas origin.
#[test]
fn canvas_changes_restore_selection_with_its_original_dimensions() {
    let mut e = session();
    marquee(&mut e, 10., 10., 30.);
    let original = e.history.pixel_selection.clone();
    let count = e.history.info().undo_count;
    command(
        &mut e,
        Command::ResizeCanvas {
            options: CanvasSizeOptions {
                width: 60,
                height: 50,
                anchor: 4,
                fill: None,
            },
        },
    );
    assert!(e.history.pixel_selection.is_none());
    assert_eq!(e.history.info().undo_count, count + 1);
    command(&mut e, Command::Undo);
    assert_eq!(e.history.document.width, 100);
    assert_eq!(e.history.pixel_selection, original);
    command(&mut e, Command::Redo);
    assert_eq!(e.history.document.width, 60);
    assert!(e.history.pixel_selection.is_none());
    command(&mut e, Command::Undo);
    e.crop_rect = Some(CropRect {
        x: 10.,
        y: 10.,
        width: 100.,
        height: 100.,
    });
    command(&mut e, Command::CommitCrop);
    assert!(e.history.pixel_selection.is_none());
    command(&mut e, Command::Undo);
    assert_eq!(e.history.pixel_selection, original);
    command(&mut e, Command::Redo);
    assert!(e.history.pixel_selection.is_none());
}

/// Additional local raster-budget adaptation of HistoryTests.historyBoundsEntriesAndUniqueRetainedPixels.
#[test]
fn selection_buffers_are_shared_and_budgeted_across_undo_and_redo() {
    let mut e = session();
    command(&mut e, Command::SelectAllPixels);
    for amount in [6, 8] {
        command(&mut e, Command::FeatherSelection { amount });
    }
    assert_eq!(
        e.history.retained_bytes(),
        0,
        "live coverage is excluded, feather shares it"
    );
    let previous_path_bytes = e
        .history
        .pixel_selection
        .as_ref()
        .unwrap()
        .outline
        .approximate_bytes_used();
    marquee(&mut e, 10., 10., 20.);
    let next_path_bytes = e
        .history
        .pixel_selection
        .as_ref()
        .unwrap()
        .outline
        .approximate_bytes_used();
    assert_eq!(
        e.history.retained_bytes(),
        100 * 100 + previous_path_bytes,
        "count old coverage once across snapshots"
    );
    command(&mut e, Command::Undo);
    assert_eq!(
        e.history.retained_bytes(),
        100 * 100 + next_path_bytes,
        "redo coverage counts too"
    );
    command(&mut e, Command::Redo);
    e.history.byte_limit = 0;
    command(&mut e, Command::DeselectPixels);
    assert_eq!(e.history.info().undo_count, 0);
    assert_eq!(e.history.retained_bytes(), 0);
    command(&mut e, Command::SelectAllPixels);
    assert!(e.history.info().can_undo);
    command(&mut e, Command::Undo);
    assert!(
        !e.history.info().can_redo,
        "undo drops redo if its raster exceeds the budget"
    );
}
