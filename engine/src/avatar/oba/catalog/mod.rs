//! The bodies MausBot draws with this engine: nine shapes and twenty-one Open Bot Avatar objects,
//! identical to the app's tables (`SHAPE_BODIES` and `OBJECT_BODIES`,
//! `packages/phone-protocol/src/object-bodies.ts`), plus how the app fills them and inks their eyes.
//! The cursor and the festival costumes are not here; they keep their own renderers.
//!
//! Nothing outside `avatar::oba` is referenced from here.

pub mod objects;
pub mod shapes;
#[cfg(test)]
mod tests;

use std::sync::OnceLock;

use super::color::{js_round, parse_hex};
use super::types::{AvatarDefinition, AvatarEyes};

use objects::object_bodies;
use shapes::shape_bodies;

/// The shape ids, in picker order (`MASCOT_BODY_IDS` without "cursor", object-bodies.ts's
/// `SHAPE_BODIES`).
pub const SHAPE_BODY_IDS: [&str; 9] =
    ["blob", "circle", "squircle", "capsule", "drop", "shield", "hexagon", "diamond", "star"];

/// `shape()` in object-bodies.ts:143: a definition from one path and its eyes (x, y, spacing, scale).
fn definition(id: &str, name: &str, path: &str, [x, y, spacing, scale]: [f64; 4]) -> AvatarDefinition {
    AvatarDefinition {
        version: 1,
        id: id.into(),
        name: name.into(),
        body_path: path.into(),
        eyes: AvatarEyes { x, y, spacing, scale },
    }
}

struct Tables {
    objects: Vec<AvatarDefinition>,
    shapes: Vec<AvatarDefinition>,
}

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| Tables { objects: object_bodies(), shapes: shape_bodies() })
}

/// The engine definition a body is drawn from, or `None` for the cursor and the festival bodies
/// (`engineBody`, object-bodies.ts:158). Objects are searched first, then shapes; ids are exact.
pub fn engine_body(id: &str) -> Option<&'static AvatarDefinition> {
    let t = tables();
    t.objects.iter().chain(&t.shapes).find(|one| one.id == id)
}

/// MausBot's colour names and hexes (`MAUS_COLORS`, src/lib/mascot.ts:51).
pub const MAUS_COLORS: [(&str, &str); 12] = [
    ("green", "#009957"),
    ("blue", "#377FE6"),
    ("red", "#D94B52"),
    ("orange", "#E78531"),
    ("purple", "#8057C8"),
    ("cyan", "#0EA5C6"),
    ("pink", "#D84F8B"),
    ("yellow", "#D8A729"),
    ("teal", "#01A492"),
    ("coral", "#E5634E"),
    ("silver", "#A8ADB4"),
    ("chrome", "#B3BAC4"),
];

/// `MAUS_COLORS[color] ?? MAUS_COLORS.green` (Avatar.tsx:141): an unknown name is green.
fn maus_color(color: &str) -> &'static str {
    let find = |name: &str| MAUS_COLORS.iter().find(|(n, _)| *n == name).map(|(_, hex)| *hex);
    find(color).or_else(|| find("green")).unwrap_or("#009957")
}

/// The flat style wants more punch than a gradient's mid tone: saturation lifted 15%
/// (`vivid`, src/components/CarvedAvatar.tsx:123). Greys come back as they were given.
pub fn vivid(hex: &str) -> String {
    let Some([r8, g8, b8]) = parse_hex(hex) else { return hex.to_string() };
    let (r, g, b) = (r8 / 255.0, g8 / 255.0, b8 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return hex.to_string();
    }
    let s = ((if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) }) * 1.15).min(1.0);
    let h = (if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    }) / 6.0;
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let ch = |x: f64| {
        let t = (x + 1.0) % 1.0;
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    let byte = |v: f64| format!("{:02x}", js_round(v * 255.0) as u32);
    format!("#{}{}{}", byte(ch(h + 1.0 / 3.0)), byte(ch(h)), byte(ch(h - 1.0 / 3.0)))
}

/// A bot's own body colour when it picked one: `#rrggbb` in any case, returned lowercase
/// (`customFill` in Avatar.tsx:71 with `mascotBodyColorSchema`, packages/phone-protocol/src/bot-avatar.ts:8).
pub fn custom_fill(value: Option<&str>) -> Option<String> {
    let v = value?;
    let b = v.as_bytes();
    (b.len() == 7 && b[0] == b'#' && b[1..].iter().all(u8::is_ascii_hexdigit)).then(|| v.to_ascii_lowercase())
}

/// The flat body fill: `customFill(bodyColor) ?? vivid(MAUS_COLORS[color] ?? MAUS_COLORS.green)`
/// (Avatar.tsx:141).
pub fn body_fill(body_color: Option<&str>, color: &str) -> String {
    custom_fill(body_color).unwrap_or_else(|| vivid(maus_color(color)))
}

/// The eye ink for a face colour: "white" and "black" only, anything else leaves the engine's
/// automatic contrast ink (`mascotFaceInk`, bot-avatar.ts:13; used as `eyeColor` in ObjectAvatar.tsx:75).
pub fn face_ink(face_color: Option<&str>) -> Option<&'static str> {
    match face_color {
        Some("white") => Some("#ffffff"),
        Some("black") => Some("#000000"),
        _ => None,
    }
}
