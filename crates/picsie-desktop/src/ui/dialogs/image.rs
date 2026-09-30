//! ImageSizeSheet.swift form behavior, Compositor 609dbeae, MIT © 2026 Wonder Assembly LLC.
use super::*;
use picsie_core::{image_size::ImageSizeOptions, model::Sampling};
#[derive(Clone)]
pub(in crate::ui) struct ImageDraft {
    original: [f64; 2],
    size: [f64; 2],
    resolution: f64,
    locked: bool,
    resample: bool,
    unit: String,
    sampling: Sampling,
}
impl ImageDraft {
    pub(super) fn valid(&self) -> bool {
        self.resolution.is_finite()
            && (1. ..=9600.).contains(&self.resolution)
            && self
                .size
                .iter()
                .all(|n| n.is_finite() && (1. ..=8192.).contains(&n.round()))
            && self.size[0].round() * self.size[1].round() <= 24_000_000.
    }
    fn display(&self, i: usize) -> f64 {
        match self.unit.as_str() {
            "percent" => self.size[i] / self.original[i] * 100.,
            "inches" => self.size[i] / self.resolution,
            "centimeters" => self.size[i] / self.resolution * 2.54,
            _ => self.size[i],
        }
    }
    fn set(&mut self, i: usize, value: f64) {
        if !self.resample {
            self.resolution =
                self.size[i] / value * if self.unit == "centimeters" { 2.54 } else { 1. };
            return;
        }
        self.size[i] = match self.unit.as_str() {
            "percent" => value / 100. * self.original[i],
            "inches" => value * self.resolution,
            "centimeters" => value / 2.54 * self.resolution,
            _ => value,
        };
        if self.locked {
            self.size[1 - i] = self.size[i] * self.original[1 - i] / self.original[i];
        }
    }
    pub(super) fn options(&self) -> ImageSizeOptions {
        ImageSizeOptions {
            width: self.size[0].round() as u32,
            height: self.size[1].round() as u32,
            resolution: self.resolution,
            sampling: self.sampling,
        }
    }
}
impl Desktop {
    pub(in crate::ui) fn open_image_size(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = &self.state else {
            return;
        };
        let size = [state.document.width as f64, state.document.height as f64];
        let resolution = state.document.resolution;
        self.send(Command::CommitTransform);
        self.send(Command::FinishGesture);
        self.modal = Some(Modal::Image(ImageDraft {
            original: size,
            size,
            resolution,
            locked: true,
            resample: true,
            unit: "pixels".into(),
            sampling: Sampling::High,
        }));
        self.notice.clear();
        self.sync_image_fields(window, cx);
        window.focus(&self.modal_focus, cx);
        cx.notify();
    }
    pub(in crate::ui) fn image_field(
        &mut self,
        key: &str,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        if let Some(Modal::Image(d)) = &mut self.modal {
            let value = number(value).unwrap_or(f64::NAN);
            match key {
                "image-width" => d.set(0, value),
                "image-height" => d.set(1, value),
                "image-resolution" => {
                    if d.resample
                        && matches!(d.unit.as_str(), "inches" | "centimeters")
                        && value > 0.
                        && d.resolution > 0.
                    {
                        d.size[0] *= value / d.resolution;
                        d.size[1] *= value / d.resolution;
                    }
                    d.resolution = value;
                }
                _ => {}
            }
            self.sync_image_fields(window, cx);
        }
        Ok(())
    }
    pub(in crate::ui) fn image_choice(
        &mut self,
        key: &str,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(Modal::Image(d)) = &mut self.modal {
            match key {
                "image-unit" if d.resample || matches!(value, "inches" | "centimeters") => {
                    d.unit = value.into()
                }
                "image-sampling" => {
                    d.sampling = match value {
                        "nearest" => Sampling::Nearest,
                        "smooth" => Sampling::Smooth,
                        _ => Sampling::High,
                    }
                }
                _ => {}
            }
            self.sync_image_fields(window, cx);
        }
    }
    fn sync_image_fields(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(Modal::Image(d)) = &self.modal {
            self.set_field("image-width", decimal(d.display(0)), false, window, cx);
            self.set_field("image-height", decimal(d.display(1)), false, window, cx);
            self.set_field("image-resolution", decimal(d.resolution), false, window, cx);
            let items = [
                ("pixels", "Pixels"),
                ("percent", "Percent"),
                ("inches", "Inches"),
                ("centimeters", "Centimeters"),
            ]
            .into_iter()
            .filter(|(value, _)| d.resample || matches!(*value, "inches" | "centimeters"))
            .map(|(value, label)| Choice {
                value: value.into(),
                label: label.into(),
            })
            .collect::<Vec<_>>();
            self.selects["image-unit"]
                .update(cx, |select, cx| select.set_items(items.into(), window, cx));
            self.sync_select("image-unit", &d.unit, window, cx);
            self.sync_select(
                "image-sampling",
                match d.sampling {
                    Sampling::Nearest => "nearest",
                    Sampling::Smooth => "smooth",
                    Sampling::High => "high",
                },
                window,
                cx,
            );
        }
    }
    pub(super) fn image_size_content(&self, d: &ImageDraft, cx: &Context<Self>) -> Div {
        column()
            .gap(px(18.))
            .child(hint(format!(
                "Current: {} × {} pixels",
                d.original[0], d.original[1]
            )))
            .child(
                row()
                    .child(label("Units").w(px(75.)))
                    .child(self.inline_select("image-unit", 230., false)),
            )
            .child(
                row()
                    .child(label("Width").w(px(75.)))
                    .child(self.inline_field("image-width", "", 145., false)),
            )
            .child(
                row()
                    .child(label("Height").w(px(75.)))
                    .child(self.inline_field("image-height", "", 145., false)),
            )
            .child(
                self.probe(
                    "image-locked",
                    Checkbox::new("image-lock-check")
                        .label("Lock aspect ratio")
                        .checked(d.locked)
                        .disabled(!d.resample)
                        .on_click(cx.listener(|this, value, _, cx| {
                            if let Some(Modal::Image(d)) = &mut this.modal {
                                d.locked = *value;
                            }
                            cx.notify();
                        })),
                ),
            )
            .child(
                row()
                    .child(label("Resolution").w(px(75.)))
                    .child(self.inline_field("image-resolution", "", 100., false))
                    .child(label("pixels/inch")),
            )
            .child(
                self.probe(
                    "image-resample",
                    Checkbox::new("image-resample-check")
                        .label("Resample artwork")
                        .checked(d.resample)
                        .on_click(cx.listener(|this, value, window, cx| {
                            if let Some(Modal::Image(d)) = &mut this.modal {
                                d.resample = *value;
                                if !*value {
                                    d.size = d.original;
                                    d.locked = true;
                                    d.unit = "inches".into();
                                }
                            }
                            this.sync_image_fields(window, cx);
                            cx.notify();
                        })),
                ),
            )
            .when(d.resample, |v| {
                v.child(
                    row()
                        .child(label("Sampling").w(px(75.)))
                        .child(self.inline_select("image-sampling", 230., false)),
                )
            })
            .child(hint(if d.valid() {
                format!(
                    "New: {} × {} pixels at {} ppi",
                    d.size[0].round(),
                    d.size[1].round(),
                    decimal(d.resolution)
                )
            } else {
                "Use 1–8192 pixels per side, up to 24 megapixels; 1–9600 ppi.".into()
            }))
    }
}
