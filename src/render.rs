//! Rendu d'un panneau via le pipeline texte d'egui (glyphes mis en cache).

use std::collections::HashMap;
use std::sync::Arc;

use alacritty_terminal::index::Point;
use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::term::point_to_viewport;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor};
use egui::{Color32, FontFamily, FontId, Galley, Pos2, Rect, Shape, Stroke};

use crate::config::{BlinkStyle, Config};
use crate::terminal::Pane;
use crate::theme::{dim, indexed_default, rgb, Theme};

#[derive(Clone, Copy)]
pub struct FontMetrics {
    pub cell_w: f32,
    pub cell_h: f32,
}

pub fn compute_metrics(ctx: &egui::Context, config: &Config) -> FontMetrics {
    let fid = FontId::new(config.font_size, FontFamily::Name("Mono".into()));
    ctx.fonts(|f| {
        let cell_w = (f.glyph_width(&fid, 'M') + config.letter_spacing).max(1.0);
        let row_h = f.row_height(&fid).max(1.0);
        FontMetrics { cell_w, cell_h: (row_h * config.line_spacing).max(1.0) }
    })
}

fn family_name(style: u8) -> &'static str {
    match style {
        1 => "MonoBold",
        2 => "MonoItalic",
        3 => "MonoBoldItalic",
        _ => "Mono",
    }
}

fn style_of(flags: Flags) -> u8 {
    let bold = flags.contains(Flags::BOLD);
    let italic = flags.contains(Flags::ITALIC);
    match (bold, italic) {
        (false, false) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (true, true) => 3,
    }
}

/// Cache de glyphes : une galley par (caractère, style), colorée au rendu.
pub struct FontCache {
    map: HashMap<(char, u8), Arc<Galley>>,
    size: f32,
}

impl FontCache {
    pub fn new() -> Self {
        Self { map: HashMap::new(), size: 0.0 }
    }

    /// À appeler après un changement de famille de police.
    pub fn clear(&mut self) {
        self.map.clear();
    }

    fn get(&mut self, ctx: &egui::Context, ch: char, style: u8, size: f32) -> Arc<Galley> {
        if (self.size - size).abs() > f32::EPSILON {
            self.map.clear();
            self.size = size;
        }
        let key = (ch, style);
        if let Some(g) = self.map.get(&key) {
            return g.clone();
        }
        let fid = FontId::new(size, FontFamily::Name(family_name(style).into()));
        let galley = ctx.fonts(|f| f.layout_delayed_color(ch.to_string(), fid, f32::INFINITY));
        self.map.insert(key, galley.clone());
        galley
    }
}

fn named_default(n: NamedColor, theme: &Theme) -> [u8; 3] {
    let i = n as usize;
    match i {
        0..=15 => theme.ansi[i],
        256 => theme.foreground,
        257 => theme.background,
        258 => theme.cursor,
        259..=266 => dim(theme.ansi[i - 259]),
        267 => theme.ansi[15],
        268 => dim(theme.background),
        _ => theme.foreground,
    }
}

fn resolve_rgb(color: Color, colors: &Colors, theme: &Theme) -> [u8; 3] {
    match color {
        Color::Spec(c) => [c.r, c.g, c.b],
        Color::Indexed(i) => colors[i as usize]
            .map(|c| [c.r, c.g, c.b])
            .unwrap_or_else(|| indexed_default(i as usize, theme)),
        Color::Named(n) => colors[n]
            .map(|c| [c.r, c.g, c.b])
            .unwrap_or_else(|| named_default(n, theme)),
    }
}

fn in_selection(point: Point, sel: &SelectionRange) -> bool {
    if sel.is_block {
        point.line >= sel.start.line
            && point.line <= sel.end.line
            && point.column >= sel.start.column
            && point.column <= sel.end.column
    } else {
        point >= sel.start && point <= sel.end
    }
}

