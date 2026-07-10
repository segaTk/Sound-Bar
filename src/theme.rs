// src/theme.rs

use macroquad::prelude::*;
use std::cell::RefCell;
use chrono::{Local, Timelike};


#[derive(Debug, Clone)]
pub struct ThemeState {
    pub index:          usize,
    pub picker_open:    bool,
    pub is_day:         bool,
    /// true = is_day выставляется автоматически по системным часам.
    /// Становится false как только пользователь сам нажал «День»/«Ночь».
    pub auto_day_night: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// ЦВЕТОВЫЕ ТЕМЫ (без изменений)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub key:              &'static str,
    pub accent:           Color,
    pub accent_hover:     Color,
    pub list_selected_bg: Color,
    pub panel_border:     Color,
    pub section_title:    Color,
    pub panel_bg:         Color,
}

pub const THEMES: &[Theme] = &[
    Theme { key: "theme_blue",   accent: Color::new(0.30, 0.65, 1.00, 1.0), accent_hover: Color::new(0.45, 0.80, 1.00, 1.0), list_selected_bg: Color::new(0.11, 0.24, 0.46, 0.68), panel_border: Color::new(0.30, 0.65, 1.00, 0.90), section_title: Color::new(0.50, 0.78, 1.00, 1.0), panel_bg: Color::new(0.07, 0.08, 0.13, 0.98) },
    Theme { key: "theme_green",  accent: Color::new(0.20, 0.88, 0.45, 1.0), accent_hover: Color::new(0.30, 1.00, 0.55, 1.0), list_selected_bg: Color::new(0.08, 0.30, 0.16, 0.68), panel_border: Color::new(0.20, 0.88, 0.45, 0.90), section_title: Color::new(0.40, 1.00, 0.65, 1.0), panel_bg: Color::new(0.05, 0.10, 0.07, 0.98) },
    Theme { key: "theme_gold",   accent: Color::new(0.95, 0.78, 0.18, 1.0), accent_hover: Color::new(1.00, 0.92, 0.35, 1.0), list_selected_bg: Color::new(0.25, 0.18, 0.02, 0.68), panel_border: Color::new(0.95, 0.78, 0.18, 0.90), section_title: Color::new(1.00, 0.88, 0.28, 1.0), panel_bg: Color::new(0.10, 0.08, 0.02, 0.98) },
    Theme { key: "theme_purple", accent: Color::new(0.72, 0.30, 1.00, 1.0), accent_hover: Color::new(0.85, 0.50, 1.00, 1.0), list_selected_bg: Color::new(0.22, 0.08, 0.35, 0.68), panel_border: Color::new(0.72, 0.30, 1.00, 0.90), section_title: Color::new(0.85, 0.55, 1.00, 1.0), panel_bg: Color::new(0.08, 0.04, 0.12, 0.98) },
    Theme { key: "theme_sunset", accent: Color::new(1.00, 0.42, 0.18, 1.0), accent_hover: Color::new(1.00, 0.60, 0.30, 1.0), list_selected_bg: Color::new(0.35, 0.12, 0.04, 0.68), panel_border: Color::new(1.00, 0.42, 0.18, 0.90), section_title: Color::new(1.00, 0.65, 0.25, 1.0), panel_bg: Color::new(0.10, 0.06, 0.03, 0.98) },
    Theme { key: "theme_ice",    accent: Color::new(0.55, 0.92, 0.98, 1.0), accent_hover: Color::new(0.70, 1.00, 1.00, 1.0), list_selected_bg: Color::new(0.08, 0.24, 0.30, 0.68), panel_border: Color::new(0.55, 0.92, 0.98, 0.90), section_title: Color::new(0.70, 0.96, 1.00, 1.0), panel_bg: Color::new(0.04, 0.08, 0.12, 0.98) },
];

// ─────────────────────────────────────────────────────────────────────────────
// СОСТОЯНИЕ ТЕМЫ (без изменений)
// ─────────────────────────────────────────────────────────────────────────────

impl ThemeState {
    pub fn new() -> Self { 
        Self { index: 0, picker_open: false,is_day: Self::day_by_clock(), auto_day_night: true } }
        /// «День» — с 7:00 до 19:00 по локальному времени устройства.
    pub fn day_by_clock() -> bool {
        let h = Local::now().hour();
        (7..19).contains(&h)
    }
    /// Вызывать раз в кадр из главного цикла — держит is_day синхронным
    /// с реальным временем, пока тема не переключена вручную.
    pub fn tick_auto(&mut self) {
        if self.auto_day_night { self.is_day = Self::day_by_clock(); }
    }
    pub fn current(&self) -> &'static Theme { &THEMES[self.index % THEMES.len()] }
    pub fn next(&mut self) { self.index = (self.index + 1) % THEMES.len(); }
    pub fn draw_background(&self, t: f64) {
        if self.is_day { draw_background_day(t); } else { draw_background_stars(t); }
    }
    pub fn panel_bg(&self) -> Color { if self.is_day { DAY_PANEL_BG } else { self.current().panel_bg } }
    pub fn panel_bg_alpha(&self, night_alpha: f32) -> Color {
        if self.is_day { Color::new(DAY_PANEL_BG.r, DAY_PANEL_BG.g, DAY_PANEL_BG.b, 0.92) }
        else { let b = self.current().panel_bg; Color::new(b.r, b.g, b.b, night_alpha) }
    }
    pub fn panel_text(&self) -> Color { if self.is_day { Color::new(0.08, 0.12, 0.22, 1.0) } else { Color::new(0.80, 0.84, 0.94, 1.0) } }
    pub fn section_title(&self) -> Color { if self.is_day { Color::new(0.12, 0.22, 0.52, 1.0) } else { self.current().section_title } }
    pub fn panel_border(&self) -> Color {
        if self.is_day { let ac = self.current().accent; Color::new(ac.r*0.6+0.1, ac.g*0.4+0.2, ac.b*0.8, 0.55) }
        else { self.current().panel_border }
    }
    pub fn list_selected_bg(&self) -> Color {
        if self.is_day { let ac = self.current().accent; Color::new(ac.r*0.5, ac.g*0.5, ac.b*0.5, 0.22) }
        else { self.current().list_selected_bg }
    }
    pub fn panel_overlay(&self) -> Color { if self.is_day { Color::new(0.55, 0.70, 0.90, 0.20) } else { Color::new(0.03, 0.04, 0.08, 0.97) } }
    pub fn btn_bg(&self, hovered: bool) -> Color {
        if self.is_day { if hovered { Color::new(1.0,1.0,1.0,0.62) } else { Color::new(0.88,0.93,1.00,0.58) } }
        else { let ac = self.current().accent; if hovered { Color::new(ac.r*0.25, ac.g*0.25, ac.b*0.25, 0.88) } else { Color::new(0.08,0.09,0.13,0.80) } }
    }
    pub fn btn_text(&self, hovered: bool) -> Color {
        if self.is_day { if hovered { Color::new(0.08,0.12,0.28,1.0) } else { Color::new(0.14,0.22,0.48,0.95) } }
        else { let ac = self.current().accent; if hovered { WHITE } else { Color::new(ac.r, ac.g, ac.b, 0.90) } }
    }
    pub fn btn_border(&self, hovered: bool) -> Color {
        if self.is_day { Color::new(0.35,0.55,0.90, if hovered {0.90} else {0.48}) }
        else { let ac = self.current().accent; Color::new(ac.r, ac.g, ac.b, if hovered {0.95} else {0.45}) }
    }
}

