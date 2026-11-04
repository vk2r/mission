use std::sync::OnceLock;

use serde_json::Value;

use crate::pyfmt;

const CATALOGUE: &str = include_str!("../../mascot/mascot.json");

const DEFAULT_BODY: &str = "cursor";
const DEFAULT_COLOUR: &str = "silver";

const FACE_CENTRE: (f64, f64) = (120.0, 122.5);

const VIEWBOX: &str = "-15 -15 258.541 258.541";
const VIEWBOX_LEFT: f64 = -15.0;
const VIEWBOX_SIZE: f64 = 258.541;

const MOUTH_STROKE: f64 = 7.5;

const STATE_FACES: [(&str, &str); 4] =
    [("working", "working"), ("waiting", "listening"), ("failed", "alerting"), ("cancelled", "bored")];

const IDENTITY_FACES: [(&str, &str); 9] = [
    ("code|coding|developer|development|engineer|engineering|build|debug|program|software", "working"),
    ("research|researcher|search|investigate|strategy|strategist|study|learn|knowledge", "searching"),
    ("marketing|growth|launch|campaign|social|sales|outreach|brand", "excited"),
    ("overnight|night|background|async|queue|batch|long-running", "drowsy"),
    ("monitor|monitoring|incident|alert|watch|status|uptime", "radar"),
    ("review|reviewer|audit|critic|critique|quality|qa|test|legal", "suspicious"),
    ("security|secure|compliance|risk|privacy|finance|financial", "scared"),
    ("design|designer|creative|brainstorm|art|illustration|music|story", "playful"),
    ("support|help|success|onboarding|coach|teacher|guide|welcome", "happy"),
];

const LEGACY_EXPRESSIONS: [(&str, &str); 10] = [
    ("deadpan", "idle"),
    ("friendly", "happy"),
    ("focused", "working"),
    ("thinking", "thinking"),
    ("excited", "excited"),
    ("sleepy", "drowsy"),
    ("surprised", "surprised"),
    ("skeptical", "suspicious"),
    ("worried", "scared"),
    ("mischievous", "playful"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Body,
    Eyes,
}

#[derive(Debug, Clone)]
pub struct Look {
    pub body: String,
    pub colour: String,
    pub expression: String,
    pub ink: String,
    pub variant: usize,
}

pub fn catalogue() -> Option<&'static Value> {
    static PARSED: OnceLock<Option<Value>> = OnceLock::new();
    PARSED
        .get_or_init(|| {
            let data: Value = serde_json::from_str(CATALOGUE).ok()?;
            let body_count = data.get("bodies").and_then(Value::as_object).map_or(0, serde_json::Map::len);
            let expression_count = data.get("expressions").and_then(Value::as_array).map_or(0, Vec::len);
            (body_count > 0 && expression_count > 0).then_some(data)
        })
        .as_ref()
}

pub fn bot_look(bot: &Value, status: &str) -> Option<Look> {
    let data = catalogue()?;

    let bodies = data.get("bodies")?.as_object()?;
    let body = bot
        .get("mascotBody")
        .and_then(Value::as_str)
        .filter(|body| bodies.contains_key(*body))
        .unwrap_or(DEFAULT_BODY)
        .to_string();

    let colours = data.get("colours")?.as_object()?;
    let colour = bot
        .get("color")
        .and_then(Value::as_str)
        .and_then(|name| colours.get(name))
        .and_then(Value::as_str)
        .or_else(|| colours.get(DEFAULT_COLOUR).and_then(Value::as_str))
        .unwrap_or_default()
        .to_string();

    let ink = if bot.get("mascotFaceColor").and_then(Value::as_str) == Some("black") {
        "#000000".to_string()
    } else {
        "#ffffff".to_string()
    };

    Some(Look { body, colour, expression: expression_for(bot, status), ink, variant: 0 })
}