/// Construit les formes (fond, glyphes, sélection) d'un panneau.
pub fn build_shapes(
    ctx: &egui::Context,
    pane: &Pane,
    rect: Rect,
    theme: &Theme,
    config: &Config,
    metrics: FontMetrics,
    cache: &mut FontCache,
) -> Vec<Shape> {
    let mut shapes = Vec::new();
    let pad = config.padding;
    let area = Rect::from_min_max(
        rect.min + egui::vec2(pad, pad),
        rect.max - egui::vec2(pad, pad),
    );

    let content = pane.term.renderable_content();
    let colors = content.colors;
    let display_offset = content.display_offset;
    let selection = content.selection;

    for indexed in content.display_iter {
        let point = indexed.point;
        let cell = indexed.cell;
        let vp = match point_to_viewport(display_offset, point) {
            Some(p) => p,
            None => continue,
        };
        let flags = cell.flags;
        if flags.contains(Flags::WIDE_CHAR_SPACER) || flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) {
            continue;
        }
        let wide = flags.contains(Flags::WIDE_CHAR);
        let x = area.left() + point.column.0 as f32 * metrics.cell_w;
        let y = area.top() + vp.line as f32 * metrics.cell_h;
        let w = metrics.cell_w * if wide { 2.0 } else { 1.0 };
        let cell_rect = Rect::from_min_size(Pos2::new(x, y), egui::vec2(w, metrics.cell_h));

        let mut fg = cell.fg;
        let mut bg = cell.bg;
        if flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        let default_bg =
            bg == Color::Named(NamedColor::Background) && colors[NamedColor::Background].is_none();

        if !default_bg {
            let mut c = resolve_rgb(bg, colors, theme);
            if flags.contains(Flags::DIM) {
                c = dim(c);
            }
            shapes.push(Shape::rect_filled(cell_rect, 0.0, rgb(c)));
        }

        if let Some(sel) = &selection {
            if in_selection(point, sel) {
                let s = theme.selection;
                shapes.push(Shape::rect_filled(
                    cell_rect,
                    0.0,
                    Color32::from_rgba_unmultiplied(s[0], s[1], s[2], 170),
                ));
            }
        }

        let ch = cell.c;
        if ch != ' ' && ch != '\0' && !flags.contains(Flags::HIDDEN) {
            let galley = cache.get(ctx, ch, style_of(flags), config.font_size);
            let mut fg_rgb = resolve_rgb(fg, colors, theme);
            if flags.contains(Flags::DIM) {
                fg_rgb = dim(fg_rgb);
            }
            let color = rgb(fg_rgb);
            let gy = y + ((metrics.cell_h - galley.size().y) * 0.5).max(0.0);
            shapes.push(Shape::galley(Pos2::new(x, gy), galley, color));

            if flags.intersects(Flags::ALL_UNDERLINES) {
                shapes.push(Shape::rect_filled(
                    Rect::from_min_size(Pos2::new(x, y + metrics.cell_h - 2.0), egui::vec2(w, 1.5)),
                    0.0,
                    color,
                ));
            }
            if flags.contains(Flags::STRIKEOUT) {
                shapes.push(Shape::rect_filled(
                    Rect::from_min_size(Pos2::new(x, y + metrics.cell_h * 0.5), egui::vec2(w, 1.5)),
                    0.0,
                    color,
                ));
            }
        }
    }

    shapes
}

