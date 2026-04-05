use std::io::Write;
use std::process::{Command, Stdio};

use ab_glyph::{point, Font as _, FontArc, Glyph, PxScale, ScaleFont as _};
use taffy::prelude::*;
use tiny_skia::{
    BlendMode, Color, FillRule, FilterQuality, Paint, Path, PathBuilder, Pixmap, PixmapPaint,
    PremultipliedColorU8, Shader, Transform,
};

// ---------------------------------------------------------------------------
// Layout & timing
// ---------------------------------------------------------------------------

const W: u32 = 1080;
const H: u32 = 1920;
const FPS: u32 = 60;
const DUR: f64 = 10.0;
const RAD: f32 = 12.0;
const SUB_PAD: f32 = 50.0; // padding on sub-pixmaps for scale overshoot

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
const C_WHITE: [u8; 4] = [255, 255, 255, 255];

// ---------------------------------------------------------------------------
// Transitions
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Transition {
    PopIn,      // scale 0→overshoot→1 (ease_out_back)
    BounceUp,   // slide up with bounce easing
    SlideLeft,  // slide in from left
    SlideRight, // slide in from right
    ZoomBounce, // scale with elastic spring
}

/// Returns (opacity, offset_x, offset_y, scale) for entrance animation.
fn entrance_state(tr: Transition, p: f64) -> (f32, f32, f32, f32) {
    match tr {
        Transition::PopIn => {
            let s = ease_out_back(p).max(0.0);
            let op = ease_out_cubic(p.min(0.4) / 0.4);
            (op as f32, 0.0, 0.0, s as f32)
        }
        Transition::BounceUp => {
            let bounce = ease_out_bounce(p);
            let op = ease_out_cubic((p * 3.0).min(1.0));
            (op as f32, 0.0, ((1.0 - bounce) * 200.0) as f32, 1.0)
        }
        Transition::SlideLeft => {
            let ep = ease_out_cubic(p);
            (ep as f32, ((1.0 - ep) * -600.0) as f32, 0.0, 1.0)
        }
        Transition::SlideRight => {
            let ep = ease_out_cubic(p);
            (ep as f32, ((1.0 - ep) * 600.0) as f32, 0.0, 1.0)
        }
        Transition::ZoomBounce => {
            let s = ease_out_elastic(p).max(0.0);
            let op = ease_out_cubic((p * 3.0).min(1.0));
            (op as f32, 0.0, 0.0, s as f32)
        }
    }
}

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
    transition: Transition,
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
    println!("Rust MVP v3 - {DUR}s @ {FPS}fps (transitions)");

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
            transition: Transition::SlideLeft,
        },
        CommentData {
            user: "history_buff42",
            text: "Honey never spoils.\nArchaeologists found 3000-year\nold honey in Egyptian tombs\nthat was still perfectly edible.",
            votes: "1.8k",
            replies: "342",
            time: "10h",
            start: 4.5,
            avatar: [0, 200, 120, 255],
            transition: Transition::ZoomBounce,
        },
        CommentData {
            user: "animal_facts",
            text: "A group of flamingos is\ncalled a 'flamboyance'.\nI'm not even kidding.",
            votes: "956",
            replies: "89",
            time: "8h",
            start: 7.0,
            avatar: [150, 80, 255, 255],
            transition: Transition::SlideRight,
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
    // Header — PopIn (zoom with overshoot)
    composite_card(pm, rects[0], Transition::PopIn, t, 0.0, 0.6, 8.5, 1.0, |sub, o| {
        draw_header_content(sub, o, rects[0], bold, regular);
    });

    // Post — BounceUp
    composite_card(pm, rects[1], Transition::BounceUp, t, 0.3, 0.8, 8.5, 1.0, |sub, o| {
        draw_post_content(sub, o, rects[1], bold, regular);
    });

    // Comments — each with its own transition
    for (i, c) in comments.iter().enumerate() {
        let r = rects[i + 2];
        composite_card(pm, r, c.transition, t, c.start, 0.6, 8.5, 1.0, |sub, o| {
            draw_comment_content(sub, o, r, bold, regular, c);
        });
    }

    draw_progress_bar(pm, t);
}

// ---------------------------------------------------------------------------
// Sub-pixmap compositing with transitions
// ---------------------------------------------------------------------------

