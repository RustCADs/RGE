// SPLIT-EXEMPTION: cohesive hand-rolled SVG mini-parser (path-d tokenizer
// with M/L/H/V/C/S/Q/T/A/Z including arc-to-cubic conversion) + tiny-skia
// rasterizer + currentColor substitution. Splitting introduces
// parser/rasterizer interface friction without reducing complexity.
// Avoiding resvg/usvg keeps the crate offline-buildable per W06 design.
// Per PLAN.md §1.3 Rule 3 (1049 lines vs 1000-line hard cap).

//! Tint and rasterize monochrome SVG icons.
//!
//! Lucide-style icons are 24x24 viewBox documents containing a small
//! number of `<path>` / `<line>` / `<circle>` / `<rect>` / `<polyline>`
//! / `<polygon>` elements that all use `stroke="currentColor"`. Tinting
//! such an icon means substituting the requested color for
//! `currentColor` and rasterizing the result.
//!
//! This module deliberately avoids `resvg` / `usvg`: the icon subset we
//! support is narrow enough that a hand-rolled parser keeps the crate
//! self-contained, fast to compile, and offline-buildable. If RGE ever
//! needs to render arbitrary user SVG it should pull `resvg` then; for
//! the editor's icon set, this is sufficient and ~10x faster to compile.
//!
//! # Pipeline
//!
//! 1. [`apply_tint`] does string substitution on the raw SVG bytes,
//!    returning new SVG text with `currentColor` (and the literal Lucide
//!    default `#000`) replaced by the target color.
//! 2. [`rasterize`] parses that tinted SVG with a minimal pull-style
//!    parser and renders it onto a [`tiny_skia::Pixmap`], returning a
//!    [`RasterIcon`] containing premultiplied-alpha RGBA8 pixels and
//!    dimensions, ready for `egui::ColorImage::from_rgba_premultiplied`.
//!
//! A test-only `pixels_unmultiplied` helper is also exposed so that
//! callers who want to feed `egui::ColorImage::from_rgba_unmultiplied`
//! can do so without re-multiplying themselves.

// Graphics math: float-vs-zero comparisons and f32<->usize/u8 casts are
// inherent to the rasterization pipeline. The casts here are bounded
// (pixel coordinates, segment counts, color channels) so the pedantic
// truncation/precision warnings would all need #[allow] at every site.
// Allowing at module scope is clearer than peppering local annotations.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::field_reassign_with_default
)]

use std::fmt::Write as _;

use crate::ui_theme_stub::Color;

/// Output of rasterization: width × height RGBA8 pixels with
/// premultiplied alpha (matches `tiny_skia` and `egui` native).
#[derive(Debug, Clone)]
pub struct RasterIcon {
    /// Pixel width in physical pixels (post-DPR scaling, if any).
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// `width * height * 4` bytes, RGBA premultiplied.
    pub pixels: Vec<u8>,
}

impl RasterIcon {
    /// Return RGBA pixels with **un-premultiplied** alpha. Useful for
    /// `egui::ColorImage::from_rgba_unmultiplied`. Allocates a fresh
    /// vector — the canonical storage stays premultiplied.
    #[must_use]
    pub fn pixels_unmultiplied(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len());
        for chunk in self.pixels.chunks_exact(4) {
            let (r, g, b, a) = (chunk[0], chunk[1], chunk[2], chunk[3]);
            if a == 0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                let af = f32::from(a) / 255.0;
                out.push(((f32::from(r) / af).round().min(255.0)) as u8);
                out.push(((f32::from(g) / af).round().min(255.0)) as u8);
                out.push(((f32::from(b) / af).round().min(255.0)) as u8);
                out.push(a);
            }
        }
        out
    }
}

/// Errors from rasterize.
#[derive(Debug, thiserror::Error)]
pub enum TintError {
    /// Pixmap allocation failed (zero or absurd dimensions).
    #[error("invalid output dimensions: {w}x{h}")]
    BadDimensions {
        /// Requested width.
        w: u32,
        /// Requested height.
        h: u32,
    },
    /// SVG parse failed (malformed, unsupported feature, etc.).
    #[error("SVG parse error: {0}")]
    Parse(String),
}

