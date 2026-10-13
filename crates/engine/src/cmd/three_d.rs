//! Format Shape ▸ 3-D Format and 3-D Rotation: a shape's `a:scene3d` / `a:sp3d`, kept as XML on
//! the model (`Effects::raw3d`), read into fields, changed, and written back.

use serde_json::Value;

/// Camera presets PowerPoint offers (3-D Rotation ▸ Presets).
pub const CAMERAS: &[&str] = &[
    "orthographicFront",
    "isometricTopUp",
    "isometricTopDown",
    "isometricBottomUp",
    "isometricBottomDown",
    "isometricLeftUp",
    "isometricLeftDown",
    "isometricRightUp",
    "isometricRightDown",
    "obliqueTopLeft",
    "obliqueTopRight",
    "obliqueBottomLeft",
    "obliqueBottomRight",
    "perspectiveFront",
    "perspectiveLeft",
    "perspectiveRight",
    "perspectiveAbove",
    "perspectiveBelow",
    "perspectiveAboveLeftFacing",
    "perspectiveAboveRightFacing",
    "perspectiveContrastingLeftFacing",
    "perspectiveContrastingRightFacing",
    "perspectiveHeroicLeftFacing",
    "perspectiveHeroicRightFacing",
    "perspectiveHeroicExtremeLeftFacing",
    "perspectiveHeroicExtremeRightFacing",
    "perspectiveRelaxed",
    "perspectiveRelaxedModerately",
];

/// Bevel presets (`ST_BevelPresetType`).
pub const BEVELS: &[&str] =
    &["circle", "relaxedInset", "cross", "coolSlant", "angle", "softRound", "convex", "slope", "divot", "riblet", "hardEdge", "artDeco"];

/// Materials (`ST_PresetMaterialType`).
pub const MATERIALS: &[&str] =
    &["warmMatte", "matte", "plastic", "metal", "dkEdge", "softEdge", "flat", "legacyWireframe", "powder", "translucentPowder", "clear", "softmetal"];

/// Light rigs (`ST_LightRigType`) PowerPoint offers.
pub const LIGHTS: &[&str] = &[
    "threePt",
    "balanced",
    "soft",
    "harsh",
    "flood",
    "contrasting",
    "morning",
    "sunrise",
    "sunset",
    "chilly",
    "freezing",
    "flat",
    "twoPt",
    "glow",
    "brightRoom",
];

/// One bevel: preset, width and height in points.
#[derive(Clone, Debug, PartialEq)]
pub struct Bevel {
    pub preset: String,
    pub w: f64,
    pub h: f64,
}

/// The 3-D settings of a shape.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape3d {
    pub camera: String,
    /// Camera rotation (X, Y, Z in degrees, as PowerPoint shows them), when set.
    pub rot: Option<(f64, f64, f64)>,
    pub light: String,
    pub light_dir: String,
    pub top: Option<Bevel>,
    pub bottom: Option<Bevel>,
    /// Depth (extrusion), contour width and distance from ground, in points.
    pub depth: f64,
    pub contour: f64,
    pub z: f64,
    pub material: String,
    /// `a:extrusionClr` / `a:contourClr` XML, kept as is.
    pub colors: String,
}

impl Default for Shape3d {
    fn default() -> Self {
        Shape3d {
            camera: "orthographicFront".into(),
            rot: None,
            light: "threePt".into(),
            light_dir: "t".into(),
            top: None,
            bottom: None,
            depth: 0.0,
            contour: 0.0,
            z: 0.0,
            material: "warmMatte".into(),
            colors: String::new(),
        }
    }
}