fn expression_for(bot: &Value, status: &str) -> String {
    let Some(data) = catalogue() else {
        return String::new();
    };
    let faces = data.get("faces").and_then(Value::as_object);

    if let Some(wanted) = bot.get("mascotExpression").and_then(Value::as_str).filter(|name| !name.is_empty()) {
        let name =
            LEGACY_EXPRESSIONS.iter().find(|(legacy, _)| *legacy == wanted).map_or(wanted, |(_, current)| *current);
        if faces.is_some_and(|faces| faces.contains_key(name)) {
            return name.to_string();
        }
    }

    if let Some((_, face)) = STATE_FACES.iter().find(|(state, _)| *state == status) {
        return (*face).to_string();
    }

    let haystack = ["name", "title", "description"]
        .iter()
        .filter_map(|field| bot.get(*field).and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    for (words, face) in IDENTITY_FACES {
        if words.split('|').any(|word| word_in(&haystack, word)) {
            return face.to_string();
        }
    }
    "idle".to_string()
}

fn word_in(haystack: &str, word: &str) -> bool {
    let edge = |character: Option<char>| {
        character.is_none_or(|character| {
            !(character.is_alphanumeric()
                || character == '_'
                || matches!(character, '\u{300}'..='\u{36f}' | '\u{1ab0}'..='\u{1aff}' | '\u{1dc0}'..='\u{1dff}'
                    | '\u{20d0}'..='\u{20ff}' | '\u{fe20}'..='\u{fe2f}' | '\u{200c}' | '\u{200d}'))
        })
    };
    haystack.match_indices(word).any(|(at, _)| {
        edge(haystack[..at].chars().next_back()) && edge(haystack[at + word.len()..].chars().next())
    })
}

pub fn avatar_svg(look: &Look, part: Part) -> Option<String> {
    let data = catalogue()?;
    let body = data.get("bodies")?.get(&look.body)?;
    let (light, base, shade) = gradient_stops(&look.colour);
    let (rings, mouth) = face_geometry(&look.expression, look.variant)?;
    let anchor = body.get("anchor")?;

    if part == Part::Eyes {
        let paths = eye_paths(look)?;
        let mut svg =
            format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{VIEWBOX}\" width=\"96\" height=\"96\">");
        for path in paths {
            svg.push_str(&format!("<path fill=\"{}\" d=\"{path}\"/>", look.ink));
        }
        svg.push_str("</svg>");
        return Some(svg);
    }

    let point = mouth_curve(&rings, &mouth);
    let fit = body.get("fit").and_then(Value::as_str)?;
    let clip = body.get("clip").and_then(Value::as_str)?;
    let shape = body.get("body").and_then(Value::as_str)?.replace("{{GRADIENT}}", "url(#g)");

    Some(format!(
        concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{viewbox}\" width=\"96\" height=\"96\">",
            "<defs><linearGradient id=\"g\" x1=\"1\" y1=\"0\" x2=\"0\" y2=\"1\">",
            "<stop offset=\"0%\" stop-color=\"{light}\"/>",
            "<stop offset=\"55%\" stop-color=\"{base}\"/>",
            "<stop offset=\"100%\" stop-color=\"{shade}\"/>",
            "</linearGradient>",
            "<clipPath id=\"c\">{clip}</clipPath></defs>",
            "<g transform=\"{fit}\">{shape}",
            "<g clip-path=\"url(#c)\">",
            "<g transform=\"{inverse}\">",
            "<g transform=\"translate({anchor_x} {anchor_y}) scale({anchor_scale}) translate(-120 -122.5)\">",
            "<path fill=\"none\" stroke=\"{ink}\" stroke-width=\"{stroke}\" stroke-linecap=\"round\" d=\"{mouth}\"/>",
            "</g></g></g></g></svg>"
        ),
        viewbox = VIEWBOX,
        light = light,
        base = base,
        shade = shade,
        clip = clip,
        fit = fit,
        shape = shape,
        inverse = fit_inverse(fit),
        anchor_x = pyfmt::number_str(anchor.get("x")?.as_number()?),
        anchor_y = pyfmt::number_str(anchor.get("y")?.as_number()?),
        anchor_scale = pyfmt::number_str(anchor.get("scale")?.as_number()?),
        ink = look.ink,
        stroke = pyfmt::float_repr(MOUTH_STROKE),
        mouth = mouth_path(point, &mouth),
    ))
}

