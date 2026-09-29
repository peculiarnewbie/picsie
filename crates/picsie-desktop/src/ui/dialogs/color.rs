//! PickerHSB / PaletteColor from pinned Compositor ColorPalette.swift.
//! MIT © 2026 Wonder Assembly LLC. Same 8-bit form semantics as src/ui/color-math.ts.
#[derive(Clone, Copy, Default)]
pub struct Hsb {
    pub h: f64,
    pub s: f64,
    pub v: f64,
}
impl Hsb {
    pub fn from_hex(text: &str) -> Option<Self> {
        let mut value = Self::default();
        value.set_rgb(parse_hex(text)?);
        Some(value)
    }
    pub fn set_rgb(&mut self, rgb: [u8; 3]) {
        let [r, g, b] = rgb.map(|c| c as f64 / 255.);
        let high = r.max(g).max(b);
        let delta = high - r.min(g).min(b);
        self.v = high;
        if high > 0. {
            self.s = delta / high;
        }
        if delta > 0. {
            let h = if high == r {
                (g - b) / delta
            } else if high == g {
                (b - r) / delta + 2.
            } else {
                (r - g) / delta + 4.
            };
            self.h = (h * 60.).rem_euclid(360.);
        }
    }
    pub fn rgb(self) -> [u8; 3] {
        let h = self.h.rem_euclid(360.) / 60.;
        let c = self.v * self.s;
        let x = c * (1. - ((h % 2.) - 1.).abs());
        let m = self.v - c;
        let channels = match h as u8 {
            0 => [c, x, 0.],
            1 => [x, c, 0.],
            2 => [0., c, x],
            3 => [0., x, c],
            4 => [x, 0., c],
            _ => [c, 0., x],
        };
        channels.map(|n| ((n + m) * 255.).round().clamp(0., 255.) as u8)
    }
    pub fn hex(self) -> String {
        let [r, g, b] = self.rgb();
        format!("{r:02X}{g:02X}{b:02X}")
    }
}
pub fn parse_hex(value: &str) -> Option<[u8; 3]> {
    let text = value.trim();
    let text = text.strip_prefix('#').unwrap_or(text);
    if ![3, 6].contains(&text.len()) || !text.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let text = if text.len() == 3 {
        text.chars().flat_map(|c| [c, c]).collect::<String>()
    } else {
        text.to_owned()
    };
    let n = u32::from_str_radix(&text, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}
#[cfg(test)]
mod tests {
    use super::*;
    // Translated fixtures: CompositorTests/ColorPickerTests.swift, same pinned revision.
    #[test]
    fn upstream_hex_parsing() {
        assert_eq!(parse_hex("#FF8000"), Some([255, 128, 0]));
        assert_eq!(parse_hex("0f0"), Some([0, 255, 0]));
        assert_eq!(parse_hex(" 00ff00 "), Some([0, 255, 0]));
        assert!(parse_hex("12345").is_none());
        assert!(parse_hex("GGGGGG").is_none());
    }
    #[test]
    fn upstream_hsb_round_trips() {
        for hex in [
            "000000", "FFFFFF", "FF0000", "00FF00", "0000FF", "FF8000", "7F3FA2", "123456",
        ] {
            assert_eq!(Hsb::from_hex(hex).unwrap().hex(), hex);
        }
    }
    #[test]
    fn upstream_gray_and_black_preserve_hue() {
        let mut hsb = Hsb::from_hex("FF8000").unwrap();
        let hue = hsb.h;
        hsb.set_rgb([128; 3]);
        assert_eq!(hsb.h, hue);
        assert_eq!(hsb.s, 0.);
        hsb.s = 0.5;
        hsb.set_rgb([0; 3]);
        assert_eq!(hsb.h, hue);
        assert_eq!(hsb.s, 0.5);
        assert_eq!(hsb.v, 0.);
    }
}