/// Substitute `currentColor` (and a few common literal-color fallbacks)
/// in the SVG source with the target color.
///
/// Returns a fresh `String` rather than mutating in place — the caller
/// typically wants to keep the un-tinted source for re-tinting on theme
/// swap.
#[must_use]
pub fn apply_tint(svg: &str, color: Color) -> String {
    let hex = color_to_hex(color);
    let mut out = String::with_capacity(svg.len() + 32);
    let mut rest = svg;
    // Replace `currentColor` (case-insensitive on the C/c first letter
    // — Lucide ships lowercase but be lenient).
    while let Some(idx) = find_ci(rest, "currentcolor") {
        out.push_str(&rest[..idx]);
        out.push_str(&hex);
        rest = &rest[idx + "currentcolor".len()..];
    }
    out.push_str(rest);

    // Some hand-authored icons use literal "#000" / "#000000" / "black"
    // for the stroke. Replace those too — but only when they appear
    // inside an attribute value (i.e. wrapped in quotes), to avoid
    // clobbering style nodes.
    let target_d = format!("\"{hex}\"");
    let target_s = format!("'{hex}'");
    out = out.replace("\"#000\"", &target_d);
    out = out.replace("\"#000000\"", &target_d);
    out = out.replace("\"black\"", &target_d);
    out = out.replace("'#000'", &target_s);
    out = out.replace("'black'", &target_s);
    out
}

fn find_ci(haystack: &str, needle_lower: &str) -> Option<usize> {
    let nb = needle_lower.as_bytes();
    let hb = haystack.as_bytes();
    if hb.len() < nb.len() {
        return None;
    }
    'outer: for i in 0..=(hb.len() - nb.len()) {
        for j in 0..nb.len() {
            if hb[i + j].to_ascii_lowercase() != nb[j] {
                continue 'outer;
            }
        }
        return Some(i);
    }
    None
}

fn color_to_hex(c: Color) -> String {
    let mut s = String::with_capacity(7);
    let _ = write!(s, "#{:02x}{:02x}{:02x}", c.r, c.g, c.b);
    s
}

// -------------------------------------------------------------------
// Minimal SVG parser. Only handles the subset Lucide actually emits:
//
//   <svg ... viewBox="0 0 24 24" stroke-width="2" ...>
//     <path d="..." />
//     <line  x1=".." y1=".." x2=".." y2=".." />
//     <circle cx=".." cy=".." r=".." />
//     <rect   x=".." y=".." width=".." height=".." rx=".." />
//     <polyline points="x,y x,y ..." />
//     <polygon  points="x,y x,y ..." />
//   </svg>
//
// Stroke color is extracted from the root element (or per-element
// override) — after `apply_tint` it's a hex literal so we just have to
// detect a `stroke=` attribute that isn't `none`.
// -------------------------------------------------------------------

/// Render a tinted SVG to a [`RasterIcon`] of the requested pixel size.
///
/// `target_w` / `target_h` are physical pixel dimensions. The SVG's
/// own `viewBox` is used as the source coordinate system; we scale to
/// fit inside the requested size preserving aspect ratio.
///
/// # Errors
/// - [`TintError::BadDimensions`] if width or height is `0`.
/// - [`TintError::Parse`] if the SVG can't be parsed.
pub fn rasterize(svg: &str, target_w: u32, target_h: u32) -> Result<RasterIcon, TintError> {
    if target_w == 0 || target_h == 0 || target_w > 8192 || target_h > 8192 {
        return Err(TintError::BadDimensions {
            w: target_w,
            h: target_h,
        });
    }
    let parsed = parse_svg(svg)?;
    let mut pixmap =
        tiny_skia::Pixmap::new(target_w, target_h).ok_or(TintError::BadDimensions {
            w: target_w,
            h: target_h,
        })?;

    // Compute scale: fit viewBox into target preserving aspect.
    let (vbw, vbh) = (parsed.view_w.max(1.0), parsed.view_h.max(1.0));
    let sx = target_w as f32 / vbw;
    let sy = target_h as f32 / vbh;
    let s = sx.min(sy);
    let dx = (target_w as f32 - vbw * s) * 0.5 - parsed.view_x * s;
    let dy = (target_h as f32 - vbh * s) * 0.5 - parsed.view_y * s;
    let xform = tiny_skia::Transform::from_row(s, 0.0, 0.0, s, dx, dy);

    let stroke_w = parsed.stroke_width.unwrap_or(2.0);

    for shape in &parsed.shapes {
        let (paint_color, is_stroke) = match shape.paint {
            ShapePaint::Stroke(c) => (c, true),
            ShapePaint::Fill(c) => (c, false),
            ShapePaint::None => continue,
        };
        let mut paint = tiny_skia::Paint::default();
        paint.set_color_rgba8(paint_color.0, paint_color.1, paint_color.2, paint_color.3);
        paint.anti_alias = true;

        let Some(path) = shape.geometry.to_skia_path() else {
            continue;
        };
        if is_stroke {
            let stroke = tiny_skia::Stroke {
                width: stroke_w,
                line_cap: tiny_skia::LineCap::Round,
                line_join: tiny_skia::LineJoin::Round,
                ..Default::default()
            };
            pixmap.stroke_path(&path, &paint, &stroke, xform, None);
        } else {
            pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, xform, None);
        }
    }

    Ok(RasterIcon {
        width: target_w,
        height: target_h,
        pixels: pixmap.data().to_vec(),
    })
}