pub fn eye_pivot(look: &Look) -> Option<[f64; 2]> {
    let (_, centre) = eye_marks(look)?;
    Some([
        pyfmt::round_digits((centre.0 - VIEWBOX_LEFT) / VIEWBOX_SIZE, 4),
        pyfmt::round_digits((centre.1 - VIEWBOX_LEFT) / VIEWBOX_SIZE, 4),
    ])
}

fn mix(base: &str, other: &str, amount: f64) -> Option<String> {
    let a = i64::from_str_radix(base.get(1..)?, 16).ok()?;
    let b = i64::from_str_radix(other.get(1..)?, 16).ok()?;
    let mut mixed = String::from("#");
    for shift in [16, 8, 0] {
        let start = (a >> shift) & 0xFF;
        let end = (b >> shift) & 0xFF;
        let channel = pyfmt::round_digits(start as f64 + (end - start) as f64 * amount, 0) as i64;
        mixed.push_str(&format!("{:02x}", channel.clamp(0, 255)));
    }
    Some(mixed)
}

fn gradient_stops(colour: &str) -> (String, String, String) {
    let light = mix(colour, "#ffffff", 0.55).unwrap_or_else(|| colour.to_string());
    let shade = mix(colour, "#000000", 0.42).unwrap_or_else(|| colour.to_string());
    (light, colour.to_string(), shade)
}

fn centroid(points: &[(f64, f64)]) -> (f64, f64) {
    let count = points.len() as f64;
    let x: f64 = points.iter().map(|point| point.0).sum();
    let y: f64 = points.iter().map(|point| point.1).sum();
    (x / count, y / count)
}

pub fn variant_count(expression: &str) -> usize {
    catalogue()
        .and_then(|data| data.get("faces")?.get(expression)?.as_array().map(Vec::len))
        .unwrap_or(0)
}

fn face_geometry(expression: &str, variant: usize) -> Option<(Vec<Vec<(f64, f64)>>, [f64; 4])> {
    let data = catalogue()?;
    let index = data.get("faces")?.get(expression)?.get(variant)?.as_u64()? as usize;

    let geometry = data.get("expressions")?.get(index)?.as_str()?;
    let mut rings = Vec::new();
    for ring in geometry.split('|') {
        let mut points = Vec::new();
        for point in ring.split_whitespace() {
            let (x, y) = point.split_once(',')?;
            points.push((x.parse::<f64>().ok()?, y.parse::<f64>().ok()?));
        }
        rings.push(points);
    }
    if rings.len() < 2 {
        return None;
    }

    let left = centroid(&rings[0]);
    let right = centroid(&rings[1]);
    let offset = ((left.0 + right.0) / 2.0 - FACE_CENTRE.0, (left.1 + right.1) / 2.0 - FACE_CENTRE.1);
    let shifted: Vec<Vec<(f64, f64)>> =
        rings.iter().map(|ring| ring.iter().map(|point| (point.0 - offset.0, point.1 - offset.1)).collect()).collect();

    let mouth: Vec<f64> =
        data.get("mouth")?.get(index)?.as_array()?.iter().map(Value::as_f64).collect::<Option<Vec<f64>>>()?;
    let mouth: [f64; 4] = mouth.as_slice().try_into().ok()?;
    Some((shifted, mouth))
}

