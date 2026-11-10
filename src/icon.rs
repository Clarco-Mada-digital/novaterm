//! Icône de NovaTerm, dessinée par programme (aucune dépendance supplémentaire) :
//! carré arrondi dégradé nuit → violet, invite « >_ » cyan et étoile « nova » lumineuse.

use image::{Rgba, RgbaImage};

type Rgb = [f32; 3];

fn hex(c: u32) -> Rgb {
    [((c >> 16) & 0xff) as f32 / 255.0, ((c >> 8) & 0xff) as f32 / 255.0, (c & 0xff) as f32 / 255.0]
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn sd_round_box(x: f32, y: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = (x - cx).abs() - hw + r;
    let qy = (y - cy).abs() - hh + r;
    let ox = qx.max(0.0);
    let oy = qy.max(0.0);
    (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - r
}

fn sd_segment(x: f32, y: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (px, py) = (x - a.0, y - a.1);
    let (bx, by) = (b.0 - a.0, b.1 - a.1);
    let h = ((px * bx + py * by) / (bx * bx + by * by)).clamp(0.0, 1.0);
    let (dx, dy) = (px - bx * h, py - by * h);
    (dx * dx + dy * dy).sqrt()
}

// Géométrie en coordonnées normalisées (0..1).
const STAR: (f32, f32) = (0.735, 0.285);
const STAR_R: f32 = 0.145;
const CHEVRON: [(f32, f32); 3] = [(0.255, 0.375), (0.445, 0.515), (0.255, 0.655)];
const STROKE: f32 = 0.042;

fn in_background(x: f32, y: f32) -> bool {
    sd_round_box(x, y, 0.5, 0.5, 0.44, 0.44, 0.2) <= 0.0
}

fn in_chevron(x: f32, y: f32) -> bool {
    sd_segment(x, y, CHEVRON[0], CHEVRON[1]).min(sd_segment(x, y, CHEVRON[1], CHEVRON[2])) <= STROKE
}

fn in_underscore(x: f32, y: f32) -> bool {
    sd_round_box(x, y, 0.62, 0.64, 0.115, 0.034, 0.03) <= 0.0
}

/// Étoile à 4 branches (astroïde), légèrement étirée verticalement.
fn star_value(x: f32, y: f32, r: f32) -> f32 {
    let dx = ((x - STAR.0) / r).abs();
    let dy = ((y - STAR.1) / (r * 1.15)).abs();
    dx.powf(0.6) + dy.powf(0.6)
}

/// Rendu de l'icône en `size`×`size` pixels (anti-aliasing 4×4).
pub fn render(size: u32) -> RgbaImage {
    let bg_a = hex(0x141833);
    let bg_b = hex(0x3B1D63);
    let cyan = hex(0x4FE8FF);
    let violet = hex(0x9A7BFF);
    let white = hex(0xF4F6FF);
    let warm = hex(0xFFE3A3);
    const SS: u32 = 4;
    let mut img = RgbaImage::new(size, size);
    let n = size as f32;
    for py in 0..size {
        for px in 0..size {
            let mut acc = [0.0f32; 3];
            let mut cov = 0.0f32;
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) / n;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) / n;
                    if !in_background(x, y) {
                        continue;
                    }
                    cov += 1.0;
                    // Fond : dégradé diagonal + reflet en haut.
                    let mut c = mix(bg_a, bg_b, ((x + y) * 0.5).clamp(0.0, 1.0));
                    let top = (1.0 - y * 1.6).max(0.0) * 0.07;
                    c = [c[0] + top, c[1] + top, c[2] + top * 1.3];
                    // Halo cyan derrière l'invite.
                    let dg = ((x - 0.4).powi(2) + (y - 0.56).powi(2)).sqrt();
                    c = mix(c, cyan, (-(dg * dg) / 0.03).exp() * 0.16);
                    // Halo chaud autour de l'étoile.
                    let ds = ((x - STAR.0).powi(2) + (y - STAR.1).powi(2)).sqrt();
                    c = mix(c, warm, (-(ds * ds) / 0.012).exp() * 0.45);
                    // Liseré intérieur.
                    let edge = sd_round_box(x, y, 0.5, 0.5, 0.44, 0.44, 0.2);
                    if edge > -0.012 {
                        c = mix(c, white, 0.10);
                    }
                    if in_chevron(x, y) {
                        c = mix(cyan, violet, ((y - 0.37) / 0.3).clamp(0.0, 1.0));
                    }
                    if in_underscore(x, y) {
                        c = white;
                    }
                    let sv = star_value(x, y, STAR_R);
                    if sv <= 1.0 {
                        c = mix(warm, white, (1.0 - sv).powf(0.5));
                    }
                    for i in 0..3 {
                        acc[i] += c[i];
                    }
                }
            }
            if cov > 0.0 {
                let to = |v: f32| ((v / cov).clamp(0.0, 1.0) * 255.0).round() as u8;
                let a = (cov / (SS * SS) as f32 * 255.0).round() as u8;
                img.put_pixel(px, py, Rgba([to(acc[0]), to(acc[1]), to(acc[2]), a]));
            }
        }
    }
    img
}

/// Icône de fenêtre (barre des tâches, Alt+Tab).
pub fn window_icon() -> egui::IconData {
    let img = render(128);
    egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() }
}

/// SVG équivalent (pour le menu des applications).
pub const SVG: &str = include_str!("../assets/icon/novaterm.svg");

/// Écrit les PNG (16 → 512 px) et le SVG dans `dir`.
pub fn export(dir: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for size in [16, 24, 32, 48, 64, 128, 256, 512] {
        let path = dir.join(format!("novaterm-{size}.png"));
        render(size).save(&path).map_err(std::io::Error::other)?;
        println!("{}", path.display());
    }
    let svg = dir.join("novaterm.svg");
    std::fs::write(&svg, SVG)?;
    println!("{}", svg.display());
    Ok(())
}