#[derive(Debug, Default)]
struct ParsedSvg {
    view_x: f32,
    view_y: f32,
    view_w: f32,
    view_h: f32,
    stroke_width: Option<f32>,
    /// Default stroke color from root attribute; per-shape overrides win.
    root_stroke: Option<RgbaU8>,
    /// Default fill color from root attribute.
    root_fill: Option<RgbaU8>,
    shapes: Vec<Shape>,
}

#[derive(Debug, Clone, Copy)]
struct RgbaU8(u8, u8, u8, u8);

#[derive(Debug)]
enum ShapePaint {
    Stroke(RgbaU8),
    Fill(RgbaU8),
    None,
}

#[derive(Debug)]
struct Shape {
    paint: ShapePaint,
    geometry: Geometry,
}

#[derive(Debug)]
enum Geometry {
    Path(String),
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    },
    Circle {
        cx: f32,
        cy: f32,
        r: f32,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        rx: f32,
    },
    Polyline(Vec<(f32, f32)>),
    Polygon(Vec<(f32, f32)>),
}

impl Geometry {
    fn to_skia_path(&self) -> Option<tiny_skia::Path> {
        let mut pb = tiny_skia::PathBuilder::new();
        match self {
            Self::Path(d) => {
                build_path_from_d(&mut pb, d).ok()?;
            }
            Self::Line { x1, y1, x2, y2 } => {
                pb.move_to(*x1, *y1);
                pb.line_to(*x2, *y2);
            }
            Self::Circle { cx, cy, r } => {
                pb.push_circle(*cx, *cy, *r);
            }
            Self::Rect { x, y, w, h, rx } => {
                if *rx > 0.0 {
                    let rect = tiny_skia::Rect::from_xywh(*x, *y, *w, *h)?;
                    pb.push_rect(rect);
                    // tiny-skia 0.11 lacks rounded-rect convenience; for
                    // our icon set this is acceptable — Lucide rounds
                    // are subtle and the un-rounded rendering still
                    // reads correctly.
                } else {
                    let rect = tiny_skia::Rect::from_xywh(*x, *y, *w, *h)?;
                    pb.push_rect(rect);
                }
            }
            Self::Polyline(pts) => {
                if let Some(((fx, fy), rest)) = pts.split_first() {
                    pb.move_to(*fx, *fy);
                    for (x, y) in rest {
                        pb.line_to(*x, *y);
                    }
                }
            }
            Self::Polygon(pts) => {
                if let Some(((fx, fy), rest)) = pts.split_first() {
                    pb.move_to(*fx, *fy);
                    for (x, y) in rest {
                        pb.line_to(*x, *y);
                    }
                    pb.close();
                }
            }
        }
        pb.finish()
    }
}

