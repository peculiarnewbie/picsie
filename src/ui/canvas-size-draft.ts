// Transient form values only. Rust validates and applies every canvas resize.
import type { Document } from "../engine/types.ts";
const MAX_DIMENSION = 8192;
const MAX_PIXELS = 24_000_000;
export const anchorNames = [
  "Top left",
  "Top center",
  "Top right",
  "Middle left",
  "Center",
  "Middle right",
  "Bottom left",
  "Bottom center",
  "Bottom right",
] as const;
export type CanvasUnit = "pixels" | "percent";

export class CanvasSizeDraft {
  readonly originalWidth: number;
  readonly originalHeight: number;
  width: number;
  height: number;
  relative = false;
  locked = false;
  unit: CanvasUnit = "pixels";

  constructor(document: Pick<Document, "width" | "height">) {
    this.originalWidth = this.width = document.width;
    this.originalHeight = this.height = document.height;
  }
  get valid() {
    const width = Math.round(this.width),
      height = Math.round(this.height);
    return (
      Number.isFinite(width) &&
      Number.isFinite(height) &&
      width >= 1 &&
      height >= 1 &&
      width <= MAX_DIMENSION &&
      height <= MAX_DIMENSION &&
      width * height <= MAX_PIXELS
    );
  }
  displayed(widthAxis: boolean) {
    const original = widthAxis ? this.originalWidth : this.originalHeight;
    const pixels = (widthAxis ? this.width : this.height) - (this.relative ? original : 0);
    return this.unit === "percent" ? (pixels / original) * 100 : pixels;
  }
  set(value: number, widthAxis: boolean) {
    const original = widthAxis ? this.originalWidth : this.originalHeight;
    const pixels = this.unit === "percent" ? (value / 100) * original : value;
    const final = pixels + (this.relative ? original : 0);
    if (widthAxis) {
      this.width = final;
      if (this.locked) this.height = (final * this.originalHeight) / this.originalWidth;
    } else {
      this.height = final;
      if (this.locked) this.width = (final * this.originalWidth) / this.originalHeight;
    }
  }
}
