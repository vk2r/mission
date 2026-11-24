//! The festival costumes MausBot dresses its bots in while Halloween or Diwali is the
//! skin: twenty carved bodies, each with its own eyes and mouth, drawn as the app draws them.

use std::sync::OnceLock;

use serde_json::Value;

use super::mascot::{Look, Part};

const CATALOGUE: &str = include_str!("festival.json");

const VIEWBOX: &str = "-14 -14 268 268";
const VIEWBOX_LEFT: f64 = -14.0;
const VIEWBOX_SIZE: f64 = 268.0;
const SQUASH_Y: f64 = 150.0;

pub fn catalogue() -> Option<&'static Value> {
    static PARSED: OnceLock<Option<Value>> = OnceLock::new();
    PARSED
        .get_or_init(|| {
            let data: Value = serde_json::from_str(CATALOGUE).ok()?;
            let bodies = data.get("bodies").and_then(Value::as_object).map_or(0, serde_json::Map::len);
            let skins = data.get("skins").and_then(Value::as_object).map_or(0, serde_json::Map::len);
            (bodies > 0 && skins > 0).then_some(data)
        })
        .as_ref()
}

pub fn is_skin(name: &str) -> bool {
    catalogue().and_then(|data| data.get("skins")?.get(name)).is_some()
}

pub fn is_body(name: &str) -> bool {
    catalogue().and_then(|data| data.get("bodies")?.get(name)).is_some()
}

/// The body a bot wears under `skin`: its own when that already belongs to the festival,
/// otherwise the costume for its colour. Silver and chrome bots are never dressed.
pub fn dressed(skin: &str, body: &str, colour: &str) -> Option<String> {
    if matches!(colour, "silver" | "chrome") {
        return None;
    }
    let skin = catalogue()?.get("skins")?.get(skin)?;
    if skin.get("bodies")?.as_array()?.iter().any(|own| own.as_str() == Some(body)) {
        return None;
    }
    skin.get("costumes")?.get(colour)?.as_str().map(str::to_string)
}

#[derive(Debug, Default, Clone, Copy)]
struct Face {
    gx: f64,
    gy: f64,
    wide: f64,
    shut: f64,
    smile: f64,
}

fn face_for(expression: &str) -> Face {
    let base = Face { smile: 1.0, ..Face::default() };
    match expression {
        "surprised" | "alerting" | "scared" | "excited" | "celebrate" | "spawning" | "waking" => {
            Face { wide: 1.0, ..base }
        }
        "sleeping" | "powering-down" => Face { shut: 0.92, smile: 0.4, ..base },
        "drowsy" | "bored" => Face { shut: 0.55, smile: 0.6, ..base },
        "happy" | "laughing" | "proud" | "playful" => Face { smile: 1.5, ..base },
        "sad" => Face { smile: -0.8, gy: 0.6, ..base },
        "thinking" => Face { gx: 0.6, gy: -0.6, ..base },
        "working" | "writing" | "loading" => Face { gy: 0.5, ..base },
        _ => base,
    }
}

fn num(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0 + 0.0;
    format!("{rounded}")
}

fn float(value: &Value, name: &str) -> f64 {
    value.get(name).and_then(Value::as_f64).unwrap_or_default()
}

fn js_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

fn parse_hex(hex: &str) -> Option<(f64, f64, f64)> {
    let value = u32::from_str_radix(hex.strip_prefix('#')?, 16).ok()?;
    Some((f64::from(value >> 16 & 255), f64::from(value >> 8 & 255), f64::from(value & 255)))
}

fn hex_of(red: f64, green: f64, blue: f64) -> String {
    format!("#{:02x}{:02x}{:02x}", js_round(red) as u8, js_round(green) as u8, js_round(blue) as u8)
}