fn parse_svg(input: &str) -> Result<ParsedSvg, TintError> {
    let mut out = ParsedSvg::default();
    // Defaults if no viewBox present.
    out.view_w = 24.0;
    out.view_h = 24.0;

    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Skip comments and processing instructions.
            if input[i..].starts_with("<!--") {
                if let Some(end) = input[i..].find("-->") {
                    i += end + 3;
                    continue;
                }
                break;
            }
            if input[i..].starts_with("<?") {
                if let Some(end) = input[i..].find("?>") {
                    i += end + 2;
                    continue;
                }
                break;
            }
            // End tag: skip.
            if input[i..].starts_with("</") {
                if let Some(end) = input[i..].find('>') {
                    i += end + 1;
                    continue;
                }
                break;
            }
            // Find the matching '>'.
            let close = input[i..]
                .find('>')
                .ok_or_else(|| TintError::Parse(format!("unterminated tag near offset {i}")))?;
            let tag_text = &input[i + 1..i + close];
            i += close + 1;

            let (name, attrs) = split_tag(tag_text);
            match name {
                "svg" => apply_root_attrs(&mut out, &attrs),
                "path" => {
                    if let Some(d) = attrs
                        .iter()
                        .find(|(k, _)| *k == "d")
                        .map(|(_, v)| v.clone())
                    {
                        let paint = resolve_paint(&attrs, &out, true);
                        out.shapes.push(Shape {
                            paint,
                            geometry: Geometry::Path(d),
                        });
                    }
                }
                "line" => {
                    let g = Geometry::Line {
                        x1: attr_f32(&attrs, "x1", 0.0),
                        y1: attr_f32(&attrs, "y1", 0.0),
                        x2: attr_f32(&attrs, "x2", 0.0),
                        y2: attr_f32(&attrs, "y2", 0.0),
                    };
                    let paint = resolve_paint(&attrs, &out, true);
                    out.shapes.push(Shape { paint, geometry: g });
                }
                "circle" => {
                    let g = Geometry::Circle {
                        cx: attr_f32(&attrs, "cx", 0.0),
                        cy: attr_f32(&attrs, "cy", 0.0),
                        r: attr_f32(&attrs, "r", 0.0),
                    };
                    let paint = resolve_paint(&attrs, &out, true);
                    out.shapes.push(Shape { paint, geometry: g });
                }
                "rect" => {
                    let g = Geometry::Rect {
                        x: attr_f32(&attrs, "x", 0.0),
                        y: attr_f32(&attrs, "y", 0.0),
                        w: attr_f32(&attrs, "width", 0.0),
                        h: attr_f32(&attrs, "height", 0.0),
                        rx: attr_f32(&attrs, "rx", 0.0),
                    };
                    let paint = resolve_paint(&attrs, &out, true);
                    out.shapes.push(Shape { paint, geometry: g });
                }
                "polyline" => {
                    if let Some(pts_s) = attrs
                        .iter()
                        .find(|(k, _)| *k == "points")
                        .map(|(_, v)| v.clone())
                    {
                        let g = Geometry::Polyline(parse_points(&pts_s));
                        let paint = resolve_paint(&attrs, &out, true);
                        out.shapes.push(Shape { paint, geometry: g });
                    }
                }
                "polygon" => {
                    if let Some(pts_s) = attrs
                        .iter()
                        .find(|(k, _)| *k == "points")
                        .map(|(_, v)| v.clone())
                    {
                        let g = Geometry::Polygon(parse_points(&pts_s));
                        let paint = resolve_paint(&attrs, &out, true);
                        out.shapes.push(Shape { paint, geometry: g });
                    }
                }
                _ => { /* ignore <g>, <title>, <desc>, etc. */ }
            }
        } else {
            i += 1;
        }
    }
    Ok(out)
}

fn split_tag(tag: &str) -> (&str, Vec<(&str, String)>) {
    let trimmed = tag.trim().trim_end_matches('/').trim();
    let (name, rest) = match trimmed.find(|c: char| c.is_whitespace()) {
        Some(idx) => (&trimmed[..idx], &trimmed[idx..]),
        None => (trimmed, ""),
    };
    let mut attrs = Vec::new();
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        // Read attr name.
        let name_start = i;
        while i < bytes.len() && bytes[i] != b'=' && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let attr_name = &rest[name_start..i];
        if attr_name.is_empty() {
            break;
        }
        // Expect '='.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            // Boolean attr — ignore.
            attrs.push((attr_name, String::new()));
            continue;
        }
        i += 1; // skip '='
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let quote = bytes[i];
        if quote != b'"' && quote != b'\'' {
            break;
        }
        i += 1;
        let val_start = i;
        while i < bytes.len() && bytes[i] != quote {
            i += 1;
        }
        let val = &rest[val_start..i];
        if i < bytes.len() {
            i += 1;
        }
        attrs.push((attr_name, val.to_owned()));
    }
    (name, attrs)
}

fn apply_root_attrs(out: &mut ParsedSvg, attrs: &[(&str, String)]) {
    for (k, v) in attrs {
        match *k {
            "viewBox" | "viewbox" => {
                let parts: Vec<f32> = v
                    .split(|c: char| c.is_whitespace() || c == ',')
                    .filter(|s| !s.is_empty())
                    .filter_map(|s| s.parse().ok())
                    .collect();
                if parts.len() == 4 {
                    out.view_x = parts[0];
                    out.view_y = parts[1];
                    out.view_w = parts[2];
                    out.view_h = parts[3];
                }
            }
            "width" => {
                if out.view_w == 24.0 {
                    if let Ok(w) = v
                        .trim_end_matches(|c: char| !c.is_ascii_digit() && c != '.')
                        .parse::<f32>()
                    {
                        out.view_w = w;
                    }
                }
            }
            "height" => {
                if out.view_h == 24.0 {
                    if let Ok(h) = v
                        .trim_end_matches(|c: char| !c.is_ascii_digit() && c != '.')
                        .parse::<f32>()
                    {
                        out.view_h = h;
                    }
                }
            }
            "stroke-width" => {
                if let Ok(w) = v.parse::<f32>() {
                    out.stroke_width = Some(w);
                }
            }
            "stroke" => {
                out.root_stroke = parse_color(v);
            }
            "fill" => {
                out.root_fill = parse_color(v);
            }
            _ => {}
        }
    }
}

