import test from "node:test";
import assert from "node:assert/strict";
import { formatHex, fromHSB, parseHex, toHSB } from "../src/ui/color-math.ts";

// Adapted from Compositor ColorPickerTests (ColorPalette.swift / PickerHSB semantics).

test("hex parses full, shorthand and rejects invalid", () => {
  assert.deepEqual(parseHex("#FF8000"), { r: 255, g: 128, b: 0 });
  assert.deepEqual(parseHex("0f0"), { r: 0, g: 255, b: 0 });
  assert.deepEqual(parseHex(" 00ff00 "), { r: 0, g: 255, b: 0 });
  assert.equal(parseHex("12345"), undefined);
  assert.equal(parseHex("GGGGGG"), undefined);
  assert.equal(formatHex({ r: 255, g: 128, b: 0 }), "FF8000");
});

test("HSB round-trips 8-bit colors", () => {
  for (const hex of ["FF8000", "00FF00", "0000FF", "FFFFFF", "123456", "F0E1D2", "7F7F7F", "010203"]) {
    const rgb = parseHex(hex)!;
    assert.equal(formatHex(fromHSB(toHSB(rgb))), hex);
  }
});

test("grays and black keep previous hue and saturation", () => {
  const previous = { h: 210, s: 0.5, v: 0.5 };
  const gray = toHSB({ r: 128, g: 128, b: 128 }, previous);
  assert.equal(gray.h, 210, "gray should keep the previous hue");
  assert.equal(gray.s, 0);
  const black = toHSB({ r: 0, g: 0, b: 0 }, previous);
  assert.equal(black.h, 210, "black should keep the previous hue");
  assert.equal(black.s, 0.5, "black should keep the previous saturation");
});
