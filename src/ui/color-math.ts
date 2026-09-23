/** PaletteColor/PickerHSB math translated from Compositor's ColorPalette.swift. MIT © 2026 Wonder Assembly LLC. */

export type RGB = { r: number; g: number; b: number };
export type HSB = { h: number; s: number; v: number };

/** Accepts `RRGGBB` or shorthand `RGB`, with or without a leading `#`. */
export function parseHex(input: string): RGB | undefined {
  const value = input.trim().replace(/^#/, "");
  const digits = /^[0-9a-fA-F]{3}$|^[0-9a-fA-F]{6}$/.test(value) ? value : undefined;
  if (!digits) return undefined;
  const full =
    digits.length === 3
      ? digits
          .split("")
          .map((digit) => digit + digit)
          .join("")
      : digits;
  return {
    r: parseInt(full.slice(0, 2), 16),
    g: parseInt(full.slice(2, 4), 16),
    b: parseInt(full.slice(4, 6), 16),
  };
}

/** Upstream `PaletteColor.hex`: uppercase `RRGGBB` without the marker. */
export function formatHex(color: RGB): string {
  const pair = (value: number) =>
    Math.max(0, Math.min(255, Math.round(value))).toString(16).padStart(2, "0").toUpperCase();
  return `${pair(color.r)}${pair(color.g)}${pair(color.b)}`;
}

/** 8-bit quantization, as upstream `PaletteColor.quantized`. */
export function quantized(color: RGB): RGB {
  const q = (value: number) => Math.max(0, Math.min(255, Math.round(value)));
  return { r: q(color.r), g: q(color.g), b: q(color.b) };
}

/**
 * RGB → HSB keeping the previous hue for grays and the previous hue and saturation for black,
 * matching Photoshop's fields (upstream `PickerHSB.setRGB`).
 */
export function toHSB(color: RGB, previous?: HSB): HSB {
  const r = color.r / 255,
    g = color.g / 255,
    b = color.b / 255;
  const max = Math.max(r, g, b),
    min = Math.min(r, g, b),
    delta = max - min;
  let h = previous?.h ?? 0;
  let s = previous?.s ?? 0;
  if (delta > 0) {
    if (max === r) h = 60 * (((g - b) / delta) % 6);
    else if (max === g) h = 60 * ((b - r) / delta + 2);
    else h = 60 * ((r - g) / delta + 4);
    if (h < 0) h += 360;
    s = max === 0 ? 0 : delta / max;
  } else if (max > 0) {
    // Gray: hue survives, saturation goes to zero.
    s = 0;
  } // Black keeps both.
  return { h, s, v: max };
}

export function fromHSB(hsb: HSB): RGB {
  const h = ((hsb.h % 360) + 360) % 360;
  const c = hsb.v * hsb.s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = hsb.v - c;
  const sector = Math.floor(h / 60) % 6;
  const table: [number, number, number][] = [
    [c, x, 0],
    [x, c, 0],
    [0, c, x],
    [0, x, c],
    [x, 0, c],
    [c, 0, x],
  ];
  const [r, g, b] = table[sector] ?? [0, 0, 0];
  return quantized({ r: (r + m) * 255, g: (g + m) * 255, b: (b + m) * 255 });
}