/// The start tags of `raw` as (local name, tag text), in order.
fn tags(raw: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let mut rest = raw;
    while let Some(i) = rest.find('<') {
        let Some(t) = rest.get(i + 1..) else { break };
        let end = t.find('>').unwrap_or(t.len());
        let tag = t.get(..end).unwrap_or("");
        rest = t.get(end..).unwrap_or("");
        if tag.starts_with('/') || tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        let name = tag.split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("");
        let local = name.rsplit(':').next().unwrap_or(name);
        out.push((local, tag));
    }
    out
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let s = tag.find(&key)? + key.len();
    let rest = tag.get(s..)?;
    rest.get(..rest.find('"')?)
}

fn num(tag: &str, name: &str) -> Option<f64> {
    attr(tag, name).and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite())
}

const EMU: f64 = 12700.0;
const ANGLE: f64 = 60000.0;

/// An `a:`-prefixed element `local` (with its children) cut from `raw`, if there is one.
fn element(raw: &str, local: &str) -> Option<String> {
    let open = format!("<a:{local}");
    let close = format!("</a:{local}>");
    let i = raw.find(&open)?;
    let j = raw.get(i..)?.find(&close)? + i + close.len();
    raw.get(i..j).map(str::to_string)
}

impl Shape3d {
    /// Reads kept 3-D XML; unknown parts take their defaults.
    pub fn parse(raw: &str) -> Shape3d {
        let mut s = Shape3d::default();
        let mut in_camera = false;
        for (name, tag) in tags(raw) {
            match name {
                "camera" => {
                    s.camera = attr(tag, "prst").unwrap_or("orthographicFront").to_string();
                    in_camera = !tag.ends_with('/');
                }
                "rot" if in_camera => {
                    let d = |n| num(tag, n).unwrap_or(0.0) / ANGLE;
                    s.rot = Some((d("lon"), d("lat"), d("rev")));
                    in_camera = false;
                }
                "lightRig" => {
                    in_camera = false;
                    s.light = attr(tag, "rig").unwrap_or("threePt").to_string();
                    s.light_dir = attr(tag, "dir").unwrap_or("t").to_string();
                }
                "sp3d" => {
                    in_camera = false;
                    s.depth = num(tag, "extrusionH").unwrap_or(0.0) / EMU;
                    s.contour = num(tag, "contourW").unwrap_or(0.0) / EMU;
                    s.z = num(tag, "z").unwrap_or(0.0) / EMU;
                    s.material = attr(tag, "prstMaterial").unwrap_or("warmMatte").to_string();
                }
                "bevelT" | "bevelB" => {
                    let b = Bevel {
                        preset: attr(tag, "prst").unwrap_or("circle").to_string(),
                        w: num(tag, "w").unwrap_or(76200.0) / EMU,
                        h: num(tag, "h").unwrap_or(76200.0) / EMU,
                    };
                    if name == "bevelT" {
                        s.top = Some(b);
                    } else {
                        s.bottom = Some(b);
                    }
                }
                _ => {}
            }
        }
        s.colors = ["extrusionClr", "contourClr"].iter().filter_map(|n| element(raw, n)).collect();
        s
    }

    /// Whether this is a flat shape with no 3-D at all.
    pub fn is_flat(&self) -> bool {
        self.camera == "orthographicFront"
            && self.rot.is_none_or(|(x, y, z)| x == 0.0 && y == 0.0 && z == 0.0)
            && self.top.is_none()
            && self.bottom.is_none()
            && self.depth == 0.0
            && self.contour == 0.0
            && self.z == 0.0
    }