fn mouth_curve(rings: &[Vec<(f64, f64)>], mouth: &[f64; 4]) -> (f64, f64, f64) {
    let left = centroid(&rings[0]);
    let right = centroid(&rings[1]);
    let angle = (right.1 - left.1).atan2(right.0 - left.0);
    let radius = rings
        .iter()
        .map(|ring| {
            let top = ring.iter().map(|point| point.1).fold(f64::NEG_INFINITY, f64::max);
            let bottom = ring.iter().map(|point| point.1).fold(f64::INFINITY, f64::min);
            (top - bottom) / 2.0
        })
        .fold(f64::NEG_INFINITY, f64::max);
    let below = radius + mouth[2];
    (
        (left.0 + right.0) / 2.0 - angle.sin() * below,
        (left.1 + right.1) / 2.0 + angle.cos() * below,
        angle + mouth[3] * std::f64::consts::PI / 180.0,
    )
}

fn ring_path(points: &[(f64, f64)]) -> String {
    let mut path = String::from("M");
    for (index, (x, y)) in points.iter().enumerate() {
        if index > 0 {
            path.push('L');
        }
        path.push_str(&format!("{} {}", pyfmt::fixed_two(*x), pyfmt::fixed_two(*y)));
    }
    path.push('Z');
    path
}

fn mouth_path(point: (f64, f64, f64), mouth: &[f64; 4]) -> String {
    let (x, y, angle) = point;
    let cos = angle.cos();
    let sin = angle.sin();
    let at = |along: f64, across: f64| (x + along * cos - across * sin, y + along * sin + across * cos);

    let start = at(-mouth[0], 0.0);
    let control = at(0.0, mouth[1]);
    let end = at(mouth[0], 0.0);
    format!(
        "M{} {} Q{} {} {} {}",
        pyfmt::fixed_two(start.0),
        pyfmt::fixed_two(start.1),
        pyfmt::fixed_two(control.0),
        pyfmt::fixed_two(control.1),
        pyfmt::fixed_two(end.0),
        pyfmt::fixed_two(end.1)
    )
}

fn fit_inverse(fit: &str) -> String {
    let parsed = fit.strip_prefix("translate(").and_then(|rest| rest.strip_suffix(')')).and_then(|rest| {
        let (shift, scale) = rest.split_once(") scale(")?;
        let (x, y) = shift.split_once(' ')?;
        Some((x.parse::<f64>().ok()?, y.parse::<f64>().ok()?, scale.parse::<f64>().ok()?))
    });
    let Some((shift_x, shift_y, scale)) = parsed else {
        return "scale(1)".to_string();
    };
    format!(
        "scale({}) translate({} {})",
        pyfmt::float_repr(1.0 / scale),
        pyfmt::float_repr(-shift_x),
        pyfmt::float_repr(-shift_y)
    )
}

fn placed(point: (f64, f64), anchor: &Value) -> (f64, f64) {
    let x = anchor.get("x").and_then(Value::as_f64).unwrap_or_default();
    let y = anchor.get("y").and_then(Value::as_f64).unwrap_or_default();
    let scale = anchor.get("scale").and_then(Value::as_f64).unwrap_or_default();
    (x + scale * (point.0 - FACE_CENTRE.0), y + scale * (point.1 - FACE_CENTRE.1))
}

fn eye_marks(look: &Look) -> Option<(Vec<Vec<(f64, f64)>>, (f64, f64))> {
    let data = catalogue()?;
    let anchor = data.get("bodies")?.get(&look.body)?.get("anchor")?;
    let (rings, _) = face_geometry(&look.expression, look.variant)?;
    let moved: Vec<Vec<(f64, f64)>> =
        rings.iter().map(|ring| ring.iter().map(|point| placed(*point, anchor)).collect()).collect();
    let every: Vec<(f64, f64)> = moved.iter().flatten().copied().collect();
    if every.is_empty() {
        return None;
    }
    Some((moved, centroid(&every)))
}

