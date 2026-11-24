//! Minimal hex color handling for the renderer (ported from `color.ts`).
//! Only #rgb / #rrggbb are accepted.

use std::f64::consts::PI;

pub type Rgb = [f64; 3];

pub fn parse_hex(input: &str) -> Option<Rgb> {
    let s = input.trim();
    let h = s.strip_prefix('#')?;
    if !(h.len() == 3 || h.len() == 6) || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let full: String = if h.len() == 3 { h.chars().flat_map(|c| [c, c]).collect() } else { h.to_string() };
    let p = |i: usize| f64::from(u8::from_str_radix(&full[i..i + 2], 16).unwrap_or(0));
    Some([p(0), p(2), p(4)])
}

/// JS `Math.round`: halves go toward +infinity.
pub fn js_round(v: f64) -> f64 {
    (v + 0.5).floor()
}

pub fn to_hex(c: Rgb) -> String {
    let f = |v: f64| format!("{:02x}", js_round(v.clamp(0.0, 255.0)) as u32);
    format!("#{}{}{}", f(c[0]), f(c[1]), f(c[2]))
}

pub fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Darken while keeping the hue rich: blend toward a deeper, slightly
/// saturated version of the color rather than toward gray-black.
pub fn shade(c: Rgb, t: f64) -> Rgb {
    let max = c[0].max(c[1]).max(c[2]);
    let deep = [c[0] * 0.35 - (max - c[0]) * 0.1, c[1] * 0.35 - (max - c[1]) * 0.1, c[2] * 0.35 - (max - c[2]) * 0.1];
    mix(c, deep, t)
}

pub fn tint(c: Rgb, t: f64) -> Rgb {
    mix(c, [255.0, 255.0, 255.0], t)
}

/// WCAG relative luminance.
pub fn luminance(c: Rgb) -> f64 {
    let ch = |v: f64| {
        let s = v / 255.0;
        if s <= 0.03928 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * ch(c[0]) + 0.7152 * ch(c[1]) + 0.0722 * ch(c[2])
}

pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let la = luminance(a);
    let lb = luminance(b);
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

pub const DARK_INK: Rgb = [20.0, 22.0, 34.0];
pub const LIGHT_INK: Rgb = [250.0, 250.0, 255.0];

/// Pick the eye ink with the best contrast against the body color.
pub fn auto_eye_color(body: Rgb) -> Rgb {
    if contrast(body, DARK_INK) >= contrast(body, LIGHT_INK) { DARK_INK } else { LIGHT_INK }
}

// OKLCH, for colors that look equally bright whatever their hue.

fn to_linear(v: f64) -> f64 {
    let s = v / 255.0;
    if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
}

fn from_linear(v: f64) -> f64 {
    255.0 * if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

/// A color as OKLCH: lightness 0..1, chroma, hue in degrees.
pub fn to_oklch(c: Rgb) -> [f64; 3] {
    let [r, g, b] = [to_linear(c[0]), to_linear(c[1]), to_linear(c[2])];
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    let lum = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
    let a = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s;
    let bb = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
    let h = bb.atan2(a) * 180.0 / PI;
    [lum, a.hypot(bb), if h < 0.0 { h + 360.0 } else { h }]
}

fn oklch_linear(l: f64, c: f64, h: f64) -> [f64; 3] {
    let a = c * (h * PI / 180.0).cos();
    let b = c * (h * PI / 180.0).sin();
    let l_ = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s = (l - 0.0894841775 * a - 1.291485548 * b).powi(3);
    [
        4.0767416621 * l_ - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l_ + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l_ - 0.7034186147 * m + 1.707614701 * s,
    ]
}

/// An OKLCH color in sRGB, its chroma lowered as far as it takes to fit (lightness and hue are kept).
pub fn oklch(l: f64, c: f64, h: f64) -> Rgb {
    let fits = |c: f64| oklch_linear(l, c, h).iter().all(|v| *v >= -1e-4 && *v <= 1.0 + 1e-4);
    let mut lo = 0.0;
    let mut hi = c;
    if !fits(hi) {
        for _ in 0..20 {
            let mid = (lo + hi) / 2.0;
            if fits(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        hi = lo;
    }
    let v = oklch_linear(l, hi, h);
    [from_linear(v[0]), from_linear(v[1]), from_linear(v[2])]
}