fn attr_f32(attrs: &[(&str, String)], key: &str, default: f32) -> f32 {
    attrs
        .iter()
        .find(|(k, _)| *k == key)
        .and_then(|(_, v)| v.parse::<f32>().ok())
        .unwrap_or(default)
}

fn resolve_paint(attrs: &[(&str, String)], root: &ParsedSvg, prefer_stroke: bool) -> ShapePaint {
    let stroke_attr = attrs
        .iter()
        .find(|(k, _)| *k == "stroke")
        .map(|(_, v)| v.clone());
    let fill_attr = attrs
        .iter()
        .find(|(k, _)| *k == "fill")
        .map(|(_, v)| v.clone());

    let stroke_color = stroke_attr
        .as_deref()
        .and_then(parse_color)
        .or(root.root_stroke);
    let fill_color = fill_attr
        .as_deref()
        .and_then(parse_color)
        .or(root.root_fill);

    let stroke_none = stroke_attr
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("none"));
    let fill_none = fill_attr
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("none"));

    if prefer_stroke {
        if !stroke_none {
            if let Some(c) = stroke_color {
                return ShapePaint::Stroke(c);
            }
        }
        if !fill_none {
            if let Some(c) = fill_color {
                return ShapePaint::Fill(c);
            }
        }
    } else {
        if !fill_none {
            if let Some(c) = fill_color {
                return ShapePaint::Fill(c);
            }
        }
        if !stroke_none {
            if let Some(c) = stroke_color {
                return ShapePaint::Stroke(c);
            }
        }
    }
    ShapePaint::None
}

fn parse_color(s: &str) -> Option<RgbaU8> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("none") || s.eq_ignore_ascii_case("currentcolor") {
        return None;
    }
    if let Some(rest) = s.strip_prefix('#') {
        return parse_hex(rest);
    }
    // Named colors (rare in Lucide).
    match s.to_ascii_lowercase().as_str() {
        "black" => Some(RgbaU8(0, 0, 0, 255)),
        "white" => Some(RgbaU8(255, 255, 255, 255)),
        "red" => Some(RgbaU8(255, 0, 0, 255)),
        "green" => Some(RgbaU8(0, 128, 0, 255)),
        "blue" => Some(RgbaU8(0, 0, 255, 255)),
        _ => None,
    }
}

fn parse_hex(rest: &str) -> Option<RgbaU8> {
    let nibble = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        }
    };
    let bs = rest.as_bytes();
    match bs.len() {
        3 => {
            let r = nibble(bs[0])?;
            let g = nibble(bs[1])?;
            let b = nibble(bs[2])?;
            Some(RgbaU8(r * 17, g * 17, b * 17, 255))
        }
        6 => {
            let r = nibble(bs[0])? * 16 + nibble(bs[1])?;
            let g = nibble(bs[2])? * 16 + nibble(bs[3])?;
            let b = nibble(bs[4])? * 16 + nibble(bs[5])?;
            Some(RgbaU8(r, g, b, 255))
        }
        _ => None,
    }
}

fn parse_points(s: &str) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    let toks: Vec<&str> = s
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .collect();
    for pair in toks.chunks_exact(2) {
        if let (Ok(x), Ok(y)) = (pair[0].parse::<f32>(), pair[1].parse::<f32>()) {
            out.push((x, y));
        }
    }
    out
}

// -------------------------------------------------------------------
// SVG path "d" attribute parser. Supports M/m, L/l, H/h, V/v, C/c, S/s,
// Q/q, T/t, A/a, Z/z. Lucide uses primarily M/L/A/Z plus a sprinkling
// of C in folder-open / undo / etc.
// -------------------------------------------------------------------