fn composite_card(
    pm: &mut Pixmap,
    rect: CardRect,
    trans: Transition,
    t: f64,
    enter_start: f64,
    enter_dur: f64,
    exit_start: f64,
    exit_dur: f64,
    draw_fn: impl FnOnce(&mut Pixmap, f32),
) {
    if t < enter_start { return; }

    // Entrance
    let enter_p = clamp01((t - enter_start) / enter_dur);
    let (e_op, e_dx, e_dy, e_scale) = entrance_state(trans, enter_p);

    // Exit: scale down + fade
    let exit_p = clamp01((t - exit_start) / exit_dur);
    let exit_op = 1.0 - ease_out_cubic(exit_p) as f32;
    let exit_scale = 1.0 - ease_out_cubic(exit_p) as f32 * 0.2;

    let opacity = e_op * exit_op;
    let scale = e_scale * exit_scale;
    if opacity <= 0.01 { return; }

    // Render card to sub-pixmap (with padding for overshoot)
    let sw = (rect.w + SUB_PAD * 2.0).ceil() as u32;
    let sh = (rect.h + SUB_PAD * 2.0).ceil() as u32;
    let mut sub = Pixmap::new(sw, sh).unwrap();
    draw_fn(&mut sub, SUB_PAD);

    // Scale transform centered on sub-pixmap
    let hw = sw as f32 / 2.0;
    let hh = sh as f32 / 2.0;
    let transform = if (scale - 1.0).abs() > 0.001 {
        Transform::from_translate(-hw, -hh)
            .post_scale(scale, scale)
            .post_translate(hw, hh)
    } else {
        Transform::identity()
    };

    // Composite position
    let fx = rect.x + e_dx - SUB_PAD;
    let fy = rect.y + e_dy - SUB_PAD;

    pm.draw_pixmap(
        fx as i32,
        fy as i32,
        sub.as_ref(),
        &PixmapPaint {
            opacity,
            blend_mode: BlendMode::SourceOver,
            quality: FilterQuality::Bilinear,
        },
        transform,
        None,
    );
}

// ---------------------------------------------------------------------------
// Card content renderers (draw at local coords with offset `o`)
// ---------------------------------------------------------------------------

fn draw_header_content(pm: &mut Pixmap, o: f32, r: CardRect, bold: &FontArc, regular: &FontArc) {
    let (w, h) = (r.w, r.h);
    fill_rrect(pm, o, o, w, h, RAD, col(C_BORDER));
    fill_rrect(pm, o + 1.0, o + 1.0, w - 2.0, h - 2.0, RAD - 1.0, col(C_CARD));
    fill_simple_rect(pm, o + 1.0, o + 1.0, w - 2.0, 4.0, col(C_BLUE));

    fill_circle(pm, o + 36.0, o + 60.0, 22.0, col(C_UP));
    draw_text(pm, bold, "r/", o + 26.0, o + 44.0, 24.0, col(C_WHITE));
    draw_text(pm, bold, "r/AskReddit", o + 70.0, o + 28.0, 36.0, col(C_T1));
    draw_text(pm, regular, "45.2M members  -  12.4k online", o + 70.0, o + 72.0, 22.0, col(C_T2));
}

fn draw_post_content(pm: &mut Pixmap, o: f32, r: CardRect, bold: &FontArc, regular: &FontArc) {
    let (w, h) = (r.w, r.h);
    fill_rrect(pm, o, o, w, h, RAD, col(C_BORDER));
    fill_rrect(pm, o + 1.0, o + 1.0, w - 2.0, h - 2.0, RAD - 1.0, col(C_CARD));
    fill_simple_rect(pm, o + 1.0, o + 1.0, 54.0, h - 2.0, col(C_VOTE_BG));

    let vcx = o + 28.0;
    fill_tri_up(pm, vcx, o + 20.0, 14.0, col(C_UP));
    draw_text(pm, bold, "2.4k", o + 11.0, o + 42.0, 20.0, col(C_UP));
    fill_tri_down(pm, vcx, o + 72.0, 14.0, col(C_T2));

    draw_text(pm, regular, "Posted by u/curious_mind  -  12h", o + 68.0, o + 16.0, 20.0, col(C_T2));
    draw_text(pm, bold, "What's a fact that sounds completely", o + 68.0, o + 52.0, 32.0, col(C_T1));
    draw_text(pm, bold, "made up but is actually true?", o + 68.0, o + 94.0, 32.0, col(C_T1));
    draw_text(pm, bold, "1.2k Comments    Share    Save    ...", o + 68.0, o + h - 50.0, 22.0, col(C_T2));
}