fn eye_paths(look: &Look) -> Option<Vec<String>> {
    let (rings, _) = eye_marks(look)?;
    Some(rings.iter().map(|ring| ring_path(ring)).collect())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_catalogue_ships_inside_the_binary() {
        let data = catalogue().expect("catalogue");
        assert_eq!(data["bodies"].as_object().map(serde_json::Map::len), Some(10));
        assert_eq!(data["faces"].as_object().map(serde_json::Map::len), Some(39));
        assert!(data["colours"]["silver"].is_string());
    }

    #[test]
    fn a_bot_keeps_its_own_look() {
        let bot = json!({"mascotBody": "star", "color": "red", "mascotExpression": "happy"});
        let look = bot_look(&bot, "idle").expect("look");
        assert_eq!(look.body, "star");
        assert_eq!(look.colour, data_colour("red"));
        assert_eq!(look.expression, "happy");
        assert_eq!(look.ink, "#ffffff");
    }

    #[test]
    fn an_unknown_body_and_colour_fall_back_to_the_catalogue() {
        let bot = json!({"mascotBody": "not-a-body", "color": "not-a-colour"});
        let look = bot_look(&bot, "idle").expect("look");
        assert_eq!(look.body, "cursor");
        assert_eq!(look.colour, data_colour("silver"));
    }

    #[test]
    fn a_legacy_expression_is_read_as_its_current_name() {
        let bot = json!({"mascotExpression": "focused"});
        assert_eq!(bot_look(&bot, "idle").expect("look").expression, "working");
    }

    #[test]
    fn the_live_state_beats_the_role_and_the_role_beats_idle() {
        assert_eq!(bot_look(&json!({}), "waiting").expect("look").expression, "listening");
        let engineer = json!({"name": "Dev", "title": "Engineer", "description": "builds"});
        assert_eq!(bot_look(&engineer, "idle").expect("look").expression, "working");
        assert_eq!(bot_look(&json!({"name": "Nobody"}), "idle").expect("look").expression, "idle");
    }

    #[test]
    fn a_black_face_draws_its_ink_in_black() {
        assert_eq!(bot_look(&json!({"mascotFaceColor": "black"}), "idle").expect("look").ink, "#000000");
        assert_eq!(bot_look(&json!({"mascotFaceColor": "white"}), "idle").expect("look").ink, "#ffffff");
    }

    fn data_colour(name: &str) -> String {
        catalogue().expect("catalogue")["colours"][name].as_str().expect("colour").to_string()
    }
}

#[cfg(test)]
mod golden {
    use std::fmt::Write as _;
    use std::fs;
    use std::path::PathBuf;

    use super::*;

    const BODIES: usize = 10;
    const FACES: usize = 39;
    const SAMPLES: [(&str, &str, &str, &str); 10] = [
        ("cursor", "idle", "white", "body"),
        ("cursor", "working", "white", "eyes"),
        ("cursor", "scared", "black", "body"),
        ("cursor", "radar", "black", "eyes"),
        ("cursor", "excited", "white", "body"),
        ("cursor", "drowsy", "white", "eyes"),
        ("cursor", "suspicious", "black", "body"),
        ("cursor", "playful", "white", "eyes"),
        ("cursor", "happy", "white", "body"),
        ("cursor", "searching", "black", "eyes"),
    ];