fn build_path_from_d(pb: &mut tiny_skia::PathBuilder, d: &str) -> Result<(), TintError> {
    let mut cursor = (0.0_f32, 0.0_f32);
    let mut start = (0.0_f32, 0.0_f32);
    let mut prev_ctrl: Option<(f32, f32)> = None;
    let mut tokens = tokenize_path(d).into_iter().peekable();
    while let Some(tok) = tokens.next() {
        let cmd = match tok {
            PathTok::Cmd(c) => c,
            PathTok::Num(_) => continue,
        };
        let abs = cmd.is_ascii_uppercase();
        match cmd.to_ascii_lowercase() {
            'm' => {
                let x = expect_num(&mut tokens)?;
                let y = expect_num(&mut tokens)?;
                let (px, py) = if abs {
                    (x, y)
                } else {
                    (cursor.0 + x, cursor.1 + y)
                };
                pb.move_to(px, py);
                cursor = (px, py);
                start = (px, py);
                prev_ctrl = None;
                // Subsequent pairs are implicit L/l.
                while let Some(PathTok::Num(_)) = tokens.peek() {
                    let x = expect_num(&mut tokens)?;
                    let y = expect_num(&mut tokens)?;
                    let (px, py) = if abs {
                        (x, y)
                    } else {
                        (cursor.0 + x, cursor.1 + y)
                    };
                    pb.line_to(px, py);
                    cursor = (px, py);
                }
            }
            'l' => {
                while let Some(PathTok::Num(_)) = tokens.peek() {
                    let x = expect_num(&mut tokens)?;
                    let y = expect_num(&mut tokens)?;
                    let (px, py) = if abs {
                        (x, y)
                    } else {
                        (cursor.0 + x, cursor.1 + y)
                    };
                    pb.line_to(px, py);
                    cursor = (px, py);
                }
                prev_ctrl = None;
            }
            'h' => {
                while let Some(PathTok::Num(_)) = tokens.peek() {
                    let x = expect_num(&mut tokens)?;
                    let px = if abs { x } else { cursor.0 + x };
                    pb.line_to(px, cursor.1);
                    cursor.0 = px;
                }
                prev_ctrl = None;
            }
            'v' => {
                while let Some(PathTok::Num(_)) = tokens.peek() {
                    let y = expect_num(&mut tokens)?;
                    let py = if abs { y } else { cursor.1 + y };
                    pb.line_to(cursor.0, py);
                    cursor.1 = py;
                }
                prev_ctrl = None;
            }
            'c' => {
                while let Some(PathTok::Num(_)) = tokens.peek() {
                    let x1 = expect_num(&mut tokens)?;
                    let y1 = expect_num(&mut tokens)?;
                    let x2 = expect_num(&mut tokens)?;
                    let y2 = expect_num(&mut tokens)?;
                    let x = expect_num(&mut tokens)?;
                    let y = expect_num(&mut tokens)?;
                    let (c1, c2, end) = if abs {
                        ((x1, y1), (x2, y2), (x, y))
                    } else {
                        (
                            (cursor.0 + x1, cursor.1 + y1),
                            (cursor.0 + x2, cursor.1 + y2),
                            (cursor.0 + x, cursor.1 + y),
                        )
                    };
                    pb.cubic_to(c1.0, c1.1, c2.0, c2.1, end.0, end.1);
                    cursor = end;
                    prev_ctrl = Some(c2);
                }
            }
            's' => {
                while let Some(PathTok::Num(_)) = tokens.peek() {
                    let x2 = expect_num(&mut tokens)?;
                    let y2 = expect_num(&mut tokens)?;
                    let x = expect_num(&mut tokens)?;
                    let y = expect_num(&mut tokens)?;
                    let c1 = match prev_ctrl {
                        Some((px, py)) => (2.0 * cursor.0 - px, 2.0 * cursor.1 - py),
                        None => cursor,
                    };
                    let (c2, end) = if abs {
                        ((x2, y2), (x, y))
                    } else {
                        ((cursor.0 + x2, cursor.1 + y2), (cursor.0 + x, cursor.1 + y))
                    };
                    pb.cubic_to(c1.0, c1.1, c2.0, c2.1, end.0, end.1);
                    cursor = end;
                    prev_ctrl = Some(c2);
                }
            }
            'q' => {
                while let Some(PathTok::Num(_)) = tokens.peek() {
                    let x1 = expect_num(&mut tokens)?;
                    let y1 = expect_num(&mut tokens)?;
                    let x = expect_num(&mut tokens)?;
                    let y = expect_num(&mut tokens)?;
                    let (c1, end) = if abs {
                        ((x1, y1), (x, y))
                    } else {
                        ((cursor.0 + x1, cursor.1 + y1), (cursor.0 + x, cursor.1 + y))
                    };
                    pb.quad_to(c1.0, c1.1, end.0, end.1);
                    cursor = end;
                    prev_ctrl = Some(c1);
                }
            }
            't' => {
                while let Some(PathTok::Num(_)) = tokens.peek() {
                    let x = expect_num(&mut tokens)?;
                    let y = expect_num(&mut tokens)?;
                    let c1 = match prev_ctrl {
                        Some((px, py)) => (2.0 * cursor.0 - px, 2.0 * cursor.1 - py),
                        None => cursor,
                    };
                    let end = if abs {
                        (x, y)
                    } else {
                        (cursor.0 + x, cursor.1 + y)
                    };
                    pb.quad_to(c1.0, c1.1, end.0, end.1);
                    cursor = end;
                    prev_ctrl = Some(c1);
                }
            }
            'a' => {
                // Approximate arc-to with cubic beziers. tiny-skia 0.11
                // doesn't have an arc primitive, so we lower to cubics.
                while let Some(PathTok::Num(_)) = tokens.peek() {
                    let rx = expect_num(&mut tokens)?;
                    let ry = expect_num(&mut tokens)?;
                    let x_axis_rot = expect_num(&mut tokens)?;
                    let large = expect_num(&mut tokens)? > 0.5;
                    let sweep = expect_num(&mut tokens)? > 0.5;
                    let x = expect_num(&mut tokens)?;
                    let y = expect_num(&mut tokens)?;
                    let end = if abs {
                        (x, y)
                    } else {
                        (cursor.0 + x, cursor.1 + y)
                    };
                    arc_to(pb, cursor, rx, ry, x_axis_rot, large, sweep, end);
                    cursor = end;
                    prev_ctrl = None;
                }
            }
            'z' => {
                pb.close();
                cursor = start;
                prev_ctrl = None;
            }
            other => {
                return Err(TintError::Parse(format!(
                    "unsupported path command {other:?}"
                )));
            }
        }
    }
    Ok(())
}