    /// The `a:scene3d` and `a:sp3d` XML, or `None` for a flat shape.
    pub fn to_xml(&self) -> Option<String> {
        if self.is_flat() {
            return None;
        }
        let e = |pt: f64| (pt * EMU).round() as i64;
        let mut x = format!("<a:scene3d><a:camera prst=\"{}\"", self.camera);
        match self.rot {
            Some((rx, ry, rz)) => {
                let a = |d: f64| (d.rem_euclid(360.0) * ANGLE).round() as i64 % 21_600_000;
                x += &format!("><a:rot lat=\"{}\" lon=\"{}\" rev=\"{}\"/></a:camera>", a(ry), a(rx), a(rz));
            }
            None => x += "/>",
        }
        x += &format!("<a:lightRig rig=\"{}\" dir=\"{}\"/></a:scene3d><a:sp3d", self.light, self.light_dir);
        if self.z != 0.0 {
            x += &format!(" z=\"{}\"", e(self.z));
        }
        if self.depth > 0.0 {
            x += &format!(" extrusionH=\"{}\"", e(self.depth));
        }
        if self.contour > 0.0 {
            x += &format!(" contourW=\"{}\"", e(self.contour));
        }
        if self.material != "warmMatte" {
            x += &format!(" prstMaterial=\"{}\"", self.material);
        }
        x += ">";
        for (tag, b) in [("bevelT", &self.top), ("bevelB", &self.bottom)] {
            if let Some(b) = b {
                x += &format!("<a:{tag} w=\"{}\" h=\"{}\" prst=\"{}\"/>", e(b.w), e(b.h), b.preset);
            }
        }
        x += &self.colors;
        x += "</a:sp3d>";
        Some(x)
    }

    /// Applies `{camera?, rotX?, rotY?, rotZ?, z?, bevelTop?, bevelTopW?, bevelTopH?, bevelBottom?,
    /// bevelBottomW?, bevelBottomH?, depth?, contour?, material?, lighting?, lightAngle?}`.
    pub fn apply(&mut self, p: &Value) {
        let f = |k: &str| p.get(k).and_then(Value::as_f64).filter(|v| v.is_finite());
        let pick = |k: &str, list: &[&str]| p.get(k).and_then(Value::as_str).filter(|v| list.contains(v)).map(str::to_string);
        if let Some(c) = pick("camera", CAMERAS) {
            self.camera = c;
            // A preset sets its own angles.
            self.rot = None;
        }
        let (rx, ry, rz) = (f("rotX"), f("rotY"), f("rotZ"));
        if rx.is_some() || ry.is_some() || rz.is_some() {
            let (ox, oy, oz) = self.rot.unwrap_or((0.0, 0.0, 0.0));
            let d = |v: Option<f64>, o: f64| v.map_or(o, |v| v.rem_euclid(360.0));
            self.rot = Some((d(rx, ox), d(ry, oy), d(rz, oz)));
        }
        if let Some(v) = f("z") {
            self.z = v.clamp(-1584.0, 1584.0);
        }
        for (key, slot) in [("bevelTop", &mut self.top), ("bevelBottom", &mut self.bottom)] {
            match p.get(key).and_then(Value::as_str) {
                Some("none") => *slot = None,
                Some(b) if BEVELS.contains(&b) => {
                    let old = slot.take().unwrap_or(Bevel { preset: String::new(), w: 6.0, h: 6.0 });
                    *slot = Some(Bevel { preset: b.to_string(), ..old });
                }
                _ => {}
            }
            if let Some(b) = slot.as_mut() {
                if let Some(w) = f(&format!("{key}W")) {
                    b.w = w.clamp(0.0, 1584.0);
                }
                if let Some(h) = f(&format!("{key}H")) {
                    b.h = h.clamp(0.0, 1584.0);
                }
            }
        }
        if let Some(v) = f("depth") {
            self.depth = v.clamp(0.0, 1584.0);
        }
        if let Some(v) = f("contour") {
            self.contour = v.clamp(0.0, 1584.0);
        }
        if let Some(m) = pick("material", MATERIALS) {
            self.material = m;
        }
        if let Some(l) = pick("lighting", LIGHTS) {
            self.light = l;
        }
    }
}

/// Sets a shape's 3-D settings on its effects, keeping `bevel` (the top bevel's preset) in step.
pub fn store(e: &mut deckcraft_model::Effects, s: &Shape3d) {
    e.raw3d = s.to_xml();
    e.bevel = if e.raw3d.is_some() { s.top.as_ref().map(|b| b.preset.clone()) } else { None };
}