/// The saturated tone the app draws a bot's costume in (its colour, pushed a little further).
fn vivid(hex: &str) -> String {
    let Some((red, green, blue)) = parse_hex(hex) else {
        return hex.to_string();
    };
    let (red, green, blue) = (red / 255.0, green / 255.0, blue / 255.0);
    let high = red.max(green).max(blue);
    let low = red.min(green).min(blue);
    let light = (high + low) / 2.0;
    let spread = high - low;
    if spread == 0.0 {
        return hex.to_string();
    }
    let saturation = ((if light > 0.5 { spread / (2.0 - high - low) } else { spread / (high + low) }) * 1.15).min(1.0);
    let hue = (if high == red {
        (green - blue) / spread + if green < blue { 6.0 } else { 0.0 }
    } else if high == green {
        (blue - red) / spread + 2.0
    } else {
        (red - green) / spread + 4.0
    }) / 6.0;
    let upper = if light < 0.5 { light * (1.0 + saturation) } else { light + saturation - light * saturation };
    let lower = 2.0 * light - upper;
    let channel = |shift: f64| {
        let at = (shift + 1.0) % 1.0;
        let value = if at < 1.0 / 6.0 {
            lower + (upper - lower) * 6.0 * at
        } else if at < 0.5 {
            upper
        } else if at < 2.0 / 3.0 {
            lower + (upper - lower) * (2.0 / 3.0 - at) * 6.0
        } else {
            lower
        };
        value * 255.0
    };
    hex_of(channel(hue + 1.0 / 3.0), channel(hue), channel(hue - 1.0 / 3.0))
}

fn pale(hex: &str) -> String {
    let (Some((red, green, blue)), amount) = (parse_hex(hex), 0.86) else {
        return hex.to_string();
    };
    let lift = |channel: f64| channel + (255.0 - channel) * amount;
    hex_of(lift(red), lift(green), lift(blue))
}

fn paints(body: &Value, colour: &str) -> (String, String) {
    let accent = vivid(colour);
    match body.get("paint").and_then(Value::as_str) {
        Some("bone") => (body.get("bone").and_then(Value::as_str).unwrap_or("#f1ede6").to_string(), accent),
        Some("pale") => (pale(colour), accent),
        _ => (accent.clone(), accent),
    }
}

struct Drawn {
    squash: String,
    eyes: [[f64; 4]; 2],
    turn: f64,
    smile: (String, f64, f64, f64),
    open: (f64, f64, f64, f64, f64),
    centre: (f64, f64),
}

fn geometry(eyes: &Value, face: Face) -> Drawn {
    let (cx, cy, dx) = (float(eyes, "cx"), float(eyes, "cy"), float(eyes, "dx"));
    let (width, height, round) = (float(eyes, "w"), float(eyes, "h"), float(eyes, "round"));
    let glance = face.gx.abs();
    let tilt = -face.gx * 9.0;
    let squeeze = 1.0 - 0.06 * face.wide;
    let stretch = 1.0 + 0.09 * face.wide;
    let apart = dx * (1.0 - 0.1 * glance) * (1.0 + 0.14 * face.wide);
    let left = cx + face.gx * dx * 1.05;
    let top = cy + face.gy * 16.0 - face.wide * height * 0.42;
    let mut wide_by = width * (1.0 + 0.05 * glance);
    let mut high_by = height * (1.0 - 0.3 * glance);
    wide_by += (round - wide_by) * face.wide;
    high_by += (round * 1.08 - high_by) * face.wide;
    high_by *= 1.0 - 0.9 * face.shut;
    let turn = -face.gx * 18.0 * (1.0 - face.wide);

    let eye = |side: f64| {
        let shrink = 1.0 - 0.1 * (side * -face.gx).max(0.0);
        let w = wide_by * shrink;
        let h = (high_by * shrink).max(0.5);
        let at = left + side * apart;
        [at - w / 2.0, top - h / 2.0, w, h]
    };

    let reach = dx * 0.46;
    let half = reach * (1.0 - 0.25 * glance);
    let bend = reach * 0.5 * face.smile;
    let line = eyes.get("mouthY").and_then(Value::as_f64).unwrap_or(cy + height * 0.5 + dx * 0.42) + face.gy * 10.0;
    let smile = format!(
        "M{} {} Q{} {} {} {}",
        num(left - half),
        num(line - bend * 0.35),
        num(left),
        num(line + bend),
        num(left + half),
        num(line - bend * 0.35)
    );

    let (sin, cos) = (tilt.to_radians().sin(), tilt.to_radians().cos());
    let (offset_x, offset_y) = (squeeze * (left - 120.0), stretch * (top - SQUASH_Y));
    let centre = (120.0 + cos * offset_x - sin * offset_y, SQUASH_Y + sin * offset_x + cos * offset_y);

    Drawn {
        squash: format!(
            "translate(120 {SQUASH_Y}) rotate({}) scale({} {}) translate(-120 -{SQUASH_Y})",
            num(tilt),
            num(squeeze),
            num(stretch),
        ),
        eyes: [eye(-1.0), eye(1.0)],
        turn,
        smile: (smile, left, line, turn * 0.6),
        open: (left, line + reach * 0.1, reach * 0.5 * face.wide, reach * 0.62 * face.wide, face.wide),
        centre,
    }
}