#[derive(Debug)]
enum PathTok {
    Cmd(char),
    Num(f32),
}

fn tokenize_path(d: &str) -> Vec<PathTok> {
    let mut out = Vec::new();
    let bytes = d.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() || c == b',' {
            i += 1;
            continue;
        }
        if c.is_ascii_alphabetic() {
            out.push(PathTok::Cmd(c as char));
            i += 1;
            continue;
        }
        // Number: optional sign, digits, optional dot, optional exponent.
        let start = i;
        if c == b'+' || c == b'-' {
            i += 1;
        }
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
        if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
            i += 1;
            if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                i += 1;
            }
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
        if start == i {
            // Defensive: avoid infinite loop on unrecognised byte.
            i += 1;
            continue;
        }
        let lex = &d[start..i];
        if let Ok(n) = lex.parse::<f32>() {
            out.push(PathTok::Num(n));
        }
    }
    out
}

fn expect_num<I: Iterator<Item = PathTok>>(
    it: &mut std::iter::Peekable<I>,
) -> Result<f32, TintError> {
    match it.next() {
        Some(PathTok::Num(n)) => Ok(n),
        Some(PathTok::Cmd(c)) => Err(TintError::Parse(format!(
            "expected number, got command {c:?}"
        ))),
        None => Err(TintError::Parse("unexpected end of path data".into())),
    }
}

