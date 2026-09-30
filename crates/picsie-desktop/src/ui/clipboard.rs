//! Native clipboard integration. The worker owns copy/cut rendering and paste decoding.
use super::*;
use picsie_core::model::Point as EnginePoint;
#[derive(Default)]
pub(super) struct ClipboardCache(Option<(gpui_kit::Image, EnginePoint)>);
impl Global for ClipboardCache {}
impl Desktop {
    pub(super) fn store_clipboard(
        &mut self,
        bytes: Vec<u8>,
        origin: EnginePoint,
        cx: &mut Context<Self>,
    ) {
        let image = gpui_kit::Image::from_bytes(ImageFormat::Png, bytes);
        cx.write_to_clipboard(ClipboardItem::new_image(&image));
        cx.set_global(ClipboardCache(Some((image, origin))));
        self.notice = "Copied pixels".into();
    }
    pub(super) fn paste_clipboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.busy = true;
        let read = cx.read_from_clipboard_async();
        cx.spawn_in(window, async move |this, cx| {
            let result = read.await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.busy = false;
                match result {
                    Ok(Some(item)) => {
                        if let Some(ClipboardEntry::Image(image)) = item
                            .into_entries()
                            .find(|e| matches!(e, ClipboardEntry::Image(_)))
                        {
                            let origin = cx
                                .try_global::<ClipboardCache>()
                                .and_then(|cache| cache.0.as_ref())
                                .filter(|(cached, _)| cached.bytes == image.bytes)
                                .map(|(_, origin)| *origin);
                            this.blocking(Operation::Paste {
                                bytes: image.bytes,
                                origin,
                            });
                        } else {
                            this.notice = "The clipboard does not contain an image".into();
                        }
                    }
                    Ok(None) => this.notice = "The clipboard is empty".into(),
                    Err(error) => this.notice = format!("Could not read clipboard: {error}"),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