/// Curseur dessiné par-dessus : forme pilotée par la config, déplacement fluide
/// (traînée) et styles de clignotement (Blink/Fade/Pulse/Flip).
#[allow(clippy::too_many_arguments)]
pub fn draw_cursor(
    painter: &egui::Painter,
    pane: &mut Pane,
    rect: Rect,
    theme: &Theme,
    config: &Config,
    metrics: FontMetrics,
    focused: bool,
    time: f64,
) {
    use std::f64::consts::PI;

    let (term_shape, cursor_point, display_offset) = {
        let content = pane.term.renderable_content();
        (content.cursor.shape, content.cursor.point, content.display_offset)
    };
    // L'application peut masquer le curseur (DECTCEM) ; sinon la forme suit la config.
    if term_shape == CursorShape::Hidden {
        pane.cursor_prev_point = None;
        return;
    }
    let shape = if config.cursor_app_shape {
        term_shape
    } else {
        config.cursor_shape.to_alacritty()
    };

    let vp = match point_to_viewport(display_offset, cursor_point) {
        Some(p) => p,
        None => {
            pane.cursor_prev_point = None;
            return;
        }
    };
    let pad = config.padding;
    let origin = rect.min + egui::vec2(pad, pad);
    let target = Pos2::new(
        origin.x + cursor_point.column.0 as f32 * metrics.cell_w,
        origin.y + vp.line as f32 * metrics.cell_h,
    );
    let w = metrics.cell_w;
    let h = metrics.cell_h;

    // Déplacement fluide : interpolation ease-out sur 0.12 s quand la position change.
    let key = (cursor_point.line.0, cursor_point.column.0);
    if pane.cursor_prev_point != Some(key) {
        if config.cursor_smooth && pane.cursor_prev_point.is_some() {
            pane.cursor_from = pane.cursor_display;
            pane.cursor_move_start = time;
        } else {
            pane.cursor_display = target;
            pane.cursor_from = target;
            pane.cursor_move_start = time;
        }
        pane.cursor_prev_point = Some(key);
    }
    let t = if config.cursor_smooth {
        ((time - pane.cursor_move_start) / 0.12).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let pos = if t >= 1.0 {
        pane.cursor_display = target;
        pane.cursor_moving = false;
        target
    } else {
        let eased = 1.0 - (1.0 - t).powi(3);
        pane.cursor_moving = true;
        pane.cursor_display = pane.cursor_from.lerp(target, eased as f32);
        pane.cursor_display
    };

    let c = theme.cursor;
    if !focused {
        let cell = Rect::from_min_size(pos, egui::vec2(w, h));
        painter.rect_stroke(
            cell,
            0.0,
            Stroke::new(1.0, rgb(c).gamma_multiply(0.5)),
            egui::epaint::StrokeKind::Middle,
        );
        return;
    }

    let mut alpha = 1.0_f32;
    let mut scale_y = 1.0_f32;
    if config.cursor_blink {
        match config.cursor_blink_style {
            BlinkStyle::Blink => {
                if ((time * 2.0) as i64) % 2 != 0 {
                    return;
                }
            }
            BlinkStyle::Fade => {
                alpha = 0.15 + 0.85 * (0.5 + 0.5 * (time * 2.0 * PI).cos()) as f32;
            }
            BlinkStyle::Pulse => {
                scale_y = 0.75 + 0.25 * (0.5 + 0.5 * (time * 3.0 * PI).cos()) as f32;
            }
            BlinkStyle::Flip => {
                scale_y = ((time * 1.5 * PI).cos().abs() as f32).max(0.08);
            }
        }
    }

    // Traînée (smear) pendant le déplacement.
    if pane.cursor_moving && config.cursor_smooth {
        let a = pane.cursor_from;
        let trail = Rect::from_min_max(
            Pos2::new(a.x.min(pos.x), a.y.min(pos.y)),
            Pos2::new(a.x.max(pos.x) + w, a.y.max(pos.y) + h),
        );
        painter.rect_filled(trail, 2.0, Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 45));
    }

    let color = Color32::from_rgba_unmultiplied(c[0], c[1], c[2], (alpha * 255.0) as u8);
    let ch = h * scale_y;
    let cy = pos.y + (h - ch) * 0.5;
    match shape {
        CursorShape::Block => {
            let fill = Color32::from_rgba_unmultiplied(c[0], c[1], c[2], (alpha * 150.0) as u8);
            painter.rect_filled(
                Rect::from_min_size(Pos2::new(pos.x, cy), egui::vec2(w, ch)),
                0.0,
                fill,
            );
        }
        CursorShape::Beam => {
            painter.rect_filled(
                Rect::from_min_size(Pos2::new(pos.x, cy), egui::vec2(2.0, ch)),
                0.0,
                color,
            );
        }
        CursorShape::Underline => {
            painter.rect_filled(
                Rect::from_min_size(
                    Pos2::new(pos.x, pos.y + h - 2.0),
                    egui::vec2(w, (2.0 * scale_y).max(1.0)),
                ),
                0.0,
                color,
            );
        }
        CursorShape::HollowBlock => {
            painter.rect_stroke(
                Rect::from_min_size(Pos2::new(pos.x, cy), egui::vec2(w, ch)),
                0.0,
                Stroke::new(1.5, color),
                egui::epaint::StrokeKind::Middle,
            );
        }
        CursorShape::Hidden => {}
    }
}

/// Taille en cellules d'une zone de terminal (padding déduit).
pub fn grid_size(rect: Rect, config: &Config, metrics: FontMetrics) -> (usize, usize) {
    let pad = config.padding * 2.0;
    let cols = ((rect.width() - pad) / metrics.cell_w).floor().max(2.0) as usize;
    let rows = ((rect.height() - pad) / metrics.cell_h).floor().max(1.0) as usize;
    (cols, rows)
}

/// Convertit une position pixel en point terminal (coords grille).
pub fn pixel_to_point(
    pos: Pos2,
    rect: Rect,
    config: &Config,
    metrics: FontMetrics,
    display_offset: usize,
) -> (Point, alacritty_terminal::index::Side) {
    use alacritty_terminal::index::{Column, Side};
    use alacritty_terminal::term::viewport_to_point;
    let origin = rect.min + egui::vec2(config.padding, config.padding);
    let fx = ((pos.x - origin.x) / metrics.cell_w).max(0.0);
    let fy = ((pos.y - origin.y) / metrics.cell_h).max(0.0);
    let col = (fx.floor() as usize).max(0);
    let row = fy.floor() as usize;
    let side = if fx.fract() > 0.5 { Side::Right } else { Side::Left };
    let vp = Point::new(row, Column(col));
    (viewport_to_point(display_offset, vp), side)
}

/// Cellule écran (colonne, ligne) sous une position pixel, bornée à la grille.
pub fn pixel_to_cell(pos: Pos2, rect: Rect, config: &Config, metrics: FontMetrics, cols: usize, rows: usize) -> (usize, usize) {
    let origin = rect.min + egui::vec2(config.padding, config.padding);
    let col = ((pos.x - origin.x) / metrics.cell_w).floor().max(0.0) as usize;
    let row = ((pos.y - origin.y) / metrics.cell_h).floor().max(0.0) as usize;
    (col.min(cols.saturating_sub(1)), row.min(rows.saturating_sub(1)))
}