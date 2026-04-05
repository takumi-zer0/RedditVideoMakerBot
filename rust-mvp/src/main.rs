use std::io::Write;
use std::process::{Command, Stdio};

use ab_glyph::{FontArc, PxScale};
use image::{Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_rect_mut, draw_text_mut};
use imageproc::rect::Rect;

const WIDTH: u32 = 1080;
const HEIGHT: u32 = 1920;
const FPS: u32 = 30;
const DURATION: f64 = 10.0;

// Colors
const BG_TOP: [u8; 3] = [15, 10, 40];
const BG_BOTTOM: [u8; 3] = [25, 15, 60];
const REDDIT_ORANGE: [u8; 3] = [255, 69, 0];
const CARD_BG: [u8; 3] = [30, 30, 48];
const TEXT_PRIMARY: [u8; 3] = [220, 220, 235];
const TEXT_SECONDARY: [u8; 3] = [120, 120, 150];
const ACCENT_BLUE: [u8; 3] = [100, 150, 255];

struct Comment {
    username: &'static str,
    text: &'static str,
    votes: &'static str,
    start_time: f64,
}

fn main() {
    println!("Rust Video MVP - Generating {DURATION}s at {FPS}fps...");

    let font = FontArc::try_from_slice(include_bytes!("../fonts/Roboto-Bold.ttf"))
        .expect("Failed to load font");

    let comments = [
        Comment {
            username: "u/science_nerd",
            text: "What's a fact that sounds\ncompletely made up but\nis actually true?",
            votes: "2.4k",
            start_time: 2.0,
        },
        Comment {
            username: "u/history_buff42",
            text: "Honey never spoils.\nArchaeologists found 3000-year\nold honey in Egyptian tombs\nthat was still perfectly edible.",
            votes: "1.8k",
            start_time: 4.5,
        },
        Comment {
            username: "u/animal_facts",
            text: "A group of flamingos is\ncalled a 'flamboyance'.\nI'm not even kidding.",
            votes: "956",
            start_time: 7.0,
        },
    ];

    let total_frames = (DURATION * FPS as f64) as u32;

    let mut ffmpeg = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            &format!("{WIDTH}x{HEIGHT}"),
            "-r",
            &FPS.to_string(),
            "-i",
            "pipe:0",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-preset",
            "medium",
            "-crf",
            "23",
            "output.mp4",
        ])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn ffmpeg. Is ffmpeg installed?");

    {
        let stdin = ffmpeg.stdin.as_mut().unwrap();
        for i in 0..total_frames {
            let t = i as f64 / FPS as f64;
            let frame = render_frame(t, &font, &comments);
            stdin.write_all(frame.as_raw()).unwrap();

            if i % (FPS * 2) == 0 {
                println!("  {t:.1}s / {DURATION:.1}s");
            }
        }
    }
    // stdin is dropped here, closing the pipe to ffmpeg
    let result = ffmpeg.wait_with_output().unwrap();
    if result.status.success() {
        println!("Done! Saved to output.mp4");
    } else {
        eprintln!(
            "FFmpeg error:\n{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

// ---------------------------------------------------------------------------
// Frame rendering
// ---------------------------------------------------------------------------

fn render_frame(t: f64, font: &FontArc, comments: &[Comment]) -> RgbaImage {
    let mut img = RgbaImage::new(WIDTH, HEIGHT);

    draw_gradient(&mut img);
    draw_title(&mut img, t, font);

    for (i, comment) in comments.iter().enumerate() {
        draw_comment_card(&mut img, t, font, comment, i);
    }

    draw_progress_bar(&mut img, t);

    img
}

// ---------------------------------------------------------------------------
// Background
// ---------------------------------------------------------------------------

fn draw_gradient(img: &mut RgbaImage) {
    for y in 0..HEIGHT {
        let ratio = y as f64 / HEIGHT as f64;
        let r = lerp(BG_TOP[0] as f64, BG_BOTTOM[0] as f64, ratio) as u8;
        let g = lerp(BG_TOP[1] as f64, BG_BOTTOM[1] as f64, ratio) as u8;
        let b = lerp(BG_TOP[2] as f64, BG_BOTTOM[2] as f64, ratio) as u8;
        for x in 0..WIDTH {
            img.put_pixel(x, y, Rgba([r, g, b, 255]));
        }
    }
}

// ---------------------------------------------------------------------------
// Title card
// ---------------------------------------------------------------------------

fn draw_title(img: &mut RgbaImage, t: f64, font: &FontArc) {
    let opacity = fade_in_out(t, 0.0, 0.8, 8.5, 9.5);
    if opacity <= 0.01 {
        return;
    }
    let a = (opacity * 255.0) as u8;

    // Slide down from top
    let slide = ((1.0 - ease_out_cubic(clamp01(t / 0.8))) * -200.0) as i32;

    // Orange title bar
    fill_rect(
        img,
        60,
        150 + slide,
        960,
        160,
        Rgba([REDDIT_ORANGE[0], REDDIT_ORANGE[1], REDDIT_ORANGE[2], a]),
    );

    // Icon placeholder
    fill_rect(
        img,
        80,
        170 + slide,
        120,
        120,
        Rgba([255, 255, 255, (opacity * 60.0) as u8]),
    );

    // Subreddit name
    draw_text_mut(
        img,
        Rgba([255, 255, 255, a]),
        220,
        190 + slide,
        PxScale::from(44.0),
        &font,
        "r/AskReddit",
    );

    // Author
    draw_text_mut(
        img,
        Rgba([255, 255, 255, (opacity * 180.0) as u8]),
        220,
        245 + slide,
        PxScale::from(26.0),
        &font,
        "Posted by u/curious_mind  -  12h",
    );
}

// ---------------------------------------------------------------------------
// Comment card
// ---------------------------------------------------------------------------

fn draw_comment_card(
    img: &mut RgbaImage,
    t: f64,
    font: &FontArc,
    comment: &Comment,
    index: usize,
) {
    let slide_in = ease_out_cubic(clamp01((t - comment.start_time) / 0.6));
    let fade_out = 1.0 - ease_out_cubic(clamp01((t - 8.5) / 1.0));

    if t < comment.start_time || fade_out <= 0.01 {
        return;
    }

    let opacity = slide_in.min(fade_out);
    let a = (opacity * 255.0) as u8;
    let x_off = ((1.0 - slide_in) * 1200.0) as i32;

    let cx = 60 + x_off;
    let cy = 420 + (index as i32 * 400);
    let cw: u32 = 960;
    let ch: u32 = 340;

    // Shadow
    fill_rect(
        img,
        cx + 10,
        cy + 10,
        cw,
        ch,
        Rgba([0, 0, 0, (opacity * 50.0) as u8]),
    );

    // Card background
    fill_rect(
        img,
        cx,
        cy,
        cw,
        ch,
        Rgba([CARD_BG[0], CARD_BG[1], CARD_BG[2], a]),
    );

    // Left accent bar
    fill_rect(
        img,
        cx,
        cy,
        6,
        ch,
        Rgba([ACCENT_BLUE[0], ACCENT_BLUE[1], ACCENT_BLUE[2], a]),
    );

    // Vote arrow + count
    draw_text_mut(
        img,
        Rgba([255, 140, 0, a]),
        cx + 24,
        cy + 30,
        PxScale::from(30.0),
        &font,
        "^",
    );
    draw_text_mut(
        img,
        Rgba([TEXT_PRIMARY[0], TEXT_PRIMARY[1], TEXT_PRIMARY[2], a]),
        cx + 18,
        cy + 70,
        PxScale::from(24.0),
        &font,
        comment.votes,
    );

    // Username
    draw_text_mut(
        img,
        Rgba([ACCENT_BLUE[0], ACCENT_BLUE[1], ACCENT_BLUE[2], a]),
        cx + 90,
        cy + 25,
        PxScale::from(24.0),
        &font,
        comment.username,
    );

    // Comment text (multiline)
    let mut ly = cy + 75;
    for line in comment.text.split('\n') {
        draw_text_mut(
            img,
            Rgba([TEXT_PRIMARY[0], TEXT_PRIMARY[1], TEXT_PRIMARY[2], a]),
            cx + 90,
            ly,
            PxScale::from(32.0),
            &font,
            line,
        );
        ly += 46;
    }

    // Footer
    draw_text_mut(
        img,
        Rgba([TEXT_SECONDARY[0], TEXT_SECONDARY[1], TEXT_SECONDARY[2], a]),
        cx + 90,
        cy + ch as i32 - 50,
        PxScale::from(22.0),
        &font,
        "Reply    Share    Award",
    );
}

// ---------------------------------------------------------------------------
// Progress bar
// ---------------------------------------------------------------------------

fn draw_progress_bar(img: &mut RgbaImage, t: f64) {
    let w = ((t / DURATION) * WIDTH as f64) as u32;
    if w > 0 {
        fill_rect(
            img,
            0,
            HEIGHT as i32 - 6,
            w.min(WIDTH),
            6,
            Rgba([REDDIT_ORANGE[0], REDDIT_ORANGE[1], REDDIT_ORANGE[2], 200]),
        );
    }
}

// ---------------------------------------------------------------------------
// Drawing helpers
// ---------------------------------------------------------------------------

/// Draw a filled rectangle with bounds clipping.
fn fill_rect(img: &mut RgbaImage, x: i32, y: i32, w: u32, h: u32, color: Rgba<u8>) {
    let x0 = x.max(0) as u32;
    let y0 = y.max(0) as u32;
    let x1 = ((x as i64 + w as i64).min(WIDTH as i64)).max(0) as u32;
    let y1 = ((y as i64 + h as i64).min(HEIGHT as i64)).max(0) as u32;
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    draw_filled_rect_mut(
        img,
        Rect::at(x0 as i32, y0 as i32).of_size(x1 - x0, y1 - y0),
        color,
    );
}

// ---------------------------------------------------------------------------
// Easing & math utilities
// ---------------------------------------------------------------------------

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

fn ease_out_cubic(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

fn fade_in_out(t: f64, in_s: f64, in_e: f64, out_s: f64, out_e: f64) -> f64 {
    if t < in_s {
        0.0
    } else if t < in_e {
        ease_out_cubic((t - in_s) / (in_e - in_s))
    } else if t < out_s {
        1.0
    } else if t < out_e {
        1.0 - ease_out_cubic((t - out_s) / (out_e - out_s))
    } else {
        0.0
    }
}