#[allow(clippy::too_many_arguments)]
fn arc_to(
    pb: &mut tiny_skia::PathBuilder,
    start: (f32, f32),
    rx_in: f32,
    ry_in: f32,
    x_axis_rot_deg: f32,
    large: bool,
    sweep: bool,
    end: (f32, f32),
) {
    // Implementation of SVG arc-to-cubic conversion per the Implementation
    // Notes in https://www.w3.org/TR/SVG/implnote.html#ArcImplementationNotes
    if rx_in == 0.0 || ry_in == 0.0 || (start.0 == end.0 && start.1 == end.1) {
        pb.line_to(end.0, end.1);
        return;
    }
    let rx = rx_in.abs();
    let ry = ry_in.abs();
    let phi = x_axis_rot_deg.to_radians();
    let cosp = phi.cos();
    let sinp = phi.sin();

    // Step 1: compute (x1', y1')
    let dx = (start.0 - end.0) / 2.0;
    let dy = (start.1 - end.1) / 2.0;
    let x1p = cosp * dx + sinp * dy;
    let y1p = -sinp * dx + cosp * dy;

    // Step 2: ensure radii are large enough.
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    let (rx, ry) = if lambda > 1.0 {
        let scale = lambda.sqrt();
        (rx * scale, ry * scale)
    } else {
        (rx, ry)
    };

    // Step 3: compute (cx', cy')
    let sign = if large == sweep { -1.0 } else { 1.0 };
    let num = (rx * rx) * (ry * ry) - (rx * rx) * (y1p * y1p) - (ry * ry) * (x1p * x1p);
    let den = (rx * rx) * (y1p * y1p) + (ry * ry) * (x1p * x1p);
    let factor = if den == 0.0 {
        0.0
    } else {
        ((num / den).max(0.0)).sqrt()
    };
    let cxp = sign * factor * (rx * y1p) / ry;
    let cyp = sign * factor * -(ry * x1p) / rx;

    // Step 4: compute (cx, cy)
    let cx = cosp * cxp - sinp * cyp + (start.0 + end.0) / 2.0;
    let cy = sinp * cxp + cosp * cyp + (start.1 + end.1) / 2.0;

    // Step 5: compute angles.
    let angle = |ux: f32, uy: f32, vx: f32, vy: f32| {
        let dot = ux * vx + uy * vy;
        let len = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        let mut ang = (dot / len).clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            ang = -ang;
        }
        ang
    };
    let theta1 = angle(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut delta = angle(
        (x1p - cxp) / rx,
        (y1p - cyp) / ry,
        (-x1p - cxp) / rx,
        (-y1p - cyp) / ry,
    );
    if !sweep && delta > 0.0 {
        delta -= std::f32::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f32::consts::TAU;
    }

    // Step 6: lower to cubics, splitting at every quarter-turn for accuracy.
    let segments = ((delta.abs() / (std::f32::consts::FRAC_PI_2)).ceil() as usize).max(1);
    let seg_delta = delta / segments as f32;
    let alpha = (4.0 / 3.0) * (seg_delta / 4.0).tan();

    let mut cur_theta = theta1;
    let mut cur = start;
    for _ in 0..segments {
        let next_theta = cur_theta + seg_delta;
        let cos_t1 = cur_theta.cos();
        let sin_t1 = cur_theta.sin();
        let cos_t2 = next_theta.cos();
        let sin_t2 = next_theta.sin();

        let p_end_x = cosp * (rx * cos_t2) - sinp * (ry * sin_t2) + cx;
        let p_end_y = sinp * (rx * cos_t2) + cosp * (ry * sin_t2) + cy;

        let c1x = cur.0 + alpha * (-cosp * rx * sin_t1 - sinp * ry * cos_t1);
        let c1y = cur.1 + alpha * (-sinp * rx * sin_t1 + cosp * ry * cos_t1);
        let c2x = p_end_x - alpha * (-cosp * rx * sin_t2 - sinp * ry * cos_t2);
        let c2y = p_end_y - alpha * (-sinp * rx * sin_t2 + cosp * ry * cos_t2);

        pb.cubic_to(c1x, c1y, c2x, c2y, p_end_x, p_end_y);

        cur = (p_end_x, p_end_y);
        cur_theta = next_theta;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tint_substitutes_currentcolor() {
        let svg = r#"<svg><path stroke="currentColor" d="M0 0 L1 1"/></svg>"#;
        let red = Color { r: 255, g: 0, b: 0 };
        let out = apply_tint(svg, red);
        assert!(out.contains("#ff0000"), "got: {out}");
        assert!(!out.contains("currentColor"), "got: {out}");
    }

    #[test]
    fn tint_three_distinct_colors() {
        let svg = r#"<svg><path stroke="currentColor" d="M0 0 L1 1"/></svg>"#;
        let a = apply_tint(svg, Color { r: 255, g: 0, b: 0 });
        let b = apply_tint(svg, Color { r: 0, g: 255, b: 0 });
        let c = apply_tint(svg, Color { r: 0, g: 0, b: 255 });
        assert!(a.contains("#ff0000"));
        assert!(b.contains("#00ff00"));
        assert!(c.contains("#0000ff"));
        assert_ne!(a, b);
        assert_ne!(b, c);
    }

    #[test]
    fn rasterize_small_svg() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#ff0000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 3 L21 21"/></svg>"##;
        let img = rasterize(svg, 32, 32).expect("ok");
        assert_eq!(img.width, 32);
        assert_eq!(img.height, 32);
        assert_eq!(img.pixels.len(), 32 * 32 * 4);
        // At least one pixel should have nonzero alpha — the diagonal stroke.
        assert!(img.pixels.chunks_exact(4).any(|p| p[3] > 0));
    }

    #[test]
    fn rasterize_zero_dim_rejected() {
        let svg = r#"<svg viewBox="0 0 24 24"/>"#;
        assert!(matches!(
            rasterize(svg, 0, 32),
            Err(TintError::BadDimensions { .. })
        ));
    }
}
