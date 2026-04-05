use std::io::Write;
use std::process::{Command, Stdio};

use ab_glyph::{point, Font as _, FontArc, Glyph, PxScale, ScaleFont as _};
use taffy::prelude::*;
use tiny_skia::{
    BlendMode, Color, FillRule, Paint, Path, PathBuilder, Pixmap, PremultipliedColorU8, Shader,
    Transform,
};

// ---------------------------------------------------------------------------
// Layout & timing
// ---------------------------------------------------------------------------

const W: u32 = 1080;
const H: u32 = 1920;
const FPS: u32 = 60;
const DUR: f64 = 10.0;
const RAD: f32 = 12.0;

// Reddit dark mode palette [r, g, b, a]
const C_BG: [u8; 4] = [3, 3, 3, 255];
const C_CARD: [u8; 4] = [26, 26, 27, 255];
const C_BORDER: [u8; 4] = [52, 53, 54, 255];
const C_VOTE_BG: [u8; 4] = [20, 20, 21, 255];
const C_UP: [u8; 4] = [255, 69, 0, 255];
const C_T1: [u8; 4] = [215, 218, 220, 255];
const C_T2: [u8; 4] = [129, 131, 132, 255];
const C_BLUE: [u8; 4] = [0, 121, 211, 255];
const C_THREAD: [u8; 4] = [52, 53, 54, 255];

// ---------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------

struct CommentData {
    user: &'static str,
    text: &'static str,
    votes: &'static str,
    replies: &'static str,
    time: &'static str,
    start: f64,
    avatar: [u8; 4],
}

#[derive(Clone, Copy)]
struct CardRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    println!("Rust MVP v2 - {DUR}s @ {FPS}fps (taffy + tiny-skia)");

    let bold = FontArc::try_from_slice(include_bytes!("../fonts/Roboto-Bold.ttf")).unwrap();
    let regular = FontArc::try_from_slice(include_bytes!("../fonts/Roboto-Regular.ttf")).unwrap();

    let comments = [
        CommentData {
            user: "science_nerd",
            text: "What's a fact that sounds\ncompletely made up but\nis actually true?",
            votes: "2.4k",
            replies: "1.2k",
            time: "12h",
            start: 2.0,
            avatar: [255, 69, 0, 255],
        },
        CommentData {
            user: "history_buff42",
            text: "Honey never spoils.\nArchaeologists found 3000-year\nold honey in Egyptian tombs\nthat was still perfectly edible.",
            votes: "1.8k",
            replies: "342",
            time: "10h",
            start: 4.5,
            avatar: [0, 200, 120, 255],
        },
        CommentData {
            user: "animal_facts",
            text: "A group of flamingos is\ncalled a 'flamboyance'.\nI'm not even kidding.",
            votes: "956",
            replies: "89",
            time: "8h",
            start: 7.0,
            avatar: [150, 80, 255, 255],
        },
    ];

    let rects = compute_layout(&comments);
    let total = (DUR * FPS as f64) as u32;

    let mut ffmpeg = Command::new("ffmpeg")
        .args([
            "-y", "-f", "rawvideo", "-pix_fmt", "rgba",
            "-s", &format!("{W}x{H}"), "-r", &FPS.to_string(),
            "-i", "pipe:0",
            "-c:v", "libx264", "-pix_fmt", "yuv420p",
            "-preset", "medium", "-crf", "23",
            "output.mp4",
        ])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn ffmpeg");

    {
        let mut pm = Pixmap::new(W, H).unwrap();
        let stdin = ffmpeg.stdin.as_mut().unwrap();

        for i in 0..total {
            let t = i as f64 / FPS as f64;
            pm.fill(col(C_BG));
            render_frame(&mut pm, t, &bold, &regular, &comments, &rects);
            stdin.write_all(pm.data()).unwrap();

            if i % (FPS * 2) == 0 {
                println!("  {t:.1}s / {DUR:.1}s");
            }
        }
    }

    let res = ffmpeg.wait_with_output().unwrap();
    if res.status.success() {
        println!("Done! output.mp4");
    } else {
        eprintln!("FFmpeg error:\n{}", String::from_utf8_lossy(&res.stderr));
    }
}

// ---------------------------------------------------------------------------
// Layout (taffy flexbox)
// ---------------------------------------------------------------------------