fn draw_comment_content(
    pm: &mut Pixmap,
    o: f32,
    r: CardRect,
    bold: &FontArc,
    regular: &FontArc,
    c: &CommentData,
) {
    let (w, h) = (r.w, r.h);
    fill_rrect(pm, o, o, w, h, RAD, col(C_BORDER));
    fill_rrect(pm, o + 1.0, o + 1.0, w - 2.0, h - 2.0, RAD - 1.0, col(C_CARD));

    // Thread line + avatar
    fill_simple_rect(pm, o + 34.0, o + 48.0, 3.0, h - 76.0, col(C_THREAD));
    fill_circle(pm, o + 35.0, o + 26.0, 14.0, col(c.avatar));

    // Username + time
    let ux = o + 60.0;
    draw_text(pm, bold, &format!("u/{}", c.user), ux, o + 14.0, 24.0, col(C_T1));
    let name_w = c.user.len() as f32 * 13.0 + 30.0;
    draw_text(pm, regular, &format!(" -  {}", c.time), ux + name_w, o + 16.0, 20.0, col(C_T2));

    // Body
    let mut ly = o + 55.0;
    for line in c.text.split('\n') {
        draw_text(pm, regular, line, o + 54.0, ly, 32.0, col(C_T1));
        ly += 46.0;
    }

    // Footer
    let fy = o + h - 52.0;
    fill_tri_up(pm, o + 66.0, fy + 4.0, 12.0, col(C_T2));
    draw_text(pm, bold, c.votes, o + 84.0, fy, 22.0, col(C_T1));
    let vw = c.votes.len() as f32 * 12.0 + 16.0;
    fill_tri_down(pm, o + 84.0 + vw, fy + 4.0, 12.0, col(C_T2));
    draw_text(
        pm, bold,
        &format!("Reply ({})    Share    ...", c.replies),
        o + 84.0 + vw + 30.0, fy, 22.0, col(C_T2),
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

fn fill_rrect(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, c: Color) {
    if let Some(path) = rrect_path(x, y, w, h, r) {
        pm.fill_path(&path, &make_paint(c), FillRule::Winding, Transform::identity(), None);
    }
}

fn rrect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    if w <= 0.0 || h <= 0.0 { return None; }
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
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

fn fill_simple_rect(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, c: Color) {
    if let Some(rect) = tiny_skia::Rect::from_xywh(x, y, w, h) {
        pm.fill_rect(rect, &make_paint(c), Transform::identity(), None);
    }
}

fn fill_circle(pm: &mut Pixmap, cx: f32, cy: f32, r: f32, c: Color) {
    if let Some(path) = PathBuilder::from_circle(cx, cy, r) {
        pm.fill_path(&path, &make_paint(c), FillRule::Winding, Transform::identity(), None);
    }
}

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
                let out_a = (a * 255.0 + dst.alpha() as f32 * inv).min(255.0) as u8;
                let r = r.min(out_a);
                let g = g.min(out_a);
                let b = b.min(out_a);

                if let Some(px) = PremultipliedColorU8::from_rgba(r, g, b, out_a) {
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
// Easing functions
// ===========================================================================

fn clamp01(v: f64) -> f64 { v.clamp(0.0, 1.0) }

fn ease_out_cubic(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

/// Overshoot ~1.05 then settle to 1.0
fn ease_out_back(t: f64) -> f64 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

/// Ball-bounce effect
fn ease_out_bounce(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let n1 = 7.5625;
    let d1 = 2.75;
    if t < 1.0 / d1 {
        n1 * t * t
    } else if t < 2.0 / d1 {
        let t = t - 1.5 / d1;
        n1 * t * t + 0.75
    } else if t < 2.5 / d1 {
        let t = t - 2.25 / d1;
        n1 * t * t + 0.9375
    } else {
        let t = t - 2.625 / d1;
        n1 * t * t + 0.984375
    }
}

/// Spring oscillation that settles to 1.0
fn ease_out_elastic(t: f64) -> f64 {
    if t <= 0.0 { return 0.0; }
    if t >= 1.0 { return 1.0; }
    let c4 = (2.0 * std::f64::consts::PI) / 3.0;
    2.0_f64.powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
}