fn body_of(look: &Look) -> Option<&'static Value> {
    catalogue()?.get("bodies")?.get(&look.body)
}

pub fn eye_pivot(look: &Look) -> Option<[f64; 2]> {
    let body = body_of(look)?;
    let drawn = geometry(body.get("eyes")?, face_for(&look.expression));
    let round = |value: f64| (value * 10_000.0).round() / 10_000.0;
    Some([round((drawn.centre.0 - VIEWBOX_LEFT) / VIEWBOX_SIZE), round((drawn.centre.1 - VIEWBOX_LEFT) / VIEWBOX_SIZE)])
}

fn ink_of(look: &Look, own: &Value, name: &str) -> String {
    if look.ink_set { look.ink.clone() } else { own.get(name).and_then(Value::as_str).unwrap_or("#0d0a10").to_string() }
}

pub fn avatar_svg(look: &Look, part: Part) -> Option<String> {
    let body = body_of(look)?;
    let eyes = body.get("eyes")?;
    let drawn = geometry(eyes, face_for(&look.expression));
    let open = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{VIEWBOX}\" width=\"96\" height=\"96\">");

    if part == Part::Eyes {
        let clip = body.get("clip")?.as_str()?;
        let ink = ink_of(look, eyes, "ink");
        let mut svg = format!("{open}<defs><clipPath id=\"c\"><path d=\"{clip}\"/></clipPath></defs>");
        svg.push_str(&format!("<g transform=\"{}\"><g clip-path=\"url(#c)\">", drawn.squash));
        for [x, y, width, height] in drawn.eyes {
            let radius = width.min(height) / 2.0;
            svg.push_str(&format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{ink}\" transform=\"rotate({} {} {})\"/>",
                num(x),
                num(y),
                num(width),
                num(height),
                num(radius),
                num(drawn.turn),
                num(x + width / 2.0),
                num(y + height / 2.0),
            ));
        }
        svg.push_str("</g></g></svg>");
        return Some(svg);
    }

    let (body_paint, accent) = paints(body, &look.colour);
    let mut svg = format!("{open}<g transform=\"{}\">", drawn.squash);
    for part in body.get("parts")?.as_array()? {
        let fill = match part.get("fill")?.as_str()? {
            "body" => body_paint.as_str(),
            "accent" => accent.as_str(),
            literal => literal,
        };
        svg.push_str(&format!("<path d=\"{}\" fill=\"{fill}\"/>", part.get("d")?.as_str()?));
    }
    if eyes.get("mouth").and_then(Value::as_bool) == Some(true) {
        let ink = ink_of(look, eyes, "mouthInk");
        let (curve, left, line, turn) = &drawn.smile;
        let (centre_x, centre_y, across, down, wide) = drawn.open;
        if wide < 1.0 {
            svg.push_str(&format!(
                "<path d=\"{curve}\" transform=\"rotate({} {} {})\" opacity=\"{}\" fill=\"none\" stroke=\"{ink}\" stroke-width=\"{}\" stroke-linecap=\"round\"/>",
                num(*turn),
                num(*left),
                num(*line),
                num(1.0 - wide),
                num(float(eyes, "w") * 0.36),
            ));
        }
        if wide > 0.0 {
            svg.push_str(&format!(
                "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\" opacity=\"{}\" fill=\"{ink}\"/>",
                num(centre_x),
                num(centre_y),
                num(across),
                num(down),
                num(wide),
            ));
        }
    }
    svg.push_str("</g></svg>");
    Some(svg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look(body: &str, expression: &str, colour: &str) -> Look {
        Look {
            body: body.to_string(),
            colour: colour.to_string(),
            expression: expression.to_string(),
            ink: "#ffffff".to_string(),
            ink_set: false,
            variant: 0,
            paint: Default::default(),
        }
    }

    #[test]
    fn the_catalogue_ships_twenty_bodies_for_two_skins() {
        let data = catalogue().expect("catalogue");
        assert_eq!(data["bodies"].as_object().map(serde_json::Map::len), Some(20));
        assert!(is_skin("halloween") && is_skin("diwali"));
        assert!(!is_skin("graphite") && !is_skin("system"));
        for skin in ["halloween", "diwali"] {
            for body in data["skins"][skin]["bodies"].as_array().expect("bodies") {
                assert!(is_body(body.as_str().expect("id")));
            }
            assert_eq!(data["skins"][skin]["costumes"].as_object().map(serde_json::Map::len), Some(10));
        }
    }

    #[test]
    fn a_bot_is_dressed_by_its_colour_and_silver_stays_plain() {
        assert_eq!(dressed("halloween", "cursor", "orange").as_deref(), Some("pumpkin"));
        assert_eq!(dressed("halloween", "star", "purple").as_deref(), Some("ghost"));
        assert_eq!(dressed("diwali", "blob", "red").as_deref(), Some("rocket"));
        assert_eq!(dressed("diwali", "cursor", "orange").as_deref(), Some("laddu"));
        assert_eq!(dressed("diwali", "cursor", "silver"), None);
        assert_eq!(dressed("diwali", "cursor", "chrome"), None);
        assert_eq!(dressed("halloween", "pumpkin", "blue"), None, "a bot already in this skin's costume keeps it");
        assert_eq!(dressed("graphite", "cursor", "orange"), None);
        assert_eq!(dressed("halloween", "cursor", "not-a-colour"), None);
    }

    #[test]
    fn every_costume_draws_every_face_in_both_parts() {
        let data = catalogue().expect("catalogue");
        for body in data["bodies"].as_object().expect("bodies").keys() {
            for face in ["idle", "working", "surprised", "sleeping", "happy", "sad", "thinking", "scared"] {
                let drawn = look(body, face, "#E78531");
                for part in [Part::Body, Part::Eyes] {
                    let svg = avatar_svg(&drawn, part).unwrap_or_else(|| panic!("{body} {face}"));
                    assert!(svg.starts_with("<svg ") && svg.ends_with("</svg>"), "{body} {face}");
                    assert!(!svg.contains("NaN") && !svg.contains("inf"), "{body} {face}: {svg}");
                }
                let pivot = eye_pivot(&drawn).expect("pivot");
                assert!(pivot.iter().all(|value| (0.0..=1.0).contains(value)), "{body} {face}: {pivot:?}");
            }
        }
    }

    #[test]
    fn a_costume_takes_the_bot_colour_and_its_own_ink_unless_the_bot_chose_one() {
        let orange = avatar_svg(&look("pumpkin", "idle", "#E78531"), Part::Body).expect("body");
        assert!(orange.contains("fill=\"#3d3d3d\"") && !orange.contains("{{"), "{orange}");
        let plain = avatar_svg(&look("pumpkin", "idle", "#E78531"), Part::Eyes).expect("eyes");
        assert!(plain.contains("fill=\"#0d0a10\""));
        let white = Look { ink: "#ffffff".into(), ink_set: true, ..look("pumpkin", "idle", "#E78531") };
        assert!(avatar_svg(&white, Part::Eyes).expect("eyes").contains("fill=\"#ffffff\""));
        let skull = avatar_svg(&look("skull", "idle", "#0EA5C6"), Part::Body).expect("body");
        assert!(skull.contains("#f1ede6") && !skull.contains("<ellipse") && !skull.contains("stroke-linecap"));
    }

    #[test]
    fn a_wide_face_opens_the_mouth_and_a_calm_one_smiles() {
        let calm = avatar_svg(&look("cat", "idle", "#377FE6"), Part::Body).expect("body");
        assert!(calm.contains("stroke-linecap") && !calm.contains("<ellipse"));
        let startled = avatar_svg(&look("cat", "surprised", "#377FE6"), Part::Body).expect("body");
        assert!(startled.contains("<ellipse") && !startled.contains("stroke-linecap"));
    }

    #[test]
    fn the_vivid_tone_matches_the_apps_own() {
        assert_eq!(vivid("#808080"), "#808080");
        assert_eq!(pale("#000000"), "#dbdbdb");
        let tone = vivid("#E78531");
        assert!(tone.starts_with('#') && tone.len() == 7);
        assert_ne!(tone, "#e78531");
    }
}