fn compute_layout(comments: &[CommentData]) -> Vec<CardRect> {
    let mut tree: TaffyTree<()> = TaffyTree::new();

    let header = tree
        .new_leaf(Style {
            size: Size { width: Dimension::Auto, height: Dimension::Length(120.0) },
            ..Default::default()
        })
        .unwrap();

    let post = tree
        .new_leaf(Style {
            size: Size { width: Dimension::Auto, height: Dimension::Length(260.0) },
            ..Default::default()
        })
        .unwrap();

    let comment_nodes: Vec<NodeId> = comments
        .iter()
        .map(|c| {
            let lines = c.text.split('\n').count();
            let h = 130.0 + lines as f32 * 46.0;
            tree.new_leaf(Style {
                size: Size { width: Dimension::Auto, height: Dimension::Length(h) },
                ..Default::default()
            })
            .unwrap()
        })
        .collect();

    let mut children = vec![header, post];
    children.extend(&comment_nodes);

    let root = tree
        .new_with_children(
            Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                size: Size {
                    width: Dimension::Length(W as f32),
                    height: Dimension::Length(H as f32),
                },
                padding: taffy::geometry::Rect {
                    left: LengthPercentage::Length(40.0),
                    right: LengthPercentage::Length(40.0),
                    top: LengthPercentage::Length(50.0),
                    bottom: LengthPercentage::Length(50.0),
                },
                gap: Size {
                    width: LengthPercentage::Length(0.0),
                    height: LengthPercentage::Length(20.0),
                },
                ..Default::default()
            },
            &children,
        )
        .unwrap();

    tree.compute_layout(
        root,
        Size {
            width: AvailableSpace::Definite(W as f32),
            height: AvailableSpace::Definite(H as f32),
        },
    )
    .unwrap();

    children
        .iter()
        .map(|&n| {
            let l = tree.layout(n).unwrap();
            CardRect { x: l.location.x, y: l.location.y, w: l.size.width, h: l.size.height }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Frame composition
// ---------------------------------------------------------------------------

fn render_frame(
    pm: &mut Pixmap,
    t: f64,
    bold: &FontArc,
    regular: &FontArc,
    comments: &[CommentData],
    rects: &[CardRect],
) {
    draw_header(pm, t, rects[0], bold, regular);
    draw_post(pm, t, rects[1], bold, regular);
    for (i, c) in comments.iter().enumerate() {
        draw_comment(pm, t, rects[i + 2], bold, regular, c);
    }
    draw_progress_bar(pm, t);
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

fn draw_header(pm: &mut Pixmap, t: f64, r: CardRect, bold: &FontArc, regular: &FontArc) {
    let op = fade_in_out(t, 0.0, 0.5, 8.5, 9.5) as f32;
    if op <= 0.01 { return; }

    let slide = ((1.0 - ease_out_cubic(clamp01(t / 0.5))) * -160.0) as f32;
    let y = r.y + slide;

    // Card
    fill_rrect(pm, r.x, y, r.w, r.h, RAD, col_a(C_BORDER, op));
    fill_rrect(pm, r.x + 1.0, y + 1.0, r.w - 2.0, r.h - 2.0, RAD - 1.0, col_a(C_CARD, op));

    // Blue accent line
    fill_simple_rect(pm, r.x + 1.0, y + 1.0, r.w - 2.0, 4.0, col_a(C_BLUE, op));

    // Subreddit icon
    fill_circle(pm, r.x + 36.0, y + 60.0, 22.0, col_a(C_UP, op));
    draw_text(pm, bold, "r/", r.x + 26.0, y + 44.0, 24.0, col_a([255, 255, 255, 255], op));

    // Name & info
    draw_text(pm, bold, "r/AskReddit", r.x + 70.0, y + 28.0, 36.0, col_a(C_T1, op));
    draw_text(pm, regular, "45.2M members  -  12.4k online", r.x + 70.0, y + 72.0, 22.0, col_a(C_T2, op));
}

// ---------------------------------------------------------------------------
// Post card
// ---------------------------------------------------------------------------

fn draw_post(pm: &mut Pixmap, t: f64, r: CardRect, bold: &FontArc, regular: &FontArc) {
    let op = fade_in_out(t, 0.3, 0.8, 8.5, 9.5) as f32;
    if op <= 0.01 { return; }

    let slide = ((1.0 - ease_out_cubic(clamp01((t - 0.3) / 0.5))) * 80.0) as f32;
    let y = r.y + slide;

    // Card
    fill_rrect(pm, r.x, y, r.w, r.h, RAD, col_a(C_BORDER, op));
    fill_rrect(pm, r.x + 1.0, y + 1.0, r.w - 2.0, r.h - 2.0, RAD - 1.0, col_a(C_CARD, op));

    // Vote bar (left strip)
    fill_simple_rect(pm, r.x + 1.0, y + 1.0, 54.0, r.h - 2.0, col_a(C_VOTE_BG, op));

    // Upvote / count / downvote
    let vcx = r.x + 28.0;
    fill_tri_up(pm, vcx, y + 20.0, 14.0, col_a(C_UP, op));
    draw_text(pm, bold, "2.4k", r.x + 11.0, y + 42.0, 20.0, col_a(C_UP, op));
    fill_tri_down(pm, vcx, y + 72.0, 14.0, col_a(C_T2, op));

    // Metadata
    draw_text(pm, regular, "Posted by u/curious_mind  -  12h", r.x + 68.0, y + 16.0, 20.0, col_a(C_T2, op));

    // Title
    draw_text(pm, bold, "What's a fact that sounds completely", r.x + 68.0, y + 52.0, 32.0, col_a(C_T1, op));
    draw_text(pm, bold, "made up but is actually true?", r.x + 68.0, y + 94.0, 32.0, col_a(C_T1, op));

    // Footer
    draw_text(pm, bold, "1.2k Comments    Share    Save    ...", r.x + 68.0, y + r.h - 50.0, 22.0, col_a(C_T2, op));
}

// ---------------------------------------------------------------------------
// Comment card
// ---------------------------------------------------------------------------

fn draw_comment(
    pm: &mut Pixmap,
    t: f64,
    r: CardRect,
    bold: &FontArc,
    regular: &FontArc,
    c: &CommentData,
) {
    let slide = ease_out_back(clamp01((t - c.start) / 0.5));
    let fade = 1.0 - ease_out_cubic(clamp01((t - 8.5) / 1.0));
    if t < c.start || fade <= 0.01 { return; }

    let op = (slide.min(1.0).min(fade)) as f32;
    let y_off = ((1.0 - slide.min(1.0)) * 100.0) as f32;
    let y = r.y + y_off;

    // Card
    fill_rrect(pm, r.x, y, r.w, r.h, RAD, col_a(C_BORDER, op));
    fill_rrect(pm, r.x + 1.0, y + 1.0, r.w - 2.0, r.h - 2.0, RAD - 1.0, col_a(C_CARD, op));

    // Thread line
    fill_simple_rect(pm, r.x + 34.0, y + 48.0, 3.0, r.h - 76.0, col_a(C_THREAD, op));

    // Avatar
    fill_circle(pm, r.x + 35.0, y + 26.0, 14.0, col_a(c.avatar, op));

    // Username + time
    let ux = r.x + 60.0;
    draw_text(pm, bold, &format!("u/{}", c.user), ux, y + 14.0, 24.0, col_a(C_T1, op));
    let name_w = c.user.len() as f32 * 13.0 + 30.0;
    draw_text(pm, regular, &format!(" -  {}", c.time), ux + name_w, y + 16.0, 20.0, col_a(C_T2, op));

    // Body
    let mut ly = y + 55.0;
    for line in c.text.split('\n') {
        draw_text(pm, regular, line, r.x + 54.0, ly, 32.0, col_a(C_T1, op));
        ly += 46.0;
    }

    // Footer: votes + actions
    let fy = y + r.h - 52.0;
    fill_tri_up(pm, r.x + 66.0, fy + 4.0, 12.0, col_a(C_T2, op));
    draw_text(pm, bold, c.votes, r.x + 84.0, fy, 22.0, col_a(C_T1, op));
    let vw = c.votes.len() as f32 * 12.0 + 16.0;
    fill_tri_down(pm, r.x + 84.0 + vw, fy + 4.0, 12.0, col_a(C_T2, op));
    draw_text(
        pm, bold,
        &format!("Reply ({})    Share    ...", c.replies),
        r.x + 84.0 + vw + 30.0, fy, 22.0, col_a(C_T2, op),
    );
}

// ---------------------------------------------------------------------------
// Progress bar
// ---------------------------------------------------------------------------

fn draw_progress_bar(pm: &mut Pixmap, t: f64) {
    let w = ((t / DUR) * W as f64) as f32;
    if w > 0.0 {
        fill_simple_rect(pm, 0.0, H as f32 - 6.0, w.min(W as f32), 6.0, col_a(C_UP, 0.85));
    }
}

// ===========================================================================
// Drawing primitives (tiny-skia)
// ===========================================================================

fn make_paint(c: Color) -> Paint<'static> {
    Paint {
        shader: Shader::SolidColor(c),
        blend_mode: BlendMode::SourceOver,
        anti_alias: true,
        force_hq_pipeline: false,
    }
}

/// Filled rounded rectangle with anti-aliased edges.
fn fill_rrect(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, c: Color) {
    if let Some(path) = rrect_path(x, y, w, h, r) {
        pm.fill_path(&path, &make_paint(c), FillRule::Winding, Transform::identity(), None);
    }
}

/// Build a rounded-rectangle path via quadratic bezier corners.
fn rrect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    if w <= 0.0 || h <= 0.0 { return None; }
    let r = r.min(w / 2.0).min(h / 2.0);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish()
}

/// Simple axis-aligned rectangle (no rounding).
fn fill_simple_rect(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, c: Color) {
    if let Some(rect) = tiny_skia::Rect::from_xywh(x, y, w, h) {
        pm.fill_rect(rect, &make_paint(c), Transform::identity(), None);
    }
}

/// Anti-aliased filled circle.
fn fill_circle(pm: &mut Pixmap, cx: f32, cy: f32, r: f32, c: Color) {
    if let Some(path) = PathBuilder::from_circle(cx, cy, r) {
        pm.fill_path(&path, &make_paint(c), FillRule::Winding, Transform::identity(), None);
    }
}

/// Upward-pointing triangle (upvote arrow).
fn fill_tri_up(pm: &mut Pixmap, cx: f32, top: f32, sz: f32, c: Color) {
    let mut pb = PathBuilder::new();
    pb.move_to(cx, top);
    pb.line_to(cx + sz * 0.7, top + sz);
    pb.line_to(cx - sz * 0.7, top + sz);
    pb.close();
    if let Some(path) = pb.finish() {
        pm.fill_path(&path, &make_paint(c), FillRule::Winding, Transform::identity(), None);
    }
}

/// Downward-pointing triangle (downvote arrow).
fn fill_tri_down(pm: &mut Pixmap, cx: f32, top: f32, sz: f32, c: Color) {
    let mut pb = PathBuilder::new();
    pb.move_to(cx - sz * 0.7, top);
    pb.line_to(cx + sz * 0.7, top);
    pb.line_to(cx, top + sz);
    pb.close();
    if let Some(path) = pb.finish() {
        pm.fill_path(&path, &make_paint(c), FillRule::Winding, Transform::identity(), None);
    }
}

// ===========================================================================
// Text rendering (ab_glyph → tiny-skia pixmap)
// ===========================================================================

fn draw_text(pm: &mut Pixmap, font: &FontArc, text: &str, x: f32, y: f32, size: f32, c: Color) {
    let scaled = font.as_scaled(PxScale::from(size));
    let ascent = scaled.ascent();
    let baseline = y + ascent;
    let pw = pm.width();
    let ph = pm.height();

    let mut cx = x;
    for ch in text.chars() {
        let gid = font.glyph_id(ch);
        let glyph = Glyph { id: gid, scale: PxScale::from(size), position: point(cx, baseline) };

        if let Some(og) = font.outline_glyph(glyph) {
            let bb = og.px_bounds();
            let cr = c.red();
            let cg = c.green();
            let cb = c.blue();
            let ca = c.alpha();
            let pixels = pm.pixels_mut();

            og.draw(|gx, gy, cov| {
                let px = bb.min.x as i32 + gx as i32;
                let py = bb.min.y as i32 + gy as i32;
                if px < 0 || py < 0 || px >= pw as i32 || py >= ph as i32 {
                    return;
                }
                let a = cov * ca;
                if a < 0.004 { return; }
                let inv = 1.0 - a;
                let idx = py as usize * pw as usize + px as usize;
                let dst = pixels[idx];

                let r = (cr * a * 255.0 + dst.red() as f32 * inv).min(255.0) as u8;
                let g = (cg * a * 255.0 + dst.green() as f32 * inv).min(255.0) as u8;
                let b = (cb * a * 255.0 + dst.blue() as f32 * inv).min(255.0) as u8;

                if let Some(px) = PremultipliedColorU8::from_rgba(r, g, b, 255) {
                    pixels[idx] = px;
                }
            });
        }

        cx += scaled.h_advance(gid);
    }
}

// ===========================================================================
// Color helpers
// ===========================================================================

fn col(c: [u8; 4]) -> Color {
    Color::from_rgba8(c[0], c[1], c[2], c[3])
}

fn col_a(c: [u8; 4], opacity: f32) -> Color {
    Color::from_rgba8(c[0], c[1], c[2], (c[3] as f32 * opacity).min(255.0) as u8)
}

// ===========================================================================
// Easing
// ===========================================================================

fn clamp01(v: f64) -> f64 { v.clamp(0.0, 1.0) }

fn ease_out_cubic(t: f64) -> f64 { 1.0 - (1.0 - t).powi(3) }

fn ease_out_back(t: f64) -> f64 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

fn fade_in_out(t: f64, is: f64, ie: f64, os: f64, oe: f64) -> f64 {
    if t < is { 0.0 }
    else if t < ie { ease_out_cubic((t - is) / (ie - is)) }
    else if t < os { 1.0 }
    else if t < oe { 1.0 - ease_out_cubic((t - os) / (oe - os)) }
    else { 0.0 }
}