const DAY_PANEL_BG: Color = Color::new(0.88, 0.93, 0.98, 0.90);
pub fn day_panel_bg(night: Color) -> Color {
    let mix = 0.78_f32;
    Color::new(night.r*(1.0-mix)+DAY_PANEL_BG.r*mix, night.g*(1.0-mix)+DAY_PANEL_BG.g*mix, night.b*(1.0-mix)+DAY_PANEL_BG.b*mix, 0.90)
}

// ─────────────────────────────────────────────────────────────────────────────
// LCG
// ─────────────────────────────────────────────────────────────────────────────

struct Lcg(u32);
impl Lcg {
    #[inline] fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0 as f32 / u32::MAX as f32
    }
    #[inline] fn range(&mut self, lo: f32, hi: f32) -> f32 { lo + self.next() * (hi - lo) }
}

// ─────────────────────────────────────────────────────────────────────────────
// НОЧНОЙ ФОН
// ─────────────────────────────────────────────────────────────────────────────

fn draw_background_stars(t: f64) {
    let tf = t as f32;
    let w = screen_width();
    let h = screen_height();
    for i in 0..60 {
        let x = (i as f32 * 137.508 + tf * 8.0).rem_euclid(w);
        let y = (i as f32 * 293.417 + tf * 4.5).rem_euclid(h);
        let r = 0.8 + (i % 3) as f32 * 0.6;
        let a = 0.25 + 0.25 * (tf * 1.8 + i as f32 * 0.55).sin().abs();
        draw_circle(x, y, r, Color::new(1.0, 1.0, 1.0, a));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ДНЕВНОЙ ФОН
// ─────────────────────────────────────────────────────────────────────────────

fn draw_background_day(t: f64) {
    let tf = t as f32;
    let w = screen_width();
    let h = screen_height();
    draw_sky_gradient(0.0, 0.0, w, h);
    draw_god_rays(w, h, tf, 1.0, 1.0);
    draw_all_clouds(w, h, tf, 1.0, 1.0);
}

pub fn draw_background_day_gc(gc: &crate::graphics::GraphicsContext, t: f64) {
    let tf = t as f32;
    let w = gc.base_w;
    let h = gc.base_h;
    draw_sky_gradient(gc.sx(0.0), gc.sy(0.0), gc.sx(w), gc.sy(h));
    draw_god_rays(w, h, tf, gc.scale_x, gc.scale_y);
    draw_all_clouds(w, h, tf, gc.scale_x, gc.scale_y);
}

// ─────────────────────────────────────────────────────────────────────────────
// НЕБО — градиент
// ─────────────────────────────────────────────────────────────────────────────

fn sky_col(t: f32) -> Color {
    let zenith = Color::new(0.10, 0.35, 0.78, 1.0);
    let upper  = Color::new(0.28, 0.58, 0.92, 1.0);
    let mid    = Color::new(0.52, 0.76, 0.98, 1.0);
    let horiz  = Color::new(0.82, 0.92, 1.00, 1.0);
    if t < 0.33 {
        let s = t / 0.33;
        Color::new(zenith.r+(upper.r-zenith.r)*s, zenith.g+(upper.g-zenith.g)*s, zenith.b+(upper.b-zenith.b)*s, 1.0)
    } else if t < 0.66 {
        let s = (t - 0.33) / 0.33;
        Color::new(upper.r+(mid.r-upper.r)*s, upper.g+(mid.g-upper.g)*s, upper.b+(mid.b-upper.b)*s, 1.0)
    } else {
        let s = (t - 0.66) / 0.34;
        Color::new(mid.r+(horiz.r-mid.r)*s, mid.g+(horiz.g-mid.g)*s, mid.b+(horiz.b-mid.b)*s, 1.0)
    }
}

fn draw_sky_gradient(px: f32, py: f32, pw: f32, ph: f32) {
    let bands = 80usize;
    for i in 0..bands {
        let t = (i as f32 + 0.5) / bands as f32;
        let c = sky_col(t);
        let y0 = py + ph * (i as f32 / bands as f32);
        let y1 = py + ph * ((i+1) as f32 / bands as f32);
        draw_rectangle(px, y0, pw, (y1 - y0 + 0.5).max(0.5), c);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GOD-RAYS
// ─────────────────────────────────────────────────────────────────────────────

fn draw_god_rays(w: f32, h: f32, tf: f32, sx: f32, sy: f32) {
    let src_x = w * 1.06 * sx;
    let src_y = h * -0.20 * sy;
    let ray_len = (w*sx*w*sx + h*sy*h*sy).sqrt() * 1.85;
    let slow_rot = tf * 0.008;
    for i in 0..22usize {
        let base = std::f32::consts::PI * 0.52 + (i as f32 / 22.0) * std::f32::consts::PI * 0.96;
        let angle = base + slow_rot + (i as f32 * 0.41).sin() * 0.032;
        let pulse = 0.45 + 0.55 * (tf * 0.25 + i as f32 * 0.73).sin().powi(2);
        let (alpha, hs) = match i % 3 {
            0 => (0.050 * pulse, 0.016_f32),
            1 => (0.022 * pulse, 0.009_f32),
            _ => (0.009 * pulse, 0.004_f32),
        };
        for si in 0..6usize {
            let ts = si as f32 / 5.0 - 0.5;
            let a2 = angle + ts * hs * 2.0;
            let edge = 1.0 - (ts * 2.0).powi(2);
            let la = alpha * edge * 0.9;
            if la < 0.003 { continue; }
            let thick = (ray_len * hs / 6.0 * 1.8).max(1.0);
            let ex = src_x + a2.cos() * ray_len;
            let ey = src_y + a2.sin() * ray_len;
            draw_line(src_x, src_y, ex, ey, thick, Color::new(1.0, 0.97, 0.84, la));
        }
    }
    for layer in 0..4usize {
        let lf = layer as f32 / 3.0;
        draw_circle(src_x, src_y, w*sx*(0.5+lf*0.7), Color::new(1.0, 0.96, 0.76, 0.024*(1.0-lf)));
    }
}


// ─────────────────────────────────────────────────────────────────────────────
// ГРОЗОВАЯ ТУЧА (highway дневной темы) — тёмная, с периодическими молниями
// ─────────────────────────────────────────────────────────────────────────────

/// Тёмная грозовая туча для дневной темы — ПОЛУПРОЗРАЧНАЯ дымка над highway:
/// небо и облака дневной темы должны просвечивать сквозь неё, а не быть
/// полностью перекрыты сплошной заливкой.
/// Тёмная грозовая туча для дневной темы — НИКАКОГО фонового прямоугольника,
/// только клочья-пуфы поверх обычного дневного неба/облаков. Между пуфами
/// должно быть чистое небо.
pub fn draw_storm_highway(
    gc: &crate::graphics::GraphicsContext, t: f64,
    highway_top: f32, highway_bottom: f32, highway_x: f32, highway_w: f32,
) {
    SOFT_CIRCLE_TEX.with(|c| if c.borrow().is_none() { init_cloud_textures(); });

    let tf     = t as f32;
    let band_h = highway_bottom - highway_top;

    let mut rng = Lcg(0xC10D5);
    let puff_count = 40usize;
    for i in 0..puff_count {
        let drift = (tf * (3.0 + (i % 5) as f32 * 0.4)).rem_euclid(highway_w + 300.0) - 150.0;
        let px = highway_x + (i as f32 * 53.0 + drift).rem_euclid(highway_w + 200.0) - 100.0;
        let vt = rng.range(0.0, 1.0);
        let py = highway_top + band_h * (0.10 + vt * 0.72);
        let radius = (70.0 + rng.range(0.0, 80.0)) * (0.6 + (1.0 - vt) * 0.5);

        draw_soft_circle(gc.sx(px), gc.sy(py), gc.s(radius),
            Color::new(0.05, 0.06, 0.09, 0.0), false);
        draw_soft_circle(gc.sx(px), gc.sy(py - radius * 0.30), gc.s(radius * 0.55),
            Color::new(0.35, 0.38, 0.45, 0.0), false);
    }

    // ── Молния — без изменений ───────────────────────────────────────────────
    let period = 5.5_f64;
    let phase  = t.rem_euclid(period) as f32;
    let flash_window = 0.22_f32;
    if phase < flash_window {
        let cycle = (t / period).floor() as u32;
        let mut seed = Lcg(0x9E3779B1 ^ cycle.wrapping_mul(2654435761));
        let bolt_x = highway_x + seed.range(0.15, 0.85) * highway_w;

        let flash_t = (1.0 - phase / flash_window).clamp(0.0, 1.0);
        let flash_alpha = flash_t * flash_t * 0.25;
        draw_rectangle(gc.sx(highway_x), gc.sy(highway_top), gc.sx(highway_w), gc.sy(band_h),
            Color::new(0.85, 0.90, 1.0, flash_alpha));

        let top_anchor = highway_top + band_h * 0.08;
        let mut points = vec![(bolt_x, top_anchor)];
        let mut cur_x = bolt_x;
        let segs = 7;
        for s in 1..=segs {
            let ty = top_anchor + (highway_bottom - 10.0 - top_anchor) * (s as f32 / segs as f32);
            cur_x += seed.range(-26.0, 26.0);
            points.push((cur_x, ty));
        }
        let bolt_alpha = flash_t.clamp(0.15, 1.0);
        for (thick, base_a) in [(6.0_f32, 0.18), (3.0, 0.45), (1.4, 0.95)] {
            for k in 0..points.len() - 1 {
                let (x1, y1) = points[k];
                let (x2, y2) = points[k + 1];
                draw_line(gc.sx(x1), gc.sy(y1), gc.sx(x2), gc.sy(y2), gc.s(thick),
                    Color::new(0.85, 0.92, 1.0, base_a * bolt_alpha));
            }
        }
        if seed.range(0.0, 1.0) > 0.5 && points.len() > 3 {
            let (bx, by) = points[2];
            let ex = bx + seed.range(-40.0, 40.0);
            let ey = by + seed.range(20.0, 50.0);
            draw_line(gc.sx(bx), gc.sy(by), gc.sx(ex), gc.sy(ey), gc.s(1.4),
                Color::new(0.85, 0.92, 1.0, 0.7 * bolt_alpha));
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// DYNAMIC LIGHTING — цвет освещения в зависимости от положения солнца
// ─────────────────────────────────────────────────────────────────────────────
//
// sun_angle:
//   0.0        = солнце на горизонте (рассвет)
//   PI/2 ≈ 1.57 = солнце в зените (полдень)
//   PI   ≈ 3.14 = солнце на горизонте (закат)
//
// Возвращает:
//   sun_color     — цвет прямого солнечного света (для бликов)
//   shadow_color  — цвет теневой стороны облака
//   ambient_color — цвет рассеянного света (средние тона)
//   sun_dir_x/y   — направление на солнце в 2D (для rim lighting)

struct SunLight {
    sun_color:     Color,
    shadow_color:  Color,
    ambient_color: Color,
    rim_color:     Color,
    sun_dir_x:     f32,
    sun_dir_y:     f32,
    warmth:        f32, // 0 = полдень (холодный), 1 = закат (тёплый)
}

fn sun_lighting(sun_angle: f32) -> SunLight {
    // Высота солнца над горизонтом: 0 на горизонте, 1 в зените
    let elevation = sun_angle.sin().max(0.05);
    let warmth = (1.0 - elevation).clamp(0.0, 1.0); // 1 на закате, 0 в полдень

    // Направление на солнце (для rim lighting)
    // Закат справа (x > 0), рассвет слева (x < 0), полдень — сверху
    let sun_dir_x = sun_angle.cos();
    let sun_dir_y = -sun_angle.sin(); // Y вниз на экране, поэтому отрицательный

    // Цвет солнечного света: белый днём → оранжево-красный на закате
    let sun_r = 1.00;
    let sun_g = 0.96 - warmth * 0.38; // 0.58 на закате
    let sun_b = 0.92 - warmth * 0.65; // 0.27 на закате

    // Цвет теней: голубоватый днём → фиолетовый на закате
    let sh_r = 0.52 + warmth * 0.22; // фиолетовый оттенок
    let sh_g = 0.60 - warmth * 0.12;
    let sh_b = 0.78 + warmth * 0.02;

    // Ambient (рассеянный свет неба): холодный голубой днём → тёплый на закате
    let am_r = 0.82 + warmth * 0.10;
    let am_g = 0.86 - warmth * 0.08;
    let am_b = 0.92 - warmth * 0.15;

    // Rim color — очень яркий, почти белый с оттенком солнца
    let rim_r = 1.00;
    let rim_g = 0.98 - warmth * 0.15;
    let rim_b = 0.95 - warmth * 0.35;

    SunLight {
        sun_color:     Color::new(sun_r, sun_g, sun_b, 1.0),
        shadow_color:  Color::new(sh_r, sh_g, sh_b, 1.0),
        ambient_color: Color::new(am_r, am_g, am_b, 1.0),
        rim_color:     Color::new(rim_r, rim_g, rim_b, 1.0),
        sun_dir_x,
        sun_dir_y,
        warmth,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SOFT-CIRCLE ТЕКСТУРЫ
// ─────────────────────────────────────────────────────────────────────────────

thread_local! {
    static SOFT_CIRCLE_TEX:  RefCell<Option<Texture2D>> = RefCell::new(None);
    static SOFT_CIRCLE_WISP: RefCell<Option<Texture2D>> = RefCell::new(None);
    static RAIN_SHAFT_TEX:   RefCell<Option<Texture2D>> = RefCell::new(None);
}

pub fn init_cloud_textures() {
    SOFT_CIRCLE_TEX.with(|c|  *c.borrow_mut() = Some(create_soft_circle_texture(128, 1.8)));
    SOFT_CIRCLE_WISP.with(|c| *c.borrow_mut() = Some(create_soft_circle_texture(64, 2.5)));
    RAIN_SHAFT_TEX.with(|c|   *c.borrow_mut() = Some(create_rain_shaft_texture()));
}

fn create_soft_circle_texture(size: usize, falloff_power: f32) -> Texture2D {
    let mut image = Image::gen_image_color(size as u16, size as u16, Color::new(0.0, 0.0, 0.0, 0.0));
    let center = size as f32 / 2.0;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center + 0.5;
            let dy = y as f32 - center + 0.5;
            let dist = (dx*dx + dy*dy).sqrt() / center;
            let alpha = if dist < 1.0 {
                let t = 1.0 - dist;
                t.powf(falloff_power) * (3.0 - 2.0 * t)
            } else { 0.0 };
            image.set_pixel(x.try_into().unwrap(), y.try_into().unwrap(), Color::new(1.0, 1.0, 1.0, alpha));
        }
    }
    let texture = Texture2D::from_image(&image);
    texture.set_filter(FilterMode::Linear);
    texture
}

/// Текстура для rain shafts: вертикальная полоса с мягкими краями
/// Затухает сверху вниз (сверху ярче) и по горизонтали (гауссиана)
fn create_rain_shaft_texture() -> Texture2D {
    let w = 48usize;
    let h = 320usize;
    let mut image = Image::gen_image_color(w as u16, h as u16, Color::new(0.0, 0.0, 0.0, 0.0));
    for y in 0..h {
        let yt = y as f32 / h as f32; // 0 сверху, 1 снизу
        // Вертикальный градиент: сверху максимально, снизу плавно затухает
        let vert_alpha = (1.0 - yt).powf(1.3);
        for x in 0..w {
            let xt = (x as f32 / w as f32 - 0.5) * 2.0; // -1..1
            // Гауссовский профиль по горизонтали
            let horiz_alpha = (-xt * xt * 2.5).exp();
            let alpha = vert_alpha * horiz_alpha;
            image.set_pixel(x.try_into().unwrap(), y.try_into().unwrap(), Color::new(1.0, 1.0, 1.0, alpha));
        }
    }
    let texture = Texture2D::from_image(&image);
    texture.set_filter(FilterMode::Linear);
    texture
}

fn draw_soft_circle(x: f32, y: f32, radius: f32, color: Color, is_wisp: bool) {
    let tex_cell = if is_wisp { &SOFT_CIRCLE_WISP } else { &SOFT_CIRCLE_TEX };
    tex_cell.with(|cell| {
        if let Some(tex) = cell.borrow().as_ref() {
            let size = radius * 2.0;
            let params = DrawTextureParams { dest_size: Some(Vec2::new(size, size)), ..Default::default() };
            draw_texture_ex(tex, x - radius, y - radius, color, params);
        }
    });
}

/// Рисование вертикальной полосы дождя через специальную текстуру
fn draw_rain_shaft_texture(x: f32, y: f32, width: f32, height: f32, color: Color) {
    RAIN_SHAFT_TEX.with(|cell| {
        if let Some(tex) = cell.borrow().as_ref() {
            let params = DrawTextureParams { dest_size: Some(Vec2::new(width, height)), ..Default::default() };
            draw_texture_ex(tex, x - width * 0.5, y, color, params);
        }
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// ОБЛАКА — определение
// ─────────────────────────────────────────────────────────────────────────────

struct CloudDef {
    x0:            f32,
    y_frac:        f32,
    speed:         f32,
    scale:         f32,
    seed:          u32,
    alpha:         f32,
    tower_count:   usize,
    turbulence:    f32,
    density:       f32,
    // ── Новые параметры для фотореализма ────────────────────────────────────
    wind_shear:    f32,  // 0.0..1.0 — сила сдвига ветра (верх vs низ)
    has_rain:      bool, // есть ли дождевые полосы (virga)
    rain_intensity:f32,  // 0.0..1.0 — интенсивность дождя
    rain_width:    f32,  // 0.3..1.0 — ширина зоны дождя относительно облака
}

fn make_cloud_defs() -> [CloudDef; 20] {
    [
        // Крупные кучевые с дождём и сильным wind shear
        CloudDef { x0:0.00, y_frac:0.04, speed: 14.0, scale:1.30, seed:0xA1B2, alpha:0.88, tower_count:5, turbulence:0.4, density:1.0, wind_shear:0.55, has_rain:true,  rain_intensity:0.70, rain_width:0.7 },
        CloudDef { x0:0.38, y_frac:0.06, speed: 18.0, scale:1.50, seed:0xE5F6, alpha:0.85, tower_count:6, turbulence:0.5, density:1.1, wind_shear:0.65, has_rain:true,  rain_intensity:0.85, rain_width:0.8 },
        CloudDef { x0:0.30, y_frac:0.03, speed:  8.0, scale:1.20, seed:0x9DAE, alpha:0.86, tower_count:5, turbulence:0.5, density:1.0, wind_shear:0.50, has_rain:true,  rain_intensity:0.60, rain_width:0.6 },
        CloudDef { x0:0.60, y_frac:0.05, speed: 17.0, scale:1.40, seed:0xF415, alpha:0.87, tower_count:6, turbulence:0.5, density:1.2, wind_shear:0.60, has_rain:true,  rain_intensity:0.75, rain_width:0.7 },
        CloudDef { x0:0.45, y_frac:0.10, speed:-11.0, scale:1.35, seed:0x9EAF, alpha:0.84, tower_count:6, turbulence:0.6, density:1.1, wind_shear:0.70, has_rain:true,  rain_intensity:0.80, rain_width:0.75 },
        // Средние облака — без дождя или с лёгким
        CloudDef { x0:0.18, y_frac:0.13, speed:  7.5, scale:0.75, seed:0xC3D4, alpha:0.80, tower_count:3, turbulence:0.3, density:0.8, wind_shear:0.35, has_rain:false, rain_intensity:0.0,  rain_width:0.5 },
        CloudDef { x0:0.72, y_frac:0.09, speed: 11.0, scale:1.10, seed:0x3748, alpha:0.82, tower_count:4, turbulence:0.4, density:0.9, wind_shear:0.45, has_rain:true,  rain_intensity:0.35, rain_width:0.5 },
        CloudDef { x0:0.10, y_frac:0.33, speed: 16.0, scale:0.90, seed:0x7B8C, alpha:0.74, tower_count:4, turbulence:0.4, density:0.8, wind_shear:0.40, has_rain:false, rain_intensity:0.0,  rain_width:0.5 },
        CloudDef { x0:0.80, y_frac:0.15, speed: 13.0, scale:1.05, seed:0xD1E2, alpha:0.78, tower_count:4, turbulence:0.4, density:0.9, wind_shear:0.50, has_rain:true,  rain_intensity:0.45, rain_width:0.55 },
        CloudDef { x0:0.70, y_frac:0.30, speed: -7.0, scale:1.15, seed:0x596B, alpha:0.77, tower_count:5, turbulence:0.5, density:1.0, wind_shear:0.55, has_rain:true,  rain_intensity:0.50, rain_width:0.6 },
        CloudDef { x0:0.05, y_frac:0.18, speed: 20.0, scale:0.80, seed:0x7C8D, alpha:0.73, tower_count:4, turbulence:0.4, density:0.8, wind_shear:0.45, has_rain:false, rain_intensity:0.0,  rain_width:0.5 },
        CloudDef { x0:0.35, y_frac:0.28, speed: -6.5, scale:0.95, seed:0x1637, alpha:0.75, tower_count:4, turbulence:0.4, density:0.9, wind_shear:0.40, has_rain:false, rain_intensity:0.0,  rain_width:0.5 },
        // Мелкие / дальние — лёгкий wind shear, без дождя
        CloudDef { x0:0.55, y_frac:0.20, speed: -5.5, scale:0.60, seed:0x1726, alpha:0.72, tower_count:3, turbulence:0.2, density:0.7, wind_shear:0.25, has_rain:false, rain_intensity:0.0,  rain_width:0.4 },
        CloudDef { x0:0.88, y_frac:0.26, speed: -9.0, scale:0.55, seed:0x596A, alpha:0.68, tower_count:3, turbulence:0.3, density:0.6, wind_shear:0.20, has_rain:false, rain_intensity:0.0,  rain_width:0.4 },
        CloudDef { x0:0.62, y_frac:0.40, speed:-12.0, scale:0.70, seed:0xBFC0, alpha:0.65, tower_count:3, turbulence:0.2, density:0.6, wind_shear:0.15, has_rain:false, rain_intensity:0.0,  rain_width:0.4 },
        CloudDef { x0:0.48, y_frac:0.44, speed:  6.0, scale:0.50, seed:0xF304, alpha:0.60, tower_count:3, turbulence:0.2, density:0.5, wind_shear:0.10, has_rain:false, rain_intensity:0.0,  rain_width:0.3 },
        CloudDef { x0:0.92, y_frac:0.08, speed:-15.0, scale:0.85, seed:0x1526, alpha:0.76, tower_count:4, turbulence:0.3, density:0.8, wind_shear:0.35, has_rain:false, rain_intensity:0.0,  rain_width:0.5 },
        CloudDef { x0:0.25, y_frac:0.22, speed: 10.0, scale:0.65, seed:0x3748, alpha:0.70, tower_count:3, turbulence:0.3, density:0.7, wind_shear:0.30, has_rain:false, rain_intensity:0.0,  rain_width:0.4 },
        CloudDef { x0:0.85, y_frac:0.38, speed:  9.0, scale:0.58, seed:0xB0C1, alpha:0.62, tower_count:3, turbulence:0.2, density:0.6, wind_shear:0.20, has_rain:false, rain_intensity:0.0,  rain_width:0.4 },
        CloudDef { x0:0.15, y_frac:0.42, speed:-13.0, scale:0.72, seed:0xD2E3, alpha:0.67, tower_count:3, turbulence:0.3, density:0.7, wind_shear:0.25, has_rain:false, rain_intensity:0.0,  rain_width:0.4 },
    ]
}

fn draw_all_clouds(w: f32, h: f32, tf: f32, sx: f32, sy: f32) {
    // Инициализация текстур при первом вызове
    SOFT_CIRCLE_TEX.with(|c| if c.borrow().is_none() { init_cloud_textures(); });

    // Автоматический цикл солнца: медленное колебание между рассветом и закатом
    // Период ~420 секунд (7 минут), угол от 0.4 до 2.7 радиан
    let sun_angle = std::f32::consts::FRAC_PI_2 + (tf * 0.015).sin() * 1.15;
    let light = sun_lighting(sun_angle);

    let margin = 450.0_f32;
    let loop_w = w + margin * 2.0;

    // ═══ ПРОХОД 1: Rain shafts (под всеми облаками) ═══════════════════════
    for def in &make_cloud_defs() {
        if !def.has_rain { continue; }
        let breathe = 1.0 + 0.012 * (tf * 0.12 + def.seed as f32 * 0.9).sin();
        let raw_x = (def.x0 * loop_w + tf * def.speed).rem_euclid(loop_w) - margin;
        let cy    = h * def.y_frac;
        draw_rain_shafts(raw_x, cy, def.scale * breathe, def.alpha, def.seed, sx, sy, tf, def, &light);
    }

    // ═══ ПРОХОД 2: Облака (поверх rain shafts) ════════════════════════════
    for def in &make_cloud_defs() {
        let breathe = 1.0 + 0.012 * (tf * 0.12 + def.seed as f32 * 0.9).sin();
        let raw_x = (def.x0 * loop_w + tf * def.speed).rem_euclid(loop_w) - margin;
        let cy    = h * def.y_frac;
        draw_cumulus_realistic(raw_x, cy, def.scale * breathe, def.alpha, def.seed, sx, sy, def, tf, &light);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// RAIN SHAFTS — вертикальные полосы дождя (virga)
// ─────────────────────────────────────────────────────────────────────────────
//
// Virga — дождь, испаряющийся до достижения земли. Выглядит как серые/синеватые
// вертикальные полосы, свисающие с основания облака и растворяющиеся книзу.

fn draw_rain_shafts(
    cx: f32, cy: f32, scale: f32, alpha: f32, seed: u32,
    sx: f32, sy: f32, tf: f32, def: &CloudDef, light: &SunLight
) {
    let mut rng = Lcg(seed ^ 0x7461);
    let cloud_width = 280.0 * scale * (0.8 + def.tower_count as f32 * 0.1);
    let flat_base_y = cy + 120.0 * scale * 0.28;

    // Зона дождя — центральная часть облака
    let rain_zone_w = cloud_width * def.rain_width;
    let shaft_count = 3 + (seed % 4) as usize;

    // Базовый цвет virga — смесь shadow_color и ambient с низкой яркостью
    let virga_r = light.shadow_color.r * 0.7 + light.ambient_color.r * 0.3;
    let virga_g = light.shadow_color.g * 0.7 + light.ambient_color.g * 0.3;
    let virga_b = light.shadow_color.b * 0.7 + light.ambient_color.b * 0.3;

    // Лёгкий ветер сносит virga (как и облако — wind shear)
    let wind_drift = def.wind_shear * 40.0 * scale;

    for i in 0..shaft_count {
        let t = if shaft_count > 1 { i as f32 / (shaft_count - 1) as f32 } else { 0.5 };
        let base_x = cx - rain_zone_w * 0.4 + t * rain_zone_w * 0.8;
        let jitter_x = rng.range(-15.0, 15.0) * scale;

        // Высота и ширина каждой полосы
        let shaft_height = (180.0 + rng.range(0.0, 120.0)) * scale * def.rain_intensity;
        let shaft_width  = (25.0 + rng.range(0.0, 35.0)) * scale * def.rain_intensity;

        // Мерцание/анимация — лёгкая пульсация прозрачности
        let flicker = 0.82 + 0.18 * (tf * 1.2 + i as f32 * 1.7 + seed as f32 * 0.01).sin();

        // Рисуем shaft через специальную текстуру с вертикальным градиентом
        let base_alpha = alpha * def.rain_intensity * 0.28 * flicker;

        // Основной слой virga
        draw_rain_shaft_texture(
            (base_x + jitter_x + wind_drift * 0.5) * sx,
            flat_base_y * sy,
            shaft_width * sx,
            shaft_height * sy,
            Color::new(virga_r, virga_g, virga_b, base_alpha)
        );

        // Второй, более широкий и бледный слой — для мягкости
        draw_rain_shaft_texture(
            (base_x + jitter_x * 0.7 + wind_drift * 0.3) * sx,
            (flat_base_y + shaft_height * 0.05) * sy,
            shaft_width * 1.6 * sx,
            shaft_height * 0.85 * sy,
            Color::new(virga_r, virga_g, virga_b, base_alpha * 0.45)
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// РЕАЛИСТИЧНОЕ КУЧЕВОЕ ОБЛАКО — многослойный рендеринг
// ─────────────────────────────────────────────────────────────────────────────

fn draw_cumulus_realistic(
    cx: f32, cy: f32, scale: f32, alpha: f32, seed: u32,
    sx: f32, sy: f32, def: &CloudDef, tf: f32, light: &SunLight
) {
    let mut rng = Lcg(seed);

    let cloud_width  = 280.0 * scale * (0.8 + def.tower_count as f32 * 0.1);
    let cloud_height = 120.0 * scale;
    let flat_base_y  = cy + cloud_height * 0.28;

    // ── Генерация башен с учётом wind shear ─────────────────────────────────
    let mut towers: Vec<(f32, f32, f32, f32, f32)> = Vec::with_capacity(def.tower_count);
    for i in 0..def.tower_count {
        let t = i as f32 / (def.tower_count.max(2) - 1) as f32;
        let tx = cx - cloud_width * 0.45 + t * cloud_width * 0.9;
        let center_factor = 1.0 - (t - 0.5).abs() * 2.0;
        let tower_height = cloud_height * (0.6 + center_factor * 0.4) * rng.range(0.8, 1.2);
        let tower_width  = cloud_width * 0.18 * rng.range(0.7, 1.3);
        let turb_x = rng.range(-15.0, 15.0) * scale * def.turbulence;
        let turb_y = rng.range(-10.0, 10.0) * scale * def.turbulence;
        towers.push((tx + turb_x, flat_base_y + turb_y, tower_width, tower_height, center_factor));
    }

    // ── Генерация пуфов с УЛУЧШЕННОЙ ПЛОТНОСТЬЮ ────────────────────────────
    let mut puffs: Vec<(f32, f32, f32, f32)> = Vec::new();

    for &(tx, base_y, tw, th, cf) in &towers {
        // УВЕЛИЧЕНО количество пуфов (особенно для плотности)
        let puffs_per_tower = (18.0 * def.density) as usize; // было 12.0
        
        for j in 0..puffs_per_tower {
            let vertical_t = j as f32 / puffs_per_tower as f32;
            let y_offset = -vertical_t * th;
            
            // ★ ИСПРАВЛЕНИЕ: УМЕНЬШЕН разброс по горизонтали (особенно внизу)
            // Внизу облака пуфы должны быть плотнее
            let spread_factor = if vertical_t < 0.3 {
                // Нижняя треть: очень плотная упаковка
                0.35
            } else if vertical_t < 0.6 {
                // Средняя часть: умеренный разброс
                0.55
            } else {
                // Верхушка: можно больше разброса
                0.75
            };
            
            let x_spread = tw * spread_factor * (1.0 - vertical_t * 0.2);
            let x_offset = rng.range(-x_spread, x_spread);

            // ★ ИСПРАВЛЕНИЕ: УВЕЛИЧЕНЫ радиусы пуфов (особенно внизу)
            let base_radius = tw * rng.range(0.75, 1.35); // было 0.6, 1.2
            let radius_multiplier = if vertical_t < 0.25 {
                // Самые нижние пуфы — самые большие для перекрытия
                1.15
            } else if vertical_t < 0.5 {
                1.0
            } else {
                // Верхние могут быть меньше
                0.85 - vertical_t * 0.15
            };
            
            let radius = base_radius * (0.75 + cf * 0.25) * radius_multiplier;
            let height_factor = vertical_t;

            // ★ ИСПРАВЛЕНИЕ: МЕНЬШЕ вертикального разброса внизу
            let y_jitter = if vertical_t < 0.3 {
                rng.range(-5.0, 5.0) * scale
            } else {
                rng.range(-10.0, 10.0) * scale
            };

            // WIND SHEAR
            let shear_power = vertical_t.powf(1.5);
            let wind_offset = shear_power * def.wind_shear * cloud_width * 0.22;

            puffs.push((
                tx + x_offset + wind_offset,
                base_y + y_offset + y_jitter,
                radius,
                height_factor
            ));
        }
    }

    // ★ ДОПОЛНИТЕЛЬНО: Добавляем "связующие" пуфы между башнями внизу
    if towers.len() >= 2 {
        let connector_puffs = 6;
        for i in 0..connector_puffs {
            let t = i as f32 / (connector_puffs - 1) as f32;
            
            // Интерполяция между соседними башнями
            let tower_idx = (t * (towers.len() - 1) as f32) as usize;
            let next_idx = (tower_idx + 1).min(towers.len() - 1);
            
            let (tx1, by1, tw1, _, _) = towers[tower_idx];
            let (tx2, by2, tw2, _, _) = towers[next_idx];
            
            let interp_t = (t * (towers.len() - 1) as f32) - tower_idx as f32;
            let cx_pos = tx1 + (tx2 - tx1) * interp_t;
            let cy_pos = by1 + (by2 - by1) * interp_t;
            let cw = tw1 + (tw2 - tw1) * interp_t;
            
            // Пуфы прямо на линии основания
            let cr = cw * rng.range(0.55, 0.85);
            let jitter_x = rng.range(-cw * 0.2, cw * 0.2);
            let jitter_y = rng.range(-8.0, 8.0) * scale;
            
            puffs.push((
                cx_pos + jitter_x,
                cy_pos + jitter_y,
                cr,
                0.15 // низкий уровень = тёмные тени
            ));
        }
    }

    // Сортировка по глубине (Y координате)
    puffs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

    // Цветовые палитры из dynamic lighting
    let sc = light.sun_color;
    let sh = light.shadow_color;
    let am = light.ambient_color;
    let rm = light.rim_color;

    // ═══ ПРОХОД 1: Глубокие тени (под основанием) ═════════════════════════
    for &(px, py, radius, hf) in &puffs {
        if hf < 0.35 {
            let shadow_offset = radius * 0.28;
            let shadow_alpha = alpha * 0.28 * (1.0 - hf * 2.5);
            draw_soft_circle(
                px * sx, (py + shadow_offset) * sy,
                radius * 1.3 * sx,
                Color::new(sh.r * 0.85, sh.g * 0.85, sh.b * 0.90, shadow_alpha),
                false
            );
        }
    }

    // ═══ ПРОХОД 2: Основной объём (средние тона) ══════════════════════════
    for &(px, py, radius, hf) in &puffs {
        let mix = hf;
        let r = sh.r * (1.0 - mix) + am.r * mix;
        let g = sh.g * (1.0 - mix) + am.g * mix;
        let b = sh.b * (1.0 - mix) + am.b * mix;
        draw_soft_circle(
            px * sx, py * sy,
            radius * sx,
            Color::new(r, g, b, alpha * 0.55),
            false
        );
    }

    // ═══ ПРОХОД 3: Светлые купола (верхние части башен) ═══════════════════
    for &(px, py, radius, hf) in &puffs {
        if hf > 0.25 {
            let t = (hf - 0.25) / 0.75;
            let r = am.r * (1.0 - t) + sc.r * t;
            let g = am.g * (1.0 - t) + sc.g * t;
            let b = am.b * (1.0 - t) + sc.b * t;
            let dome_alpha = alpha * 0.65 * t;
            let lift = radius * 0.12 * hf;
            draw_soft_circle(
                px * sx, (py - lift) * sy,
                radius * 0.9 * sx,
                Color::new(r, g, b, dome_alpha),
                false
            );
        }
    }

    // ═══ ПРОХОД 4: Яркие блики (вершины) ══════════════════════════════════
    for &(px, py, radius, hf) in &puffs {
        if hf > 0.6 {
            let t = (hf - 0.6) / 0.4;
            let highlight_alpha = alpha * 0.88 * t;
            let lift = radius * 0.22 * hf;
            draw_soft_circle(
                px * sx, (py - lift) * sy,
                radius * 0.55 * sx,
                Color::new(sc.r, sc.g, sc.b, highlight_alpha),
                false
            );
        }
    }

    // ═══ ПРОХОД 5: Subsurface scattering (тёплое свечение) ════════════════
    let sss_strength = 0.15 + light.warmth * 0.25;
    for &(px, py, radius, hf) in &puffs {
        if hf > 0.35 {
            let t = (hf - 0.35) / 0.65;
            let scatter_alpha = alpha * sss_strength * t;
            let warm_r = 1.0;
            let warm_g = 0.97 - light.warmth * 0.10;
            let warm_b = 0.88 - light.warmth * 0.25;
            draw_soft_circle(
                px * sx, py * sy,
                radius * 1.4 * sx,
                Color::new(warm_r, warm_g, warm_b, scatter_alpha),
                false
            );
        }
    }

    // ═══ ПРОХОД 6: RIM LIGHTING — контровой свет на краю ☀ ════════════════
    for &(px, py, radius, hf) in &puffs {
        if hf > 0.35 && radius > 5.0 {
            let rim_strength = (hf - 0.35) / 0.65;
            let rim_dx = light.sun_dir_x * radius * 0.35;
            let rim_dy = light.sun_dir_y * radius * 0.35;
            let rim_alpha = alpha * (0.35 + light.warmth * 0.25) * rim_strength;

            draw_soft_circle(
                (px + rim_dx) * sx, (py + rim_dy) * sy,
                radius * 0.38 * sx,
                Color::new(rm.r, rm.g, rm.b, rim_alpha),
                false
            );
        }
    }

    // ═══ ПРОХОД 7: Wisps (рваные края) ════════════════════════════════════
    let wisp_count = (8.0 * def.turbulence * def.density) as usize;
    for _ in 0..wisp_count {
        let angle = rng.range(0.0, std::f32::consts::TAU);
        let dist  = cloud_width * 0.5 * rng.range(0.7, 1.2);

        let wx = cx + angle.cos() * dist;
        let wy = cy + angle.sin() * dist * 0.25 - cloud_height * 0.15;

        let wisp_hf = (angle.sin() * 0.5 + 0.5).clamp(0.0, 1.0);
        let wisp_shear = wisp_hf.powf(1.5) * def.wind_shear * cloud_width * 0.15;

        let wr = 25.0 * scale * rng.range(0.4, 0.9);
        let wa = alpha * rng.range(0.20, 0.45);

        draw_soft_circle(
            (wx + wisp_shear) * sx, wy * sy,
            wr * sx,
            Color::new(am.r, am.g, am.b, wa),
            true
        );
    }

    // ═══ ПРОХОД 8: Соединительный мост по основанию ═══════════════════════
    if towers.len() >= 2 {
        let leftmost  = towers.iter().map(|t| t.0 - t.2).fold(f32::MAX, f32::min);
        let rightmost = towers.iter().map(|t| t.0 + t.2).fold(f32::MIN, f32::max);
        let bridge_segments = 16; // УВЕЛИЧЕНО с 12 до 16 для плотности
        
        for i in 0..bridge_segments {
            let t  = i as f32 / (bridge_segments - 1) as f32;
            let bx = leftmost + t * (rightmost - leftmost);
            let by = flat_base_y;
            let wave = (t * std::f32::consts::PI * 3.0).sin() * 8.0 * scale;
            let br = 38.0 * scale * rng.range(0.75, 1.15); // НЕМНОГО увеличено

            draw_soft_circle(
                bx * sx, (by + wave + br * 0.15) * sy,
                br * 1.15 * sx,
                Color::new(sh.r * 0.9, sh.g * 0.9, sh.b * 0.95, alpha * 0.35),
                false
            );
            draw_soft_circle(
                bx * sx, (by + wave) * sy,
                br * sx,
                Color::new(am.r, am.g, am.b, alpha * 0.75),
                false
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Публичные хелперы
// ─────────────────────────────────────────────────────────────────────────────

pub fn adaptive_text(is_day: bool) -> Color {
    if is_day { Color::new(0.08, 0.14, 0.28, 1.0) } else { Color::new(0.80, 0.84, 0.94, 1.0) }
}
pub fn adaptive_menu_bg(is_day: bool) -> Color {
    if is_day { Color::new(0.86, 0.92, 0.97, 0.93) } else { Color::new(0.07, 0.08, 0.12, 0.97) }
}
pub fn adaptive_row_hover(is_day: bool) -> Color {
    if is_day { Color::new(0.30, 0.50, 0.80, 0.15) } else { Color::new(0.10, 0.28, 0.14, 0.30) }
}
pub fn adaptive_row_selected(is_day: bool) -> Color {
    if is_day { Color::new(0.25, 0.45, 0.80, 0.22) } else { Color::new(0.18, 0.55, 0.28, 0.55) }
}