    fn golden_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
    }

    fn blue() -> String {
        catalogue().expect("catalogue")["colours"]["blue"].as_str().expect("colour").to_string()
    }

    fn look(body: &str, expression: &str, ink: &str, colour: String) -> Look {
        Look { body: body.to_string(), colour, expression: expression.to_string(), ink: ink.to_string(), variant: 0 }
    }

    fn ink_of(name: &str) -> &'static str {
        if name == "black" { "#000000" } else { "#ffffff" }
    }

    fn digest(svg: &str) -> String {
        sha1_smol::Sha1::from(svg).digest().to_string()
    }

    fn drawings() -> Vec<(String, String)> {
        let catalogue = catalogue().expect("catalogue");
        let bodies: Vec<String> = catalogue["bodies"].as_object().expect("bodies").keys().cloned().collect();
        let faces: Vec<String> = catalogue["faces"].as_object().expect("faces").keys().cloned().collect();
        assert_eq!((bodies.len(), faces.len()), (BODIES, FACES));
        let mut drawings = Vec::new();
        for body in &bodies {
            for face in &faces {
                for ink in ["white", "black"] {
                    for (part_name, part) in [("body", Part::Body), ("eyes", Part::Eyes)] {
                        let svg = avatar_svg(&look(body, face, ink_of(ink), blue()), part).expect("drawn");
                        drawings.push((format!("{body}__{face}__{ink}__{}", part_name), svg));
                    }
                }
            }
        }
        for (name, hex) in catalogue["colours"].as_object().expect("colours") {
            let hex = hex.as_str().expect("colour").to_string();
            let svg = avatar_svg(&look("cursor", "idle", "#ffffff", hex), Part::Body).expect("drawn");
            drawings.push((format!("colour__{name}"), svg));
        }
        drawings
    }

    fn pivots() -> String {
        let catalogue = catalogue().expect("catalogue");
        let mut text = String::new();
        for body in catalogue["bodies"].as_object().expect("bodies").keys() {
            for face in catalogue["faces"].as_object().expect("faces").keys() {
                let pivot = eye_pivot(&look(body, face, "#ffffff", blue())).expect("pivot");
                let _ = writeln!(text, "{body} {face} {:016x} {:016x}", pivot[0].to_bits(), pivot[1].to_bits());
            }
        }
        text
    }

    #[test]
    fn every_drawing_and_pivot_matches_its_digest() {
        let drawings = drawings();
        let digests: String = drawings.iter().map(|(name, svg)| format!("{name} {}\n", digest(svg))).collect();
        let pivots = pivots();
        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            let samples = golden_dir().join("samples");
            let _ = fs::remove_dir_all(&samples);
            fs::create_dir_all(&samples).expect("samples");
            for (body, face, ink, part) in SAMPLES {
                let name = format!("{body}__{face}__{ink}__{part}");
                let svg = &drawings.iter().find(|(drawn, _)| *drawn == name).expect("sample").1;
                fs::write(samples.join(format!("{name}.svg")), svg).expect("write sample");
            }
            fs::write(golden_dir().join("digests.txt"), &digests).expect("digests");
            fs::write(golden_dir().join("pivots.txt"), &pivots).expect("pivots");
            return;
        }
        let expected = fs::read_to_string(golden_dir().join("digests.txt")).expect("digests.txt");
        let mismatches: Vec<&str> = digests.lines().zip(expected.lines()).filter(|(now, then)| now != then).map(|(now, _)| now).collect();
        assert_eq!(digests.lines().count(), expected.lines().count(), "drawing count changed: UPDATE_GOLDEN=1");
        if !mismatches.is_empty() {
            let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/golden-mismatch");
            fs::create_dir_all(&out).expect("out");
            for (name, svg) in &drawings {
                if mismatches.iter().any(|line| line.starts_with(&format!("{name} "))) {
                    fs::write(out.join(format!("{name}.svg")), svg).expect("mismatch");
                }
            }
        }
        assert!(mismatches.is_empty(), "{} drawings changed (see target/golden-mismatch): {:?}", mismatches.len(), &mismatches[..mismatches.len().min(5)]);
        let expected = fs::read_to_string(golden_dir().join("pivots.txt")).expect("pivots.txt");
        assert_eq!(pivots, expected, "eye pivots changed: UPDATE_GOLDEN=1");
        for (body, face, ink, part) in SAMPLES {
            let name = format!("{body}__{face}__{ink}__{part}");
            let sample = fs::read_to_string(golden_dir().join("samples").join(format!("{name}.svg"))).expect("sample");
            assert_eq!(drawings.iter().find(|(drawn, _)| *drawn == name).expect("sample").1, sample, "{name}");
        }
    }
}
