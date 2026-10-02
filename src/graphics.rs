// src/graphics.rs
use macroquad::prelude::*;
use cpal::traits::DeviceTrait;
use super::localization::Localization;
use std::collections::HashMap;
use macroquad::texture::{Texture2D, draw_texture_ex, DrawTextureParams};
use macroquad::math::Vec2;
use super::lessons::{LessonsFile, LessonsState, LessonStep};
use super::theme::{ThemeState, THEMES};
use super::lessons::LessonLinkAction;
use crate::tuner_presets::{get_tunings, all_tunings, GuitarTuning};
//use crate::audio_engine;

use super::{
    GameNote, SongStats, GameLogger, SearchState, MicController, GameState, Song,
    STRING_NAMES, NUM_STRINGS, TOTAL_FRETS, SPEED_MULTIPLIERS,
    SPEED_LABELS, get_note_name, safe_string_color,
    safe_string_name, group_notes_into_chords, filter_and_sort_songs, OutputController,
};

use super::{
    Font,
    game_types::{GameEvent, GameEventNote},
    lesson_game::{LessonState, HintData},
    PLAYHEAD_X, HIT_ZONE_Y, HIT_TOLERANCE, HIGHWAY_H,
    //graphics::draw_glow_circle, graphics::draw_glow_rect_lines, graphics::draw_glow_text,
};
use super::config::CalculatedLayout;

const OVL_W: f32    = 52.0;
const OVL_H: f32    = 26.0;
const OVL_MARGIN: f32 = 10.0;
const OVL_GAP: f32  = 6.0;

// ─────────────────────────────────────────────────────────────────────────────
// ПАЛИТРА КНОПОК — единые цвета для панели управления игрой и панели тюнера.
// Подобраны так, чтобы дневной фон совпадал с тоном выбранной строки в списке
// песен (adaptive_row_selected), а наведение давало затемнение в той же гамме.
// ─────────────────────────────────────────────────────────────────────────────
const BTN_PANEL_BG_DAY:      Color = Color::new(0.82, 0.89, 0.96, 0.92);
const BTN_PANEL_BORDER_DAY:  Color = Color::new(0.55, 0.65, 0.80, 0.45);
const BTN_PANEL_BG_NIGHT:    Color = Color::new(0.04, 0.05, 0.08, 0.65);
const BTN_PANEL_BORDER_NIGHT:Color = Color::new(1.00, 1.00, 1.00, 0.08);

const BTN_HOVER_BG_DAY:   Color = Color::new(0.55, 0.65, 0.80, 0.30);
const BTN_SPEED_BORDER_DAY:   Color = Color::new(0.55, 0.65, 0.80, 0.45);
const BTN_SPEED_BORDER_NIGHT: Color = Color::new(0.90, 0.90, 0.40, 0.45);

fn btn_panel_bg(is_day: bool) -> Color { if is_day { BTN_PANEL_BG_DAY } else { BTN_PANEL_BG_NIGHT } }
fn btn_panel_border(is_day: bool) -> Color { if is_day { BTN_PANEL_BORDER_DAY } else { BTN_PANEL_BORDER_NIGHT } }
fn btn_speed_border(is_day: bool) -> Color { if is_day { BTN_SPEED_BORDER_DAY } else { BTN_SPEED_BORDER_NIGHT } }

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum StatsAction {
    None,
    Restart,
    BackToSongs,
    ClearLogs,
}



/// Результат диалога подтверждения выхода
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ExitDialogResult {
    None,
    Confirm,
    Cancel,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TunerControlClicks {
    pub mic_select: bool,
    pub output_select: bool,
    pub record_toggle: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UiButtonClicks {
    pub mic: bool,
    pub record_toggle: bool,
    pub speed_down: bool,
    pub speed_up: bool,
    pub pause: bool,
    pub output: bool,
    pub automode: bool,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum MainMenuChoice {
    None,
    Lessons,
    Practice,
    Studio,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PreGameConfirmResult { None, Yes, No }

pub enum LessonsPanelResult {
    None,
    GoBack,        // ← назад (в MainMenu или в Lessons)
    GoTheory,      // → открыть Базу (только для draw_lessons_panel)
    GoTuner,       // → перейти в Тюнер (из ссылки в шаге)
    GoPractice,    // → перейти в Практику (из ссылки в шаге)
    PlaySong(String),
}

pub struct GraphicsContext {
    pub base_w: f32,
    pub base_h: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub layout: CalculatedLayout,
}

pub struct InfoImageData<'a> {
    pub texture: &'a macroquad::texture::Texture2D,
    /// Позиция X как доля ширины контентной зоны панели
    pub x: f32,
    /// Позиция Y как доля высоты контентной зоны панели
    pub y: f32,
    /// Ширина как доля ширины контентной зоны
    pub w: f32,
    pub alpha: f32,
}

impl GraphicsContext {
    pub fn new(base_w: f32, base_h: f32, layout: CalculatedLayout) -> Self {
        let sw = screen_width();
        let sh = screen_height();
        Self {
            base_w,
            base_h,
            scale_x: if sw > 0.0 { sw / base_w } else { 1.0 },
            scale_y: if sh > 0.0 { sh / base_h } else { 1.0 },
            layout,
        }
    }

    pub fn update(&mut self) {
        let sw = screen_width();
        let sh = screen_height();
        self.scale_x = if sw > 0.0 { sw / self.base_w } else { 1.0 };
        self.scale_y = if sh > 0.0 { sh / self.base_h } else { 1.0 };
    }

    pub fn sx(&self, x: f32) -> f32 { x * self.scale_x }
    pub fn sy(&self, y: f32) -> f32 { y * self.scale_y }
    pub fn s(&self, size: f32) -> f32 { size * self.scale_x.min(self.scale_y) }
}

pub fn mouse_position_logical(gc: &GraphicsContext) -> (f32, f32) {
    let (sx, sy) = mouse_position();
    (sx / gc.scale_x, sy / gc.scale_y)
}

fn draw_text_custom(gc: &GraphicsContext, t: &str, x: f32, y: f32, fs: u16, c: Color, f: &Font) {
    draw_text_ex(
        t, gc.sx(x), gc.sy(y),
        TextParams {
            font_size: gc.s(fs as f32) as u16,
            font: Some(f),
            color: c,
            ..Default::default()
        },
    );
}

pub fn draw_game_event(
    gc: &GraphicsContext,
    ev: &GameEvent,
    f: &Font,
    song_time: f64,
    lesson_mode: bool,
) {
    if ev.hit || ev.missed { return; }
    let y = ev.y; // вертикальная позиция события (летит сверху вниз)
    let is_chord = ev.notes.len() > 1;

    // ── Горизонтальная линия, связывающая ноты аккорда ─────────────────────
    if is_chord && ev.notes.len() >= 2 {
        let xs: Vec<f32> = ev.notes.iter().map(|n| n.y).collect();
        let x_min = xs.iter().cloned().fold(f32::INFINITY, f32::min);
        let x_max = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        
        // Фоновая линия (тень)
        draw_line(
            gc.sx(x_min), gc.sy(y),
            gc.sx(x_max), gc.sy(y),
            gc.s(6.0), Color::new(0.0, 0.0, 0.0, 0.35),
        );
        // Основная линия
        draw_line(
            gc.sx(x_min), gc.sy(y),
            gc.sx(x_max), gc.sy(y),
            gc.s(2.5), Color::new(0.85, 0.9, 1.0, 0.70),
        );
    }

    // ── Каждая нота аккорда ────────────────────────────────────────────────
    let time_to_hit = ev.target_time - song_time;
    let warn_secs = 0.5_f64;
    
    for note in &ev.notes {
        let x = note.y; // горизонтальная позиция ноты (зависит от string_idx)
        let sz = 28.0_f32;
        
        // Размер: чуть больше чем ближе к хит-зоне
        let distance_to_hit = (HIT_ZONE_Y - y).abs();
        let dr = 1.0 - (distance_to_hit / 350.0).clamp(0.0, 1.0);
        let scale = 1.0 + dr * 0.2;
        let cs = sz * scale;
        
        // Цвет: в lesson_mode подтверждённые — зелёные
        let base_color = if lesson_mode && note.confirmed {
            Color::new(0.2, 1.0, 0.35, 1.0)
        } else {
            note.color
        };
        
        // Кольцо предупреждения (пульсирует перед хит-зоной)
        if time_to_hit >= -0.15 && time_to_hit <= warn_secs {
            let progress = (1.0 - time_to_hit / warn_secs).clamp(0.0, 1.0) as f32;
            let pulse = ((get_time() as f32 * (6.0 + progress * 6.0)).sin() * 0.2 + 0.8).clamp(0.0, 1.0);
            let alpha = progress * 0.5 * pulse;
            draw_circle_lines(
                gc.sx(x), gc.sy(y),
                gc.s(cs * (1.4 + progress * 0.4)),
                gc.s(2.5),
                Color::new(base_color.r, base_color.g, base_color.b, alpha),
            );
        }
        
        // Основной шар
        draw_glow_circle(gc, x, y, cs, base_color, 4);
        
        // Блик
        draw_circle(
            gc.sx(x - cs * 0.3), gc.sy(y - cs * 0.3),
            gc.s(cs * 0.25),
            Color::new(1.0, 1.0, 1.0, 0.6),
        );
        
        // Имя ноты
        draw_text_ex(
            &note.note_name,
            gc.sx(x - 18.0), gc.sy(y - 10.0),
            TextParams {
                font_size: gc.s(24.0) as u16,
                font: Some(f),
                color: BLACK,
                ..Default::default()
            },
        );
        
        // В lesson_mode: галочка для подтверждённых нот
        if lesson_mode && note.confirmed {
            draw_text_ex(
                "WONDEFULL",
                gc.sx(x - 8.0), gc.sy(y + 22.0),
                TextParams {
                    font_size: gc.s(16.0) as u16,
                    font: Some(f),
                    color: Color::new(0.1, 0.95, 0.3, 1.0),
                    ..Default::default()
                },
            );
        }
    }

    // ── Название аккорда над событием ──────────────────────────────────────
    if let Some(ref name) = ev.chord_name {
        let chord_y = y - 40.0; // над нотами
        let xs: Vec<f32> = ev.notes.iter().map(|n| n.y).collect();
        let x_center = xs.iter().sum::<f32>() / xs.len() as f32;
        
        let fs = gc.s(18.0) as u16;
        let tw = measure_text(name, Some(f), fs, 1.0).width / gc.scale_x;
        draw_text_ex(
            name,
            gc.sx(x_center - tw / 2.0), gc.sy(chord_y),
            TextParams {
                font_size: fs,
                font: Some(f),
                color: Color::new(1.0, 0.92, 0.35, 0.95),
                ..Default::default()
            },
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// СТУДИЯ - UI и обработка кликов
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct StudioUiResult {
    pub song_click: Option<usize>,
    pub file_click: Option<usize>,
    pub generate_melody_click: bool,  // Генерация случайной мелодии
    pub generate_wav_click: bool,
    pub output_select: bool,
    pub piano_key_click: Option<String>,  // Название ноты при клике по клавише
    pub generate_sequence_click: bool,
}

pub fn draw_studio_screen(
    gc: &GraphicsContext,
    f: &Font,
    songs: &[Song],
    selected_song_idx: usize,
    generated_files: &[String],
    selected_file_idx: usize,
    is_playing: bool,
    is_rendering: bool,
    message: Option<&str>,
    theme: &ThemeState,
) -> StudioUiResult {
    let mut result = StudioUiResult::default();
    let is_day = theme.is_day;

    // ── Кнопка выбора выходного устройства (верхний правый угол) ────────────
    let out_btn_w = 180.0_f32;
    let out_btn_h = 30.0_f32;
    let out_btn_x = gc.base_w - out_btn_w - 20.0;
    let out_btn_y = 20.0;

    let (mx, my) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);
    let out_hover = mx >= out_btn_x && mx <= out_btn_x + out_btn_w 
        && my >= out_btn_y && my <= out_btn_y + out_btn_h;

    let out_bg = if out_hover {
        Color::new(0.4, 0.7, 0.9, 0.9)
    } else {
        Color::new(0.2, 0.5, 0.8, 0.8)
    };

    draw_rectangle(gc.sx(out_btn_x), gc.sy(out_btn_y), gc.sx(out_btn_w), gc.sy(out_btn_h), out_bg);

    if out_hover {
        draw_glow_rect_lines(gc, out_btn_x, out_btn_y, out_btn_w, out_btn_h, Color::new(0.5, 0.8, 1.0, 1.0));
    } else {
        draw_rectangle_lines(gc.sx(out_btn_x), gc.sy(out_btn_y), gc.sx(out_btn_w), gc.sy(out_btn_h),
            gc.s(1.5), Color::new(0.3, 0.6, 0.9, 0.8));
    }

    let out_text = "Выходное устройство";
    let otw = measure_text(out_text, Some(f), gc.s(13.0) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, out_text, out_btn_x + (out_btn_w - otw) / 2.0, out_btn_y + out_btn_h * 0.68, 13, WHITE, f);

    if lmb && out_hover {
        result.output_select = true;
    }
    
    // Заголовок
    let title = "STUDIO";
    let tw = measure_text(title, Some(f), gc.s(36.0) as u16, 1.0).width / gc.scale_x;
    draw_glow_text(gc, title, (gc.base_w - tw) / 2.0, 40.0, 36, 
        Color::new(0.3, 0.9, 0.6, 1.0), f);
    
    // Подсказки управления
    let hints = [
        ("W/S - Выбор песни | ↑/↓ - Выбор файла", if is_day { Color::new(0.20, 0.22, 0.35, 1.0) } else { Color::new(0.75, 0.75, 0.88, 1.0) }),
        ("Space - Воспроизвести/Стоп | R - Обновить список", if is_day { Color::new(0.20, 0.22, 0.35, 1.0) } else { Color::new(0.75, 0.75, 0.88, 1.0) }),
    ];
    
    for (i, (text, color)) in hints.iter().enumerate() {
        draw_text_custom(gc, text, 50.0, 80.0 + i as f32 * 20.0, 13, *color, f);
    }
    
    // Разделение экрана: левая половина - песни, правая - файлы + пианино
    let split_x = gc.base_w / 2.0;
    let panel_y = 140.0;
    let panel_h = gc.base_h - panel_y - 60.0;
    let left_panel_w = gc.base_w / 2.0 - 40.0;
    let right_panel_w = gc.base_w / 2.0 - 40.0;
    
    // ── ЛЕВАЯ ПАНЕЛЬ: Список песен ────────────────────────────────────────
    let left_x = 30.0;
    
    let songs_title = "SONGS";
    let stw = measure_text(songs_title, Some(f), gc.s(20.0) as u16, 1.0).width / gc.scale_x;
    draw_glow_text(gc, songs_title, left_x + (left_panel_w - stw) / 2.0, panel_y - 20.0, 20,
        if is_day { Color::new(0.10, 0.12, 0.25, 1.0) } else { Color::new(0.6, 0.8, 1.0, 1.0) }, f);
    
    // Фон панели
    draw_rectangle(gc.sx(left_x), gc.sy(panel_y), gc.sx(left_panel_w), gc.sy(panel_h),
        btn_panel_bg(is_day));
    draw_glow_rect_lines(gc, left_x, panel_y, left_panel_w, panel_h,
        Color::new(0.3, 0.5, 0.8, 0.9));
    
    // Список песен
    if songs.is_empty() {
        draw_text_custom(gc, "No songs found", left_x + 20.0, panel_y + 40.0, 16, RED, f);
    } else {
        let item_h = 32.0;
        let max_visible = ((panel_h - 20.0) / item_h).floor() as usize;
        let start_idx = if selected_song_idx >= max_visible {
            selected_song_idx - max_visible + 1
        } else {
            0
        };
        
        let (mx, my) = mouse_position_logical(gc);
        let lmb = is_mouse_button_pressed(MouseButton::Left);
        
        for (i, song) in songs.iter().enumerate().skip(start_idx).take(max_visible) {
            let vi = i - start_idx;
            let y = panel_y + 10.0 + vi as f32 * item_h;
            let is_selected = i == selected_song_idx;
            
            let is_hover = mx >= left_x + 5.0 && mx <= left_x + left_panel_w - 5.0
                && my >= y && my <= y + item_h - 2.0;
            
            let bg = if is_selected {
                if is_day { Color::new(0.75, 0.80, 0.88, 0.92) } else { Color::new(0.2, 0.4, 0.8, 0.5) }
            } else if is_hover {
                if is_day { Color::new(0.85, 0.88, 0.94, 0.60) } else { Color::new(0.2, 0.4, 0.8, 0.18) }
            } else {
                Color::new(0.0, 0.0, 0.0, 0.0)
            };
            
            draw_rectangle(gc.sx(left_x + 5.0), gc.sy(y), gc.sx(left_panel_w - 10.0), gc.sy(item_h - 2.0), bg);
            
            if is_selected {
                draw_glow_rect_lines(gc, left_x + 5.0, y, left_panel_w - 10.0, item_h - 2.0,
                    if is_day { Color::new(0.3, 0.5, 0.8, 0.8) } else { Color::new(0.4, 0.6, 1.0, 0.8) });
            }
            
            if lmb && is_hover {
                result.song_click = Some(i);
            }
            
            let prefix = if is_selected { "> " } else { "  " };
            let text_color = if is_selected {
                if is_day { Color::new(0.05, 0.08, 0.18, 1.0) } else { WHITE }
            } else if is_hover {
                if is_day { Color::new(0.10, 0.13, 0.25, 1.0) } else { Color::new(0.85, 0.88, 0.98, 1.0) }
            } else {
                if is_day { Color::new(0.35, 0.38, 0.48, 1.0) } else { Color::new(0.60, 0.62, 0.70, 1.0) }
            };
            
            let label = format!("{}{}", prefix, song.name);
            draw_text_custom(gc, &label, left_x + 15.0, y + item_h * 0.68, 14, text_color, f);
        }
        
        if songs.len() > max_visible {
            let sb_x = left_x + left_panel_w - 12.0;
            let sb_area_y = panel_y + 10.0;
            let sb_area_h = panel_h - 20.0;
            let sb_ratio = start_idx as f32 / (songs.len() - max_visible) as f32;
            let sb_h = ((max_visible as f32 / songs.len() as f32) * sb_area_h).max(18.0);
            let sb_y = sb_area_y + sb_ratio * (sb_area_h - sb_h);
            
            draw_rectangle(gc.sx(sb_x), gc.sy(sb_area_y), gc.s(6.0), gc.sy(sb_area_h),
                Color::new(0.12, 0.12, 0.18, 0.75));
            draw_rectangle(gc.sx(sb_x + 1.0), gc.sy(sb_y), gc.s(4.0), gc.sy(sb_h),
                Color::new(0.35, 0.55, 0.95, 0.88));
        }
    }
    
    // ── Кнопка генерации WAV из выбранной песни (над списком слева) ───────
    let gen_wav_btn_w = left_panel_w - 20.0;
    let gen_wav_btn_h = 40.0;
    let gen_wav_btn_x = left_x + 10.0;
    let gen_wav_btn_y = panel_y + panel_h + 8.0;
    
    let (mx, my) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);
    let hover_gen_wav = mx >= gen_wav_btn_x && mx <= gen_wav_btn_x + gen_wav_btn_w 
        && my >= gen_wav_btn_y && my <= gen_wav_btn_y + gen_wav_btn_h;
    
    let gen_wav_bg = if hover_gen_wav {
        Color::new(0.4, 0.7, 0.9, 0.9)
    } else {
        Color::new(0.2, 0.5, 0.8, 0.8)
    };
    
    draw_rectangle(gc.sx(gen_wav_btn_x), gc.sy(gen_wav_btn_y), gc.sx(gen_wav_btn_w), gc.sy(gen_wav_btn_h), gen_wav_bg);
    
    if hover_gen_wav {
        draw_glow_rect_lines(gc, gen_wav_btn_x, gen_wav_btn_y, gen_wav_btn_w, gen_wav_btn_h, Color::new(0.5, 0.8, 1.0, 1.0));
    } else {
        draw_rectangle_lines(gc.sx(gen_wav_btn_x), gc.sy(gen_wav_btn_y), gc.sx(gen_wav_btn_w), gc.sy(gen_wav_btn_h),
            gc.s(1.5), Color::new(0.3, 0.6, 0.9, 0.8));
    }
    
    let gen_wav_text = "Сгенерировать WAV из песни";
    let gvw = measure_text(gen_wav_text, Some(f), gc.s(14.0) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, gen_wav_text, gen_wav_btn_x + (gen_wav_btn_w - gvw) / 2.0, gen_wav_btn_y + gen_wav_btn_h * 0.65, 14, WHITE, f);
    
    if lmb && hover_gen_wav && !is_rendering {
        result.generate_wav_click = true;
    }
    
    // ── ПРАВАЯ ПАНЕЛЬ: Разделена на две части ─────────────────────────────
    let right_x = split_x + 10.0;
    
    // Верхняя часть: GENERATED WAV (60% высоты)
    let top_panel_h = panel_h * 0.55;
    let top_panel_y = panel_y;
    
    // Заголовок верхней панели
    let files_title = "GENERATED WAV";
    let ftw = measure_text(files_title, Some(f), gc.s(18.0) as u16, 1.0).width / gc.scale_x;
    draw_glow_text(gc, files_title, right_x + (right_panel_w - ftw) / 2.0, top_panel_y - 20.0, 18,
        if is_playing { Color::new(0.2, 1.0, 0.5, 1.0) } 
        else if is_day { Color::new(0.10, 0.12, 0.25, 1.0) } 
        else { Color::new(0.6, 0.8, 1.0, 1.0) }, f);
    
    // Фон верхней панели
    draw_rectangle(gc.sx(right_x), gc.sy(top_panel_y), gc.sx(right_panel_w), gc.sy(top_panel_h),
        btn_panel_bg(is_day));
    draw_glow_rect_lines(gc, right_x, top_panel_y, right_panel_w, top_panel_h,
        if is_playing { Color::new(0.2, 1.0, 0.5, 1.0) } else { Color::new(0.25, 0.45, 0.85, 0.9) });
    
    // Список файлов в верхней панели
    if generated_files.is_empty() {
        draw_text_custom(gc, "No generated files", right_x + 20.0, top_panel_y + 30.0, 14, 
            if is_day { Color::new(0.35, 0.38, 0.48, 1.0) } else { GRAY }, f);
    } else {
        let item_h = 26.0;
        let header_h = 30.0;
        let max_visible = ((top_panel_h - header_h - 10.0) / item_h).floor() as usize;
        let start_idx = if selected_file_idx >= max_visible {
            selected_file_idx - max_visible + 1
        } else {
            0
        };
        
        let (mx, my) = mouse_position_logical(gc);
        let lmb = is_mouse_button_pressed(MouseButton::Left);
        
        for (i, file_name) in generated_files.iter().enumerate().skip(start_idx).take(max_visible) {
            let vi = i - start_idx;
            let y = top_panel_y + header_h + vi as f32 * item_h;
            let is_selected = i == selected_file_idx;
            let is_now_playing = is_selected && is_playing;
            
            let is_hover = mx >= right_x + 5.0 && mx <= right_x + right_panel_w - 5.0
                && my >= y && my <= y + item_h - 2.0;
            
            let bg = if is_now_playing {
                Color::new(0.05, 0.30, 0.15, 0.65)
            } else if is_selected {
                if is_day { Color::new(0.75, 0.80, 0.88, 0.92) } else { Color::new(0.12, 0.25, 0.55, 0.55) }
            } else if is_hover {
                if is_day { Color::new(0.85, 0.88, 0.94, 0.60) } else { Color::new(0.08, 0.14, 0.30, 0.40) }
            } else {
                Color::new(0.0, 0.0, 0.0, 0.0)
            };
            
            draw_rectangle(gc.sx(right_x + 5.0), gc.sy(y), gc.sx(right_panel_w - 10.0), gc.sy(item_h - 2.0), bg);
            
            if is_now_playing {
                draw_glow_rect_lines(gc, right_x + 5.0, y, right_panel_w - 10.0, item_h - 2.0,
                    Color::new(0.1, 0.9, 0.4, 0.9));
            } else if is_selected {
                draw_glow_rect_lines(gc, right_x + 5.0, y, right_panel_w - 10.0, item_h - 2.0,
                    Color::new(0.2, 0.6, 1.0, 0.8));
            }
            
            if lmb && is_hover {
                result.file_click = Some(i);
            }
            
            let prefix = if is_now_playing { "▶ " } else if is_selected { "> " } else { "  " };
            let text_color = if is_now_playing {
                Color::new(0.15, 1.0, 0.5, 1.0)
            } else if is_selected {
                if is_day { Color::new(0.05, 0.08, 0.18, 1.0) } else { WHITE }
            } else if is_hover {
                if is_day { Color::new(0.10, 0.13, 0.25, 1.0) } else { Color::new(0.85, 0.88, 0.98, 1.0) }
            } else {
                if is_day { Color::new(0.35, 0.38, 0.48, 1.0) } else { Color::new(0.60, 0.62, 0.70, 1.0) }
            };
            
            let label = format!("{}{}", prefix, file_name);
            draw_text_custom(gc, &label, right_x + 15.0, y + item_h * 0.72, 12, text_color, f);
        }
        
        if generated_files.len() > max_visible {
            let sb_x = right_x + right_panel_w - 12.0;
            let sb_area_y = top_panel_y + header_h;
            let sb_area_h = top_panel_h - header_h - 10.0;
            let sb_ratio = start_idx as f32 / (generated_files.len() - max_visible) as f32;
            let sb_h = ((max_visible as f32 / generated_files.len() as f32) * sb_area_h).max(18.0);
            let sb_y = sb_area_y + sb_ratio * (sb_area_h - sb_h);
            
            draw_rectangle(gc.sx(sb_x), gc.sy(sb_area_y), gc.s(6.0), gc.sy(sb_area_h),
                Color::new(0.12, 0.12, 0.18, 0.75));
            draw_rectangle(gc.sx(sb_x + 1.0), gc.sy(sb_y), gc.s(4.0), gc.sy(sb_h),
                Color::new(0.35, 0.55, 0.95, 0.88));
        }
    }
    
    // Нижняя часть: Piano + кнопка генерации (45% высоты)
    let bottom_panel_h = panel_h * 0.40;
    let bottom_panel_y = top_panel_y + top_panel_h + 20.0;
    
    // Заголовок нижней панели
    let piano_title = "PIANO GENERATOR";
    let ptw = measure_text(piano_title, Some(f), gc.s(16.0) as u16, 1.0).width / gc.scale_x;
    draw_glow_text(gc, piano_title, right_x + (right_panel_w - ptw) / 2.0, bottom_panel_y - 28.0, 16,
        if is_day { Color::new(0.10, 0.12, 0.25, 1.0) } else { Color::new(0.8, 0.6, 1.0, 1.0) }, f);
    
    // Фон нижней панели
    draw_rectangle(gc.sx(right_x), gc.sy(bottom_panel_y), gc.sx(right_panel_w), gc.sy(bottom_panel_h),
        btn_panel_bg(is_day));
    draw_glow_rect_lines(gc, right_x, bottom_panel_y, right_panel_w, bottom_panel_h,
        Color::new(0.5, 0.3, 0.8, 0.9));
    
    // Клавиатура (занимает верхнюю часть нижней панели)
    // Клавиатура (занимает верхнюю часть нижней панели)
    let kb_x = right_x + 15.0;
    let kb_y = bottom_panel_y + 25.0;
    let kb_w = right_panel_w - 30.0;
    let kb_h = bottom_panel_h * 0.45;
    
    if let Some(note_name) = draw_piano_keys(gc, f, kb_x, kb_y, kb_w, kb_h, is_day) {
        result.piano_key_click = Some(note_name);
    }
    
    // Кнопка генерации случайной мелодии (под клавиатурой)
    let btn_x = right_x + 20.0;
    let btn_y = kb_y + kb_h + 10.0;
    let btn_w = right_panel_w - 40.0;
    let btn_h = 30.0;
    
    let (mx, my) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);
    let hover = mx >= btn_x && mx <= btn_x + btn_w && my >= btn_y && my <= btn_y + btn_h;
    
    let btn_bg = if hover {
        Color::new(0.3, 0.8, 0.4, 0.9)
    } else {
        Color::new(0.2, 0.6, 0.3, 0.8)
    };
    
    draw_rectangle(gc.sx(btn_x), gc.sy(btn_y), gc.sx(btn_w), gc.sy(btn_h), btn_bg);
    
    if hover {
        draw_glow_rect_lines(gc, btn_x, btn_y, btn_w, btn_h, Color::new(0.4, 1.0, 0.5, 1.0));
    } else {
        draw_rectangle_lines(gc.sx(btn_x), gc.sy(btn_y), gc.sx(btn_w), gc.sy(btn_h),
            gc.s(1.5), Color::new(0.3, 0.9, 0.4, 0.8));
    }
    
    let btn_text = "Сгенерировать случайную мелодию";
    let btw = measure_text(btn_text, Some(f), gc.s(12.0) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, btn_text, btn_x + (btn_w - btw) / 2.0, btn_y + btn_h * 0.65, 12, WHITE, f);
    
    if lmb && hover && !is_rendering {
        result.generate_melody_click = true;
    }
    
    // Кнопка генерации мелодии из загруженных нот (под первой кнопкой)
    let seq_btn_y = btn_y + btn_h + 8.0;
    let seq_btn_h = 30.0;
    
    let seq_hover = mx >= btn_x && mx <= btn_x + btn_w && my >= seq_btn_y && my <= seq_btn_y + seq_btn_h;
    
    let seq_btn_bg = if seq_hover {
        Color::new(0.8, 0.3, 0.4, 0.9)
    } else {
        Color::new(0.6, 0.2, 0.3, 0.8)
    };
    
    draw_rectangle(gc.sx(btn_x), gc.sy(seq_btn_y), gc.sx(btn_w), gc.sy(seq_btn_h), seq_btn_bg);
    
    if seq_hover {
        draw_glow_rect_lines(gc, btn_x, seq_btn_y, btn_w, seq_btn_h, Color::new(1.0, 0.4, 0.5, 1.0));
    } else {
        draw_rectangle_lines(gc.sx(btn_x), gc.sy(seq_btn_y), gc.sx(btn_w), gc.sy(seq_btn_h),
            gc.s(1.5), Color::new(0.9, 0.3, 0.4, 0.8));
    }
    
    let seq_btn_text = "Сгенерировать мелодию";
    let sbtw = measure_text(seq_btn_text, Some(f), gc.s(12.0) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, seq_btn_text, btn_x + (btn_w - sbtw) / 2.0, seq_btn_y + seq_btn_h * 0.65, 12, WHITE, f);
    
    if lmb && seq_hover && !is_rendering {
        result.generate_sequence_click = true;
    }
    
    // Индикатор рендеринга
    if is_rendering {
        let pulse = ((get_time() as f32 * 4.0).sin() * 0.3 + 0.7).clamp(0.0, 1.0);
        let render_text = "RENDERING...";
        let rtw = measure_text(render_text, Some(f), gc.s(24.0) as u16, 1.0).width / gc.scale_x;
        draw_glow_text(gc, render_text, (gc.base_w - rtw) / 2.0, gc.base_h / 2.0, 24,
            Color::new(1.0, 0.8, 0.2, pulse), f);
    }
    
    // Сообщение
    if let Some(msg) = message {
        let mw = measure_text(msg, Some(f), gc.s(16.0) as u16, 1.0).width / gc.scale_x;
        let msg_y = gc.base_h - 30.0;
        draw_rectangle(gc.sx(gc.base_w / 2.0 - mw / 2.0 - 10.0), gc.sy(msg_y - 20.0),
            gc.sx(mw + 20.0), gc.sy(26.0), btn_panel_bg(is_day));
        draw_glow_text(gc, msg, gc.base_w / 2.0 - mw / 2.0, msg_y, 16,
            if is_day { Color::new(0.45, 0.30, 0.0, 1.0) } else { Color::new(1.0, 0.9, 0.2, 1.0) }, f);
    }
    
    result
}
// ─────────────────────────────────────────────────────────────────────────────
// ПИАНИНО - 88 клавиш
// ─────────────────────────────────────────────────────────────────────────────

pub fn draw_piano_keyboard(
    gc: &GraphicsContext,
    f: &Font,
    theme: &ThemeState,
    message: Option<&str>,
) -> bool {
    let is_day = theme.is_day;
    
    // Размеры панели
    let panel_x = gc.base_w - 345.0;
    let panel_y = 150.0;
    let panel_w = 300.0;
    let panel_h = 500.0;
    
    // Фон панели
    draw_rectangle(gc.sx(panel_x), gc.sy(panel_y), gc.sx(panel_w), gc.sy(panel_h), btn_panel_bg(is_day));
    draw_glow_rect_lines(gc, panel_x, panel_y, panel_w, panel_h, Color::new(0.5, 0.3, 0.8, 0.9));
    
    // Заголовок
    let title = "PIANO (88 keys)";
    let title_color = if is_day { Color::new(0.10, 0.12, 0.25, 1.0) } else { Color::new(0.8, 0.6, 1.0, 1.0) };
    let tw = measure_text(title, Some(f), gc.s(17.0) as u16, 1.0).width / gc.scale_x;
    draw_glow_text(gc, title, panel_x + (panel_w - tw) / 2.0, panel_y + 28.0, 17, title_color, f);
    
    // Разделитель
    draw_line(gc.sx(panel_x + 5.0), gc.sy(panel_y + 44.0), 
              gc.sx(panel_x + panel_w - 5.0), gc.sy(panel_y + 44.0),
              gc.s(1.0), btn_panel_border(is_day));
    
    // Клавиатура
    let kb_x = panel_x + 10.0;
    let kb_y = panel_y + 55.0;
    let kb_w = panel_w - 20.0;
    let kb_h = 180.0;
    
    // Рисуем клавиатуру
    draw_piano_keys(gc, f, kb_x, kb_y, kb_w, kb_h, is_day);
    
    // Кнопка генерации
    let btn_x = panel_x + 20.0;
    let btn_y = kb_y + kb_h + 30.0;
    let btn_w = panel_w - 40.0;
    let btn_h = 50.0;
    
    let (mx, my) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);
    let hover = mx >= btn_x && mx <= btn_x + btn_w && my >= btn_y && my <= btn_y + btn_h;
    
    let btn_bg = if hover {
        Color::new(0.3, 0.8, 0.4, 0.9)
    } else {
        Color::new(0.2, 0.6, 0.3, 0.8)
    };
    
    draw_rectangle(gc.sx(btn_x), gc.sy(btn_y), gc.sx(btn_w), gc.sy(btn_h), btn_bg);
    
    if hover {
        draw_glow_rect_lines(gc, btn_x, btn_y, btn_w, btn_h, Color::new(0.4, 1.0, 0.5, 1.0));
    } else {
        draw_rectangle_lines(gc.sx(btn_x), gc.sy(btn_y), gc.sx(btn_w), gc.sy(btn_h),
            gc.s(1.5), Color::new(0.3, 0.9, 0.4, 0.8));
    }
    
    let btn_text = "🎵 Сгенерировать мелодию";
    let btw = measure_text(btn_text, Some(f), gc.s(14.0) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, btn_text, btn_x + (btn_w - btw) / 2.0, btn_y + btn_h * 0.65, 14, WHITE, f);
    
    // Подсказка
    let hint = "Случайная мелодия до 60 сек";
    let hw = measure_text(hint, Some(f), gc.s(11.0) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, hint, btn_x + (btn_w - hw) / 2.0, btn_y + btn_h + 18.0, 11,
        if is_day { Color::new(0.30, 0.34, 0.44, 0.85) } else { Color::new(0.5, 0.5, 0.6, 0.85) }, f);
    
    // Сообщение
    if let Some(msg) = message {
        let msg_y = btn_y + btn_h + 40.0;
        let mw = measure_text(msg, Some(f), gc.s(12.0) as u16, 1.0).width / gc.scale_x;
        draw_rectangle(gc.sx(panel_x + 10.0), gc.sy(msg_y - 10.0), gc.sx(panel_w - 20.0), gc.sy(30.0),
            Color::new(0.1, 0.1, 0.2, 0.9));
        draw_text_custom(gc, msg, panel_x + (panel_w - mw) / 2.0, msg_y + 8.0, 12,
            Color::new(1.0, 0.9, 0.3, 1.0), f);
    }
    
    // Возвращаем true если кнопка нажата
    hover && lmb
}

/// Отрисовка 88 клавиш фортепиано
/// Возвращает Some(note_name) если была нажата клавиша
/// Отрисовка 88 клавиш фортепиано
/// Возвращает Some(note_name) если была нажата клавиша
fn draw_piano_keys(
    gc: &GraphicsContext,
    f: &Font,
    x: f32, y: f32, w: f32, h: f32,
    is_day: bool,
) -> Option<String> {
    let (mx, my) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);
    let mut clicked_note: Option<String> = None;
    
    // 88 клавиш: 52 белых + 36 черных
    let white_key_w = w / 52.0;
    let white_key_h = h;
    let black_key_w = white_key_w * 0.6;
    let black_key_h = h * 0.6;
    
    let white_bg = if is_day {
        Color::new(0.98, 0.98, 0.98, 1.0)
    } else {
        Color::new(0.95, 0.95, 0.95, 1.0)
    };
    
    let black_bg = if is_day {
        Color::new(0.15, 0.15, 0.15, 1.0)
    } else {
        Color::new(0.1, 0.1, 0.1, 1.0)
    };
    
    let border_col = if is_day {
        Color::new(0.3, 0.3, 0.3, 0.5)
    } else {
        Color::new(0.5, 0.5, 0.5, 0.6)
    };
    
    // Рисуем белые клавиши
    let mut white_idx = 0;
    for midi_note in 21..=108 {
        // ИСПРАВЛЕНО: используем midi_note % 12 вместо (midi_note - 21) % 12
        // Это правильно определяет паттерн белых/черных клавиш относительно C
        let note_in_octave = midi_note % 12;
        
        // Белые клавиши: C(0), D(2), E(4), F(5), G(7), A(9), B(11)
        if ![1, 3, 6, 8, 10].contains(&note_in_octave) {
            let kx = x + white_idx as f32 * white_key_w;
            
            // Проверка клика
            let is_hover = mx >= kx && mx <= kx + white_key_w - 1.0
                && my >= y && my <= y + white_key_h;
            
            let bg = if is_hover {
                Color::new(0.7, 0.9, 1.0, 1.0)
            } else {
                white_bg
            };
            
            draw_rectangle(gc.sx(kx), gc.sy(y), gc.sx(white_key_w - 1.0), gc.sy(white_key_h), bg);
            draw_rectangle_lines(gc.sx(kx), gc.sy(y), gc.sx(white_key_w - 1.0), gc.sy(white_key_h),
                gc.s(1.0), border_col);
            
            if lmb && is_hover {
                clicked_note = Some(crate::audio_engine::midi_to_note_name(midi_note));
            }
            
            white_idx += 1;
        }
    }
    
    // Рисуем черные клавиши
    white_idx = 0;
    for midi_note in 21..=108 {
        // ИСПРАВЛЕНО: используем midi_note % 12
        let note_in_octave = midi_note % 12;
        
        // Черные клавиши: C#(1), D#(3), F#(6), G#(8), A#(10)
        if [1, 3, 6, 8, 10].contains(&note_in_octave) {
            let black_x = x + (white_idx as f32 - 0.5) * white_key_w;
            
            let is_hover = mx >= black_x && mx <= black_x + black_key_w
                && my >= y && my <= y + black_key_h;
            
            let bg = if is_hover {
                Color::new(0.3, 0.3, 0.5, 1.0)
            } else {
                black_bg
            };
            
            draw_rectangle(gc.sx(black_x), gc.sy(y), gc.sx(black_key_w), gc.sy(black_key_h), bg);
            draw_rectangle_lines(gc.sx(black_x), gc.sy(y), gc.sx(black_key_w), gc.sy(black_key_h),
                gc.s(1.0), Color::new(0.0, 0.0, 0.0, 0.8));
            
            if lmb && is_hover {
                clicked_note = Some(crate::audio_engine::midi_to_note_name(midi_note));
            }
        } else {
            white_idx += 1;
        }
    }
    
    // Подписи октав (только C) — ИСПРАВЛЕНО
    let label_fs = gc.s(9.0) as u16;
    let mut white_idx = 0;
    for midi_note in 21..=108 {
        let note_in_octave = midi_note % 12;
        if ![1, 3, 6, 8, 10].contains(&note_in_octave) {
            // Белая клавиша
            if note_in_octave == 0 {
                // Это нота C
                let octave = midi_note / 12 - 1;
                let label_x = x + white_idx as f32 * white_key_w + white_key_w * 0.2;
                let label_y = y + white_key_h - 15.0;
                let label = format!("C{}", octave);
                draw_text_ex(&label, gc.sx(label_x), gc.sy(label_y),
                    TextParams {
                        font_size: label_fs,
                        font: Some(f),
                        color: if is_day { Color::new(0.4, 0.4, 0.4, 0.7) } else { Color::new(0.5, 0.5, 0.5, 0.8) },
                        ..Default::default()
                    });
            }
            white_idx += 1;
        }
    }
    
    clicked_note
}


// ─────────────────────────────────────────────────────────────────────────────
// МОДУЛИ ПРАКТИКИ (Guitar / Piano / Tuner)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum PracticeModuleChoice {
    None,
    Guitar,
    Piano,
    Tuner,
}
// 1. Обновите draw_practice_modules, чтобы принимать active модуль и рисовать золотую рамку:
pub fn draw_practice_modules(
    gc: &GraphicsContext, f: &Font, theme: &ThemeState, active: PracticeModuleChoice,
) -> PracticeModuleChoice {
    let is_day = theme.is_day;
    let ac = theme.current().accent;
    let btn_w = 180.0_f32; let btn_h = 56.0_f32; let gap = 24.0_f32;
    let total_w = btn_w * 3.0 + gap * 2.0;
    let btn_y = gc.base_h - btn_h - 30.0;
    let start_x = gc.base_w / 2.0 - total_w / 2.0;
    let guitar_x = start_x; let piano_x = start_x + btn_w + gap; let tuner_x = start_x + (btn_w + gap) * 2.0;
    
    let (mx, my) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);
    let hg = mx >= guitar_x && mx <= guitar_x + btn_w && my >= btn_y && my <= btn_y + btn_h;
    let hp = mx >= piano_x  && mx <= piano_x  + btn_w && my >= btn_y && my <= btn_y + btn_h;
    let ht = mx >= tuner_x  && mx <= tuner_x  + btn_w && my >= btn_y && my <= btn_y + btn_h;

    let draw_btn = |gc: &GraphicsContext, f: &Font, x: f32, y: f32, w: f32, h: f32,
        label: &str, accent: Color, hov: bool, is_active: bool, is_day: bool| {
        let bg = if is_day {
            if hov { Color::new(0.75, 0.80, 0.88, 0.92) } else { Color::new(0.90, 0.93, 0.97, 0.85) }
        } else {
            if hov { Color::new(ac.r*0.18, ac.g*0.18, ac.b*0.18, 0.88) } else { Color::new(0.09, 0.11, 0.16, 0.78) }
        };
        draw_rectangle(gc.sx(x), gc.sy(y), gc.sx(w), gc.sy(h), bg);
        
        // Золотая рамка для активного модуля
        if is_active {
            draw_glow_rect_lines(gc, x, y, w, h, Color::new(1.0, 0.85, 0.2, 1.0));
        } else if hov {
            draw_glow_rect_lines(gc, x, y, w, h, accent);
        } else {
            draw_rectangle_lines(gc.sx(x), gc.sy(y), gc.sx(w), gc.sy(h), gc.s(1.5), Color::new(accent.r, accent.g, accent.b, 0.75));
        }
        
        let lfs = gc.s(20.0).round() as u16;
        let lw = measure_text(label, Some(f), lfs, 1.0).width / gc.scale_x;
        let tc = if is_day {
            if hov { Color::new(0.05, 0.08, 0.18, 1.0) } else { Color::new(0.10, 0.14, 0.30, 0.92) }
        } else {
            if hov { WHITE } else { Color::new(ac.r*0.85+0.15, ac.g*0.85+0.15, ac.b*0.85+0.15, 1.0) }
        };
        draw_text_ex(label, gc.sx(x + w / 2.0 - lw / 2.0), gc.sy(y + h / 2.0 + 8.0),
            TextParams { font_size: lfs, font: Some(f), color: tc, ..Default::default() });
    };

    draw_btn(gc, f, guitar_x, btn_y, btn_w, btn_h, "Guitar", Color::new(0.2, 0.9, 0.4, 1.0), hg, active == PracticeModuleChoice::Guitar, is_day);
    draw_btn(gc, f, piano_x,  btn_y, btn_w, btn_h, "Piano",  Color::new(0.8, 0.3, 0.9, 1.0), hp, active == PracticeModuleChoice::Piano, is_day);
    draw_btn(gc, f, tuner_x,  btn_y, btn_w, btn_h, "Tuner",  Color::new(0.3, 0.85, 0.55, 1.0), ht, active == PracticeModuleChoice::Tuner, is_day);

    if lmb {
        if hg { return PracticeModuleChoice::Guitar; }
        if hp { return PracticeModuleChoice::Piano; }
        if ht { return PracticeModuleChoice::Tuner; }
    }
    PracticeModuleChoice::None
}

// 2. Добавьте новые функции для отрисовки пианино в игре (в конец файла graphics.rs):

/// Возвращает (x, y, width, height) прямоугольника клавиши для заданной MIDI-ноты
pub fn get_piano_key_rect_game(midi_note: u8, px: f32, py: f32, pw: f32, ph: f32) -> (f32, f32, f32, f32) {
    let white_key_w = pw / 52.0;
    let white_key_h = ph;
    let black_key_w = white_key_w * 0.65;
    let black_key_h = ph * 0.65;

    let mut white_idx = 0;
    for mn in 21..=108 {
        let note_in_octave = mn % 12;
        let is_black = [1, 3, 6, 8, 10].contains(&note_in_octave);

        if !is_black {
            if mn == midi_note {
                return (px + white_idx as f32 * white_key_w, py, white_key_w, white_key_h);
            }
            white_idx += 1;
        } else {
            if mn == midi_note {
                let bx = px + (white_idx as f32 - 0.5) * white_key_w;
                return (bx, py, black_key_w, black_key_h);
            }
        }
    }
    (px, py, white_key_w, white_key_h)
}

/// Отрисовка игровой доски с пианино внизу и падающими нотами
pub fn draw_piano_game_board(gc: &GraphicsContext, s: &GameState, f: &Font, theme: &ThemeState) {
    let l = &gc.layout;
    let is_day = theme.is_day;

    // 1. Рисуем пианино внизу экрана, ВЫШЕ панели управления
    let piano_h = 180.0_f32;
    let piano_y = l.window_h - 82.0 - 10.0 - piano_h;
    let piano_x = 0.0;
    let piano_w = l.window_w;

    // 2. Линия зоны попадания (Hit Zone) — прямо над верхней кромкой клавиатуры
    let hit_zone_y = piano_y - 6.0;
    draw_line(gc.sx(0.0), gc.sy(hit_zone_y), gc.sx(l.window_w), gc.sy(hit_zone_y),
        gc.s(3.0), Color::new(0.2, 0.9, 0.3, 0.9));

    // 3. Собираем информацию об активных нотах (для подсветки клавиш)
    //    Ключ: midi_note, Значение: (color, progress)
    let mut active_notes: std::collections::HashMap<u8, (Color, f32)> = std::collections::HashMap::new();
    let warn_secs = 0.5_f64;

    for ev in &s.events {
        if ev.hit || ev.missed { continue; }
        let time_to_hit = ev.target_time - s.song_time;
        if time_to_hit >= -0.15 && time_to_hit <= warn_secs {
            let progress = (1.0 - time_to_hit / warn_secs).clamp(0.0, 1.0) as f32;
            for note in &ev.notes {
                let entry = active_notes.entry(note.midi_note).or_insert((note.color, 0.0));
                if progress > entry.1 {
                    entry.0 = note.color;
                    entry.1 = progress;
                }
            }
        }
    }

    // ── Индикатор MIDI подключения (правый верхний угол) ───
    let midi_connected = s.midi_data.is_some();
    let indicator_x = gc.base_w - 180.0;
    let indicator_y = 20.0;
    let indicator_w = 160.0;
    let indicator_h = 30.0;
    
    let bg_color = if midi_connected {
        Color::new(0.2, 0.6, 0.3, 0.9)
    } else {
        Color::new(0.6, 0.2, 0.2, 0.9)
    };
    
    draw_rectangle(
        gc.sx(indicator_x), gc.sy(indicator_y),
        gc.sx(indicator_w), gc.sy(indicator_h),
        bg_color
    );
    
    draw_rectangle_lines(
        gc.sx(indicator_x), gc.sy(indicator_y),
        gc.sx(indicator_w), gc.sy(indicator_h),
        gc.s(2.0),
        if midi_connected {
            Color::new(0.3, 0.8, 0.4, 1.0)
        } else {
            Color::new(0.8, 0.3, 0.3, 1.0)
        }
    );
    
    let status_text = if midi_connected {
        "MIDI: Подключено"
    } else {
        "MIDI: Нет сигнала"
    };
    
    let text_color = WHITE;
    let text_w = measure_text(status_text, Some(f), gc.s(14.0) as u16, 1.0).width / gc.scale_x;
    
    draw_text_ex(
        status_text,
        gc.sx(indicator_x + (indicator_w - text_w) / 2.0),
        gc.sy(indicator_y + indicator_h * 0.68),
        TextParams {
            font_size: gc.s(14.0) as u16,
            font: Some(f),
            color: text_color,
            ..Default::default()
        }
    );

    // 3.5 Собираем MIDI-активные ноты
    let midi_active: Vec<u8> = if let Some(ref midi) = s.midi_data {
        midi.lock().map(|d| d.active_notes.clone()).unwrap_or_default()
    } else {
        Vec::new()
    };

    // 4. Рисуем клавиатуру с подсветкой активных нот
    draw_piano_keys_game(gc, f, piano_x, piano_y, piano_w, piano_h, is_day, &active_notes, &midi_active);
    // 4.5 Нотный стан в правом верхнем углу
    draw_mini_notation_top_right(gc, s, f, theme);        
    // 5. Падающие ноты + ромбики на клавишах
    for ev in &s.events {
        if ev.hit || ev.missed { continue; }
        let time_to_hit = ev.target_time - s.song_time;

        for note in &ev.notes {
            // Точные координаты клавиши, над которой должна падать нота
            let (kx, ky, kw, kh) = get_piano_key_rect_game(note.midi_note, piano_x, piano_y, piano_w, piano_h);
            let target_x = kx + kw / 2.0;
            let ny = ev.y; // Вертикальная позиция падения

            // Увеличение ноты при приближении
            let distance_to_hit = (hit_zone_y - ny).abs();
            let dr = 1.0 - (distance_to_hit / 350.0).clamp(0.0, 1.0);
            let scale = 1.0 + dr * 0.2;
            let cs = 28.0_f32 * scale;

            // Ромбик на клавише в момент попадания
            if time_to_hit >= -0.15 && time_to_hit <= warn_secs {
                let progress = (1.0 - time_to_hit / warn_secs).clamp(0.0, 1.0) as f32;
                let pulse = ((get_time() as f32 * (6.0 + progress * 6.0)).sin() * 0.2 + 0.8).clamp(0.0, 1.0);
                let alpha = progress * 0.8 * pulse;

                let diamond_size = gc.s(14.0 + progress * 8.0);
                // Ромбик рисуется РОВНО по центру клавиши
                let cx = gc.sx(target_x);
                let cy = gc.sy(ky + kh * 0.5); // центр клавиши по вертикали

                // Внешний glow ромба
                for layer in 1..=3u8 {
                    let lf = layer as f32 / 3.0;
                    let ls = diamond_size * (1.0 + lf * 0.8);
                    let la = alpha * (1.0 - lf) * 0.4;
                    draw_triangle(Vec2::new(cx, cy - ls), Vec2::new(cx + ls * 0.65, cy), Vec2::new(cx - ls * 0.65, cy),
                        Color::new(note.color.r, note.color.g, note.color.b, la));
                    draw_triangle(Vec2::new(cx, cy + ls), Vec2::new(cx + ls * 0.65, cy), Vec2::new(cx - ls * 0.65, cy),
                        Color::new(note.color.r, note.color.g, note.color.b, la));
                }

                // Основной ромб цвета ноты
                draw_triangle(Vec2::new(cx, cy - diamond_size), Vec2::new(cx + diamond_size * 0.65, cy), Vec2::new(cx - diamond_size * 0.65, cy),
                    Color::new(note.color.r, note.color.g, note.color.b, alpha));
                draw_triangle(Vec2::new(cx, cy + diamond_size), Vec2::new(cx + diamond_size * 0.65, cy), Vec2::new(cx - diamond_size * 0.65, cy),
                    Color::new(note.color.r, note.color.g, note.color.b, alpha));

                // Белое ядро ромба для контраста
                let core = diamond_size * 0.3;
                draw_triangle(Vec2::new(cx, cy - core), Vec2::new(cx + core * 0.65, cy), Vec2::new(cx - core * 0.65, cy),
                    Color::new(1.0, 1.0, 1.0, alpha * 0.85));
                draw_triangle(Vec2::new(cx, cy + core), Vec2::new(cx + core * 0.65, cy), Vec2::new(cx - core * 0.65, cy),
                    Color::new(1.0, 1.0, 1.0, alpha * 0.85));
            }

            // Рисуем саму падающую ноту (цвет теперь радужный)
            draw_glow_circle(gc, target_x, ny, cs, note.color, 4);

            // Имя ноты
            draw_text_ex(&note.note_name, gc.sx(target_x - 18.0), gc.sy(ny - 10.0),
                TextParams { font_size: gc.s(24.0) as u16, font: Some(f), color: BLACK, ..Default::default() });
        }
    }

    // 6. Частицы и вспышки
    for p in &s.particles {
        let al = (p.life / 0.7).min(1.0);
        draw_circle(gc.sx(p.x), gc.sy(p.y), gc.s(p.size), Color::new(p.color.r, p.color.g, p.color.b, al));
    }
    if s.flash_timer > 0. {
        let al = (s.flash_timer / 0.15).min(1.0) * 0.4;
        draw_rectangle(gc.sx(0.), gc.sy(0.), gc.sx(l.window_w), gc.sy(l.window_h),
            Color::new(s.flash_color.r, s.flash_color.g, s.flash_color.b, al));
    }
}

/// Отрисовка 88 клавиш с подсветкой активных нот их радужным цветом
fn draw_piano_keys_game(
    gc: &GraphicsContext, f: &Font,
    px: f32, py: f32, pw: f32, ph: f32,
    is_day: bool,
    active_notes: &std::collections::HashMap<u8, (Color, f32)>,
    midi_active_notes: &[u8],
) {
    let white_key_w = pw / 52.0;
    let white_key_h = ph;
    let black_key_w = white_key_w * 0.65;
    let black_key_h = ph * 0.65;

    let white_bg = if is_day { Color::new(0.98, 0.98, 0.98, 1.0) } else { Color::new(0.95, 0.95, 0.95, 1.0) };
    let black_bg = if is_day { Color::new(0.15, 0.15, 0.15, 1.0) } else { Color::new(0.1, 0.1, 0.1, 1.0) };
    let border_col = if is_day { Color::new(0.3, 0.3, 0.3, 0.5) } else { Color::new(0.5, 0.5, 0.5, 0.6) };

    // Рисуем белые клавиши
    let mut white_idx = 0;
    for midi_note in 21..=108u8 {
        let note_in_octave = midi_note % 12;
        if ![1, 3, 6, 8, 10].contains(&note_in_octave) {
            let kx = px + white_idx as f32 * white_key_w;

            // Подсветка: приоритет — физически нажатая MIDI-клавиша
            let bg = if midi_active_notes.contains(&midi_note) {
                // Клавиша физически нажата на MIDI-клавиатуре — яркая подсветка
                if let Some((color, _)) = active_notes.get(&midi_note) {
                    Color::new(color.r * 0.7 + 0.3, color.g * 0.7 + 0.3, color.b * 0.7 + 0.3, 1.0)
                } else {
                    Color::new(0.6, 0.8, 1.0, 1.0) // голубая подсветка
                }
            } else if let Some((color, progress)) = active_notes.get(&midi_note) {
                // Падающая нота приближается
                let t = progress * 0.6;
                Color::new(
                    white_bg.r * (1.0 - t) + color.r * t,
                    white_bg.g * (1.0 - t) + color.g * t,
                    white_bg.b * (1.0 - t) + color.b * t,
                    1.0,
                )
            } else {
                white_bg
            };

            draw_rectangle(gc.sx(kx), gc.sy(py), gc.sx(white_key_w - 1.0), gc.sy(white_key_h), bg);
            draw_rectangle_lines(gc.sx(kx), gc.sy(py), gc.sx(white_key_w - 1.0), gc.sy(white_key_h),
                gc.s(1.0), border_col);

            // Подписи октав (C)
            if note_in_octave == 0 {
                let octave = midi_note / 12 - 1;
                let label = format!("C{}", octave);
                let lfs = gc.s(10.0) as u16;
                let lw = measure_text(&label, Some(f), lfs, 1.0).width / gc.scale_x;
                draw_text_ex(&label, gc.sx(kx + white_key_w * 0.5 - lw / 2.0), gc.sy(py + white_key_h - 15.0),
                    TextParams { font_size: lfs, font: Some(f), color: Color::new(0.4, 0.4, 0.4, 0.7), ..Default::default() });
            }
            white_idx += 1;
        }
    }

    // Рисуем черные клавиши
    white_idx = 0;
    for midi_note in 21..=108u8 {
        let note_in_octave = midi_note % 12;
        if [1, 3, 6, 8, 10].contains(&note_in_octave) {
            let black_x = px + (white_idx as f32 - 0.5) * white_key_w;

            let bg = if midi_active_notes.contains(&midi_note) {
                if let Some((color, _)) = active_notes.get(&midi_note) {
                    Color::new(color.r * 0.7 + 0.3, color.g * 0.7 + 0.3, color.b * 0.7 + 0.3, 1.0)
                } else {
                    Color::new(0.4, 0.55, 0.85, 1.0) // светло-голубая подсветка
                }
            } else if let Some((color, progress)) = active_notes.get(&midi_note) {
                let t = progress * 0.7;
                Color::new(
                    black_bg.r * (1.0 - t) + color.r * t,
                    black_bg.g * (1.0 - t) + color.g * t,
                    black_bg.b * (1.0 - t) + color.b * t,
                    1.0,
                )
            } else {
                black_bg
            };

            draw_rectangle(gc.sx(black_x), gc.sy(py), gc.sx(black_key_w), gc.sy(black_key_h), bg);
            draw_rectangle_lines(gc.sx(black_x), gc.sy(py), gc.sx(black_key_w), gc.sy(black_key_h),
                gc.s(1.0), Color::new(0.0, 0.0, 0.0, 0.8));
        } else {
            white_idx += 1;
        }
    }
}

// ─── UI паузы в режиме урока ──────────────────────────────────────────────────
 
/// Рисует оверлей паузы урока: прогресс, подсказка, инструкция.
/// Вызывать поверх draw_fretboard() когда lesson.paused == true.
pub fn draw_lesson_pause_overlay(
    gc: &GraphicsContext,
    f: &Font,
    lesson: &LessonState,
    hint: Option<&HintData>,
    event: Option<&GameEvent>,
) {
    let w = gc.base_w;
    let _h = gc.base_h;
 
    // Полупрозрачная плашка по центру снизу
    let box_w = 520.0_f32;
    let box_h = if hint.is_some() { 310.0_f32 } else { 130.0_f32 };
    let box_x = (w - box_w) / 2.0;
    let box_y = HIT_ZONE_Y + 60.0;
 
    draw_rectangle(
        gc.sx(box_x), gc.sy(box_y),
        gc.sx(box_w), gc.sy(box_h),
        Color::new(0.04, 0.05, 0.10, 0.92),
    );
    draw_glow_rect_lines(gc, box_x, box_y, box_w, box_h,
        Color::new(0.3, 0.75, 1.0, 0.85));
 
    // Заголовок
    let title = "Сыграйте аккорд";
    let tfs = gc.s(20.0) as u16;
    let tw = measure_text(title, Some(f), tfs, 1.0).width / gc.scale_x;
    draw_glow_text(gc, title, box_x + (box_w - tw) / 2.0, box_y + 28.0,
        20, Color::new(0.3, 0.85, 1.0, 1.0), f);
 
    // Прогресс-бар подтверждения
    if let Some(ev) = event {
        let progress = ev.confirm_progress();
        let bar_w = box_w - 60.0;
        let bar_h = 10.0_f32;
        let bar_x = box_x + 30.0;
        let bar_y = box_y + 44.0;
 
        draw_rectangle(gc.sx(bar_x), gc.sy(bar_y), gc.sx(bar_w), gc.sy(bar_h),
            Color::new(0.15, 0.18, 0.25, 0.9));
        draw_rectangle(
            gc.sx(bar_x), gc.sy(bar_y),
            gc.sx(bar_w * progress), gc.sy(bar_h),
            Color::new(0.2, 1.0, 0.4, 0.9),
        );
        draw_rectangle_lines(gc.sx(bar_x), gc.sy(bar_y), gc.sx(bar_w), gc.sy(bar_h),
            gc.s(1.0), Color::new(0.3, 0.5, 0.7, 0.6));
 
        // Счётчик нот
        let count_str = format!("{}/{}", ev.confirmed_count(), ev.notes.len());
        let cfs = gc.s(14.0) as u16;
        draw_text_ex(&count_str,
            gc.sx(bar_x + bar_w + 8.0), gc.sy(bar_y + bar_h - 1.0),
            TextParams { font_size: cfs, font: Some(f),
                color: Color::new(0.75, 0.85, 1.0, 1.0), ..Default::default() });
    }
 
    // Инструкция
    let instr = "Дёргайте каждую струну — детектор подтвердит ноты по одной";
    let ifs = gc.s(13.0) as u16;
    let iw = measure_text(instr, Some(f), ifs, 1.0).width / gc.scale_x;
    let instr_x = if iw < box_w - 20.0 {
        box_x + (box_w - iw) / 2.0
    } else {
        box_x + 10.0
    };
    draw_text_ex(instr, gc.sx(instr_x), gc.sy(box_y + 70.0),
        TextParams { font_size: ifs, font: Some(f),
            color: Color::new(0.55, 0.62, 0.75, 0.90), ..Default::default() });
 
    // Счётчик зависания (показывает когда близко к показу подсказки)
    let stall_ratio = (lesson.stall_frames as f32 / 180.0).min(1.0);
    if stall_ratio > 0.5 && !lesson.show_hint {
        let hint_bar_w = (box_w - 60.0) * (stall_ratio - 0.5) * 2.0;
        let hint_bar_y = box_y + 84.0;
        draw_rectangle(
            gc.sx(box_x + 30.0), gc.sy(hint_bar_y),
            gc.sx(hint_bar_w), gc.sy(4.0),
            Color::new(1.0, 0.75, 0.2, 0.6),
        );
    }
 
    // ── Подсказка: диаграмма аккорда ─────────────────────────────────────────
    if let Some(hint) = hint {
        draw_chord_diagram(gc, f, hint, box_x, box_y + 100.0, box_w);
    }
}
 
/// Рисует диаграмму аккорда с подсветкой подтверждённых/неподтверждённых нот.
fn draw_chord_diagram(
    gc: &GraphicsContext,
    f: &Font,
    hint: &HintData,
    ox: f32, oy: f32,
    available_w: f32,
) {
    // Заголовок
    let title = if let Some(ref name) = hint.chord_name {
        format!("Аккорд: {}", name)
    } else {
        "Зажмите эти ноты:".to_string()
    };
    let tfs = gc.s(16.0) as u16;
    let tw = measure_text(&title, Some(f), tfs, 1.0).width / gc.scale_x;
    draw_text_ex(&title, gc.sx(ox + (available_w - tw) / 2.0), gc.sy(oy + 18.0),
        TextParams { font_size: tfs, font: Some(f),
            color: Color::new(1.0, 0.88, 0.3, 1.0), ..Default::default() });
 
    // Сетка: 6 строк (струн)
    let grid_x = ox + 40.0;
    let grid_w  = available_w - 80.0;
    let cell_h  = 26.0_f32;
    let str_names = ["E2","A2","D3","G3","B3","E4"];
 
    for si in 0..NUM_STRINGS {
        let y = oy + 34.0 + si as f32 * cell_h;
        let color = safe_string_color(si);
 
        // Линия струны
        draw_line(gc.sx(grid_x + 40.0), gc.sy(y + cell_h / 2.0),
            gc.sx(grid_x + grid_w), gc.sy(y + cell_h / 2.0),
            gc.s(1.5), Color::new(color.r, color.g, color.b, 0.25));
 
        // Имя струны
        draw_text_ex(str_names[si],
            gc.sx(grid_x), gc.sy(y + cell_h * 0.68),
            TextParams { font_size: gc.s(13.0) as u16, font: Some(f), color, ..Default::default() });
 
        // Ищем ноту на этой струне
        let note_opt = hint.notes.iter().find(|n| n.string_idx == si);
 
        if let Some(n) = note_opt {
            let dot_x = grid_x + 60.0 + n.fret as f32 * 14.0;
            let dot_y = y + cell_h / 2.0;
 
            // Цвет: зелёный если подтверждена, оранжевый если нет
            let dot_color = if n.confirmed {
                Color::new(0.2, 1.0, 0.35, 1.0)
            } else {
                Color::new(1.0, 0.6, 0.1, 1.0)
            };
 
            draw_circle(gc.sx(dot_x), gc.sy(dot_y), gc.s(9.0), dot_color);
 
            // Номер лада внутри точки
            let fret_str = n.fret.to_string();
            let ffs = gc.s(11.0) as u16;
            let fw = measure_text(&fret_str, Some(f), ffs, 1.0).width / gc.scale_x;
            draw_text_ex(&fret_str,
                gc.sx(dot_x - fw / 2.0), gc.sy(dot_y + 4.5),
                TextParams { font_size: ffs, font: Some(f),
                    color: Color::new(0.05, 0.05, 0.08, 1.0), ..Default::default() });
 
            // Имя ноты и статус
            let status = if n.confirmed { "✓" } else { "→" };
            let label = format!("{} {} {:.0}Hz", status, n.note_name, n.freq);
            draw_text_ex(&label,
                gc.sx(grid_x + grid_w - 90.0), gc.sy(y + cell_h * 0.68),
                TextParams {
                    font_size: gc.s(11.0) as u16,
                    font: Some(f),
                    color: if n.confirmed {
                        Color::new(0.2, 1.0, 0.35, 0.9)
                    } else {
                        Color::new(0.8, 0.75, 0.5, 0.85)
                    },
                    ..Default::default()
                });
        } else {
            // Струна не используется в аккорде
            draw_text_ex("×",
                gc.sx(grid_x + 44.0), gc.sy(y + cell_h * 0.68),
                TextParams { font_size: gc.s(12.0) as u16, font: Some(f),
                    color: Color::new(0.35, 0.38, 0.45, 0.7), ..Default::default() });
        }
    }
 
    // Точность
    let acc_str = format!("Точность: {:.0}%", hint.progress * 100.0);
    let afs = gc.s(12.0) as u16;
    let aw = measure_text(&acc_str, Some(f), afs, 1.0).width / gc.scale_x;
    let diag_bottom = oy + 34.0 + NUM_STRINGS as f32 * cell_h + 10.0;
    draw_text_ex(&acc_str,
        gc.sx(ox + (available_w - aw) / 2.0), gc.sy(diag_bottom),
        TextParams { font_size: afs, font: Some(f),
            color: Color::new(0.55, 0.62, 0.75, 0.85), ..Default::default() });
}
 
/// Рисует экран INFO (справка из locales/info_*.toml).
/// Возвращает true если нужно закрыть (Esc или кнопка «Назад»).
///
/// Вызов из main.rs:
///   let sections: Vec<(&str,&str)> = info_file.sections.iter()
///       .map(|s| (s.heading.as_str(), s.body.as_str())).collect();
///   let close = graphics::draw_info_screen(
///       &gc, &font, &info_file.title, &sections,
///       &mut info_state.scroll_offset, &current_locale, &theme_state);
pub fn draw_info_screen(
    gc: &GraphicsContext, f: &Font,
    title: &str,
    no_show_label: &str,
    sections: &[(&str, &str)],
    images: &[InfoImageData],
    show_on_start: bool,
    scroll_offset: &mut f32,
    locale: &Localization,
    theme: &ThemeState,
) -> (bool, Option<bool>) {
    let th = theme.current();
    let panel_bg     = theme.panel_bg();
    let panel_border = theme.panel_border();
    let title_color  = theme.section_title();
    let body_color   = theme.panel_text();
    let _ac           = theme.current().accent;
 
    draw_rectangle(
        gc.sx(0.0), gc.sy(0.0), gc.sx(gc.base_w), gc.sy(gc.base_h),
        theme.panel_overlay());

    let pw = gc.base_w * 0.82;
    let ph = gc.base_h * 0.86;
    let px = (gc.base_w - pw) / 2.0;
    let py = (gc.base_h - ph) / 2.0;
 
    draw_rectangle(gc.sx(px), gc.sy(py), gc.sx(pw), gc.sy(ph), panel_bg);
    draw_glow_rect_lines(gc, px, py, pw, ph, panel_border);
 
    // ── Заголовок ─────────────────────────────────────────────────────────────
    let title_fs = gc.s(26.0).round() as u16;
    let tw = measure_text(title, Some(f), title_fs, 1.0).width / gc.scale_x;
    draw_text_ex(title,
        gc.sx(px + (pw - tw) / 2.0), gc.sy(py + 38.0),
        TextParams { font_size: title_fs, font: Some(f),
            color: th.section_title, ..Default::default() });
    draw_line(
        gc.sx(px + 18.0), gc.sy(py + 52.0),
        gc.sx(px + pw - 18.0), gc.sy(py + 52.0),
        gc.s(1.0), Color::new(panel_border.r, panel_border.g, panel_border.b, 0.30));
 
    // ── Кнопка «Назад» ────────────────────────────────────────────────────────
    let (mx, my) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);
 
let back_w = 94.0_f32; let back_h = 26.0_f32;
    let back_x = px + 10.0; let back_y = py + 8.0;
    let hb = mx >= back_x && mx <= back_x + back_w
          && my >= back_y && my <= back_y + back_h;
    small_btn(gc, f, back_x, back_y, back_w, back_h,
        &locale.overlay_back, th.accent, th.accent_hover, hb, theme.is_day);
 
    let close = (lmb && hb) || is_key_pressed(KeyCode::Escape);
 
    // ── Флаг «Больше не показывать» ──────────────────────────────────────────
    // Располагается внизу панели, над подсказкой.
    let checkbox_y    = py + ph - 42.0;
    let checkbox_size = 16.0_f32;
    let checkbox_x    = px + pw / 2.0 - 90.0; // приблизительно по центру
    let _label_x      = checkbox_x + checkbox_size + 8.0;
    let lfs_cb        = gc.s(14.0).round() as u16;
    let lw_cb         = measure_text(no_show_label, Some(f), lfs_cb, 1.0).width / gc.scale_x;
    // Центровка: checkbox + label вместе
    let total_w       = checkbox_size + 8.0 + lw_cb;
    let cb_x          = px + (pw - total_w) / 2.0;
    let lb_x          = cb_x + checkbox_size + 8.0;
 
    let cb_hover = mx >= cb_x && mx <= cb_x + total_w
                && my >= checkbox_y - 2.0 && my <= checkbox_y + checkbox_size + 2.0;
    let cb_clicked = lmb && cb_hover;
 
    // Фон при наведении
    if cb_hover {
        draw_rectangle(
            gc.sx(cb_x - 6.0), gc.sy(checkbox_y - 4.0),
            gc.sx(total_w + 12.0), gc.sy(checkbox_size + 8.0),
            Color::new(th.accent.r, th.accent.g, th.accent.b, 0.08));
    }
 
    // Чекбокс — рамка
    draw_rectangle_lines(
        gc.sx(cb_x), gc.sy(checkbox_y),
        gc.sx(checkbox_size), gc.sy(checkbox_size),
        gc.s(if cb_hover { 1.8 } else { 1.2 }),
        Color::new(th.accent.r, th.accent.g, th.accent.b, if cb_hover { 0.95 } else { 0.65 }));
 
    // Галочка — если show_on_start = FALSE (флаг «не показывать» установлен)
    let checked = !show_on_start;
    if checked {
        // Заливка
        draw_rectangle(
            gc.sx(cb_x + 3.0), gc.sy(checkbox_y + 3.0),
            gc.sx(checkbox_size - 6.0), gc.sy(checkbox_size - 6.0),
            Color::new(th.accent.r, th.accent.g, th.accent.b, 0.90));
        // Галочка ✓
        let fs_check = gc.s(14.0).round() as u16;
        draw_text_ex(" ",
            gc.sx(cb_x + 1.0), gc.sy(checkbox_y + checkbox_size - 1.0),
            TextParams { font_size: fs_check, font: Some(f),
                color: Color::new(0.05, 0.05, 0.08, 1.0), ..Default::default() });
    }
 
    // Текст метки
    let tc = if cb_hover { WHITE }
             else { Color::new(th.accent.r * 0.8 + 0.2, th.accent.g * 0.8 + 0.2, th.accent.b * 0.8 + 0.2, 0.90) };
    draw_text_ex(no_show_label,
        gc.sx(lb_x), gc.sy(checkbox_y + checkbox_size - 2.0),
        TextParams { font_size: lfs_cb, font: Some(f), color: tc, ..Default::default() });
 
    // Возвращаем новое значение если кликнули
    let pref_change: Option<bool> = if cb_clicked {
        // show_on_start инвертируется (clicked = «не показывать» → show_on_start = false)
        Some(!show_on_start)
    } else {
        None
    };
 
    // ── Прокрутка ─────────────────────────────────────────────────────────────
    let (_, wy) = mouse_wheel();
    *scroll_offset -= wy * 28.0;
    *scroll_offset = scroll_offset.max(0.0);
 
    // ── Контентная зона ───────────────────────────────────────────────────────
    let content_x   = px + 28.0;
    let content_w   = pw - 56.0;
    let content_top = py + 68.0;
    let content_h   = ph - 120.0; // оставить место для чекбокса и подсказки
 
    // ── Картинки (фиксированные, не скроллятся) ───────────────────────────────
    let mut sorted_images: Vec<&InfoImageData> = images.iter().collect();
    sorted_images.sort_by_key(|img| (img.y * 1000.0) as i32);
    for img in &sorted_images {
        let ix = content_x + img.x * content_w;
        let iy = content_top + img.y * content_h;
        let iw = img.w * content_w;
        let tex = img.texture;
        let ih  = if tex.width() > 0.0 { iw * tex.height() / tex.width() } else { iw };
        draw_texture_ex(
            tex, gc.sx(ix), gc.sy(iy),
            Color::new(1.0, 1.0, 1.0, img.alpha),
            macroquad::texture::DrawTextureParams {
                dest_size: Some(macroquad::math::Vec2::new(gc.sx(iw), gc.sy(ih))),
                ..Default::default()
            });
    }
 
    // ── Текстовые секции ──────────────────────────────────────────────────────
    let mut cursor_y = content_top - *scroll_offset;
    let mut total_h  = 0.0_f32;
 
    for (heading, body) in sections {
        let hfs = gc.s(17.0).round() as u16;
        let h_h = 17.0_f32 * 1.6;
 
        if cursor_y + h_h >= content_top && cursor_y <= content_top + content_h {
            draw_rectangle(
                gc.sx(content_x), gc.sy(cursor_y - 13.0),
                gc.sx(4.0), gc.sy(16.0),
                Color::new(th.accent.r, th.accent.g, th.accent.b, 0.80));
            draw_text_ex(heading,
                gc.sx(content_x + 10.0), gc.sy(cursor_y),
                TextParams { font_size: hfs, font: Some(f),
                    color: title_color, ..Default::default() });
        }
        cursor_y += h_h + 4.0;
        total_h  += h_h + 4.0;
 
        let bfs  = gc.s(14.0).round() as u16;
        let b_lh = 14.0_f32 * 1.65;
        for raw_line in body.split('\n') {
            let mut buf = String::new();
            for word in raw_line.split_whitespace() {
                let test = if buf.is_empty() { word.to_string() }
                           else { format!("{} {}", buf, word) };
                let w = measure_text(&test, Some(f), bfs, 1.0).width / gc.scale_x;
                if w > content_w - 10.0 && !buf.is_empty() {
                    if cursor_y + b_lh >= content_top && cursor_y <= content_top + content_h {
                        draw_text_ex(&buf, gc.sx(content_x + 10.0), gc.sy(cursor_y),
                            TextParams { font_size: bfs, font: Some(f),
                                color: body_color, ..Default::default() });
                    }
                    cursor_y += b_lh; total_h += b_lh; buf = word.to_string();
                } else { buf = test; }
            }
            if !buf.is_empty() {
                if cursor_y + b_lh >= content_top && cursor_y <= content_top + content_h {
                    draw_text_ex(&buf, gc.sx(content_x + 10.0), gc.sy(cursor_y),
                        TextParams { font_size: bfs, font: Some(f),
                            color: body_color, ..Default::default() });
                }
                cursor_y += b_lh; total_h += b_lh;
            }
        }
        cursor_y += 14.0; total_h += 14.0;
        if cursor_y >= content_top && cursor_y <= content_top + content_h {
            draw_line(
                gc.sx(content_x), gc.sy(cursor_y - 7.0),
                gc.sx(content_x + content_w), gc.sy(cursor_y - 7.0),
                gc.s(1.0), Color::new(th.accent.r, th.accent.g, th.accent.b, 0.12));
        }
    }
 
    let max_scroll = (total_h - content_h).max(0.0);
    *scroll_offset = scroll_offset.min(max_scroll);
 
    // ── Скроллбар ─────────────────────────────────────────────────────────────
    if max_scroll > 0.0 {
        let sb_x     = px + pw - 10.0;
        let sb_ratio = *scroll_offset / max_scroll;
        let sb_h     = ((content_h / (total_h + 1.0)) * content_h).max(24.0);
        let sb_y     = content_top + sb_ratio * (content_h - sb_h);
        draw_rectangle(gc.sx(sb_x), gc.sy(content_top), gc.s(6.0), gc.sy(content_h),
            Color::new(0.12, 0.13, 0.18, 0.7));
        draw_rectangle(gc.sx(sb_x + 1.0), gc.sy(sb_y), gc.s(4.0), gc.sy(sb_h),
            Color::new(th.accent.r * 0.8, th.accent.g * 0.8, th.accent.b * 0.6, 0.85));
    }
 
    // ── Подсказка ─────────────────────────────────────────────────────────────
    let hint   = "↑↓ — прокрутка  |  Esc — назад";
    let hfs2   = gc.s(11.0).round() as u16;
    let hint_w = measure_text(hint, Some(f), hfs2, 1.0).width / gc.scale_x;
    draw_text_ex(hint,
        gc.sx(px + (pw - hint_w) / 2.0), gc.sy(py + ph - 12.0),
        TextParams { font_size: hfs2, font: Some(f),
            color: Color::new(0.32, 0.36, 0.46, 0.82), ..Default::default() });
 
    (close, pref_change)
}


pub fn draw_pregame_confirm_dialog(
    gc: &GraphicsContext, f: &Font, locale: &Localization, theme: &ThemeState,
) -> PreGameConfirmResult {
    let is_day = theme.is_day;
    let overlay_col = if is_day { Color::new(0.10, 0.13, 0.22, 0.55) } else { Color::new(0.0, 0.0, 0.0, 0.72) };
    draw_rectangle(gc.sx(0.0), gc.sy(0.0), gc.sx(gc.base_w), gc.sy(gc.base_h), overlay_col);

    let dlg_w = 420.0_f32;
    let dlg_h = 200.0_f32;
    let dlg_x = (gc.base_w - dlg_w) / 2.0;
    let dlg_y = (gc.base_h - dlg_h) / 2.0;

    draw_rectangle(gc.sx(dlg_x + 6.0), gc.sy(dlg_y + 10.0), gc.sx(dlg_w), gc.sy(dlg_h), Color::new(0.0, 0.0, 0.0, 0.30));

    let dlg_bg     = if is_day { Color::new(0.97, 0.97, 0.99, 0.99) } else { Color::new(0.09, 0.09, 0.14, 0.99) };
    let dlg_border = if is_day { Color::new(0.15, 0.55, 0.35, 0.9) } else { Color::new(0.25, 0.85, 0.55, 0.9) };
    draw_rectangle(gc.sx(dlg_x), gc.sy(dlg_y), gc.sx(dlg_w), gc.sy(dlg_h), dlg_bg);
    draw_glow_rect_lines(gc, dlg_x, dlg_y, dlg_w, dlg_h, dlg_border);
    draw_rectangle(gc.sx(dlg_x), gc.sy(dlg_y), gc.sx(dlg_w), gc.s(6.0), dlg_border);

    let title = &locale.pregame_title;
    let title_fs: u16 = 24;
    let title_col = if is_day { Color::new(0.10, 0.16, 0.14, 1.0) } else { Color::new(0.95, 0.98, 0.96, 1.0) };
    let title_mw = measure_text(title, Some(f), gc.s(title_fs as f32) as u16, 1.0).width / gc.scale_x;
    draw_text_ex(title, gc.sx(dlg_x + (dlg_w - title_mw) / 2.0), gc.sy(dlg_y + 70.0),
        TextParams { font_size: title_fs, font: Some(f), color: title_col, ..Default::default() });

    draw_line(gc.sx(dlg_x + 24.0), gc.sy(dlg_y + 92.0), gc.sx(dlg_x + dlg_w - 24.0), gc.sy(dlg_y + 92.0),
        gc.s(1.0), Color::new(dlg_border.r, dlg_border.g, dlg_border.b, 0.25));

    let btn_w = 150.0_f32; let btn_h = 44.0_f32;
    let btn_y = dlg_y + dlg_h - btn_h - 24.0;
    let gap   = 20.0_f32;
    let total = btn_w * 2.0 + gap;
    let btn_no_x  = dlg_x + (dlg_w - total) / 2.0;
    let btn_yes_x = btn_no_x + btn_w + gap;

    let (mx, my) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);
    let hover_no  = mx >= btn_no_x  && mx <= btn_no_x  + btn_w && my >= btn_y && my <= btn_y + btn_h;
    let hover_yes = mx >= btn_yes_x && mx <= btn_yes_x + btn_w && my >= btn_y && my <= btn_y + btn_h;

    draw_stat_btn(gc, f, btn_no_x, btn_y, btn_w, btn_h, &locale.pregame_no, 17,
        if is_day { Color::new(0.55, 0.15, 0.12, 1.0) } else { Color::new(0.9, 0.35, 0.3, 1.0) },
        if is_day { Color::new(0.04, 0.06, 0.10, 1.0) } else { WHITE },
        Color::new(1.0, 0.3, 0.25, 1.0), hover_no);

    draw_stat_btn(gc, f, btn_yes_x, btn_y, btn_w, btn_h, &locale.pregame_yes, 17,
        if is_day { Color::new(0.10, 0.45, 0.25, 1.0) } else { Color::new(0.30, 0.85, 0.45, 1.0) },
        if is_day { Color::new(0.04, 0.06, 0.10, 1.0) } else { WHITE },
        Color::new(0.25, 1.0, 0.45, 1.0), hover_yes);

    let hint_y = btn_y + btn_h + 16.0;
    let hint_col = if is_day { Color::new(0.40, 0.42, 0.48, 0.9) } else { Color::new(0.45, 0.45, 0.50, 0.9) };
    let hnw = measure_text("Esc",   Some(f), gc.s(11.0) as u16, 1.0).width / gc.scale_x;
    let hyw = measure_text("Enter", Some(f), gc.s(11.0) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, "Esc",   btn_no_x  + (btn_w - hnw) / 2.0, hint_y, 11, hint_col, f);
    draw_text_custom(gc, "Enter", btn_yes_x + (btn_w - hyw) / 2.0, hint_y, 11, hint_col, f);

    if is_key_pressed(KeyCode::Enter)  { return PreGameConfirmResult::Yes; }
    if is_key_pressed(KeyCode::Escape) { return PreGameConfirmResult::No; }
    if lmb {
        if hover_yes { return PreGameConfirmResult::Yes; }
        if hover_no  { return PreGameConfirmResult::No; }
    }
    PreGameConfirmResult::None
}

/// Полноэкранный обратный отсчёт 3-2-1 поверх затемнённого грифа. Каждая
/// цифра "вспыхивает" крупнее в начале своей секунды и сжимается к нормальному
/// размеру к концу — чисто визуальный акцент смены числа.
pub fn draw_countdown_overlay(gc: &GraphicsContext, f: &Font, remaining: f32, locale: &Localization) {
    draw_rectangle(gc.sx(0.0), gc.sy(0.0), gc.sx(gc.base_w), gc.sy(gc.base_h), Color::new(0.0, 0.0, 0.0, 0.45));

    let number = remaining.ceil().max(1.0) as i32;
    let label = number.to_string();

    let phase = (number as f32 - remaining).clamp(0.0, 1.0); // 0 = начало секунды, 1 = конец
    let scale = 1.0 + (1.0 - phase) * 0.45;
    let alpha = (0.45 + (1.0 - phase) * 0.55).clamp(0.0, 1.0);

    let base_fs: f32 = 120.0;
    let logical_fs = (base_fs * scale).round() as u16;

    let mw = measure_text(&label, Some(f), gc.s(logical_fs as f32) as u16, 1.0).width / gc.scale_x;
    let x = gc.base_w / 2.0 - mw / 2.0;
    let y = gc.base_h / 2.0 + base_fs * 0.35;

    draw_glow_text(gc, &label, x, y, logical_fs, Color::new(1.0, 0.95, 0.3, alpha), f);

    let hint = &locale.countdown_hint;
    let hfs = 22u16;
    let hw = measure_text(hint, Some(f), gc.s(hfs as f32) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, hint, gc.base_w / 2.0 - hw / 2.0, gc.base_h / 2.0 - 90.0, hfs,
        Color::new(0.9, 0.92, 1.0, 0.9), f);
}

 
/// Панель кнопок тюнера: выбор входного/выходного устройства и запись.
/// Зафиксирована по верхнему краю экрана (под заголовком и текстовыми
/// подсказками), чтобы не зависеть от высоты окна и не пересекаться с
/// центральной нотой/waveform/списком устройств снизу.
pub fn draw_tuner_controls(gc: &GraphicsContext, f: &Font, is_recording: bool, theme: &ThemeState) -> TunerControlClicks {
    let mut clicks = TunerControlClicks::default();
    let is_day = theme.is_day;

    let (mx, my) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);

    let btn_h = 30.0_f32;
    let gap   = 10.0_f32;
    let mic_w = 180.0_f32;
    let out_w = 180.0_f32;
    let rec_w = 110.0_f32;
    let total_w = mic_w + out_w + rec_w + gap * 2.0;
    let bx0 = gc.base_w / 2.0 - total_w / 2.0;
    // Перенесено вниз экрана — над списком микрофонов (тот начинается
    // на gc.base_h - 148.0 - 22.0), не пересекается с подсказками сверху.
    let by  = gc.base_h - 240.0;

    draw_rectangle(gc.sx(bx0 - 14.0), gc.sy(by - 8.0), gc.sx(total_w + 28.0), gc.sy(btn_h + 16.0), btn_panel_bg(is_day));
    draw_rectangle_lines(gc.sx(bx0 - 14.0), gc.sy(by - 8.0), gc.sx(total_w + 28.0), gc.sy(btn_h + 16.0),
        gc.s(1.0), btn_panel_border(is_day));

    let mut bx = bx0;
    {
        let hov = mx >= bx && mx <= bx + mic_w && my >= by && my <= by + btn_h;
        draw_stat_btn_themed(gc, f, bx, by, mic_w, btn_h, "Входное устройство", 13,
            Color::new(0.55, 0.75, 1.0, 1.0), WHITE, Color::new(0.4, 0.7, 1.0, 1.0), hov, is_day);
        if lmb && hov { clicks.mic_select = true; }
        bx += mic_w + gap;
    }
    {
        let hov = mx >= bx && mx <= bx + out_w && my >= by && my <= by + btn_h;
        draw_stat_btn_themed(gc, f, bx, by, out_w, btn_h, "Выходное устройство", 13,
            Color::new(0.6, 0.85, 1.0, 1.0), WHITE, Color::new(0.5, 0.8, 1.0, 1.0), hov, is_day);
        if lmb && hov { clicks.output_select = true; }
        bx += out_w + gap;
    }
    {
        let label = if is_recording { "Стоп" } else { "Rec" };
        let (nc, hc, glow) = if is_recording {
            (Color::new(1.0, 0.3, 0.3, 1.0), WHITE, Color::new(1.0, 0.15, 0.15, 1.0))
        } else {
            (Color::new(0.9, 0.9, 0.9, 1.0), WHITE, Color::new(1.0, 0.4, 0.4, 1.0))
        };
        let hov = mx >= bx && mx <= bx + rec_w && my >= by && my <= by + btn_h;
        draw_stat_btn_themed(gc, f, bx, by, rec_w, btn_h, label, 14, nc, hc, glow, hov, is_day);
        if is_recording {
            let pulse = ((get_time() as f32 * 4.0).sin() * 0.3 + 0.7).clamp(0.0, 1.0);
            draw_circle(gc.sx(bx + 14.0), gc.sy(by + btn_h / 2.0), gc.s(4.0), Color::new(1.0, 0.0, 0.0, pulse));
        }
        if lmb && hov { clicks.record_toggle = true; }
    }

    clicks
}
/// Кликабельная панель управления игрой — две строки:
/// верхняя (Микрофон | Rec | Аудиовыход | Автомод),
/// нижняя (W− | Скорость | S+ | Пауза). Рисуется и принимает клики каждый
/// кадр независимо от паузы.
pub fn draw_game_controls(gc: &GraphicsContext, s: &GameState, f: &Font, is_recording: bool, theme: &ThemeState) -> UiButtonClicks {
    let mut clicks = UiButtonClicks::default();
    if s.game_over { return clicks; }
    let is_day = theme.is_day;

    let l = &gc.layout;
    let t = &s.locale;
    let (mx, my) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);

    let btn_h  = 26.0_f32;
    let gap    = 6.0_f32;
    let row2_y = l.window_h - 25.0 - btn_h / 2.0;
    let row1_y = row2_y - btn_h - 10.0;

    let mic_w = 130.0_f32; let rec_w = 90.0_f32; let out_w = 140.0_f32; let auto_w = 110.0_f32;
    let row1_total = mic_w + rec_w + out_w + auto_w + gap * 3.0;
    let wm_w = 34.0_f32; let speed_w = 96.0_f32; let sp_w = 34.0_f32; let pause_w = 150.0_f32;
    let row2_total = wm_w + speed_w + sp_w + pause_w + gap * 3.0;

    let bar_w = row1_total.max(row2_total) + 28.0;
    let bar_x = l.window_w / 2.0 - bar_w / 2.0;
    let bar_y = row1_y - 8.0;
    let bar_h = (row2_y + btn_h + 8.0) - bar_y;
    draw_rectangle(gc.sx(bar_x), gc.sy(bar_y), gc.sx(bar_w), gc.sy(bar_h), btn_panel_bg(is_day));
    draw_rectangle_lines(gc.sx(bar_x), gc.sy(bar_y), gc.sx(bar_w), gc.sy(bar_h), gc.s(1.0), btn_panel_border(is_day));

    // ── Строка 1: Микрофон | Rec | Аудиовыход | Автомод ─────────────────────
    let mut bx = l.window_w / 2.0 - row1_total / 2.0;
    {
        let hov = mx >= bx && mx <= bx + mic_w && my >= row1_y && my <= row1_y + btn_h;
        draw_stat_btn_themed(gc, f, bx, row1_y, mic_w, btn_h, &t.hint_mic, 13,
            Color::new(0.55, 0.75, 1.0, 1.0), WHITE, Color::new(0.4, 0.7, 1.0, 1.0), hov, is_day);
        if lmb && hov { clicks.mic = true; }
        bx += mic_w + gap;
    }
    {
        let hov = mx >= bx && mx <= bx + rec_w && my >= row1_y && my <= row1_y + btn_h;
        let label = if is_recording { "Стоп" } else { "Rec" };
        let (nc, hc, glow) = if is_recording {
            (Color::new(1.0, 0.3, 0.3, 1.0), WHITE, Color::new(1.0, 0.15, 0.15, 1.0))
        } else {
            (Color::new(0.9, 0.9, 0.9, 1.0), WHITE, Color::new(1.0, 0.4, 0.4, 1.0))
        };
        draw_stat_btn_themed(gc, f, bx, row1_y, rec_w, btn_h, label, 13, nc, hc, glow, hov, is_day);
        if is_recording {
            let pulse = ((get_time() as f32 * 4.0).sin() * 0.3 + 0.7).clamp(0.0, 1.0);
            draw_circle(gc.sx(bx + 12.0), gc.sy(row1_y + btn_h / 2.0), gc.s(4.0), Color::new(1.0, 0.0, 0.0, pulse));
        }
        if lmb && hov { clicks.record_toggle = true; }
        bx += rec_w + gap;
    }
    {
        let hov = mx >= bx && mx <= bx + out_w && my >= row1_y && my <= row1_y + btn_h;
        draw_stat_btn_themed(gc, f, bx, row1_y, out_w, btn_h, "Аудиовыход", 13,
            Color::new(0.6, 0.85, 1.0, 1.0), WHITE, Color::new(0.5, 0.8, 1.0, 1.0), hov, is_day);
        if lmb && hov { clicks.output = true; }
        bx += out_w + gap;
    }
    {
        let label = if s.auto_play_mode { "Авто: ВЫКЛ" } else { "Автомод" };
        let (nc, hc, glow) = if s.auto_play_mode {
            (Color::new(1.0, 0.6, 0.3, 1.0), WHITE, Color::new(1.0, 0.5, 0.2, 1.0))
        } else {
            (Color::new(0.7, 0.9, 0.7, 1.0), WHITE, Color::new(0.5, 1.0, 0.5, 1.0))
        };
        let hov = mx >= bx && mx <= bx + auto_w && my >= row1_y && my <= row1_y + btn_h;
        draw_stat_btn_themed(gc, f, bx, row1_y, auto_w, btn_h, label, 13, nc, hc, glow, hov, is_day);
        if lmb && hov { clicks.automode = true; }
    }

    // ── Строка 2: W− | Скорость | S+ | Пауза ─────────────────────────────────
    let mut bx2 = l.window_w / 2.0 - row2_total / 2.0;
    {
        let hov = mx >= bx2 && mx <= bx2 + wm_w && my >= row2_y && my <= row2_y + btn_h;
        draw_stat_btn_themed(gc, f, bx2, row2_y, wm_w, btn_h, "W−", 14,
            Color::new(0.9, 0.9, 0.4, 1.0), WHITE, Color::new(1.0, 1.0, 0.3, 1.0), hov, is_day);
        if lmb && hov { clicks.speed_down = true; }
        bx2 += wm_w + gap;
    }
    {
        let label = format!("Скорость {}", SPEED_LABELS[s.speed_index]);
        draw_rectangle_lines(gc.sx(bx2), gc.sy(row2_y), gc.sx(speed_w), gc.sy(btn_h),
            gc.s(1.0), btn_speed_border(is_day));
        let lfs = gc.s(13.0) as u16;
        let lw = measure_text(&label, Some(f), lfs, 1.0).width / gc.scale_x;
        let speed_text_col = if is_day { Color::new(0.35, 0.32, 0.10, 1.0) } else { Color::new(0.9, 0.9, 0.4, 1.0) };
        draw_text_custom(gc, &label, bx2 + (speed_w - lw) / 2.0, row2_y + btn_h * 0.68, 13,
            speed_text_col, f);
        bx2 += speed_w + gap;
    }
    {
        let hov = mx >= bx2 && mx <= bx2 + sp_w && my >= row2_y && my <= row2_y + btn_h;
        draw_stat_btn_themed(gc, f, bx2, row2_y, sp_w, btn_h, "S+", 14,
            Color::new(0.9, 0.9, 0.4, 1.0), WHITE, Color::new(1.0, 1.0, 0.3, 1.0), hov, is_day);
        if lmb && hov { clicks.speed_up = true; }
        bx2 += sp_w + gap;
    }
    {
        let label: &str = if s.paused { &t.hint_pause_off } else { &t.hint_pause_on };
        let hov = mx >= bx2 && mx <= bx2 + pause_w && my >= row2_y && my <= row2_y + btn_h;
        draw_stat_btn_themed(gc, f, bx2, row2_y, pause_w, btn_h, label, 13,
            Color::new(0.5, 1.0, 0.6, 1.0), WHITE, Color::new(0.4, 1.0, 0.5, 1.0), hov, is_day);
        if lmb && hov { clicks.pause = true; }
    }

    clicks
}
/// Обрезает строку так, чтобы её ширина в пикселях не превышала max_px.
/// Добавляет «…» если текст не влезает. Использует measure_text для точности.
fn truncate_str_px(s: &str, max_px: f32, font: &Font, font_size: u16, scale: f32) -> String {
    let fs = (font_size as f32 * scale) as u16;
    let full_w = measure_text(s, Some(font), fs, 1.0).width;
    if full_w <= max_px * scale {
        return s.to_string();
    }
    // Ширина суффикса «…»
    let ellipsis_w = measure_text("…", Some(font), fs, 1.0).width;
    let target_w = max_px * scale - ellipsis_w;
    if target_w <= 0.0 {
        return "…".to_string();
    }
    // Бинарный поиск по количеству символов
    let chars: Vec<char> = s.chars().collect();
    let mut lo = 0usize;
    let mut hi = chars.len();
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        let candidate: String = chars[..mid].iter().collect();
        let w = measure_text(&candidate, Some(font), fs, 1.0).width;
        if w <= target_w { lo = mid; } else { hi = mid; }
    }
    let truncated: String = chars[..lo].iter().collect();
    format!("{}…", truncated)
}

/// Безопасное обрезание строки по количеству символов Unicode
fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() > max_chars {
        let t: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{}…", t)
    } else {
        s.to_string()
    }
}

// ─── GLOW-хелперы ────────────────────────────────────────────────────────────

pub fn draw_glow_circle(gc: &GraphicsContext, x: f32, y: f32, r: f32, c: Color, layers: u8) {
    for i in (1..=layers).rev() {
        let factor = i as f32 / layers as f32;
        let alpha  = c.a * (1.0 - factor) * 0.35;
        let radius = r * (1.0 + factor * 1.6);
        draw_circle(gc.sx(x), gc.sy(y), gc.s(radius), Color::new(c.r, c.g, c.b, alpha));
    }
    draw_circle(gc.sx(x), gc.sy(y), gc.s(r), c);
    draw_circle(
        gc.sx(x), gc.sy(y), gc.s(r * 0.45),
        Color::new(
            (c.r + 0.6).min(1.0),
            (c.g + 0.6).min(1.0),
            (c.b + 0.6).min(1.0),
            c.a * 0.7,
        ),
    );
}

pub fn draw_glow_line(
    gc: &GraphicsContext,
    x1: f32, y1: f32, x2: f32, y2: f32,
    thickness: f32, c: Color,
) {
    draw_line(gc.sx(x1), gc.sy(y1), gc.sx(x2), gc.sy(y2),
        gc.s(thickness * 4.0), Color::new(c.r, c.g, c.b, c.a * 0.12));
    draw_line(gc.sx(x1), gc.sy(y1), gc.sx(x2), gc.sy(y2),
        gc.s(thickness * 2.5), Color::new(c.r, c.g, c.b, c.a * 0.22));
    draw_line(gc.sx(x1), gc.sy(y1), gc.sx(x2), gc.sy(y2), gc.s(thickness), c);
    draw_line(gc.sx(x1), gc.sy(y1), gc.sx(x2), gc.sy(y2),
        gc.s(thickness * 0.4), Color::new(1.0, 1.0, 1.0, c.a * 0.55));
}

pub fn draw_glow_rect_lines(gc: &GraphicsContext, x: f32, y: f32, w: f32, h: f32, c: Color) {
    draw_rectangle_lines(gc.sx(x), gc.sy(y), gc.sx(w), gc.sy(h),
        gc.s(6.0), Color::new(c.r, c.g, c.b, c.a * 0.15));
    draw_rectangle_lines(gc.sx(x), gc.sy(y), gc.sx(w), gc.sy(h),
        gc.s(3.5), Color::new(c.r, c.g, c.b, c.a * 0.30));
    draw_rectangle_lines(gc.sx(x), gc.sy(y), gc.sx(w), gc.sy(h),
        gc.s(1.5), c);
}

pub fn draw_glow_text(gc: &GraphicsContext, text: &str, x: f32, y: f32, fs: u16, c: Color, f: &Font) {
    let offsets: [(f32, f32); 8] = [
        (-2., -2.), (2., -2.), (-2., 2.), (2., 2.),
        (0., -2.), (0., 2.), (-2., 0.), (2., 0.),
    ];
    for (ox, oy) in offsets {
        draw_text_ex(text, gc.sx(x + ox), gc.sy(y + oy), TextParams {
            font_size: gc.s(fs as f32) as u16, font: Some(f),
            color: Color::new(c.r, c.g, c.b, c.a * 0.18),
            ..Default::default()
        });
    }
    draw_text_ex(text, gc.sx(x), gc.sy(y), TextParams {
        font_size: gc.s(fs as f32) as u16, font: Some(f), color: c, ..Default::default()
    });
}

fn draw_stat_btn(
    gc: &GraphicsContext, f: &Font,
    x: f32, y: f32, w: f32, h: f32,
    label: &str, fs: u16,
    normal_color: Color, hover_color: Color, glow_color: Color,
    is_hover: bool,
) {
    draw_stat_btn_themed(gc, f, x, y, w, h, label, fs, normal_color, hover_color, glow_color, is_hover, false);
}

fn draw_stat_btn_themed(
    gc: &GraphicsContext, f: &Font,
    x: f32, y: f32, w: f32, h: f32,
    label: &str, fs: u16,
    normal_color: Color, hover_color: Color, glow_color: Color,
    is_hover: bool, is_day: bool,
) {
    let bg = if is_hover {
        if is_day { BTN_HOVER_BG_DAY } else { Color::new(glow_color.r, glow_color.g, glow_color.b, 0.18) }
    } else {
        if is_day { Color::new(1.0, 1.0, 1.0, 0.25) } else { Color::new(0.0, 0.0, 0.0, 0.0) }
    };
    draw_rectangle(gc.sx(x), gc.sy(y), gc.sx(w), gc.sy(h), bg);

    // Цвет рамки в невыделенном состоянии: на дне делаем темнее и насыщеннее,
    // а не используем бледный normal_color напрямую — иначе сливается с фоном.
    let border_col = if is_hover {
        glow_color
    } else if is_day {
        Color::new(normal_color.r * 0.45, normal_color.g * 0.45, normal_color.b * 0.55, 0.85)
    } else {
        Color::new(normal_color.r, normal_color.g, normal_color.b, 0.4)
    };
    if is_hover {
        draw_glow_rect_lines(gc, x, y, w, h, border_col);
    } else {
        draw_rectangle_lines(gc.sx(x), gc.sy(y), gc.sx(w), gc.sy(h), gc.s(1.0), border_col);
    }

    // Цвет текста: на дне в невыделенном состоянии — тёмный (затемнённая
    // версия normal_color), при наведении — почти чёрный для максимального
    // контраста на светлой подложке.
    let text_color = if is_hover {
        if is_day { Color::new(0.04, 0.06, 0.10, 1.0) } else { hover_color }
    } else if is_day {
        Color::new(normal_color.r * 0.35, normal_color.g * 0.35, normal_color.b * 0.45, 1.0)
    } else {
        normal_color
    };

    let tw = measure_text(label, Some(f), gc.s(fs as f32) as u16, 1.0).width / gc.scale_x;
    let tx = x + (w - tw) / 2.0;
    let ty_text = y + h / 2.0 + fs as f32 * 0.38;
    if is_hover {
        draw_glow_text(gc, label, tx, ty_text, fs, text_color, f);
    } else {
        draw_text_custom(gc, label, tx, ty_text, fs, text_color, f);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ДИАЛОГ ПОДТВЕРЖДЕНИЯ ВЫХОДА
// Вызывается поверх любого экрана когда нажимается Esc.
// Возвращает ExitDialogResult.
// ─────────────────────────────────────────────────────────────────────────────
pub fn draw_exit_confirm_dialog(
    gc: &GraphicsContext,
    f: &Font,
    locale: &Localization,
    theme: &ThemeState,
) -> ExitDialogResult {
    let is_day = theme.is_day;

    let overlay_col = if is_day { Color::new(0.10, 0.13, 0.22, 0.55) } else { Color::new(0.0, 0.0, 0.0, 0.72) };
    draw_rectangle(gc.sx(0.0), gc.sy(0.0), gc.sx(gc.base_w), gc.sy(gc.base_h), overlay_col);

    let dlg_w = 420.0_f32;
    let dlg_h = 230.0_f32;
    let dlg_x = (gc.base_w - dlg_w) / 2.0;
    let dlg_y = (gc.base_h - dlg_h) / 2.0;

    // Мягкая тень под панелью
    draw_rectangle(gc.sx(dlg_x + 6.0), gc.sy(dlg_y + 10.0), gc.sx(dlg_w), gc.sy(dlg_h),
        Color::new(0.0, 0.0, 0.0, 0.30));

    let dlg_bg     = if is_day { Color::new(0.97, 0.97, 0.99, 0.99) } else { Color::new(0.09, 0.09, 0.14, 0.99) };
    let dlg_border = if is_day { Color::new(0.85, 0.30, 0.22, 0.9) } else { Color::new(1.0, 0.35, 0.30, 0.9) };
    draw_rectangle(gc.sx(dlg_x), gc.sy(dlg_y), gc.sx(dlg_w), gc.sy(dlg_h), dlg_bg);
    draw_glow_rect_lines(gc, dlg_x, dlg_y, dlg_w, dlg_h, dlg_border);

    // Цветная плашка сверху панели
    draw_rectangle(gc.sx(dlg_x), gc.sy(dlg_y), gc.sx(dlg_w), gc.s(6.0), dlg_border);

    // Иконка предупреждения в кружке
    let icon_cy = dlg_y + 52.0;
    //let icon_r  = 22.0_f32;
   // let icon_bg = if is_day { Color::new(1.0, 0.92, 0.85, 1.0) } else { Color::new(0.30, 0.12, 0.08, 1.0) };
    //draw_circle(gc.sx(dlg_x + dlg_w / 2.0), gc.sy(icon_cy), gc.s(icon_r), icon_bg);
    //draw_circle_lines(gc.sx(dlg_x + dlg_w / 2.0), gc.sy(icon_cy), gc.s(icon_r), gc.s(2.0), dlg_border);

    let icon = "Ah ?";
    let ifs = gc.s(26.0) as u16;
    let idim = measure_text(icon, Some(f), ifs, 1.0);
    let iw = idim.width / gc.scale_x;
    let ih = idim.offset_y / gc.scale_y; // реальная высота глифа над базовой линией
    draw_text_ex(icon,
        gc.sx(dlg_x + dlg_w / 2.0) - iw / 2.0,
        gc.sy(icon_cy) + ih / 2.0,
        TextParams { font_size: ifs, font: Some(f), color: dlg_border, ..Default::default() });

    // Заголовок — размер уменьшен (21 → 18) и подвинут чуть выше, под
    // уменьшенный кружок; межстрочный отступ от линии-разделителя увеличен,
    // чтобы текст не казался прижатым к иконке.
    let title = &locale.exit_confirm_title;
    let title_fs: u16 = 18;
    let title_col = if is_day { Color::new(0.16, 0.18, 0.26, 1.0) } else { Color::new(0.95, 0.95, 1.0, 1.0) };
    let title_mw = measure_text(title, Some(f), gc.s(title_fs as f32) as u16, 1.0).width / gc.scale_x;
    draw_text_ex(title, gc.sx(dlg_x + (dlg_w - title_mw) / 2.0), gc.sy(dlg_y + 102.0),
        TextParams { font_size: title_fs, font: Some(f), color: title_col, ..Default::default() });


    draw_line(gc.sx(dlg_x + 24.0), gc.sy(dlg_y + 122.0), gc.sx(dlg_x + dlg_w - 24.0), gc.sy(dlg_y + 122.0),
        gc.s(1.0), Color::new(dlg_border.r, dlg_border.g, dlg_border.b, 0.25));
    let btn_w  = 150.0_f32;
    let btn_h  = 44.0_f32;
    let btn_y  = dlg_y + dlg_h - btn_h - 24.0;
    let gap    = 20.0_f32;
    let total  = btn_w * 2.0 + gap;
    let btn_no_x  = dlg_x + (dlg_w - total) / 2.0;
    let btn_yes_x = btn_no_x + btn_w + gap;

    let (mx, my) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);

    let hover_no  = mx >= btn_no_x  && mx <= btn_no_x  + btn_w && my >= btn_y && my <= btn_y + btn_h;
    let hover_yes = mx >= btn_yes_x && mx <= btn_yes_x + btn_w && my >= btn_y && my <= btn_y + btn_h;

    draw_stat_btn(gc, f, btn_no_x, btn_y, btn_w, btn_h,
        &locale.exit_no, 17,
        if is_day { Color::new(0.10, 0.40, 0.70, 1.0) } else { Color::new(0.25, 0.65, 1.0, 1.0) },
        if is_day { Color::new(0.04, 0.06, 0.10, 1.0) } else { WHITE },
        Color::new(0.15, 0.80, 1.0, 1.0),
        hover_no);

    draw_stat_btn(gc, f, btn_yes_x, btn_y, btn_w, btn_h,
        &locale.exit_yes, 17,
        if is_day { Color::new(0.65, 0.12, 0.10, 1.0) } else { Color::new(0.9, 0.25, 0.25, 1.0) },
        if is_day { Color::new(0.04, 0.06, 0.10, 1.0) } else { WHITE },
        Color::new(1.0, 0.15, 0.15, 1.0),
        hover_yes);

    let hint_y = btn_y + btn_h + 16.0;
    let hint_col = if is_day { Color::new(0.40, 0.42, 0.48, 0.9) } else { Color::new(0.45, 0.45, 0.50, 0.9) };
    let hnw = measure_text("Esc",   Some(f), gc.s(11.0) as u16, 1.0).width / gc.scale_x;
    let hyw = measure_text("Enter", Some(f), gc.s(11.0) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, "Esc",   btn_no_x  + (btn_w - hnw) / 2.0, hint_y, 11, hint_col, f);
    draw_text_custom(gc, "Enter", btn_yes_x + (btn_w - hyw) / 2.0, hint_y, 11, hint_col, f);

    if is_key_pressed(KeyCode::Enter) { return ExitDialogResult::Confirm; }
    if lmb {
        if hover_yes { return ExitDialogResult::Confirm; }
        if hover_no  { return ExitDialogResult::Cancel;  }
    }
    ExitDialogResult::None
}
// ─────────────────────────────────────────────────────────────────────────────

// ── Диспетчер фона — делегирует в theme.rs ───────────────────────────────────
// theme.rs содержит draw_background_day, draw_cloud и draw_sky_gradient.
// Здесь оставляем только тонкий диспетчер.
pub fn draw_background(gc: &GraphicsContext, t: f64, is_day: bool) {
    if is_day {
        // Дневной фон: градиент неба + god-rays + реалистичные облака
        // Реализован в theme.rs → ThemeState::draw_background()
        // Но так как theme.rs использует screen_width/height напрямую,
        // а graphics.rs использует gc — вызываем inline адаптер:
        crate::theme::draw_background_day_gc(gc, t);
    } else {
        draw_background_stars(gc, t);
    }
}


pub fn draw_background_stars(gc: &GraphicsContext, t: f64) {
    let tf = t as f32;
    for i in 0..50 {
        let x = (i as f32 * 137.5 + tf * 10.0) % gc.base_w;
        let y = (i as f32 * 293.3 + tf * 5.0) % gc.base_h;
        let s = gc.s(1.0 + (i % 3) as f32);
        let a = 0.3 + 0.2 * (tf * 2.0 + i as f32).sin().abs();
        draw_circle(gc.sx(x), gc.sy(y), s, Color::new(1.0, 1.0, 1.0, a));
    }
}

fn draw_string_fret_stars(gc: &GraphicsContext, notes: &[GameNote]) {
    let l = &gc.layout;
    for n in notes {
        if n.hit || n.missed { continue; }
        let sx = gc.sx(l.fretboard_x + n.fret as f32 * l.fret_spacing + l.fret_spacing / 2.0);
        let sy = gc.sy(l.hit_zone_y + n.string_idx as f32 * 40.0 + 20.0);
        let sz = gc.s(12.0);
        let ba = 0.4;
        let pu = (get_time() as f32 * 4.0 + n.string_idx as f32).sin() * 0.1 + 0.1;
        let al = (ba + pu).min(0.8);
        let c = Color::new(n.color.r, n.color.g, n.color.b, al);
        for dx in [-1., 0., 1.] {
            for dy in [-1., 1.] {
                draw_line(sx, sy + dy * sz, sx + dx * sz / 2.0, sy, gc.s(2.0), c);
            }
        }
        draw_circle(sx, sy, gc.s(3.0), Color::new(1.0, 1.0, 1.0, al * 0.8));
    }
}

fn draw_measure_grid(gc: &GraphicsContext, s: &GameState) {
    let l = &gc.layout;
    let (beats, beat_type) = s.time_signature;
    if beats == 0 || beat_type == 0 { return; }
    let beat_duration = 60.0 / s.initial_tempo as f64 * (4.0 / beat_type as f64);
    let measure_duration = beats as f64 * beat_duration;
    let eff_spd = 250.0 * SPEED_MULTIPLIERS[s.speed_index];
    let start_measure = (s.song_time / measure_duration).floor() as i32;
    for i in 0..15 {
        let measure_time = (start_measure + i) as f64 * measure_duration;
        if measure_time < s.song_time { continue; }
        let tt = (measure_time - s.song_time) as f32;
        let x = l.playhead_x - (tt * eff_spd);
        if x < l.fretboard_x { break; }
        if x > l.playhead_x { continue; }
        draw_line(gc.sx(x), gc.sy(l.hit_zone_y - 350.0),
            gc.sx(x), gc.sy(l.hit_zone_y + l.fretboard_h),
            gc.s(1.0), Color::new(0.5, 0.5, 0.5, 0.3));
    }
}

fn draw_notation_track(
    gc: &GraphicsContext, notes: &[GameNote], f: &Font,
    ru: bool, off: f32, song_time: f64, speed_index: usize, theme: &ThemeState,
) {
    let l = &gc.layout;
    let is_day = theme.is_day;
    let bg = if is_day { Color::new(0.72, 0.76, 0.84, 0.90) } else { Color::new(0.06, 0.06, 0.1, 0.85) };
    let border = if is_day { Color::new(0.35, 0.45, 0.65, 0.55) } else { Color::new(0.4, 0.5, 0.7, 0.6) };
    let label_col = if is_day { Color::new(0.15, 0.20, 0.35, 0.95) } else { Color::new(0.7, 0.8, 1.0, 0.9) };
    let staff_line_col = if is_day { Color::new(0.30, 0.34, 0.45, 0.55) } else { Color::new(0.6, 0.6, 0.7, 0.4) };

    draw_rectangle(gc.sx(l.track_base_x), gc.sy(l.notation_track_y),
        gc.sx(l.track_end_x - l.track_base_x), gc.sy(100.0), bg);
    draw_rectangle_lines(gc.sx(l.track_base_x), gc.sy(l.notation_track_y),
        gc.sx(l.track_end_x - l.track_base_x), gc.sy(100.0), gc.s(2.), border);
    draw_text_custom(gc, if ru { "Ноты" } else { "Notation" },
        l.track_base_x + 10., l.notation_track_y + 12., 14, label_col, f);
    let sc = l.notation_track_y + 50.0;
    for i in -2..=2 {
        draw_line(gc.sx(l.track_base_x + 5.), gc.sy(sc + i as f32 * 12.0),
            gc.sx(l.track_end_x - 5.), gc.sy(sc + i as f32 * 12.0), gc.s(1.), staff_line_col);
    }

    // Скорость и точка "сейчас", собственные для стана: нота входит у левого
    // края (track_base_x) ровно в момент спавна на грифе и достигает
    // середины стана (track_mid_x) в момент удара — это и есть синхронизация
    // с реальным временем игры, не привязанная к узкому highway грифа.
    let half_width = (l.track_end_x - l.track_base_x) / 2.0;
    let track_mid_x = l.track_base_x + half_width;
    let eff_spd = 250.0 * SPEED_MULTIPLIERS[speed_index];
    let eff_spd_track = half_width * eff_spd / HIGHWAY_H;

    let mut track_notes: Vec<GameNote> = notes.iter().filter(|n| !n.hit && !n.missed)
        .map(|n| {
            let tt = (n.target_time - song_time) as f32;
            let mut c = n.clone();
            c.x = track_mid_x - (tt * eff_spd_track) + off;
            c
        })
        .collect();
    track_notes.retain(|n| n.x >= l.track_base_x - 20.0 && n.x <= l.track_end_x + 20.0);
    for ch in group_notes_into_chords(&track_notes) {
        draw_chord_notation(gc, &ch, sc, l.notation_track_y, 100.0, f);
    }

    // Хит-линия — теперь в середине стана
    draw_line(gc.sx(track_mid_x), gc.sy(l.notation_track_y - 5.),
        gc.sx(track_mid_x), gc.sy(l.notation_track_y + 105.),
        gc.s(3.), Color::new(1.0, 0.3, 0.3, 0.7));
    // Линия начала окна засчитывания — смещение пропорционально HIT_TOLERANCE
    let tol_x = track_mid_x - HIT_TOLERANCE * half_width / HIGHWAY_H;
    draw_line(gc.sx(tol_x), gc.sy(l.notation_track_y - 5.),
        gc.sx(tol_x), gc.sy(l.notation_track_y + 105.),
        gc.s(1.5), Color::new(1.0, 0.75, 0.2, 0.55));
}

fn draw_chord_notation(
    gc: &GraphicsContext, chord: &[&GameNote], sc: f32,
    track_y: f32, track_h: f32, _f: &Font,
) {
    if chord.is_empty() { return; }
    let cp = -5;
    let mut mn = i32::MAX; let mut mx = i32::MIN;
    let sx = chord[0].x; let mut sd = f64::MAX;
    for n in chord {
        if n.pitch_step < mn { mn = n.pitch_step; }
        if n.pitch_step > mx { mx = n.pitch_step; }
        if n.duration < sd { sd = n.duration; }
    }

    // Клэмп: ноты с экстремальным pitch_step не должны выходить за пределы
    // стана (раньше уходили в зону Tabs и перекрывались его фоном —
    // визуально выглядело как "нота проваливается и не отображается").
    let clamp_top = track_y + 6.0;
    let clamp_bottom = track_y + track_h - 6.0;
    let clamp_y = |raw_y: f32| raw_y.clamp(clamp_top, clamp_bottom);

    let top = clamp_y(sc - ((mx - cp) as f32) * 6.0);
    let bot = clamp_y(sc - ((mn - cp) as f32) * 6.0);
    let st_up = chord.iter().map(|n| n.pitch_step).sum::<i32>() / chord.len() as i32 <= cp;
    let sl = if sd <= 0.125 { 35. } else { 25. };
    for n in chord {
        let y = clamp_y(sc - ((n.pitch_step - cp) as f32) * 6.0);
        if sd >= 1.0 {
            draw_circle_lines(gc.sx(n.x), gc.sy(y), gc.s(7.5), gc.s(2.), n.color);
            draw_circle(gc.sx(n.x), gc.sy(y), gc.s(6.5), Color::new(0.1, 0.1, 0.1, 0.9));
            continue;
        }
        if sd >= 0.5 { draw_circle_lines(gc.sx(n.x), gc.sy(y), gc.s(5.0), gc.s(2.), n.color); }
        else { draw_circle(gc.sx(n.x), gc.sy(y), gc.s(5.0), n.color); }
    }
    if sd < 1.0 {
        let sy = if st_up { top } else { bot };
        let ey = clamp_y(if st_up { top - sl } else { bot + sl });
        draw_line(gc.sx(sx + 3.), gc.sy(sy), gc.sx(sx + 3.), gc.sy(ey), gc.s(2.), chord[0].color);
        if sd <= 0.25 {
            let cnt = if sd <= 0.0625 { 2 } else { 1 };
            let ln  = if sd <= 0.0625 { 18. } else { 14. };
            for b in 0..cnt {
                let yf = b as f32 * 5.0;
                let s  = clamp_y(ey + yf);
                let e  = clamp_y(s + (if st_up { 4.0 } else { -4.0 }));
                draw_line(gc.sx(sx + 3.), gc.sy(s),
                    gc.sx(sx + 3. + ln * if st_up { -1.0 } else { 1.0 }), gc.sy(e),
                    gc.s(2.), chord[0].color);
            }
        }
    }
}

fn draw_chord_tabs(gc: &GraphicsContext, chord: &[&GameNote], tb: f32, font: &Font) {
    if chord.is_empty() { return; }
    let mut mn = f32::MAX; let mut mx = f32::MIN;
    for n in chord {
        let si = NUM_STRINGS - 1 - n.string_idx;
        let y = tb + si as f32 * 11.0;
        draw_circle(gc.sx(n.x), gc.sy(y), gc.s(13.), Color::new(n.color.r, n.color.g, n.color.b, 0.25));
        let fs = if n.fret == 0 { "0".into() } else { n.fret.to_string() };
        draw_text_ex(&fs, gc.sx(n.x - 5.), gc.sy(y - 7.),
            TextParams { font_size: gc.s(13.) as u16, font: Some(font), color: WHITE, ..Default::default() });
        if n.x < mn { mn = n.x; }
        if n.x > mx { mx = n.x; }
    }
    if chord.len() > 1 {
        let top = tb + (NUM_STRINGS - 1 - chord[0].string_idx) as f32 * 11.0;
        let bot = tb + (NUM_STRINGS - 1 - chord[chord.len() - 1].string_idx) as f32 * 11.0;
        draw_line(gc.sx(mn - 14.), gc.sy(top), gc.sx(mn - 14.), gc.sy(bot),
            gc.s(2.), Color::new(0.9, 0.9, 0.9, 0.7));
    }
}

fn draw_tab_track(
    gc: &GraphicsContext, notes: &[GameNote], f: &Font,
    ru: bool, off: f32, song_time: f64, speed_index: usize, theme: &ThemeState,
) {
    let l = &gc.layout;
    let is_day = theme.is_day;
    let bg = if is_day { Color::new(0.70, 0.78, 0.74, 0.90) } else { Color::new(0.06, 0.06, 0.1, 0.85) };
    let border = if is_day { Color::new(0.30, 0.55, 0.40, 0.55) } else { Color::new(0.4, 0.7, 0.5, 0.6) };
    let label_col = if is_day { Color::new(0.10, 0.30, 0.20, 0.95) } else { Color::new(0.7, 1.0, 0.8, 0.9) };
    let staff_line_col = if is_day { Color::new(0.35, 0.40, 0.45, 0.45) } else { Color::new(0.7, 0.7, 0.8, 0.3) };

    draw_rectangle(gc.sx(l.track_base_x), gc.sy(l.tab_track_y),
        gc.sx(l.track_end_x - l.track_base_x), gc.sy(80.0), bg);
    draw_rectangle_lines(gc.sx(l.track_base_x), gc.sy(l.tab_track_y),
        gc.sx(l.track_end_x - l.track_base_x), gc.sy(80.0), gc.s(2.), border);
    draw_text_custom(gc, if ru { "Табы" } else { "Tabs" },
        l.track_base_x + 10., l.tab_track_y + 12., 14, label_col, f);
    let tb = l.tab_track_y + 25.;
    for st in 0..NUM_STRINGS {
        let y = tb + st as f32 * 11.0;
        draw_line(gc.sx(l.track_base_x + 5.), gc.sy(y), gc.sx(l.track_end_x - 5.), gc.sy(y), gc.s(2.), staff_line_col);
        draw_text_custom(gc, STRING_NAMES[NUM_STRINGS - 1 - st],
            l.track_base_x - 35., y + 4., 11, safe_string_color(NUM_STRINGS - 1 - st), f);
    }

    let half_width = (l.track_end_x - l.track_base_x) / 2.0;
    let track_mid_x = l.track_base_x + half_width;
    let eff_spd = 250.0 * SPEED_MULTIPLIERS[speed_index];
    let eff_spd_track = half_width * eff_spd / HIGHWAY_H;

    let mut track_notes: Vec<GameNote> = notes.iter().filter(|n| !n.hit && !n.missed)
        .map(|n| {
            let tt = (n.target_time - song_time) as f32;
            let mut c = n.clone();
            c.x = track_mid_x - (tt * eff_spd_track) + off;
            c
        })
        .collect();
    track_notes.retain(|n| n.x >= l.track_base_x - 20.0 && n.x <= l.track_end_x + 20.0);
    for ch in group_notes_into_chords(&track_notes) { draw_chord_tabs(gc, &ch, tb, f); }

    draw_line(gc.sx(track_mid_x), gc.sy(l.tab_track_y - 5.),
        gc.sx(track_mid_x), gc.sy(l.tab_track_y + 85.),
        gc.s(3.), Color::new(1.0, 0.3, 0.3, 0.7));
    let tol_x = track_mid_x - HIT_TOLERANCE * half_width / HIGHWAY_H;
    draw_line(gc.sx(tol_x), gc.sy(l.tab_track_y - 5.),
        gc.sx(tol_x), gc.sy(l.tab_track_y + 85.),
        gc.s(1.5), Color::new(1.0, 0.75, 0.2, 0.55));
}

// ─────────────────────────────────────────────────────────────────────────────
// Название ноты с диезом/бемолем для отображения на грифе.
// Для натуральных нот и нот с диезом: "C", "C#", "D" …
// Для нот которые можно записать как бемоль: показываем "C#/Db".
// ─────────────────────────────────────────────────────────────────────────────
fn fret_note_label(tuning: &[f32; NUM_STRINGS], string_idx: usize, fret: usize) -> String {
    // Полутоновые имена: диез
    const SHARP: [&str; 12] = ["C","C#","D","D#","E","F","F#","G","G#","A","A#","B"];
    // Энгармоничный бемоль для тех, у кого есть бемольный эквивалент
    const FLAT:  [Option<&str>; 12] = [
        None,       // C
        Some("Db"), // C#/Db
        None,       // D
        Some("Eb"), // D#/Eb
        None,       // E
        None,       // F
        Some("Gb"), // F#/Gb
        None,       // G
        Some("Ab"), // G#/Ab
        None,       // A
        Some("Bb"), // A#/Bb
        None,       // B
    ];
    let base = tuning.get(string_idx.min(5)).copied().unwrap_or(82.41);
    let freq = base * 2.0_f32.powf(fret as f32 / 12.0);
    let st = 12.0 * (freq / 440.0).log2();
    let idx = (((st + 69.0).round() as i32) % 12 + 12) as usize % 12;
    let sharp = SHARP[idx];
    if let Some(flat) = FLAT[idx] {
        format!("{}/{}", sharp, flat)
    } else {
        sharp.to_string()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ГРИФ
// ─────────────────────────────────────────────────────────────────────────────
pub fn draw_fretboard(gc: &GraphicsContext, s: &GameState, f: &Font, theme: &ThemeState) {
    let l = &gc.layout;
    let is_day = theme.is_day;

    // ── Геометрия ────────────────────────────────────────────────────────────
    // Порожек (nut) сдвинут на один fret_spacing вправо от fretboard_x.
    // Зона fretboard_x … nut_x — «открытые струны».
    // Лады 1..TOTAL_FRETS рисуются правее порожка.
    let nut_x      = l.fretboard_x + l.fret_spacing; // физическое положение порожка
    let open_zone_w = l.fret_spacing;                // ширина зоны открытых струн

    let highway_top    = l.hit_zone_y - 350.0 - 10.0;
    let highway_bottom = l.hit_zone_y;
    let highway_x       = l.fretboard_x - 10.0;
    let highway_w       = l.fretboard_w + 20.0;

    if is_day {
        // Дневная тема: вместо светлого неба — тёмная грозовая туча с молниями
        crate::theme::draw_storm_highway(gc, get_time(), highway_top, highway_bottom, highway_x, highway_w);
    }

    // Накладка грифа — почти чёрное дерево (эбен), одинаково в обеих темах
    draw_wood_neck_realistic(gc, l);

    draw_rectangle(gc.sx(l.fretboard_x), gc.sy(l.hit_zone_y),
        gc.sx(open_zone_w), gc.sy(l.fretboard_h),
        Color::new(0.05, 0.05, 0.06, 0.35));


    // Подсветка активных ладов (hint). fret 0 = открытые, fret 1..N = за порожком
    for ft in 0..=TOTAL_FRETS {
        let xs = if ft == 0 {
            l.fretboard_x // зона открытых струн
        } else {
            nut_x + (ft as f32 - 1.0) * l.fret_spacing
        };
        if let Some(c) = s.active_hints[ft] {
            draw_rectangle(gc.sx(xs), gc.sy(l.hit_zone_y), gc.sx(l.fret_spacing), gc.sy(l.fretboard_h),
                Color::new(c.r, c.g, c.b, 0.25));
            draw_rectangle_lines(gc.sx(xs), gc.sy(l.hit_zone_y), gc.sx(l.fret_spacing), gc.sy(l.fretboard_h),
                gc.s(2.), Color::new(c.r, c.g, c.b, 0.6));
        }
    }

    // ── Порожек (NUT) ─────────────────────────────────────────────────────────
    {
        let nut_w = gc.s(9.0);
        let x = nut_x;
        draw_rectangle(gc.sx(x - 2.0), gc.sy(l.hit_zone_y - 2.0),
            nut_w + gc.s(4.0), gc.sy(l.fretboard_h + 4.0), Color::new(0.0, 0.0, 0.0, 0.55));
        draw_rectangle(gc.sx(x), gc.sy(l.hit_zone_y),
            nut_w, gc.sy(l.fretboard_h), Color::new(0.88, 0.85, 0.75, 1.0));
        draw_rectangle(gc.sx(x), gc.sy(l.hit_zone_y),
            gc.s(2.5), gc.sy(l.fretboard_h), Color::new(1.0, 0.98, 0.92, 0.65));
        draw_rectangle(gc.sx(x) + nut_w - gc.s(2.0), gc.sy(l.hit_zone_y),
            gc.s(2.0), gc.sy(l.fretboard_h), Color::new(0.35, 0.30, 0.22, 0.85));
        draw_rectangle_lines(gc.sx(x - 1.0), gc.sy(l.hit_zone_y - 1.0),
            nut_w + gc.s(2.0), gc.sy(l.fretboard_h + 2.0),
            gc.s(2.5), Color::new(0.95, 0.88, 0.45, 0.55));
        draw_rectangle_lines(gc.sx(x - 2.5), gc.sy(l.hit_zone_y - 2.5),
            nut_w + gc.s(5.0), gc.sy(l.fretboard_h + 5.0),
            gc.s(1.0), Color::new(0.95, 0.88, 0.45, 0.20));
        // Подпись "0" под зоной открытых струн (по центру зоны)
        let label_x = l.fretboard_x + open_zone_w / 2.0 - 4.0;
        draw_text_ex("0", gc.sx(label_x), gc.sy(l.hit_zone_y + l.fretboard_h + 15.0),
            TextParams { font_size: gc.s(14.0) as u16, font: Some(f),
                color: Color::new(0.9, 0.85, 0.5, 1.0), ..Default::default() });
    }

    // ── Лады 1..TOTAL_FRETS (правее порожка) ─────────────────────────────────
    for ft in 1..=TOTAL_FRETS {
        // Левый край лада ft — на расстоянии (ft-1)*fret_spacing от порожка
        let x = nut_x + (ft as f32 - 1.0) * l.fret_spacing;
        let al = if [3, 5, 7, 9, 12, 15, 17, 19].contains(&ft) { 0.8 } else { 0.3 };
        draw_line(gc.sx(x + 1.), gc.sy(l.hit_zone_y),
            gc.sx(x + 1.), gc.sy(l.hit_zone_y + l.fretboard_h), gc.s(2.), BLACK);
        draw_line(gc.sx(x), gc.sy(l.hit_zone_y),
            gc.sx(x), gc.sy(l.hit_zone_y + l.fretboard_h), gc.s(2.), Color::new(0.7, 0.7, 0.8, al));

        // Номер лада под грифом — по центру ячейки
        let label_x = x + l.fret_spacing / 2.0 - 5.0;
        draw_text_custom(gc, &ft.to_string(), label_x,
            l.hit_zone_y + l.fretboard_h + 15., 13, Color::new(0.7, 0.7, 0.8, 0.8), f);

        // Маркерные точки (по центру ячейки лада)
        let marker_x = x + l.fret_spacing / 2.0;
        if [3, 5, 7, 9, 15, 17, 19].contains(&ft) {
            draw_circle(gc.sx(marker_x), gc.sy(l.hit_zone_y + l.fretboard_h / 2.),
                gc.s(6.), Color::new(0.9, 0.9, 0.95, 0.6));
        } else if ft == 12 {
            draw_circle(gc.sx(marker_x), gc.sy(l.hit_zone_y + l.fretboard_h / 2. - 12.),
                gc.s(6.), Color::new(0.9, 0.9, 0.95, 0.6));
            draw_circle(gc.sx(marker_x), gc.sy(l.hit_zone_y + l.fretboard_h / 2. + 12.),
                gc.s(6.), Color::new(0.9, 0.9, 0.95, 0.6));
        }
    }

    // ── Струны + названия нот на грифе ────────────────────────────────────────
    for st in 0..NUM_STRINGS {
        let y = l.hit_zone_y + st as f32 * 40.0 + 20.0;
        draw_text_custom(gc, safe_string_name(st), l.fretboard_x - 70., y + 5., 16, safe_string_color(st), f);

        let th = match st { 0 => 3.5, 1 => 3.0, 2 => 2.5, 3 => 2.0, 4 => 1.5, _ => 1.2 };

        // Струна тянется через всю зону включая открытые
        draw_rectangle(gc.sx(l.fretboard_x), gc.sy(y - th / 2.),
            gc.sx(l.fretboard_w), gc.sy(th), Color::new(0.85, 0.75, 0.5, 0.4));

        // Нота открытой струны (fret 0) — по центру зоны открытых струн
        let open_xc = l.fretboard_x + open_zone_w / 2.0;
        let open_label = fret_note_label(&s.tuning, st, 0);
        let open_fs = if open_label.len() > 2 { 13u16 } else { 16u16 };
        let open_off = if open_label.len() > 2 { 14.0 } else { 9.0 };
        draw_text_ex(&open_label, gc.sx(open_xc - open_off), gc.sy(y + 6.),
            TextParams { font_size: gc.s(open_fs as f32) as u16, font: Some(f),
                color: Color::new(0.95, 0.85, 0.55, 0.55), ..Default::default() });

        // Ноты на ладах 1..TOTAL_FRETS
        for ft in 1..=TOTAL_FRETS {
            let xc = nut_x + (ft as f32 - 1.0) * l.fret_spacing + l.fret_spacing / 2.0;
            let label = fret_note_label(&s.tuning, st, ft);
            // Уменьшаем шрифт для "C#/Db" (6 символов)
            let (fs, x_off) = if label.len() > 2 { (11u16, 17.0_f32) } else { (14u16, 10.0_f32) };
            draw_text_ex(&label, gc.sx(xc - x_off), gc.sy(y + 6.),
                TextParams { font_size: gc.s(fs as f32) as u16, font: Some(f),
                    color: Color::new(0.9, 0.9, 0.9, 0.28), ..Default::default() });
        }
    }

    // После отрисовки грифа, если lesson.paused:
    if s.lesson_mode && s.lesson.paused {
        let current_event = s.lesson.current_event_idx
            .and_then(|i| s.events.get(i));
        let hint = if s.lesson.show_hint {
            current_event.map(|ev| HintData::from_event(ev))
            } else { None };
            draw_lesson_pause_overlay(gc, f, &s.lesson, hint.as_ref(), current_event);
    }
    // ── Hit-zone полоса ───────────────────────────────────────────────────────
    let pu = (get_time() as f32 * 5.0).sin() * 0.1 + 0.9;
    let hz_col = Color::new(0.2 * pu, 0.9 * pu, 0.3 * pu, 0.9);
    draw_rectangle(gc.sx(l.fretboard_x - 5.), gc.sy(l.hit_zone_y - 4.),
        gc.sx(l.fretboard_w + 10.), gc.s(8.), hz_col);
    draw_rectangle(gc.sx(l.fretboard_x - 5.), gc.sy(l.hit_zone_y - 7.),
        gc.sx(l.fretboard_w + 10.), gc.s(14.), Color::new(hz_col.r, hz_col.g, hz_col.b, 0.15));
    draw_text_custom(gc, "HIT ZONE", l.fretboard_x + l.fretboard_w + 10., l.hit_zone_y + 5.,
        14, Color::new(0.2, 0.9, 0.3, 0.9), f);

    draw_string_fret_stars(gc, &s.notes_compat);

    // ── Подсветка ромбами — над нужным ладом ─────────────────────────────────
    // Для каждой ноты вычисляем X-позицию её лада, ромб рисуется над грифом
    // (выше hit_zone_y - 350, то есть выше верхнего края грифа).
    let warn_secs = 0.5_f64;

    // Для каждой струны: (color, progress, fret)
    let mut string_warn: [Option<(Color, f32, usize)>; NUM_STRINGS] = [None; NUM_STRINGS];
    for n in &s.notes_compat {
        if n.hit || n.missed { continue; }
        let time_to_hit = n.target_time - s.song_time;
        let hit_tol = 0.15_f64;
        if time_to_hit >= -hit_tol && time_to_hit <= warn_secs {
            let progress = (1.0 - time_to_hit / warn_secs).clamp(0.0, 1.0) as f32;
            let si = n.string_idx;
            let replace = match string_warn[si] {
                None => true,
                Some((_, prev_p, _)) => progress > prev_p,
            };
            if replace {
                string_warn[si] = Some((n.color, progress, n.fret));
            }
        }
    }

    // X-центр ячейки для лада (0 = открытая струна, 1..N = за порожком)
    let fret_center_x = |fret: usize| -> f32 {
        if fret == 0 {
            l.fretboard_x + open_zone_w / 2.0
        } else {
            nut_x + (fret as f32 - 1.0) * l.fret_spacing + l.fret_spacing / 2.0
        }
    };

    for si in 0..NUM_STRINGS {
        if let Some((color, progress, fret)) = string_warn[si] {
            let pulse_freq = 4.0 + progress * 8.0;
            let pulse = ((get_time() as f32 * pulse_freq).sin() * 0.25 + 0.75).clamp(0.0, 1.0);
            let alpha = (progress * 0.7 + 0.3) * pulse;
            let size = gc.s(10.0 + progress * 5.0);

            // Ромб рисуется НАД верхним краем грифа, по X — над нужным ладом
            let cx = gc.sx(fret_center_x(fret));
            // Верх грифа = hit_zone_y - 350; ромб чуть выше него
            let diamond_y = l.hit_zone_y - 350.0 - 18.0;
            let cy = gc.sy(diamond_y);

            // Glow
            for layer in 1..=3u8 {
                let lf = layer as f32 / 3.0;
                let ls = size * (1.0 + lf * 0.8);
                let la = alpha * (1.0 - lf) * 0.4;
                draw_triangle(Vec2::new(cx, cy - ls), Vec2::new(cx + ls * 0.65, cy), Vec2::new(cx - ls * 0.65, cy),
                    Color::new(color.r, color.g, color.b, la));
                draw_triangle(Vec2::new(cx, cy + ls), Vec2::new(cx + ls * 0.65, cy), Vec2::new(cx - ls * 0.65, cy),
                    Color::new(color.r, color.g, color.b, la));
            }
            // Основной ромб
            draw_triangle(Vec2::new(cx, cy - size), Vec2::new(cx + size * 0.65, cy), Vec2::new(cx - size * 0.65, cy),
                Color::new(color.r, color.g, color.b, alpha));
            draw_triangle(Vec2::new(cx, cy + size), Vec2::new(cx + size * 0.65, cy), Vec2::new(cx - size * 0.65, cy),
                Color::new(color.r, color.g, color.b, alpha));
            // Белое ядро
            let core = size * 0.3;
            draw_triangle(Vec2::new(cx, cy - core), Vec2::new(cx + core * 0.65, cy), Vec2::new(cx - core * 0.65, cy),
                Color::new(1.0, 1.0, 1.0, alpha * 0.75));
            draw_triangle(Vec2::new(cx, cy + core), Vec2::new(cx + core * 0.65, cy), Vec2::new(cx - core * 0.65, cy),
                Color::new(1.0, 1.0, 1.0, alpha * 0.75));

            // Вертикальная линия вниз от ромба до верхнего края грифа
            draw_line(cx, cy + size,
                cx, gc.sy(l.hit_zone_y - 350.0),
                gc.s(1.5), Color::new(color.r, color.g, color.b, alpha * 0.5));

            // Цветная подсветка вертикальной полосы лада на грифе
            let fx = fret_center_x(fret) - l.fret_spacing / 2.0;
            let fw = l.fret_spacing;
            draw_rectangle(gc.sx(fx), gc.sy(l.hit_zone_y),
                gc.sx(fw), gc.sy(l.fretboard_h),
                Color::new(color.r, color.g, color.b, alpha * 0.18));
        }
    }
    // ── Конец подсветки ───────────────────────────────────────────────────────

    // ── Ноты (летящие шары) ───────────────────────────────────────────────────
    for ev in &s.events  {
        draw_game_event(gc, ev, f, s.song_time, s.lesson_mode);
}
    for p in &s.particles {
        let al = (p.life / 0.7).min(1.0);
        draw_circle(gc.sx(p.x), gc.sy(p.y), gc.s(p.size), Color::new(p.color.r, p.color.g, p.color.b, al));
    }
    if s.flash_timer > 0. {
        let al = (s.flash_timer / 0.15).min(1.0) * 0.4;
        draw_rectangle(gc.sx(0.), gc.sy(0.), gc.sx(l.window_w), gc.sy(l.window_h), Color::new(s.flash_color.r, s.flash_color.g, s.flash_color.b, al));
    }
    draw_measure_grid(gc, s);
    draw_notation_track(gc, &s.notes_compat, f, s.lang_ru, s.manual_track_offset, s.song_time, s.speed_index, theme);
    draw_tab_track(gc, &s.notes_compat, f, s.lang_ru, s.manual_track_offset, s.song_time, s.speed_index, theme);
}

/// Реалистичная деревянная накладка грифа для дневной темы: градиент
/// тёмного дерева (палисандр) + тонкие волокна. Вызывать как фон ДО ладов/струн.
fn draw_wood_neck_realistic(gc: &GraphicsContext, l: &CalculatedLayout) {
    let top_y    = l.hit_zone_y;
    let bottom_y = l.hit_zone_y + l.fretboard_h;
    let total_h  = (bottom_y - top_y).max(1.0);

    let bands = 18usize;
    for i in 0..bands {
        let t = i as f32 / bands as f32;
        let r = 0.085 - t * 0.02;
        let g = 0.065 - t * 0.015;
        let b = 0.055 - t * 0.012;
        let y0 = top_y + total_h * (i as f32 / bands as f32);
        let y1 = top_y + total_h * ((i + 1) as f32 / bands as f32);
        draw_rectangle(
            gc.sx(l.fretboard_x - 10.0), gc.sy(y0),
            gc.sx(l.fretboard_w + 20.0), gc.sy((y1 - y0 + 0.5).max(0.5)),
            Color::new(r.max(0.0), g.max(0.0), b.max(0.0), 0.95),
        );
    }

    let mut seed: u32 = 0x9E3779B1;
    let mut rnd = || -> f32 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        seed as f32 / u32::MAX as f32
    };
    for _ in 0..26 {
        let ry    = top_y + rnd() * total_h;
        let rw    = l.fretboard_w * (0.25 + rnd() * 0.55);
        let rx    = l.fretboard_x + rnd() * (l.fretboard_w - rw).max(0.0);
        let alpha = 0.04 + rnd() * 0.06;
        draw_line(
            gc.sx(rx), gc.sy(ry),
            gc.sx(rx + rw), gc.sy(ry + rnd() * 2.0 - 1.0),
            gc.s(1.0), Color::new(0.03, 0.02, 0.01, alpha),
        );
    }

    // Холодный лаковый блик сверху накладки
    draw_rectangle(
        gc.sx(l.fretboard_x - 10.0), gc.sy(top_y),
        gc.sx(l.fretboard_w + 20.0), gc.sy((total_h * 0.15).max(4.0)),
        Color::new(0.6, 0.65, 0.75, 0.06),
    );
}
// ─────────────────────────────────────────────────────────────────────────────

pub fn draw_waveform_and_mic_info(gc: &GraphicsContext, mc: &MicController, _: &GameState, f: &Font, theme: &ThemeState) {
    let l = &gc.layout;
    let is_day = theme.is_day;
    let bg = if is_day {Color::new(0.68, 0.72, 0.82, 0.92) } else { Color::new(0.05, 0.06, 0.1, 0.9) };
    let border = if is_day { Color::new(0.30, 0.35, 0.55, 0.55) } else { Color::new(0.3, 0.5, 0.8, 0.8) };
    let label_col = if is_day { Color::new(0.10, 0.12, 0.25, 1.0) } else { Color::new(0.6, 0.8, 1.0, 1.0) };
    let grid_col = if is_day { Color::new(0.45, 0.48, 0.58, 0.4) } else { Color::new(0.2, 0.3, 0.4, 0.5) };

    draw_rectangle(gc.sx(l.waveform_x - 5.), gc.sy(l.waveform_y - 25.), gc.sx(180.0 + 10.0), gc.sy(160.0 + 80.0), bg);
    draw_rectangle_lines(gc.sx(l.waveform_x - 5.), gc.sy(l.waveform_y - 25.), gc.sx(190.0), gc.sy(240.0), gc.s(2.), border);
    draw_text_custom(gc, "WAVEFORM & MIC", l.waveform_x + 10., l.waveform_y - 8., 18, label_col, f);
    let ad = mc.get_audio_state();
    let (act, df, wb, cv, em, rec, _dn) = if let Ok(d) = ad.lock() {
        (d.is_stream_active, d.detected_freq, d.wave_buffer.clone(), d.current_volume, d.error_msg.clone(), d.is_recording, d.current_device_name.clone())
    } else { (false, None, vec![0.0; 512], 0.0, None, false, "Error".into()) };
    let tc = if cv > 0.0003 { GREEN } else { RED };
    draw_text_custom(gc, &format!("RMS: {:.4}", cv), l.waveform_x + 10., l.waveform_y + 160.0 + 10., 12, tc, f);
    match df {
        Some(freq) => draw_text_custom(gc, &format!("{:.1} Hz ({})", freq, get_note_name(freq)), l.waveform_x + 10., l.waveform_y + 160.0 + 25., 14, Color::new(0.2, 1.0, 0.5, 1.0), f),
        _ => draw_text_custom(gc, "NO PITCH", l.waveform_x + 10., l.waveform_y + 160.0 + 25., 14, YELLOW, f),
    }
    let stc = if act { Color::new(0.2, 1.0, 0.5, 1.0) } else { Color::new(1.0, 0.3, 0.3, 1.0) };
    draw_text_custom(gc, if act { "MIC ACTIVE" } else { "MIC INACTIVE" }, l.waveform_x + 10., l.waveform_y + 160.0 + 40., 14, stc, f);
    if let Some(e) = em {
        draw_text_custom(gc, &format!("Error: {}", e), l.waveform_x + 10., l.waveform_y + 160.0 + 55., 12, RED, f);
        return;
    }
    if rec {
        draw_circle(gc.sx(l.waveform_x + 180.0 - 20.), gc.sy(l.waveform_y - 15.), gc.s(8.), RED);
        draw_text_custom(gc, "REC", l.waveform_x + 180.0 - 40., l.waveform_y - 20., 12, RED, f);
    }
    let vu_w = 20.; let vu_h = 160.0 - 20.;
    let vx = l.waveform_x + 180.0 + 15.; let vy = l.waveform_y + 10.;
    draw_rectangle(gc.sx(vx), gc.sy(vy), gc.sx(vu_w), gc.sy(vu_h), Color::new(0.2, 0.2, 0.2, 1.0));
    let fh = (cv * 2.0).clamp(0.0, 1.0) * vu_h;
    let fc = if cv > 0.1 { RED } else if cv > 0.05 { YELLOW } else { GREEN };
    draw_rectangle(gc.sx(vx), gc.sy(vy + vu_h - fh), gc.sx(vu_w), gc.sy(fh), fc);
    draw_rectangle_lines(gc.sx(vx), gc.sy(vy), gc.sx(vu_w), gc.sy(vu_h), gc.s(1.), if is_day { Color::new(0.2, 0.25, 0.4, 1.0) } else { WHITE });
    draw_text_custom(gc, "VOL", vx - 5., vy - 15., 12, if is_day { Color::new(0.15, 0.18, 0.30, 1.0) } else { WHITE }, f);
    for i in 0..10 {
        let x = l.waveform_x + (i as f32 / 9.0) * 180.0;
        draw_line(gc.sx(x), gc.sy(l.waveform_y), gc.sx(x), gc.sy(l.waveform_y + 160.0), gc.s(1.), grid_col);
    }
    // Клэмп волны в границы прямоугольника — раньше при громком сигнале
    // (cv/амплитуда > предела) линия выходила выше/ниже бокса.
    let wave_top = l.waveform_y;
    let wave_bottom = l.waveform_y + 160.0;
    let mut pts = Vec::with_capacity(wb.len());
    for (i, &s) in wb.iter().enumerate() {
        let x = l.waveform_x + (i as f32 / (wb.len() - 1) as f32) * 180.0;
        let y = (l.waveform_y + 160.0 / 2.0 - (s * 160.0 * 0.4)).clamp(wave_top, wave_bottom);
        pts.push((x, y));
    }
    for i in 0..(pts.len() - 1) {
        draw_line(gc.sx(pts[i].0), gc.sy(pts[i].1), gc.sx(pts[i + 1].0), gc.sy(pts[i + 1].1), gc.s(2.0), Color::new(0.2, 1.0, 0.5, 0.9));
    }
}


pub fn draw_frequency_graph(gc: &GraphicsContext, log: &GameLogger, ct: f64, f: &Font, theme: &ThemeState) {
    let l = &gc.layout;
    let is_day = theme.is_day;

    // ── Привязка к нотному стану в правом верхнем углу ────────────────────
    // Используем те же константы, что и в draw_mini_notation_top_right
    let staff_panel_w = 420.0_f32;
    let staff_panel_h = 150.0_f32;
    let staff_panel_x = l.window_w - staff_panel_w - 20.0;
    let staff_panel_y = 64.0_f32;

    // График располагается строго под нотным станом с отступом 10px
    let graph_x = staff_panel_x;
    let graph_y = staff_panel_y + staff_panel_h + 10.0;
    let graph_w = staff_panel_w; // Ширина совпадает с шириной стана
    let graph_h = 150.0_f32;     // Стандартная высота графика

    // ── Отрисовка фона и рамки ────────────────────────────────────────────
    let bg = if is_day { Color::new(0.68, 0.72, 0.82, 0.90) } else { Color::new(0.05, 0.05, 0.1, 0.9) };
    let border = if is_day { Color::new(0.35, 0.38, 0.55, 0.55) } else { Color::new(0.4, 0.4, 0.6, 0.8) };
    let label_col = if is_day { Color::new(0.10, 0.12, 0.25, 1.0) } else { WHITE };
    let grid_col = if is_day { Color::new(0.45, 0.48, 0.58, 0.5) } else { Color::new(0.2, 0.2, 0.3, 0.5) };
    let grid_label_col = if is_day { Color::new(0.35, 0.38, 0.48, 1.0) } else { GRAY };

    draw_rectangle(gc.sx(graph_x), gc.sy(graph_y), gc.sx(graph_w), gc.sy(graph_h), bg);
    draw_rectangle_lines(gc.sx(graph_x), gc.sy(graph_y), gc.sx(graph_w), gc.sy(graph_h), gc.s(2.), border);
    draw_text_custom(gc, "FREQ vs TIME", graph_x + 5., graph_y + 15., 12, label_col, f);

    // ── Координатная сетка и данные ───────────────────────────────────────
    let minf = 50.0; let maxf = 1000.0;
    let top = graph_y;
    let bottom = graph_y + graph_h;

    let f2y = |fr: f32| -> f32 {
        if fr <= 0.0 { return bottom; }
        let r = (fr / minf).ln() / (maxf / minf).ln();
        (bottom - r * graph_h).clamp(top, bottom)
    };

    let t2x = |t: f64| -> f32 {
        let d = ct - t;
        if d < 0.0 { return graph_x + graph_w; }
        let r = d as f32 / 5.0;
        if r > 1.0 { return graph_x - 10.; }
        graph_x + graph_w - r * graph_w
    };

    // Горизонтальные линии сетки
    for &fr in &[110.0, 220.0, 440.0, 660.0] {
        let y = f2y(fr);
        draw_line(gc.sx(graph_x), gc.sy(y), gc.sx(graph_x + graph_w), gc.sy(y), gc.s(1.), grid_col);
        draw_text_custom(gc, &format!("{:.0}", fr), graph_x + 4., y - 8., 8, grid_label_col, f);
    }

    // Целевые ноты (Target)
    for ev in &log.events {
        if ev.is_target {
            let x = t2x(ev.timestamp);
            if x >= graph_x && x <= graph_x + graph_w {
                if let Some(ef) = ev.expected_freq {
                    let yt = f2y(ef + 20.0); 
                    let yb = f2y(ef - 20.0); 
                    let h = (yb - yt).abs();
                    let bc = safe_string_color(ev.string_idx);
                    let br = match ev.hit_result { Some(true) => GREEN, Some(false) => RED, _ => Color::new(bc.r, bc.g, bc.b, 1.0) };
                    
                    draw_rectangle(gc.sx(x - 2.), gc.sy(yt), gc.s(4.), gc.sy(h), Color::new(bc.r, bc.g, bc.b, 0.3));
                    draw_rectangle_lines(gc.sx(x - 2.), gc.sy(yt), gc.s(4.), gc.sy(h), gc.s(2.), br);
                    
                    if ev.hit_result == Some(false) {
                        draw_line(gc.sx(x - 4.), gc.sy(yt), gc.sx(x + 6.), gc.sy(yb), gc.s(2.), RED);
                        draw_line(gc.sx(x + 6.), gc.sy(yt), gc.sx(x - 4.), gc.sy(yb), gc.s(2.), RED);
                    }
                }
            }
        }
    }

    // Детектированная частота (Mic/MIDI)
    let mut pp: Option<(f32, f32)> = None;
    for ev in &log.events {
        if !ev.is_target {
            if let Some(fr) = ev.freq {
                let x = t2x(ev.timestamp); 
                let y = f2y(fr);
                if x >= graph_x && x <= graph_x + graph_w {
                    draw_circle(gc.sx(x), gc.sy(y), gc.s(2.), Color::new(0.2, 1.0, 0.2, 0.9));
                    if let Some((px, py)) = pp {
                        if (x - px).abs() < 15.0 { 
                            draw_line(gc.sx(px), gc.sy(py), gc.sx(x), gc.sy(y), gc.s(1.), Color::new(0.2, 1.0, 0.2, 0.5)); 
                        }
                    }
                    pp = Some((x, y));
                }
            }
        }
    }

    // Правая граница (ось времени)
    draw_line(gc.sx(graph_x + graph_w), gc.sy(graph_y), gc.sx(graph_x + graph_w), gc.sy(graph_y + graph_h), gc.s(2.), if is_day { Color::new(0.2, 0.25, 0.4, 1.0) } else { WHITE });
}

pub fn draw_ui(gc: &GraphicsContext, s: &GameState, f: &Font) {
    let l = &gc.layout;
    let t = &s.locale;

    // ── Заголовок и название песни (центрированы по горизонтали) ───────────
    let tw = measure_text(&t.title, Some(f), gc.s(26.0) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, &t.title, l.window_w / 2.0 - tw / 2.0, 25., 26, WHITE, f);

    let sl = format!("{} {}", t.song_label, truncate_str(&s.current_song_name, 40));
    let slw = measure_text(&sl, Some(f), gc.s(20.0) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, &sl, l.window_w / 2.0 - slw / 2.0, 60., 20, Color::new(0.8, 0.8, 1.0, 1.0), f);

    // ── Счёт, скорость, комбо ──────────────────────────────────────────────
    draw_text_custom(gc, &format!("{} {}", t.score_label, s.score), 20., l.window_h - 50., 22, if s.combo >= 20 { GOLD } else { WHITE }, f);
    draw_text_custom(gc, &format!("{} {}", t.speed_label, SPEED_LABELS[s.speed_index]), l.window_w - 120., l.window_h - 50., 18, Color::new(0.9, 0.9, 0.3, 1.0), f);
    if s.combo > 1 {
        draw_text_custom(gc, &t.combo_label.replace("{}", &s.combo.to_string()), 20., l.window_h - 25., 20, Color::new(1., 0.5, 0.2, 1.), f);
    }

    // ── Подсказки управления ───────────────────────────────────────────────
    draw_text_custom(gc, &format!("{} | {}", t.hint_record, t.hint_songs),
        l.window_w / 2. - 150., l.window_h - 52., 13, Color::new(0.6, 0.6, 0.7, 0.85), f);

    // ── Прогресс-бар песни ─────────────────────────────────────────────────
    let pr = if s.song_data.is_empty() { 0.0 } else { s.last_spawn_idx as f32 / s.song_data.len() as f32 };
    draw_rectangle(gc.sx(l.window_w / 2. - 100.), gc.sy(l.window_h - 15.), gc.sx(200. * pr), gc.s(8.), Color::new(0.3, 0.8, 0.9, 0.8));

    // ── Экран завершения песни (центрированный) ────────────────────────────
    if s.game_over {
        draw_rectangle(gc.sx(0.), gc.sy(0.), gc.sx(l.window_w), gc.sy(l.window_h), Color::new(0., 0., 0., 0.85));

        let cx = l.window_w / 2.0;
        let cy = l.window_h / 2.0;

        // Заголовок "Песня завершена!"
        let title_fs = 36u16;
        let title_mw = measure_text(&t.song_complete, Some(f), gc.s(title_fs as f32) as u16, 1.0).width / gc.scale_x;
        draw_text_custom(gc, &t.song_complete, cx - title_mw / 2.0, cy - 60.0, title_fs, GOLD, f);

        // Итоговый счёт
        let score_label = format!("{} {}", t.final_score, s.score);
        let score_fs = 26u16;
        let score_mw = measure_text(&score_label, Some(f), gc.s(score_fs as f32) as u16, 1.0).width / gc.scale_x;
        draw_text_custom(gc, &score_label, cx - score_mw / 2.0, cy - 10.0, score_fs, WHITE, f);

        // Подсказка Space → статистика
        let hint_fs = 20u16;
        let hint_mw = measure_text(&t.restart_space, Some(f), gc.s(hint_fs as f32) as u16, 1.0).width / gc.scale_x;
        draw_text_custom(gc, &t.restart_space, cx - hint_mw / 2.0, cy + 40.0, hint_fs, Color::new(0.8, 0.8, 1., 1.), f);

        // Кнопка Restart удалена по предыдущему запросу
    }

    // ── Всплывающее сообщение ──────────────────────────────────────────────
    if let Some((ref msg, _)) = s.message {
        let mw = measure_text(msg, Some(f), gc.s(20.0) as u16, 1.0).width / gc.scale_x;
        draw_text_custom(gc, msg, l.window_w / 2.0 - mw / 2.0, l.window_h / 2.0 - 80.0, 20, Color::new(1.0, 0.9, 0.3, 1.0), f);
    }
}

pub fn draw_pause_overlay(gc: &GraphicsContext, s: &GameState, f: &Font) {
    let l = &gc.layout;
    let t = &s.locale;
    draw_rectangle(gc.sx(0.), gc.sy(0.), gc.sx(l.window_w), gc.sy(l.window_h), Color::new(0.0, 0.0, 0.0, 0.6));
    let pw = measure_text(&t.paused_title, Some(f), gc.s(48.0) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, &t.paused_title, l.window_w / 2.0 - pw / 2.0, l.window_h / 2. - 40., 48, WHITE, f);
    let hw = measure_text(&t.paused_hint, Some(f), gc.s(24.0) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, &t.paused_hint, l.window_w / 2.0 - hw / 2.0, l.window_h / 2. + 20., 24, Color::new(0.8, 0.8, 1.0, 1.0), f);
}

// ─────────────────────────────────────────────────────────────────────────────
// МЕНЮ ВЫБОРА ПЕСНИ
// Возвращает (selected_idx, actual_song_idx, enter_pressed, tuner_clicked)
// ─────────────────────────────────────────────────────────────────────────────
pub fn draw_song_selection_menu(
    gc: &GraphicsContext,
    songs: &[Song],
    selected_index: usize,
    font: &Font,
    _lang_ru: bool,
    search: &mut SearchState,
    locale: &Localization,
    theme: &ThemeState,
) -> (usize, usize, bool, bool, bool, Option<usize>) {
    let t = locale;
    let menu_w = 620.0_f32;
    let menu_h = 430.0_f32;
    let menu_x = (gc.base_w - menu_w) / 2.0;
    let menu_y = (gc.base_h - menu_h) / 2.0;
 
    let (mx_log, my_log) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);
 
    draw_rectangle(gc.sx(menu_x), gc.sy(menu_y), gc.sx(menu_w), gc.sy(menu_h),
        crate::theme::adaptive_menu_bg(theme.is_day));
    draw_glow_rect_lines(gc, menu_x, menu_y, menu_w, menu_h, Color::new(0.3, 0.8, 0.5, 1.0));
 
    // ── Кнопки верхнего правого угла ──────────────────────────────────────────
    let top_btn_h = 30.0_f32;
    let top_btn_y = menu_y + 8.0;
 
    // TUNER — крайняя правая
    //let tbtn_w = 84.0_f32;
    //let tbtn_x = menu_x + menu_w - tbtn_w - 8.0;
    //let tbtn_hover = mx_log >= tbtn_x && mx_log <= tbtn_x + tbtn_w
    //    && my_log >= top_btn_y && my_log <= top_btn_y + top_btn_h;
    //let tbtn_clicked = lmb && tbtn_hover;
    //draw_stat_btn(gc, font, tbtn_x, top_btn_y, tbtn_w, top_btn_h,
    //    "TUNER", 15,
    //    Color::new(0.3, 0.85, 0.55, 1.0), WHITE, Color::new(0.2, 1.0, 0.65, 1.0),
    //    tbtn_hover);
 
    // IMP (↑) — слева от TUNER
    //let ibtn_w = 38.0_f32;
    //let ibtn_x = tbtn_x - ibtn_w - 6.0;
    let ibtn_w = 84.0_f32;
    let ibtn_x = menu_x + menu_w - 92.0;
    let ibtn_hover = mx_log >= ibtn_x && mx_log <= ibtn_x + ibtn_w
        && my_log >= top_btn_y && my_log <= top_btn_y + top_btn_h;
    let ibtn_clicked = lmb && ibtn_hover;
    draw_stat_btn(gc, font, ibtn_x, top_btn_y, ibtn_w, top_btn_h,
        "IMPORT", 18,
        Color::new(0.6, 0.6, 1.0, 1.0), WHITE, Color::new(0.5, 0.5, 1.0, 1.0),
        ibtn_hover);
    if ibtn_hover {
        let hint = "Импортировать MusicXML";
        let hw = measure_text(hint, Some(font), gc.s(12.0) as u16, 1.0).width / gc.scale_x;
        draw_rectangle(
            gc.sx(ibtn_x + ibtn_w / 2.0 - hw / 2.0 - 4.0),
            gc.sy(top_btn_y + top_btn_h + 2.0),
            gc.sx(hw + 8.0), gc.sy(18.0),
            Color::new(0.1, 0.1, 0.2, 0.9));
        draw_text_custom(gc, hint,
            ibtn_x + ibtn_w / 2.0 - hw / 2.0,
            top_btn_y + top_btn_h + 14.0,
            12, Color::new(0.8, 0.8, 1.0, 1.0), font);
    }
 
    // ── Заголовок: центр по всей ширине меню ──────────────────────────────────
    let title_fs: u16 = 28;
    let title_mw = measure_text(&t.select_song_title, Some(font),
        gc.s(title_fs as f32) as u16, 1.0).width / gc.scale_x;
    let title_x = menu_x + (menu_w - title_mw) / 2.0;
    let title_col = if theme.is_day { Color::new(0.08, 0.14, 0.30, 1.0) } else { WHITE };
    draw_glow_text(gc, &t.select_song_title, title_x, menu_y + 36.0, title_fs, title_col, font);
    // Подсказка
    let hint_mw = measure_text(&t.select_song_hint, Some(font),
        gc.s(14.0) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, &t.select_song_hint,
        menu_x + (menu_w - hint_mw) / 2.0, menu_y + 60.0,
        14, GRAY, font);
 
    // ── Поле поиска ───────────────────────────────────────────────────────────
    let search_y = menu_y + 76.0;
    let search_w = menu_w - 60.0;
    let search_h = 34.0;
    let search_x = menu_x + 30.0;
    let search_fs: u16 = 16;

    let is_day = theme.is_day;
    let search_bg = if is_day {
        if search.active { Color::new(1.0, 1.0, 1.0, 0.95) } else { Color::new(0.92, 0.94, 0.97, 0.90) }
    } else {
        if search.active { Color::new(0.25, 0.35, 0.55, 0.75) } else { Color::new(0.13, 0.13, 0.18, 0.85) }
    };
    draw_rectangle(gc.sx(search_x), gc.sy(search_y), gc.sx(search_w), gc.sy(search_h), search_bg);
    if search.active {
        draw_glow_rect_lines(gc, search_x, search_y, search_w, search_h,
            Color::new(0.5, 0.75, 1.0, 1.0));
    } else {
        draw_rectangle_lines(gc.sx(search_x), gc.sy(search_y), gc.sx(search_w), gc.sy(search_h),
            gc.s(1.0), if is_day { Color::new(0.55, 0.60, 0.70, 0.7) } else { Color::new(0.4, 0.4, 0.5, 0.7) });
    }

    let label_col = if is_day { Color::new(0.20, 0.24, 0.36, 1.0) } else { Color::new(0.75, 0.78, 0.92, 1.0) };
    let label = &t.search_label;
    let label_mw = measure_text(label, Some(font),
        gc.s(search_fs as f32) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, label,
        search_x + 8.0, search_y + search_h * 0.70,
        search_fs, label_col, font);

    let text_x = search_x + label_mw + 14.0;
    let input_text_col = if is_day { Color::new(0.06, 0.08, 0.16, 1.0) } else { WHITE };
    let placeholder_col = if is_day { Color::new(0.45, 0.48, 0.55, 0.9) } else { Color::new(0.4, 0.42, 0.52, 0.9) };
    if search.is_empty() && !search.active {
        draw_text_custom(gc, &t.search_placeholder,
            text_x, search_y + search_h * 0.70,
            search_fs - 2, placeholder_col, font);
    } else {
        let dq = truncate_str(&search.text, 34);
        draw_text_custom(gc, &dq, text_x, search_y + search_h * 0.70, search_fs, input_text_col, font);
        if search.active && search.cursor_blink < 0.3 {
            let cw = measure_text(&dq, Some(font), gc.s(search_fs as f32) as u16, 1.0).width
                / gc.scale_x;
            let cx = text_x + cw + 2.0;
            draw_line(gc.sx(cx), gc.sy(search_y + 5.0),
                gc.sx(cx), gc.sy(search_y + search_h - 5.0),
                gc.s(2.0), if is_day { Color::new(0.1, 0.2, 0.4, 0.9) } else { Color::new(0.8, 0.9, 1.0, 0.9) });
        }
    }
 
    // Клик по полю поиска — активация
    let search_hovered = mx_log >= search_x && mx_log <= search_x + search_w
        && my_log >= search_y && my_log <= search_y + search_h;
    if lmb && search_hovered { search.active = true; }
 
    // Закрытие поиска при клике вне меню или на кнопки
    let outside_menu = lmb && !(mx_log >= menu_x && mx_log <= menu_x + menu_w
        && my_log >= menu_y && my_log <= menu_y + menu_h);
    if outside_menu || ibtn_clicked {
        search.active = false;
    }
 
    // Ввод текста поиска
    if search.active {
        while let Some(c) = get_char_pressed() {
            search.add_char(c); // внутри уже фильтрует control-символы
        }
        if is_key_pressed(KeyCode::Backspace) { search.backspace(); }
        if is_key_pressed(KeyCode::Escape)    { search.active = false; search.clear(); }
        if is_key_pressed(KeyCode::Enter)     { search.active = false; }
    }
 
    // ── Ранние возвраты по кнопкам ────────────────────────────────────────────
    if ibtn_clicked {
        let actual = filter_and_sort_songs(songs, &search.text)
            .get(selected_index).map(|(i, _)| *i).unwrap_or(0);
        return (selected_index, actual, false, false, true, None);
    }
 
    // ── Список песен ──────────────────────────────────────────────────────────
    let filtered = filter_and_sort_songs(songs, &search.text);
    let mut current_selected = selected_index;
 
    if !search.active {
        let (_, wheel_y) = mouse_wheel();
        if wheel_y > 0.0 && current_selected > 0 {
            current_selected = current_selected.saturating_sub(1);
        } else if wheel_y < 0.0 && current_selected < filtered.len().saturating_sub(1) {
            current_selected += 1;
        }
    }
 
    if songs.is_empty() {
        draw_text_custom(gc, &t.no_songs_found, menu_x + 120., menu_y + 160., 20, RED, font);
        draw_text_custom(gc, &t.create_json,    menu_x + 180., menu_y + 190., 16, GRAY, font);
        return (0, 0, false, false, false, None);
    }
 
    let item_height: f32     = 40.0;
    let list_padding_top: f32    = 124.0;
    let list_padding_bottom: f32 = 30.0;
    let list_available_h = menu_h - list_padding_top - list_padding_bottom;
    let visible_count = ((list_available_h / item_height).floor() as usize).max(1);
    let start_idx = if filtered.len() > visible_count {
        current_selected
            .saturating_sub(visible_count / 2)
            .min(filtered.len().saturating_sub(visible_count))
    } else {
        0
    };
    let list_start_y = menu_y + list_padding_top;
 
    // Скроллбар
    if filtered.len() > visible_count {
        let scroll_ratio  = start_idx as f32 / (filtered.len() - visible_count) as f32;
        let scrollbar_h   = ((visible_count as f32 / filtered.len() as f32)
            * list_available_h).max(20.0);
        let scrollbar_y   = list_start_y + scroll_ratio * (list_available_h - scrollbar_h);
        draw_rectangle(gc.sx(menu_x + menu_w - 15.), gc.sy(list_start_y),
            gc.s(10.), gc.sy(list_available_h), Color::new(0.2, 0.2, 0.2, 0.5));
        draw_rectangle(gc.sx(menu_x + menu_w - 13.), gc.sy(scrollbar_y),
            gc.s(6.), gc.sy(scrollbar_h), Color::new(0.5, 0.5, 0.5, 0.8));
    }
 
    for (list_i, &(orig_idx, _)) in filtered.iter().enumerate()
        .skip(start_idx).take(visible_count)
    {
        let visual_i = list_i - start_idx;
        let y        = list_start_y + visual_i as f32 * item_height + item_height / 2.;
        let is_selected = list_i == current_selected;
        let song = &songs[orig_idx];
 
        // Зона кнопки удаления
        let del_btn_w = 30.0_f32;
        let del_btn_h = item_height - 0.0;
        let del_btn_x = menu_x + menu_w - del_btn_w - 12.0;
        let del_btn_y = y - item_height / 2.0;
 
        // Кликабельная зона строки (без кнопки удаления)
        let song_rect_x = menu_x + 20.;
        let song_rect_y = y - item_height / 2.;
        let song_rect_w = del_btn_x - song_rect_x - 4.0;
        let song_rect_h = item_height;
 
        let is_hover_row = mx_log >= song_rect_x && mx_log <= song_rect_x + song_rect_w
            && my_log >= song_rect_y && my_log <= song_rect_y + song_rect_h;
        let is_hover_any = mx_log >= song_rect_x && mx_log <= del_btn_x + del_btn_w
            && my_log >= song_rect_y && my_log <= song_rect_y + song_rect_h;
        let is_hover_del = is_hover_any
            && mx_log >= del_btn_x && mx_log <= del_btn_x + del_btn_w;
 
        // Клик по строке — запустить
        if lmb && is_hover_row {
            return (list_i, orig_idx, true, false, false, None);
        }
        // Клик по кнопке удаления
        if lmb && is_hover_del {
            return (list_i, orig_idx, false, false, false, Some(orig_idx));
        }
 
        let _is_hover = is_hover_row || is_hover_del;
 
        let bg_color = if is_selected {
            crate::theme::adaptive_row_selected(theme.is_day)
        } else if is_hover_row {
            crate::theme::adaptive_row_hover(theme.is_day)
        } else {
            Color::new(0.0, 0.0, 0.0, 0.0)
        };
        let text_color = if theme.is_day {
            if is_selected      { Color::new(0.06, 0.10, 0.25, 1.0) }
            else if is_hover_row { Color::new(0.08, 0.14, 0.30, 0.95) }
            else                { Color::new(0.20, 0.26, 0.40, 0.90) }
        } else {
            if is_selected      { WHITE }
            else if is_hover_row { Color::new(0.9, 0.95, 0.9, 1.0) }
            else                { Color::new(0.7, 0.7, 0.7, 1.0) }
        };
 
        draw_rectangle(gc.sx(song_rect_x), gc.sy(song_rect_y),
            gc.sx(song_rect_w), gc.sy(song_rect_h), bg_color);
        if is_hover_row && !is_selected {
            draw_rectangle_lines(gc.sx(song_rect_x), gc.sy(song_rect_y),
                gc.sx(song_rect_w), gc.sy(song_rect_h),
                gc.s(1.0), Color::new(0.3, 0.7, 0.4, 0.5));
        }
 
        // Кнопка удаления ✕
        if is_hover_any || is_selected {
            let del_bg = if is_hover_del {
                Color::new(0.85, 0.15, 0.15, 0.85)
            } else {
                Color::new(0.45, 0.12, 0.12, 0.50)
            };
            draw_rectangle(gc.sx(del_btn_x), gc.sy(del_btn_y),
                gc.sx(del_btn_w), gc.sy(del_btn_h), del_bg);
            if is_hover_del {
                draw_glow_rect_lines(gc, del_btn_x, del_btn_y, del_btn_w, del_btn_h,
                    Color::new(1.0, 0.3, 0.3, 0.9));
            } else {
                draw_rectangle_lines(gc.sx(del_btn_x), gc.sy(del_btn_y),
                    gc.sx(del_btn_w), gc.sy(del_btn_h),
                    gc.s(1.0), Color::new(0.7, 0.2, 0.2, 0.6));
            }
            let del_color = if is_hover_del { WHITE } else { Color::new(0.9, 0.5, 0.5, 0.9) };
            
            let x_fs = gc.s(16.0) as u16;
            let xw = measure_text("X", Some(font), x_fs, 1.0).width / gc.scale_x;
            draw_text_ex("X",
                gc.sx(del_btn_x + (del_btn_w - xw) / 2.0),
                 gc.sy(del_btn_y + del_btn_h * 0.68),
                 TextParams { font_size: x_fs, font: Some(font), color: del_color, ..Default::default() });
        }
        // Текст названия песни с подсветкой поискового запроса
        let text_x   = menu_x + 40.;
        let avail_w  = (song_rect_x + song_rect_w - text_x - 4.0).max(30.0);
 
        if !search.text.is_empty() {
            let name_lower  = song.name.to_lowercase();
            let query_lower = search.text.to_lowercase();
            if let Some(byte_pos) = name_lower.find(&query_lower) {
                let chars: Vec<char> = song.name.chars().collect();
                let char_pos = song.name[..byte_pos].chars().count();
                let char_len = search.text.chars().count();
                let safe_len = char_pos + char_len;
                if safe_len <= chars.len() {
                    let before:  String = chars[..char_pos].iter().collect();
                    let matched: String = chars[char_pos..safe_len].iter().collect();
                    let after:   String = chars[safe_len..].iter().collect();
 
                    let before_px  = truncate_str_px(&before,  avail_w, font, 18, gc.s(1.0));
                    let before_w   = measure_text(&before_px, Some(font), gc.s(18.) as u16, 1.0).width
                        / gc.scale_x;
                    let remain_w   = avail_w - before_w;
                    let matched_px = truncate_str_px(&matched, remain_w, font, 18, gc.s(1.0));
                    let matched_w  = measure_text(&matched_px, Some(font), gc.s(18.) as u16, 1.0).width
                        / gc.scale_x;
                    let after_px   = truncate_str_px(&after, remain_w - matched_w, font, 18, gc.s(1.0));
 
                    let mut cx = text_x;
                    draw_text_custom(gc, &before_px,  cx, y + 5., 18, text_color, font);
                    cx += before_w;
                    draw_text_custom(gc, &matched_px, cx, y + 5., 18, YELLOW, font);
                    cx += matched_w;
                    draw_text_custom(gc, &after_px,   cx, y + 5., 18, text_color, font);
                } else {
                    let dn = truncate_str_px(&song.name, avail_w, font, 18, gc.s(1.0));
                    draw_text_custom(gc, &dn, text_x, y + 5., 18, text_color, font);
                }
            } else {
                let dn = truncate_str_px(&song.name, avail_w, font, 18, gc.s(1.0));
                draw_text_custom(gc, &dn, text_x, y + 5., 18, text_color, font);
            }
        } else {
            let dn = truncate_str_px(&song.name, avail_w, font, 18, gc.s(1.0));
            draw_text_custom(gc, &dn, text_x, y + 5., 18, text_color, font);
        }
 
        if is_selected {
            draw_glow_text(gc, ">", menu_x + 10., y + 5., 18, YELLOW, font);
        }
    }
 
    // Счётчик
    if filtered.len() > visible_count {
        draw_text_custom(gc,
            &format!("{}/{}", start_idx + 1, filtered.len().min(start_idx + visible_count)),
            menu_x + 20., menu_y + menu_h - 14., 13, GRAY, font);
    }
 
    if filtered.is_empty() {
        return (0, 0, false, false, false, None);
    }
 
    let actual_selected = filtered.get(current_selected)
        .map(|(idx, _)| *idx).unwrap_or(0);
 
    (current_selected, actual_selected, false, false, false, None)
}
// ─────────────────────────────────────────────────────────────────────────────
// МЕНЮ ВЫБОРА МИКРОФОНА
// ─────────────────────────────────────────────────────────────────────────────
pub fn draw_mic_selection_menu(
    gc: &GraphicsContext, mc: &mut MicController,
    si: usize, f: &Font, _ru: bool, locale: &Localization, theme: &ThemeState,
) -> (usize, bool) {
    let is_day = theme.is_day;
    let t = locale;
    draw_rectangle(gc.sx(0.), gc.sy(0.), gc.sx(gc.base_w), gc.sy(gc.base_h),
        if is_day { Color::new(0.10, 0.13, 0.22, 0.55) } else { Color::new(0.0, 0.0, 0.0, 0.8) });
    let menu_w = 500.; let menu_h = 300.;
    let menu_x = (gc.base_w - menu_w) / 2.0; let menu_y = (gc.base_h - menu_h) / 2.0;
    let panel_bg = if is_day { Color::new(0.92, 0.94, 0.97, 0.97) } else { Color::new(0.1, 0.1, 0.15, 0.95) };
    draw_rectangle(gc.sx(menu_x), gc.sy(menu_y), gc.sx(menu_w), gc.sy(menu_h), panel_bg);
    draw_glow_rect_lines(gc, menu_x, menu_y, menu_w, menu_h, Color::new(0.3, 0.5, 0.8, 1.0));
    let title_col = if is_day { Color::new(0.08, 0.10, 0.22, 1.0) } else { WHITE };
    let tw = measure_text(&t.select_mic_title, Some(f), gc.s(24.) as u16, 1.0).width / gc.scale_x;
    draw_glow_text(gc, &t.select_mic_title, menu_x + (menu_w - tw) / 2.0, menu_y + 42., 24, title_col, f);
    let hint_col = if is_day { Color::new(0.30, 0.34, 0.46, 1.0) } else { GRAY };
    let hw = measure_text(&t.select_mic_hint, Some(f), gc.s(14.) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, &t.select_mic_hint, menu_x + (menu_w - hw) / 2.0, menu_y + 68., 14, hint_col, f);
    mc.refresh_devices();
    let devs = &mc.available_devices;
    if devs.is_empty() { draw_text_custom(gc, &t.no_devices, menu_x + 140., menu_y + 120., 16, RED, f); return (si, false); }
    let mut current_si = si;
    let (_, wheel_y) = mouse_wheel();
    if wheel_y != 0.0 {
        if wheel_y > 0.0 && current_si > 0 { current_si = current_si.saturating_sub(1); }
        else if wheel_y < 0.0 && current_si < devs.len().saturating_sub(1) { current_si += 1; }
    }
    let ih = 30.; let ls = menu_y + 110.;
    let vc = ((menu_h - 140.) / ih).floor() as usize;
    let si_start = if current_si >= vc { current_si.saturating_sub(vc) + 1 } else { 0 };
    let (mx_log, my_log) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);
    let mut confirmed = false;
    for (i, dev) in devs.iter().enumerate() {
        if i < si_start || i >= si_start + vc { continue; }
        let nm = dev.name().unwrap_or_else(|_| "Unknown".into());
        let y = ls + (i - si_start) as f32 * ih;
        let is = i == current_si;
        let ir_x = menu_x + 10.; let ir_y = y - ih / 2.; let ir_w = menu_w - 20.; let ir_h = ih;
        let hov = mx_log >= ir_x && mx_log <= ir_x + ir_w && my_log >= ir_y && my_log <= ir_y + ir_h;
        // Клик сразу выбирает И подтверждает — Enter больше не обязателен
        if lmb && hov { current_si = i; confirmed = true; }
        let row_bg = if is { Color::new(0.2, 0.4, 0.8, 0.5) }
            else if hov { Color::new(0.2, 0.4, 0.8, 0.18) }
            else { Color::new(0.0, 0.0, 0.0, 0.0) };
        draw_rectangle(gc.sx(ir_x), gc.sy(ir_y), gc.sx(ir_w), gc.sy(ir_h), row_bg);
        let prefix = format!("[{}] ", i);
        let prefix_w = measure_text(&prefix, Some(f), gc.s(16.) as u16, 1.0).width / gc.scale_x;
        let text_x = menu_x + 20.;
        let avail_w = (menu_w - 20.0 - 10.0 - prefix_w).max(20.0);
        let nm_short = truncate_str_px(&nm, avail_w, f, 16, gc.s(1.0));
        let label = format!("{}{}", prefix, nm_short);
        let row_col = if is_day {
            if is { Color::new(0.05, 0.08, 0.20, 1.0) } else { Color::new(0.25, 0.28, 0.38, 1.0) }
        } else {
            if is { WHITE } else { Color::new(0.7, 0.7, 0.7, 1.0) }
        };
        draw_text_custom(gc, &label, text_x, y + 5., 16, row_col, f);
        if is { draw_text_custom(gc, ">", menu_x + 5., y + 5., 16, YELLOW, f); }
    }
    (current_si, confirmed)
}
// ─────────────────────────────────────────────────────────────────────────────
// МЕНЮ ВЫБОРА АУДИОВЫХОДА
// ─────────────────────────────────────────────────────────────────────────────
pub fn draw_output_selection_menu(
    gc: &GraphicsContext, output: &OutputController,
    si: usize, f: &Font, ru: bool, locale: &Localization, theme: &ThemeState,
) -> (usize, bool) {
    let devs = output.list_devices();
    draw_device_list_menu(gc, &devs, si, f, ru, locale, theme)
}

pub fn draw_device_list_menu(
    gc: &GraphicsContext, devs: &[String],
    si: usize, f: &Font, _ru: bool, locale: &Localization, theme: &ThemeState,
) -> (usize, bool) {
    let is_day = theme.is_day;
    let t = locale;
    draw_rectangle(gc.sx(0.), gc.sy(0.), gc.sx(gc.base_w), gc.sy(gc.base_h),
        if is_day { Color::new(0.10, 0.13, 0.22, 0.55) } else { Color::new(0.0, 0.0, 0.0, 0.8) });
    let menu_w = 500.; let menu_h = 300.;
    let menu_x = (gc.base_w - menu_w) / 2.0; let menu_y = (gc.base_h - menu_h) / 2.0;
    let panel_bg = if is_day { Color::new(0.92, 0.97, 0.94, 0.97) } else { Color::new(0.1, 0.1, 0.15, 0.95) };
    draw_rectangle(gc.sx(menu_x), gc.sy(menu_y), gc.sx(menu_w), gc.sy(menu_h), panel_bg);
    draw_glow_rect_lines(gc, menu_x, menu_y, menu_w, menu_h, Color::new(0.3, 0.8, 0.5, 1.0));
    let title_col = if is_day { Color::new(0.06, 0.20, 0.12, 1.0) } else { WHITE };
    let tw = measure_text(&t.output_select_title, Some(f), gc.s(24.) as u16, 1.0).width / gc.scale_x;
    draw_glow_text(gc, &t.output_select_title, menu_x + (menu_w - tw) / 2.0, menu_y + 42., 24, title_col, f);
    let hint_col = if is_day { Color::new(0.25, 0.36, 0.30, 1.0) } else { GRAY };
    let hw = measure_text(&t.output_select_hint, Some(f), gc.s(14.) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, &t.output_select_hint, menu_x + (menu_w - hw) / 2.0, menu_y + 68., 14, hint_col, f);
    if devs.is_empty() {
        draw_text_custom(gc, &t.no_output_devices, menu_x + 120., menu_y + 120., 16, RED, f);
        return (si, false);
    }
    let mut current_si = si;
    let (_, wheel_y) = mouse_wheel();
    if wheel_y != 0.0 {
        if wheel_y > 0.0 && current_si > 0 { current_si = current_si.saturating_sub(1); }
        else if wheel_y < 0.0 && current_si < devs.len().saturating_sub(1) { current_si += 1; }
    }
    let ih = 30.; let ls = menu_y + 110.;
    let vc = ((menu_h - 140.) / ih).floor() as usize;
    let si_start = if current_si >= vc { current_si.saturating_sub(vc) + 1 } else { 0 };
    let (mx_log, my_log) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);
    let mut confirmed = false;
    for (i, nm) in devs.iter().enumerate() {
        if i < si_start || i >= si_start + vc { continue; }
        let y = ls + (i - si_start) as f32 * ih;
        let is = i == current_si;
        let ir_x = menu_x + 10.; let ir_y = y - ih / 2.; let ir_w = menu_w - 20.; let ir_h = ih;
        let hov = mx_log >= ir_x && mx_log <= ir_x + ir_w && my_log >= ir_y && my_log <= ir_y + ir_h;
        if lmb && hov { current_si = i; confirmed = true; }
        let row_bg = if is { Color::new(0.2, 0.6, 0.4, 0.5) }
            else if hov { Color::new(0.2, 0.6, 0.4, 0.18) }
            else { Color::new(0.0, 0.0, 0.0, 0.0) };
        draw_rectangle(gc.sx(ir_x), gc.sy(ir_y), gc.sx(ir_w), gc.sy(ir_h), row_bg);
        let prefix = format!("[{}] ", i);
        let prefix_w = measure_text(&prefix, Some(f), gc.s(16.) as u16, 1.0).width / gc.scale_x;
        let text_x = menu_x + 20.;
        let avail_w = (menu_w - 20.0 - 10.0 - prefix_w).max(20.0);
        let nm_short = truncate_str_px(nm, avail_w, f, 16, gc.s(1.0));
        let label = format!("{}{}", prefix, nm_short);
        let row_col = if is_day {
            if is { Color::new(0.04, 0.16, 0.08, 1.0) } else { Color::new(0.25, 0.32, 0.28, 1.0) }
        } else {
            if is { WHITE } else { Color::new(0.7, 0.7, 0.7, 1.0) }
        };
        draw_text_custom(gc, &label, text_x, y + 5., 16, row_col, f);
        if is { draw_text_custom(gc, ">", menu_x + 5., y + 5., 16, YELLOW, f); }
    }
    (current_si, confirmed)
}

// ─────────────────────────────────────────────────────────────────────────────
// МЕНЮ ВЫБОРА ИНСТРУМЕНТА
// ─────────────────────────────────────────────────────────────────────────────
pub fn draw_instrument_selection_menu(
    gc: &GraphicsContext, instruments: &[String],
    selected_index: usize, font: &Font,
    locale: &Localization, _lang_ru: bool,
) -> (usize, usize, bool) {
    let t = locale;
    let menu_w = 600.; let menu_h = 400.;
    let menu_x = (gc.base_w - menu_w) / 2.0; let menu_y = (gc.base_h - menu_h) / 2.0;
    draw_rectangle(gc.sx(menu_x), gc.sy(menu_y), gc.sx(menu_w), gc.sy(menu_h), Color::new(0.1, 0.1, 0.15, 0.95));
    draw_glow_rect_lines(gc, menu_x, menu_y, menu_w, menu_h, Color::new(0.3, 0.8, 0.5, 1.0));
    let tw = measure_text(&t.select_instr_title, Some(font), gc.s(28.) as u16, 1.0).width / gc.scale_x;
    draw_glow_text(gc, &t.select_instr_title, menu_x + (menu_w - tw) / 2.0, menu_y + 38., 28, WHITE, font);
    let hw = measure_text(&t.select_instr_hint, Some(font), gc.s(14.) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, &t.select_instr_hint, menu_x + (menu_w - hw) / 2.0, menu_y + 64., 14, GRAY, font);
    if instruments.is_empty() { draw_text_custom(gc, "No instruments found", menu_x + 180., menu_y + 150., 20, RED, font); return (0, 0, false); }
    let mut current_selected = selected_index;
    let (_, wheel_y) = mouse_wheel();
    if wheel_y != 0.0 {
        if wheel_y > 0.0 && current_selected > 0 { current_selected = current_selected.saturating_sub(1); }
        else if wheel_y < 0.0 && current_selected < instruments.len().saturating_sub(1) { current_selected += 1; }
    }
    let item_height: f32 = 40.; let lpt = 100.; let lpb = 30.;
    let lah = menu_h - lpt - lpb;
    let vc = ((lah / item_height).floor() as usize).max(1);
    let start_idx = if instruments.len() > vc { current_selected.saturating_sub(vc / 2).min(instruments.len().saturating_sub(vc)) } else { 0 };
    let lsy = menu_y + lpt;
    if instruments.len() > vc {
        let sr = start_idx as f32 / (instruments.len() - vc) as f32;
        let sh = ((vc as f32 / instruments.len() as f32) * lah).max(20.);
        let sy = lsy + sr * (lah - sh);
        draw_rectangle(gc.sx(menu_x + menu_w - 15.), gc.sy(lsy), gc.s(10.), gc.sy(lah), Color::new(0.2, 0.2, 0.2, 0.5));
        draw_rectangle(gc.sx(menu_x + menu_w - 13.), gc.sy(sy), gc.s(6.), gc.sy(sh), Color::new(0.5, 0.5, 0.5, 0.8));
    }
    let (mx_log, my_log) = mouse_position_logical(gc);
    for (list_i, instr_name) in instruments.iter().enumerate().skip(start_idx).take(vc) {
        let vi = list_i - start_idx;
        let y = lsy + vi as f32 * item_height + item_height / 2.;
        let is_selected = list_i == current_selected;
        let ir_x = menu_x + 20.; let ir_y = y - item_height / 2.; let ir_w = menu_w - 40.; let ir_h = item_height;
        if is_mouse_button_pressed(MouseButton::Left) && mx_log >= ir_x && mx_log <= ir_x + ir_w && my_log >= ir_y && my_log <= ir_y + ir_h { return (list_i, list_i, true); }
        draw_rectangle(gc.sx(ir_x), gc.sy(ir_y), gc.sx(ir_w), gc.sy(ir_h), if is_selected { Color::new(0.2, 0.6, 0.3, 0.5) } else { Color::new(0.0, 0.0, 0.0, 0.0) });
        // ir_x=menu_x+20, ir_w=menu_w-40; текст с menu_x+40 до menu_x+menu_w-20
        let avail_instr = (menu_w - 40.0 - 20.0).max(20.0);
        let instr_short = truncate_str_px(instr_name, avail_instr, font, 18, gc.s(1.0));
        draw_text_custom(gc, &instr_short, menu_x + 40., y + 5., 18, if is_selected { WHITE } else { Color::new(0.7, 0.7, 0.7, 1.0) }, font);
        if is_selected { draw_glow_text(gc, ">", menu_x + 10., y + 5., 20, YELLOW, font); }
    }
    (current_selected, current_selected, false)
}

// ─────────────────────────────────────────────────────────────────────────────
// МЕНЮ ИНВЕРСИИ СТРУН
// После выбора инструмента — пользователь выбирает ориентацию струн.
// Возвращает (confirmed, invert_strings)
// ─────────────────────────────────────────────────────────────────────────────
pub fn draw_string_inversion_menu(
    gc: &GraphicsContext,
    font: &Font,
    locale: &Localization,
) -> (bool, bool) {
    let _ = locale; // можно добавить локализацию позже
    let dlg_w = 480.0_f32;
    let dlg_h = 260.0_f32;
    let dlg_x = (gc.base_w - dlg_w) / 2.0;
    let dlg_y = (gc.base_h - dlg_h) / 2.0;

    draw_rectangle(gc.sx(0.0), gc.sy(0.0), gc.sx(gc.base_w), gc.sy(gc.base_h),
        Color::new(0.0, 0.0, 0.0, 0.7));
    draw_rectangle(gc.sx(dlg_x), gc.sy(dlg_y), gc.sx(dlg_w), gc.sy(dlg_h),
        Color::new(0.10, 0.10, 0.16, 0.98));
    draw_glow_rect_lines(gc, dlg_x, dlg_y, dlg_w, dlg_h, Color::new(0.6, 0.6, 1.0, 0.9));

    // Заголовок
    let title = "Ориентация струн";
    let tw = measure_text(title, Some(font), gc.s(22.0) as u16, 1.0).width / gc.scale_x;
    draw_glow_text(gc, title, dlg_x + (dlg_w - tw) / 2.0, dlg_y + 38.0, 22,
        Color::new(0.9, 0.9, 1.0, 1.0), font);

    // Описание
    let desc = "Как пронумерованы струны в вашем MusicXML файле?";
    let dw = measure_text(desc, Some(font), gc.s(13.0) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, desc, dlg_x + (dlg_w - dw) / 2.0, dlg_y + 62.0, 13,
        Color::new(0.7, 0.7, 0.8, 1.0), font);

    let (mx, my) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);

    // Кнопка: Стандарт (1=E4 тонкая, 6=E2 толстая) — MusicXML default
    let btn_w = 200.0_f32;
    let btn_h = 60.0_f32;
    let gap = 20.0_f32;
    let total_w = btn_w * 2.0 + gap;
    let btn_y = dlg_y + 88.0;
    let btn1_x = dlg_x + (dlg_w - total_w) / 2.0;
    let btn2_x = btn1_x + btn_w + gap;

    let hover1 = mx >= btn1_x && mx <= btn1_x + btn_w && my >= btn_y && my <= btn_y + btn_h;
    let hover2 = mx >= btn2_x && mx <= btn2_x + btn_w && my >= btn_y && my <= btn_y + btn_h;

    // Кнопка 1: Стандарт MusicXML
    let bg1 = if hover1 { Color::new(0.2, 0.4, 0.8, 0.6) } else { Color::new(0.12, 0.12, 0.20, 0.8) };
    draw_rectangle(gc.sx(btn1_x), gc.sy(btn_y), gc.sx(btn_w), gc.sy(btn_h), bg1);
    if hover1 { draw_glow_rect_lines(gc, btn1_x, btn_y, btn_w, btn_h, Color::new(0.4, 0.6, 1.0, 1.0)); }
    else { draw_rectangle_lines(gc.sx(btn1_x), gc.sy(btn_y), gc.sx(btn_w), gc.sy(btn_h), gc.s(1.0), Color::new(0.4, 0.4, 0.6, 0.6)); }

    let l1a = "Стандарт (MusicXML)";
    let l1b = "1=E4(тонкая) 6=E2(бас)";
    let l1aw = measure_text(l1a, Some(font), gc.s(14.0) as u16, 1.0).width / gc.scale_x;
    let l1bw = measure_text(l1b, Some(font), gc.s(12.0) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, l1a, btn1_x + (btn_w - l1aw) / 2.0, btn_y + 24.0, 14, WHITE, font);
    draw_text_custom(gc, l1b, btn1_x + (btn_w - l1bw) / 2.0, btn_y + 44.0, 12,
        Color::new(0.7, 0.8, 1.0, 0.9), font);

    // Кнопка 2: Инвертированный
    let bg2 = if hover2 { Color::new(0.4, 0.2, 0.8, 0.6) } else { Color::new(0.12, 0.12, 0.20, 0.8) };
    draw_rectangle(gc.sx(btn2_x), gc.sy(btn_y), gc.sx(btn_w), gc.sy(btn_h), bg2);
    if hover2 { draw_glow_rect_lines(gc, btn2_x, btn_y, btn_w, btn_h, Color::new(0.7, 0.4, 1.0, 1.0)); }
    else { draw_rectangle_lines(gc.sx(btn2_x), gc.sy(btn_y), gc.sx(btn_w), gc.sy(btn_h), gc.s(1.0), Color::new(0.4, 0.4, 0.6, 0.6)); }

    let l2a = "Инверсия";
    let l2b = "1=E2(бас) 6=E4(тонкая)";
    let l2aw = measure_text(l2a, Some(font), gc.s(14.0) as u16, 1.0).width / gc.scale_x;
    let l2bw = measure_text(l2b, Some(font), gc.s(12.0) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, l2a, btn2_x + (btn_w - l2aw) / 2.0, btn_y + 24.0, 14, WHITE, font);
    draw_text_custom(gc, l2b, btn2_x + (btn_w - l2bw) / 2.0, btn_y + 44.0, 12,
        Color::new(0.8, 0.7, 1.0, 0.9), font);

    // Визуальная схема: E2 E A D G B E4
    let strings_y = btn_y + btn_h + 18.0;
    let strings = ["E2", "A", "D", "G", "B", "E4"];
    let colors = [
        Color::new(0.8, 0.2, 0.2, 1.0), Color::new(0.9, 0.5, 0.1, 1.0),
        Color::new(0.9, 0.9, 0.1, 1.0), Color::new(0.2, 0.9, 0.2, 1.0),
        Color::new(0.1, 0.5, 0.9, 1.0), Color::new(0.5, 0.1, 0.9, 1.0),
    ];
    let total_strings_w = 6.0 * 50.0;
    let sx_start = dlg_x + (dlg_w - total_strings_w) / 2.0;
    let hint_label = "Струны гитары: ";
    let hlw = measure_text(hint_label, Some(font), gc.s(12.0) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, hint_label, sx_start - hlw - 4.0, strings_y + 12.0, 12,
        Color::new(0.6, 0.6, 0.7, 1.0), font);
    for (i, (&name, &color)) in strings.iter().zip(colors.iter()).enumerate() {
        let sx = sx_start + i as f32 * 50.0;
        draw_circle(gc.sx(sx + 15.0), gc.sy(strings_y + 10.0), gc.s(8.0), color);
        let nw = measure_text(name, Some(font), gc.s(12.0) as u16, 1.0).width / gc.scale_x;
        draw_text_custom(gc, name, sx + 15.0 - nw / 2.0, strings_y + 25.0, 12, color, font);
    }

    // Подсказка ESC
    let esc_hint = "Esc - отмена";
    let ew = measure_text(esc_hint, Some(font), gc.s(11.0) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, esc_hint, dlg_x + (dlg_w - ew) / 2.0, dlg_y + dlg_h - 14.0, 11,
        DARKGRAY, font);

    if is_key_pressed(KeyCode::Escape) { return (false, false); }

    if lmb {
        if hover1 { return (true, false); } // стандарт
        if hover2 { return (true, true);  } // инверсия
    }

    (false, false) // ожидаем выбора
}

// ─────────────────────────────────────────────────────────────────────────────
// TUNER — основной экран (полноценный, с масштабированием)
// ─────────────────────────────────────────────────────────────────────────────
pub fn draw_tuner_screen(
    gc: &GraphicsContext,
    f: &Font,
    display_note: &str,
    note_color: Color,
    last_freq: Option<f32>,
    is_frozen: bool,
    is_recording: bool,
    wave_data: &[f32],
    mic_names: &[String],
    current_mic_idx: usize,
    message: Option<&str>,
    _locale: &Localization,
    theme: &ThemeState,
    // ─── НОВЫЕ ПАРАМЕТРЫ ──────────────────────────────────────────────
    tuning: &GuitarTuning,
    selected_string: usize,
    tuning_menu_open: bool,
) -> TunerStringClicks {
    let mut clicks = TunerStringClicks::default();
    let is_day = theme.is_day;
    let panel_bg = btn_panel_bg(is_day);
    let panel_border = btn_panel_border(is_day);
    let label_col = if is_day { Color::new(0.10, 0.12, 0.25, 1.0) } else { Color::new(0.5, 0.6, 0.8, 0.8) };
    let hint_col  = if is_day { Color::new(0.15, 0.18, 0.30, 1.0) } else { YELLOW };

    // Заголовок
    let title = "TUNER MODE";
    let tw = measure_text(title, Some(f), gc.s(32.0) as u16, 1.0).width / gc.scale_x;
    draw_glow_text(gc, title, (gc.base_w - tw) / 2.0, 48.0, 32,
        Color::new(0.3, 0.9, 0.6, 1.0), f);

    // Подсказки управления (сдвинуты вправо, чтобы не перекрываться с новыми элементами)
    let hints: &[(&str, Color)] = &[
        ("F - Заморозить ноту | W/S - Микрофон", hint_col),
        ("Up/Down - Выбор записи | Enter - Воспроизвести",
            if is_day { Color::new(0.20, 0.22, 0.35, 1.0) } else { Color::new(0.75, 0.75, 0.88, 1.0) }),
        ("R - Запись вкл/выкл | ESC - Назад",
            if is_day { Color::new(0.55, 0.12, 0.12, 1.0) } else { Color::new(1.0, 0.4, 0.4, 1.0) }),
    ];
    for (i, (text, color)) in hints.iter().enumerate() {
        draw_text_custom(gc, text, 450.0, 86.0 + i as f32 * 24.0, 15, *color, f);
    }

    // ── НОВЫЕ ЭЛЕМЕНТЫ: кнопки струн и выбор строя ────────────────────
    if let Some(si) = draw_tuner_string_buttons(gc, f, tuning, selected_string, last_freq, theme) {
        clicks.string_clicked = Some(si);
    }
    let (toggle_menu, new_tuning_idx) = draw_tuner_tuning_selector(
        gc, f, &get_tunings(), 0, tuning_menu_open, theme);
    clicks.tuning_menu_toggle = toggle_menu;
    clicks.tuning_selected = new_tuning_idx;

    // ── Индикатор целевой ноты ────────────────────────────────────────
    draw_tuner_target_indicator(gc, f, tuning, selected_string, last_freq, theme);

    // ── Заморозка ─────────────────────────────────────────────────────
    let cx = gc.base_w / 2.0;
    let cy = gc.base_h / 2.0;
    if is_frozen {
        let fw = measure_text("FROZEN", Some(f), gc.s(28.0) as u16, 1.0).width / gc.scale_x;
        draw_glow_text(gc, "FROZEN", (gc.base_w - fw) / 2.0, cy - 165.0, 28,
            Color::new(0.4, 0.8, 1.0, 1.0), f);
    }

    // ── Большая нота (детектированная) ────────────────────────────────
    let note_fs: u16 = 140;
    let nw = measure_text(display_note, Some(f), gc.s(note_fs as f32) as u16, 1.0).width / gc.scale_x;
    draw_glow_text(gc, display_note, (gc.base_w - nw) / 2.0, cy + 55.0, note_fs, note_color, f);

    if let Some(freq) = last_freq {
        let freq_str = format!("{:.2} Hz", freq);
        let fw = measure_text(&freq_str, Some(f), gc.s(26.0) as u16, 1.0).width / gc.scale_x;
        let freq_col = if is_day { Color::new(0.15, 0.20, 0.35, 1.0) }
                       else { Color::new(0.75, 0.85, 1.0, 1.0) };
        draw_text_custom(gc, &freq_str, (gc.base_w - fw) / 2.0, cy + 110.0, 26, freq_col, f);
    }

    // ── Waveform ──────────────────────────────────────────────────────
    let wave_w = 580.0_f32;
    let wave_h = 90.0_f32;
    let wave_x = cx - wave_w / 2.0;
    let wave_y = cy + 140.0;
    draw_rectangle(gc.sx(wave_x - 2.0), gc.sy(wave_y - 2.0),
        gc.sx(wave_w + 4.0), gc.sy(wave_h + 4.0), panel_bg);
    draw_glow_rect_lines(gc, wave_x, wave_y, wave_w, wave_h, panel_border);

    let wlabel = "WAVEFORM";
    let wlw = measure_text(wlabel, Some(f), gc.s(13.0) as u16, 1.0).width / gc.scale_x;
    draw_text_custom(gc, wlabel, wave_x + (wave_w - wlw) / 2.0, wave_y - 10.0, 13, label_col, f);
    draw_line(gc.sx(wave_x), gc.sy(wave_y + wave_h / 2.0),
        gc.sx(wave_x + wave_w), gc.sy(wave_y + wave_h / 2.0),
        gc.s(1.0), if is_day { Color::new(0.40, 0.44, 0.55, 0.6) }
                   else { Color::new(0.25, 0.28, 0.38, 0.6) });

    let wave_top = wave_y;
    let wave_bottom = wave_y + wave_h;
    if wave_data.len() > 1 {
        let step = wave_w / (wave_data.len() - 1) as f32;
        for i in 0..wave_data.len() - 1 {
            let x1 = wave_x + i as f32 * step;
            let y1 = (wave_y + wave_h / 2.0 - wave_data[i] * wave_h * 0.46).clamp(wave_top, wave_bottom);
            let x2 = wave_x + (i + 1) as f32 * step;
            let y2 = (wave_y + wave_h / 2.0 - wave_data[i + 1] * wave_h * 0.46).clamp(wave_top, wave_bottom);
            draw_line(gc.sx(x1), gc.sy(y1), gc.sx(x2), gc.sy(y2),
                gc.s(1.8), Color::new(0.15, 0.9, 0.45, 0.85));
        }
    }

    // ── REC индикатор ─────────────────────────────────────────────────
    if is_recording {
        let pulse = ((get_time() as f32 * 3.0).sin() * 0.3 + 0.7).clamp(0.0, 1.0);
        let rec_x = gc.base_w - 120.0;
        let rec_y = 38.0;
        draw_circle(gc.sx(rec_x), gc.sy(rec_y), gc.s(11.0), Color::new(1.0, 0.0, 0.0, pulse));
        draw_circle_lines(gc.sx(rec_x), gc.sy(rec_y), gc.s(11.0), gc.s(2.0), RED);
        draw_glow_text(gc, "REC", rec_x + 18.0, rec_y + 7.0, 20,
            Color::new(1.0, 0.2, 0.2, 1.0), f);
    }

    // ── Список микрофонов ─────────────────────────────────────────────
    let mic_y_base = gc.base_h - 148.0;
    draw_text_custom(gc, "Микрофоны (W/S):", 50.0, mic_y_base, 17, hint_col, f);
    for (i, name) in mic_names.iter().enumerate().take(4) {
        let color = if i == current_mic_idx {
            if is_day { Color::new(0.05, 0.07, 0.16, 1.0) } else { WHITE }
        } else {
            if is_day { Color::new(0.35, 0.38, 0.48, 1.0) } else { Color::new(0.45, 0.47, 0.52, 1.0) }
        };
        let prefix = if i == current_mic_idx { "> " } else { "  " };
        let mic_text_x = 50.0_f32;
        let mic_avail_w = (gc.base_w - 350.0 - mic_text_x - 10.0).max(80.0);
        let prefix_w = measure_text(prefix, Some(f), gc.s(14.) as u16, 1.0).width / gc.scale_x;
        let name_short = truncate_str_px(name, mic_avail_w - prefix_w, f, 14, gc.s(1.0));
        draw_text_custom(gc, &format!("{}{}", prefix, name_short),
            mic_text_x, mic_y_base + 22.0 + i as f32 * 22.0, 14, color, f);
    }

    // ── Сообщение ─────────────────────────────────────────────────────
    if let Some(msg) = message {
        let mw = measure_text(msg, Some(f), gc.s(18.0) as u16, 1.0).width / gc.scale_x;
        let msg_y = gc.base_h - 28.0;
        draw_rectangle(gc.sx(cx - mw / 2.0 - 10.0), gc.sy(msg_y - 22.0),
            gc.sx(mw + 20.0), gc.sy(28.0), panel_bg);
        draw_glow_text(gc, msg, cx - mw / 2.0, msg_y, 18,
            if is_day { Color::new(0.45, 0.30, 0.0, 1.0) }
            else { Color::new(1.0, 0.9, 0.2, 1.0) }, f);
    }

    clicks
}
// ─────────────────────────────────────────────────────────────────────────────
// Подпись текущего выходного устройства в тюнере (нижний левый угол)
// ─────────────────────────────────────────────────────────────────────────────
pub fn draw_tuner_output_label(gc: &GraphicsContext, f: &Font, label: &str, theme: &ThemeState) {
    let is_day = theme.is_day;
    let x = 50.0_f32;
    let y = gc.base_h - 28.0;
    let _bg = if is_day { Color::new(0.68, 0.72, 0.82, 0.85) } else { Color::new(0.08, 0.09, 0.14, 0.75) };
    let text_col = if is_day { Color::new(0.10, 0.14, 0.30, 1.0) } else { Color::new(0.7, 0.85, 1.0, 0.9) };
    //draw_rectangle(gc.sx(x - 4.0), gc.sy(y - 18.0), gc.sx(420.0), gc.sy(24.0), bg);
    draw_text_ex(label, gc.sx(x), gc.sy(y),
        TextParams { font_size: gc.s(14.0) as u16, font: Some(f), color: text_col, ..Default::default() });
}

// ─────────────────────────────────────────────────────────────────────────────
// МЕНЮ СТАТИСТИКИ
// ─────────────────────────────────────────────────────────────────────────────
pub fn draw_stats_menu(
    gc: &GraphicsContext, st: &SongStats, f: &Font,
    _ru: bool, so: usize, logs_size_str: &str,
    show_confirm: bool, from_lesson: bool, locale: &Localization,
    theme: &ThemeState,
) -> (usize, StatsAction) {
    let t = locale;
    let is_day = theme.is_day;
    draw_rectangle(gc.sx(0.), gc.sy(0.), gc.sx(gc.base_w), gc.sy(gc.base_h), theme.panel_overlay());

    let bw = 820_f32; let bh = 640_f32;
    let bx = (gc.base_w - bw) / 2.0; let by = (gc.base_h - bh) / 2.0;

    draw_rectangle(gc.sx(bx + 6.0), gc.sy(by + 10.0), gc.sx(bw), gc.sy(bh), Color::new(0.0, 0.0, 0.0, 0.25));
    draw_rectangle(gc.sx(bx), gc.sy(by), gc.sx(bw), gc.sy(bh), theme.panel_bg());
    draw_glow_rect_lines(gc, bx, by, bw, bh, theme.panel_border());

    let body_col   = theme.panel_text();
    let title_col  = theme.section_title();
    let header_col = if is_day { Color::new(0.25, 0.30, 0.42, 1.0) } else { Color::new(0.75, 0.78, 0.88, 1.0) };
    let dim_col    = if is_day { Color::new(0.40, 0.43, 0.52, 1.0) } else { Color::new(0.62, 0.65, 0.72, 1.0) };
    let empty_col  = if is_day { Color::new(0.60, 0.60, 0.65, 1.0) } else { Color::new(0.42, 0.44, 0.50, 1.0) };
    let line_col   = if is_day { Color::new(0.55, 0.60, 0.70, 0.5) } else { Color::new(0.35, 0.35, 0.45, 0.7) };
    let hint_col   = if is_day { Color::new(0.40, 0.42, 0.48, 0.9) } else { Color::new(0.45, 0.45, 0.50, 0.9) };
    let chip_bg    = if is_day { Color::new(0.0, 0.0, 0.0, 0.04) } else { Color::new(1.0, 1.0, 1.0, 0.04) };

    let title_text = &t.stats_title;
    let title_w = measure_text(title_text, Some(f), gc.s(30.) as u16, 1.0).width / gc.scale_x;
    draw_text_ex(title_text, gc.sx(bx + (bw - title_w) / 2.0), gc.sy(by + 46.0),
        TextParams { font_size: gc.s(30.) as u16, font: Some(f), color: title_col, ..Default::default() });
    draw_line(gc.sx(bx + 30.0), gc.sy(by + 64.0), gc.sx(bx + bw - 30.0), gc.sy(by + 64.0), gc.s(1.0), line_col);

    // ── Карточки метрик ──────────────────────────────────────────────────────
    let chip_y = by + 84.0;
    let chip_h = 78.0_f32;
    let chip_gap = 18.0_f32;
    let chip_w = (bw - 60.0 - chip_gap * 2.0) / 3.0;

    let draw_chip = |idx: f32, accent: Color, value: &str, label: &str| {
        let cx0 = bx + 30.0 + idx * (chip_w + chip_gap);
        draw_rectangle(gc.sx(cx0), gc.sy(chip_y), gc.sx(chip_w), gc.sy(chip_h), chip_bg);
        draw_rectangle(gc.sx(cx0), gc.sy(chip_y), gc.s(4.0), gc.sy(chip_h), accent);
        let vfs = gc.s(28.0) as u16;
        let vw = measure_text(value, Some(f), vfs, 1.0).width / gc.scale_x;
        draw_text_ex(value, gc.sx(cx0 + (chip_w - vw) / 2.0), gc.sy(chip_y + 38.0),
            TextParams { font_size: vfs, font: Some(f), color: accent, ..Default::default() });
        let lfs = gc.s(13.0) as u16;
        let lw = measure_text(label, Some(f), lfs, 1.0).width / gc.scale_x;
        draw_text_ex(label, gc.sx(cx0 + (chip_w - lw) / 2.0), gc.sy(chip_y + 60.0),
            TextParams { font_size: lfs, font: Some(f), color: dim_col, ..Default::default() });
    };

    draw_chip(0.0, GREEN, &st.hits.to_string(), &t.stats_hits);
    draw_chip(1.0, RED,   &st.misses.to_string(), &t.stats_misses);
    let acc_col = if st.accuracy >= 80.0 { GREEN } else if st.accuracy >= 50.0 { YELLOW } else { RED };
    draw_chip(2.0, acc_col, &format!("{:.0}%", st.accuracy), &t.stats_accuracy);

    // ── Таблица ──────────────────────────────────────────────────────────────
    let ty = chip_y + chip_h + 26.0;
    let th = 300_f32;
    let rh = 25_f32;
    let mr = ((th / rh).floor() as usize).saturating_sub(1).max(1);
    let c1 = bx + 30.; let c2 = bx + 190.; let c3 = bx + 360.; let c4 = bx + 520.;

    draw_rectangle(gc.sx(bx + 20.0), gc.sy(ty - 6.0), gc.sx(bw - 40.0), gc.sy(26.0), chip_bg);
    draw_text_custom(gc, &t.stats_col_note,     c1, ty + 13.0, 15, header_col, f);
    draw_text_custom(gc, &t.stats_col_expected, c2, ty + 13.0, 15, header_col, f);
    draw_text_custom(gc, &t.stats_col_detected, c3, ty + 13.0, 15, header_col, f);
    draw_text_custom(gc, &t.stats_col_result,   c4, ty + 13.0, 15, header_col, f);

    let list_top = ty + 26.0;
    let te = st.details.len();
    let mut nso = so;
    let (_, wheel_y) = mouse_wheel();
    if wheel_y != 0.0 {
        if wheel_y > 0.0 && nso > 0 { nso = nso.saturating_sub(1); }
        else if wheel_y < 0.0 && nso < te.saturating_sub(mr) { nso += 1; }
    }
    if te > mr { nso = nso.min(te.saturating_sub(mr)); } else { nso = 0; }

    if te > mr {
        let sr = nso as f32 / (te - mr) as f32;
        let sh = ((mr as f32 / te as f32) * th).max(20.0);
        let sy = list_top + sr * (th - sh);
        draw_rectangle(gc.sx(bx + bw - 18.0), gc.sy(list_top), gc.s(8.0), gc.sy(th), Color::new(0.0, 0.0, 0.0, 0.08));
        draw_rectangle(gc.sx(bx + bw - 17.0), gc.sy(sy), gc.s(6.0), gc.sy(sh), Color::new(0.4, 0.6, 1.0, 0.85));
    }

    for (i, e) in st.details.iter().enumerate().skip(nso).take(mr) {
        let vi = i - nso; let ry = list_top + vi as f32 * rh;
        if ry > list_top + th { break; }
        if vi % 2 == 0 { draw_rectangle(gc.sx(bx + 20.), gc.sy(ry - 3.), gc.sx(bw - 40.), gc.sy(rh), chip_bg); }
        draw_text_custom(gc, &e.note_name, c1, ry + 14.0, 15, body_col, f);
        draw_text_custom(gc, &format!("{:.1} Hz", e.expected_freq), c2, ry + 14.0, 15, dim_col, f);
        if let Some(d) = e.detected_freq { draw_text_custom(gc, &format!("{:.1} Hz", d), c3, ry + 14.0, 15, dim_col, f); }
        else { draw_text_custom(gc, "—", c3, ry + 14.0, 15, empty_col, f); }
        let (rt, rc) = if e.is_hit { ("HIT", GREEN) } else { ("MISS", RED) };
        draw_text_custom(gc, rt, c4, ry + 14.0, 15, rc, f);
    }

    // ── Кнопки ───────────────────────────────────────────────────────────────
    let (mx_log, my_log) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);

    draw_line(gc.sx(bx + 30.0), gc.sy(by + bh - 90.0), gc.sx(bx + bw - 30.0), gc.sy(by + bh - 90.0), gc.s(1.0), line_col);

    let btn_y   = by + bh - 70.;
    let btn_h   = 38.;
    let btn_pad = 18.;

    let restart_label = t.btn_restart.as_str();
    let restart_tw    = measure_text(restart_label, Some(f), gc.s(18.) as u16, 1.0).width / gc.scale_x;
    let btn_restart_x = bx + 30.;
    let btn_restart_w = restart_tw + btn_pad * 2.0;

    let songs_label   = t.btn_songs.as_str();
    let songs_tw      = measure_text(songs_label, Some(f), gc.s(18.) as u16, 1.0).width / gc.scale_x;
    let btn_songs_w   = songs_tw + btn_pad * 2.0;
    let btn_songs_x   = bx + bw / 2.0 - songs_tw / 2.0 - btn_pad;

    let clear_str   = if show_confirm { t.clear_confirm.clone() }
                      else { format!("{} ({})", t.btn_clear_logs, logs_size_str) };
    let clear_label = clear_str.as_str();
    let clear_tw    = measure_text(clear_label, Some(f), gc.s(16.) as u16, 1.0).width / gc.scale_x;
    let btn_clear_x = bx + bw - clear_tw - btn_pad * 2.0 - 30.;
    let btn_clear_w = clear_tw + btn_pad * 2.0;

    let hover_restart = mx_log >= btn_restart_x && mx_log <= btn_restart_x + btn_restart_w
        && my_log >= btn_y && my_log <= btn_y + btn_h;
    let hover_songs   = !from_lesson
        && mx_log >= btn_songs_x && mx_log <= btn_songs_x + btn_songs_w
        && my_log >= btn_y       && my_log <= btn_y + btn_h;
    let hover_clear   = mx_log >= btn_clear_x && mx_log <= btn_clear_x + btn_clear_w
        && my_log >= btn_y && my_log <= btn_y + btn_h;

    let (restart_n, restart_h, restart_g) = if is_day {
        (Color::new(0.55, 0.45, 0.0, 1.0), Color::new(0.05, 0.05, 0.05, 1.0), Color::new(0.78, 0.62, 0.0, 1.0))
    } else {
        (Color::new(0.85, 0.82, 0.20, 1.0), WHITE, Color::new(1.0, 1.0, 0.25, 1.0))
    };
    draw_stat_btn(gc, f, btn_restart_x, btn_y, btn_restart_w, btn_h,
        restart_label, 18, restart_n, restart_h, restart_g, hover_restart);

    if !from_lesson {
        let (songs_n, songs_h, songs_g) = if is_day {
            (Color::new(0.05, 0.25, 0.55, 1.0), Color::new(0.05, 0.05, 0.05, 1.0), Color::new(0.10, 0.45, 0.85, 1.0))
        } else {
            (Color::new(0.25, 0.65, 1.0, 1.0), WHITE, Color::new(0.15, 0.80, 1.0, 1.0))
        };
        draw_stat_btn(gc, f, btn_songs_x, btn_y, btn_songs_w, btn_h,
            songs_label, 18, songs_n, songs_h, songs_g, hover_songs);
    }

    let (clear_n, clear_h, clear_g) = if is_day {
        if show_confirm { (Color::new(0.75, 0.05, 0.05, 1.0), Color::new(0.05, 0.05, 0.05, 1.0), Color::new(0.85, 0.0, 0.0, 1.0)) }
        else            { (Color::new(0.60, 0.12, 0.12, 1.0), Color::new(0.05, 0.05, 0.05, 1.0), Color::new(0.80, 0.15, 0.15, 1.0)) }
    } else {
        if show_confirm { (Color::new(1.0, 0.15, 0.15, 1.0), WHITE, Color::new(1.0, 0.05, 0.05, 1.0)) }
        else            { (Color::new(0.85, 0.20, 0.20, 1.0), WHITE, Color::new(1.0, 0.30, 0.30, 1.0)) }
    };
    draw_stat_btn(gc, f, btn_clear_x, btn_y, btn_clear_w, btn_h,
        clear_label, 16, clear_n, clear_h, clear_g, hover_clear);

    draw_text_custom(gc, "Space", btn_restart_x + btn_restart_w / 2.0 - 18., btn_y + btn_h + 14., 11, hint_col, f);
    if !from_lesson {
        draw_text_custom(gc, "Esc", btn_songs_x + btn_songs_w / 2.0 - 10., btn_y + btn_h + 14., 11, hint_col, f);
    }
    draw_text_custom(gc, "K", btn_clear_x + btn_clear_w / 2.0 - 5., btn_y + btn_h + 14., 11, hint_col, f);

    let action = if lmb {
        if hover_restart    { StatsAction::Restart }
        else if hover_songs { StatsAction::BackToSongs }
        else if hover_clear { StatsAction::ClearLogs }
        else                { StatsAction::None }
    } else { StatsAction::None };
    (nso, action)
}
 
pub fn draw_overlay_buttons_with_locale(
    gc: &GraphicsContext, f: &Font,
    locale: &Localization, theme: &mut ThemeState,
    back_label: Option<&str>,
) -> (bool, bool, bool, bool) {
    let (mx, my) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);
    let ac  = theme.current().accent;
    let base_y = gc.base_h - OVL_MARGIN - OVL_H;
 
    // Позиции справа налево: Тема | Язык | INFO | Назад
    // INFO всегда рядом с кнопкой языка; Назад — крайняя левая (только когда есть).
    let theme_x = gc.base_w - OVL_MARGIN - OVL_W;
    let lang_x  = theme_x - OVL_GAP - OVL_W;
    let info_x  = lang_x  - OVL_GAP - OVL_W;
    let back_x  = info_x  - OVL_GAP - OVL_W;
 
    // ── Назад (крайняя левая, только если задан back_label) ───────────────────
    let back_clicked = if let Some(lbl) = back_label {
        let hb = mx >= back_x && mx <= back_x + OVL_W && my >= base_y && my <= base_y + OVL_H;
        ovl_btn(gc, f, back_x, base_y, lbl, ac, hb, theme.is_day);
        lmb && hb
    } else { false };
 
    // ── INFO (всегда рядом с EN/RU) ───────────────────────────────────────────
    let hi = mx >= info_x && mx <= info_x + OVL_W && my >= base_y && my <= base_y + OVL_H;
    ovl_btn(gc, f, info_x, base_y, "INFO", ac, hi, theme.is_day);
    let info_clicked = lmb && hi;
 
    // ── Язык ──────────────────────────────────────────────────────────────────
    let hl = mx >= lang_x && mx <= lang_x + OVL_W && my >= base_y && my <= base_y + OVL_H;
    ovl_btn(gc, f, lang_x, base_y, &locale.lang_switch_label, ac, hl, theme.is_day);
    let lang_clicked = lmb && hl;
 
    // ── Тема ──────────────────────────────────────────────────────────────────
    let ht = mx >= theme_x && mx <= theme_x + OVL_W && my >= base_y && my <= base_y + OVL_H;
    {
        let bg = if theme.is_day {
            if ht { Color::new(0.75, 0.80, 0.88, 0.92) } else { Color::new(0.88, 0.91, 0.95, 0.88) }
        } else {
            if ht { Color::new(ac.r*0.25, ac.g*0.25, ac.b*0.25, 0.88) } else { Color::new(0.08, 0.09, 0.13, 0.80) }
        };
        draw_rectangle(gc.sx(theme_x), gc.sy(base_y), gc.sx(OVL_W), gc.sy(OVL_H), bg);
        if ht { draw_glow_rect_lines(gc, theme_x, base_y, OVL_W, OVL_H,
                    Color::new(ac.r, ac.g, ac.b, 0.95)); }
        else  { draw_rectangle_lines(gc.sx(theme_x), gc.sy(base_y), gc.sx(OVL_W), gc.sy(OVL_H),
                    gc.s(1.0), Color::new(ac.r, ac.g, ac.b, 0.45)); }
        let fs_px = gc.s(11.0).round() as u16;
        let lw = measure_text(&locale.theme_btn_label, Some(f), fs_px, 1.0).width / gc.scale_x;
        let tc = if theme.is_day { Color::new(0.05, 0.08, 0.15, 1.0) }
                 else if ht { WHITE } else { Color::new(ac.r, ac.g, ac.b, 0.80) };
        draw_text_ex(&locale.theme_btn_label,
            gc.sx(theme_x + (OVL_W - lw) / 2.0), gc.sy(base_y + OVL_H * 0.68),
            TextParams { font_size: fs_px, font: Some(f), color: tc, ..Default::default() });
        draw_rectangle(gc.sx(theme_x + 4.0), gc.sy(base_y + OVL_H - 5.0),
            gc.sx(OVL_W - 8.0), gc.sy(3.0), Color::new(ac.r, ac.g, ac.b, 0.85));
    }
    let theme_clicked = lmb && ht;
    if theme_clicked {
        theme.picker_open = !theme.picker_open;
    }
    if theme.picker_open && is_key_pressed(KeyCode::Escape) {
        theme.picker_open = false;
    }
 
    // ── Попап выбора темы ─────────────────────────────────────────────────────
    if theme.picker_open {
        let popup_w = 150.0_f32;
        let item_h  = 32.0_f32;
        let popup_h = THEMES.len() as f32 * item_h + 10.0 + 36.0; // +36 for day/night toggle
        let popup_x = gc.base_w - OVL_MARGIN - popup_w;
        let popup_y = gc.base_h - OVL_MARGIN - OVL_H - popup_h - 6.0;
 
        let is_day = theme.is_day;
        let popup_bg     = if is_day { Color::new(0.96, 0.97, 0.99, 0.98) } else { Color::new(0.08, 0.09, 0.13, 0.97) };
        let popup_border = if is_day { Color::new(0.45, 0.55, 0.75, 0.85) } else { Color::new(0.35, 0.40, 0.55, 0.85) };
        draw_rectangle(gc.sx(popup_x), gc.sy(popup_y), gc.sx(popup_w), gc.sy(popup_h), popup_bg);
        draw_rectangle_lines(gc.sx(popup_x), gc.sy(popup_y), gc.sx(popup_w), gc.sy(popup_h),
            gc.s(1.5), popup_border);
        // ── Переключатель День / Ночь ─────────────────────────────────────────
        {
            let toggle_h  = 28.0_f32;
            let toggle_y  = popup_y + 5.0;
            let half_w    = (popup_w - 10.0) / 2.0;
            let night_x   = popup_x + 5.0;
            let day_x     = night_x + half_w;

            // «Ночь»
            let h_night = mx >= night_x && mx <= night_x + half_w
                       && my >= toggle_y && my <= toggle_y + toggle_h;
            let bg_night = if is_day {
                if !theme.is_day      { Color::new(0.18, 0.20, 0.32, 0.85) } // выбрано (но мы днём — не должно случиться)
                else if h_night       { Color::new(0.75, 0.78, 0.86, 0.55) }
                else                  { Color::new(0.85, 0.88, 0.94, 0.45) }
            } else {
                if !theme.is_day      { Color::new(0.10, 0.12, 0.28, 0.90) }
                else if h_night       { Color::new(0.12, 0.14, 0.22, 0.70) }
                else                  { Color::new(0.06, 0.07, 0.12, 0.50) }
            };
            draw_rectangle(gc.sx(night_x), gc.sy(toggle_y),
                gc.sx(half_w), gc.sy(toggle_h), bg_night);
            if !theme.is_day {
                draw_rectangle_lines(gc.sx(night_x), gc.sy(toggle_y),
                    gc.sx(half_w), gc.sy(toggle_h),
                    gc.s(1.2), Color::new(0.55, 0.65, 1.0, 0.85));
            }
            let night_lbl = "Ночь";
            let nfs = gc.s(12.0).round() as u16;
            let nlw = measure_text(night_lbl, Some(f), nfs, 1.0).width / gc.scale_x;
            let night_col = if is_day {
                if !theme.is_day { Color::new(0.85, 0.88, 1.0, 1.0) }
                else             { Color::new(0.30, 0.33, 0.42, 0.85) }
            } else {
                if !theme.is_day { Color::new(0.75, 0.85, 1.0, 1.0) }
                else             { Color::new(0.50, 0.55, 0.70, 0.85) }
            };
            draw_text_ex(night_lbl,
                gc.sx(night_x + (half_w - nlw) / 2.0),
                gc.sy(toggle_y + toggle_h * 0.70),
                TextParams { font_size: nfs, font: Some(f), color: night_col, ..Default::default() });
            if lmb && h_night {
                theme.is_day = false; theme.picker_open = false;
                theme.auto_day_night = false;
            }

            // «День»
            //let h_day = mx >= day_x && mx <= day_x + half_w
            //         && my >= toggle_y && my <= toggle_y + toggle_h;
            let h_day = false;
            let bg_day = if is_day {
                if theme.is_day  { Color::new(1.0, 0.90, 0.55, 0.85) }
                else if h_day    { Color::new(0.95, 0.85, 0.55, 0.55) }
                else             { Color::new(0.92, 0.88, 0.75, 0.40) }
            } else {
                if theme.is_day  { Color::new(0.28, 0.20, 0.05, 0.90) }
                else if h_day    { Color::new(0.22, 0.16, 0.04, 0.70) }
                else             { Color::new(0.12, 0.10, 0.04, 0.50) }
            };
            draw_rectangle(gc.sx(day_x), gc.sy(toggle_y),
                gc.sx(half_w), gc.sy(toggle_h), bg_day);
            if theme.is_day {
                draw_rectangle_lines(gc.sx(day_x), gc.sy(toggle_y),
                    gc.sx(half_w), gc.sy(toggle_h),
                    gc.s(1.2), Color::new(1.0, 0.85, 0.30, 0.85));
            }
            let day_lbl = "День";
            let dlw = measure_text(day_lbl, Some(f), nfs, 1.0).width / gc.scale_x;
            let day_col = if is_day {
                if theme.is_day { Color::new(0.35, 0.22, 0.0, 1.0) }
                else            { Color::new(0.45, 0.40, 0.28, 0.85) }
            } else {
                if theme.is_day { Color::new(1.0, 0.92, 0.45, 1.0) }
                else            { Color::new(0.65, 0.58, 0.28, 0.85) }
            };
            draw_text_ex(day_lbl,
                gc.sx(day_x + (half_w - dlw) / 2.0),
                gc.sy(toggle_y + toggle_h * 0.70),
                TextParams { font_size: nfs, font: Some(f), color: day_col, ..Default::default() });
            if lmb && h_day {
                theme.is_day = true; theme.picker_open = false;
                theme.auto_day_night = false;
            }

            // Разделитель
            draw_line(gc.sx(popup_x + 5.0), gc.sy(toggle_y + toggle_h + 4.0),
                gc.sx(popup_x + popup_w - 5.0), gc.sy(toggle_y + toggle_h + 4.0),
                gc.s(1.0), Color::new(0.30, 0.35, 0.50, if is_day { 0.20 } else { 0.40 }));
        }
 
        let mut chosen = None;
        for (i, t) in THEMES.iter().enumerate() {
            let iy  = popup_y + 5.0 + 36.0 + i as f32 * item_h; // +36 = day/night toggle height
            let cur = i == theme.index;
            let hov = mx >= popup_x && mx <= popup_x + popup_w
                   && my >= iy && my <= iy + item_h - 2.0;
            if lmb && hov { chosen = Some(i); }
 
            let bg = if cur {
                Color::new(t.accent.r*0.20, t.accent.g*0.20, t.accent.b*0.20, 0.80)
            } else if hov {
                Color::new(t.accent.r*0.12, t.accent.g*0.12, t.accent.b*0.12, 0.60)
            } else {
                Color::new(0.0, 0.0, 0.0, 0.0)
            };
            draw_rectangle(gc.sx(popup_x), gc.sy(iy),
                gc.sx(popup_w), gc.sy(item_h - 2.0), bg);
            draw_circle(gc.sx(popup_x + 16.0), gc.sy(iy + item_h / 2.0 - 1.0),
                gc.s(7.0), t.accent);
            if cur {
                draw_circle_lines(gc.sx(popup_x + 16.0), gc.sy(iy + item_h / 2.0 - 1.0),
                    gc.s(9.5), gc.s(1.5), Color::new(1.0, 1.0, 1.0, 0.80));
            }
            let name  = locale.theme_name(t.key);
            let fs_px = gc.s(13.0).round() as u16;
            let tc    = if cur || hov { WHITE } else { Color::new(0.65, 0.68, 0.75, 1.0) };
            draw_text_ex(name, gc.sx(popup_x + 30.0), gc.sy(iy + item_h * 0.66),
                TextParams { font_size: fs_px, font: Some(f), color: tc, ..Default::default() });
        }
        if let Some(idx) = chosen { theme.index = idx; theme.picker_open = false; }
        let in_popup = mx >= popup_x && mx <= popup_x + popup_w
                    && my >= popup_y && my <= popup_y + popup_h;
        let in_tbtn  = mx >= theme_x && mx <= theme_x + OVL_W
                    && my >= base_y  && my <= base_y  + OVL_H;
        if lmb && !in_popup && !in_tbtn { theme.picker_open = false; }
    }
 
    (back_clicked, lang_clicked, theme_clicked, info_clicked)
}
 

// draw_overlay_buttons делегирует в with_locale
pub fn draw_overlay_buttons(
    gc: &GraphicsContext, f: &Font,
    locale: &Localization, theme: &mut ThemeState,
    back_label: Option<&str>,
) -> (bool, bool, bool, bool) {
    draw_overlay_buttons_with_locale(gc, f, locale, theme, back_label)
}

// Вспомогательная: одна кнопка оверлея
fn ovl_btn(gc: &GraphicsContext, f: &Font, x: f32, y: f32, label: &str, ac: Color, hov: bool, is_day: bool) {
    let bg = if is_day {
        if hov { Color::new(0.75, 0.80, 0.88, 0.92) }
        else   { Color::new(0.88, 0.91, 0.95, 0.88) }
    } else {
        if hov { Color::new(ac.r*0.25, ac.g*0.25, ac.b*0.25, 0.88) }
        else   { Color::new(0.08, 0.09, 0.13, 0.80) }
    };
    
    draw_rectangle(gc.sx(x), gc.sy(y), gc.sx(OVL_W), gc.sy(OVL_H), bg);
    
    if hov { 
        draw_glow_rect_lines(gc, x, y, OVL_W, OVL_H, Color::new(ac.r, ac.g, ac.b, 0.95)); 
    } else { 
        draw_rectangle_lines(gc.sx(x), gc.sy(y), gc.sx(OVL_W), gc.sy(OVL_H),
            gc.s(1.0), Color::new(ac.r, ac.g, ac.b, 0.45)); 
    }
    
    let fs_px = gc.s(13.0).round() as u16;
    let lw = measure_text(label, Some(f), fs_px, 1.0).width / gc.scale_x;
    
    // Текст: тёмный для дня, светлый для ночи
    let tc = if is_day { 
        Color::new(0.05, 0.08, 0.15, 1.0) 
    } else { 
        Color::new(ac.r, ac.g, ac.b, 0.90) 
    };
    
    draw_text_ex(label, gc.sx(x + (OVL_W - lw) / 2.0), gc.sy(y + OVL_H * 0.72),
        TextParams { font_size: fs_px, font: Some(f), color: tc, ..Default::default() });
}
 
// ─────────────────────────────────────────────────────────────────────────────
// ГЛАВНОЕ МЕНЮ
// ─────────────────────────────────────────────────────────────────────────────
/// Font Awesome codepoints (fa-solid-900.otf)
pub const FA_BOOK:      &str = "\u{F02D}"; // 📖 lessons
pub const FA_GUITAR: &str = "\u{F001}"; // 🎵 music (вместо гитары)
pub const FA_MUSIC:     &str = "\u{F001}"; // 🎵 music
pub const FA_WRENCH:    &str = "\u{F0AD}"; // 🔧 tuner
pub const FA_ARROW_LEFT:&str = "\u{F060}"; // ← back
pub const FA_STAR:      &str = "\u{F005}"; // ★ star
pub const FA_COG:       &str = "\u{F013}"; // ⚙ settings
pub const FA_GLOBE:     &str = "\u{F0AC}"; // 🌐 language
 
pub fn draw_main_menu(
    gc: &GraphicsContext, f: &Font, icons: Option<&Font>,
    locale: &Localization, theme: &ThemeState, t: f64,
) -> MainMenuChoice {
    draw_background(gc, t, theme.is_day);
    let ac     = theme.current().accent;
    let ac_hov = theme.current().accent_hover;
 
    // ── Заголовок ─────────────────────────────────────────────────────────────
    let tfs = gc.s(52.0).round() as u16;
    let tw  = measure_text(&locale.title, Some(f), tfs, 1.0).width / gc.scale_x;
    draw_text_ex(&locale.title,
        gc.sx(gc.base_w / 2.0 - tw / 2.0), gc.sy(gc.base_h / 2.0 - 150.0),
        TextParams { font_size: tfs, font: Some(f),
            color: Color::new(0.9, 0.95, 1.0, 1.0), ..Default::default() });
 
    // ── Автор (между заголовком и субтитром) ──────────────────────────────────
    let afs = gc.s(14.0).round() as u16;
    let aw  = measure_text(&locale.author_label, Some(f), afs, 1.0).width / gc.scale_x;
    draw_text_ex(&locale.author_label,
        gc.sx(gc.base_w / 2.0 - aw / 2.0), gc.sy(gc.base_h / 2.0 - 108.0),
        TextParams { font_size: afs, font: Some(f),
            color: Color::new(ac.r, ac.g, ac.b, 0.55), ..Default::default() });
 
    // ── Субтитр ───────────────────────────────────────────────────────────────
    let sfs = gc.s(18.0).round() as u16;
    let sw  = measure_text(&locale.main_menu_subtitle, Some(f), sfs, 1.0).width / gc.scale_x;
    draw_text_ex(&locale.main_menu_subtitle,
        gc.sx(gc.base_w / 2.0 - sw / 2.0), gc.sy(gc.base_h / 2.0 - 82.0),
        TextParams { font_size: sfs, font: Some(f),
            color: Color::new(0.48, 0.52, 0.62, 1.0), ..Default::default() });
 
    let btn_w = 190.0_f32; let btn_h = 76.0_f32; let gap = 30.0_f32;
    let btn_y = gc.base_h / 2.0 - btn_h / 2.0;

    let total_w = btn_w * 3.0 + gap * 2.0;
    let lessons_x  = gc.base_w / 2.0 - total_w / 2.0;
    let practice_x = lessons_x + btn_w + gap;
    let studio_x   = practice_x + btn_w + gap;

    let (mx, my) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);
    let hl = mx >= lessons_x  && mx <= lessons_x  + btn_w && my >= btn_y && my <= btn_y + btn_h;
    let hp = mx >= practice_x && mx <= practice_x + btn_w && my >= btn_y && my <= btn_y + btn_h;
    let hs = mx >= studio_x   && mx <= studio_x   + btn_w && my >= btn_y && my <= btn_y + btn_h;
    draw_menu_btn_text(gc, f, icons, lessons_x,  btn_y, btn_w, btn_h,
        &locale.main_menu_lessons,  FA_BOOK,   "L", hl, ac, ac_hov, theme.is_day);
    draw_menu_btn_text(gc, f, icons, practice_x, btn_y, btn_w, btn_h,
        &locale.main_menu_practice, FA_GUITAR, "P",  hp,
        Color::new(0.20, 0.78, 0.45, 1.0), Color::new(0.28, 0.95, 0.55, 1.0), theme.is_day);
    draw_menu_btn_text(gc, f, icons, studio_x, btn_y, btn_w, btn_h,
        "Studio", FA_WRENCH, "S", hs,
        Color::new(0.78, 0.20, 0.45, 1.0), Color::new(0.95, 0.28, 0.55, 1.0), theme.is_day);

    if is_key_pressed(KeyCode::L) { return MainMenuChoice::Lessons; }
    if is_key_pressed(KeyCode::P) || is_key_pressed(KeyCode::Enter) { return MainMenuChoice::Practice; }
    if is_key_pressed(KeyCode::S) { return MainMenuChoice::Studio; } // <--- Горячая клавиша S
    if lmb {
        if hl { return MainMenuChoice::Lessons; }
        if hp { return MainMenuChoice::Practice; }
        if hs { return MainMenuChoice::Studio; } // <--- Клик по Studio
    }
    MainMenuChoice::None
}
 
/// Кнопка главного меню с поддержкой Font Awesome иконки
fn draw_menu_btn_text(
    gc: &GraphicsContext, f: &Font, icons: Option<&Font>,
    x: f32, y: f32, w: f32, h: f32,
    label: &str, icon: &str, shortcut: &str,
    hov: bool, ac: Color, ac_hov: Color, is_day: bool,
) {
    let bg = if is_day {
        if hov { Color::new(0.75, 0.80, 0.88, 0.92) } else { Color::new(0.90, 0.93, 0.97, 0.85) }
    } else {
        if hov { Color::new(ac.r*0.18, ac.g*0.18, ac.b*0.18, 0.88) } else { Color::new(0.09, 0.11, 0.16, 0.78) }
    };
    draw_rectangle(gc.sx(x), gc.sy(y), gc.sx(w), gc.sy(h), bg);
    if hov { draw_glow_rect_lines(gc, x, y, w, h, Color::new(ac_hov.r, ac_hov.g, ac_hov.b, 0.92)); }
    else   { draw_rectangle_lines(gc.sx(x), gc.sy(y), gc.sx(w), gc.sy(h),
                gc.s(1.5), Color::new(ac.r, ac.g, ac.b, if is_day { 0.65 } else { 0.55 })); }

    let icon_color = if is_day {
        Color::new(0.08, 0.10, 0.20, if hov { 1.0 } else { 0.75 })
    } else {
        Color::new(ac.r, ac.g, ac.b, if hov { 1.0 } else { 0.65 })
    };

    if let Some(ifont) = icons {
        let ifs = gc.s(26.0).round() as u16;
        let iw  = measure_text(icon, Some(ifont), ifs, 1.0).width / gc.scale_x;
        draw_text_ex(icon, gc.sx(x + w / 2.0 - iw / 2.0), gc.sy(y + 28.0),
            TextParams { font_size: ifs, font: Some(ifont), color: icon_color, ..Default::default() });
    } else {
        let sfs = gc.s(13.0).round() as u16;
        let fb  = format!("[ {} ]", shortcut);
        let sw  = measure_text(&fb, Some(f), sfs, 1.0).width / gc.scale_x;
        draw_text_ex(&fb, gc.sx(x + w / 2.0 - sw / 2.0), gc.sy(y + 22.0),
            TextParams { font_size: sfs, font: Some(f), color: icon_color, ..Default::default() });
    }

    let lfs = gc.s(22.0).round() as u16;
    let lw  = measure_text(label, Some(f), lfs, 1.0).width / gc.scale_x;
    let tc  = if is_day {
        if hov { Color::new(0.05, 0.08, 0.18, 1.0) } else { Color::new(0.10, 0.14, 0.30, 0.92) }
    } else {
        if hov { WHITE } else { Color::new(ac.r*0.85+0.15, ac.g*0.85+0.15, ac.b*0.85+0.15, 1.0) }
    };
    draw_text_ex(label, gc.sx(x + w / 2.0 - lw / 2.0), gc.sy(y + h / 2.0 + 12.0),
        TextParams { font_size: lfs, font: Some(f), color: tc, ..Default::default() });

    let hfs = gc.s(11.0).round() as u16;
    let hw  = measure_text(shortcut, Some(f), hfs, 1.0).width / gc.scale_x;
    let hint_col = if is_day {
        Color::new(0.10, 0.14, 0.30, if hov { 0.75 } else { 0.35 })
    } else {
        Color::new(ac.r, ac.g, ac.b, if hov { 0.75 } else { 0.35 })
    };
    draw_text_ex(shortcut, gc.sx(x + w / 2.0 - hw / 2.0), gc.sy(y + h - 8.0),
        TextParams { font_size: hfs, font: Some(f), color: hint_col, ..Default::default() });
}
 
// ─────────────────────────────────────────────────────────────────────────────
// ПАНЕЛЬ УРОКОВ  →  (go_back_to_mainmenu, go_to_theory)
// ─────────────────────────────────────────────────────────────────────────────
pub fn draw_lessons_panel(
    gc: &GraphicsContext, f: &Font,
    lessons_file: &LessonsFile, state: &mut LessonsState,
    locale: &Localization, texture_cache: &HashMap<String, Texture2D>,
    theme: &ThemeState,
) -> LessonsPanelResult {
    let th = theme.current();
    let (pw, ph, px, py) = panel_rect(gc);
 
    draw_panel_bg(gc, px, py, pw, ph, theme.panel_bg(), theme.panel_border());
 
    let sec = lessons_file.section_title.as_deref().unwrap_or(&locale.main_menu_lessons);
    draw_sharp(gc, f, sec,
        px + (pw - msh(gc, f, sec, 26.0)) / 2.0,
        py + 36.0, 26.0, theme.section_title());
        let ac = theme.current().accent;
    draw_line(
        gc.sx(px + 18.0), gc.sy(py + 50.0),
        gc.sx(px + pw - 18.0), gc.sy(py + 50.0),
        gc.s(1.0), Color::new(ac.r, ac.g, ac.b, if theme.is_day { 0.25 } else { 0.30 }));
 
    let (mx, my) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);
    let hbtn_y  = py + 8.0;
    let hbtn_h  = 26.0_f32;
    let back_w  = 94.0_f32;
    let back_x  = px + 10.0;
    let base_bw = 74.0_f32;
    let base_x  = back_x + back_w + 8.0;
 
    let hb    = mx >= back_x && mx <= back_x + back_w   && my >= hbtn_y && my <= hbtn_y + hbtn_h;
    let hbase = mx >= base_x && mx <= base_x + base_bw  && my >= hbtn_y && my <= hbtn_y + hbtn_h;
 
    small_btn(gc, f, back_x, hbtn_y, back_w, hbtn_h,
        &locale.lessons_back, th.accent, th.accent_hover, hb, theme.is_day);
 
    // «База» — золото, чёрный шрифт
    {
        let gold  = Color::new(0.95, 0.78, 0.18, 1.0);
        let goldh = Color::new(1.00, 0.92, 0.35, 1.0);
        let bg2   = if hbase { Color::new(0.80, 0.65, 0.08, 0.92) }
                    else      { Color::new(0.60, 0.46, 0.03, 0.75) };
        draw_rectangle(gc.sx(base_x), gc.sy(hbtn_y), gc.sx(base_bw), gc.sy(hbtn_h), bg2);
        if hbase { draw_glow_rect_lines(gc, base_x, hbtn_y, base_bw, hbtn_h, goldh); }
        else     { draw_rectangle_lines(gc.sx(base_x), gc.sy(hbtn_y),
                       gc.sx(base_bw), gc.sy(hbtn_h), gc.s(1.5), gold); }
        let fs_px = gc.s(13.0).round() as u16;
        let lw = measure_text(&locale.lessons_theory_btn, Some(f), fs_px, 1.0).width / gc.scale_x;
        draw_text_ex(&locale.lessons_theory_btn,
            gc.sx(base_x + (base_bw - lw) / 2.0), gc.sy(hbtn_y + hbtn_h * 0.72),
            TextParams { font_size: fs_px, font: Some(f),
                color: Color::new(0.06, 0.04, 0.0, 1.0), ..Default::default() });
    }
 
    // Кнопка «Назад» в шапке работает всегда:
    //   • если урок открыт → закрывает урок (возвращает в список)
    //   • если в списке    → выходит из панели (GoBack)
    // Esc в списке — тоже GoBack. Esc внутри урока — только close_lesson.
    let back_btn_clicked = lmb && hb;
    let esc_pressed      = is_key_pressed(KeyCode::Escape);
    let top_go_back      = !state.is_lesson_open() && (esc_pressed || back_btn_clicked);
    let top_go_theory    = !state.is_lesson_open() && (lmb && hbase);
 
    // ── Открытый урок ─────────────────────────────────────────────────────────
    if state.is_lesson_open() {
        if lessons_file.is_empty() { return LessonsPanelResult::None; }
 
        let idx      = state.selected_lesson.min(lessons_file.lessons.len() - 1);
        let lesson   = &lessons_file.lessons[idx];
        let step_idx = state.open_step.unwrap_or(0)
                           .min(lesson.steps.len().saturating_sub(1));
 
        let theory_title_col = if theme.is_day { Color::new(0.45, 0.30, 0.02, 1.0) } else { Color::new(1.0, 0.88, 0.28, 1.0) };
        let link_action = draw_open_step(
            gc, f, lesson, step_idx, state, px, py, pw, ph, theory_title_col, texture_cache, locale, theme);
        // Переход по ссылке внутри шага
        match link_action {
            LessonLinkAction::GoTuner         => return LessonsPanelResult::GoTuner,
            LessonLinkAction::GoPractice      => return LessonsPanelResult::GoPractice,
            LessonLinkAction::PlaySong(path)  => return LessonsPanelResult::PlaySong(path),
            LessonLinkAction::Custom(_)       => {}
            LessonLinkAction::None            => {}
        }
 
        draw_step_nav(gc, f, lesson, state, px, py, pw, ph,
            th.accent, th.accent_hover, locale, mx, my, lmb, theme.is_day);
        draw_sharp(gc, f, sec,
            px + (pw - msh(gc, f, sec, 26.0)) / 2.0,
            py + 36.0, 26.0, theme.section_title());
 
        // Кнопка «Назад» в шапке — закрывает урок (в список), не выходит из панели
        // Esc внутри урока — тоже только закрывает урок
        if back_btn_clicked || esc_pressed { state.close_lesson(); }
        return LessonsPanelResult::None;
    }
 
    // ── Список уроков ─────────────────────────────────────────────────────────
    if lessons_file.is_empty() {
        draw_sharp(gc, f, sec,
            px + (pw - msh(gc, f, sec, 26.0)) / 2.0,
            py + 36.0, 26.0, theme.section_title());
        if top_go_back   { return LessonsPanelResult::GoBack; }
        if top_go_theory { return LessonsPanelResult::GoTheory; }
        return LessonsPanelResult::None;
    }
 
    draw_lesson_list(gc, f, lessons_file, state, texture_cache,
        px, py, pw, ph, lmb, mx, my, theme.current().accent, theme.list_selected_bg(), theme.is_day);
 
    draw_sharp(gc, f, "Enter — открыть  |  Esc — назад",
        px + (pw - msh(gc, f, "Enter — открыть  |  Esc — назад", 11.0)) / 2.0,
        py + ph - 12.0, 11.0, Color::new(0.32, 0.36, 0.46, 0.82));
 
    if top_go_back   { return LessonsPanelResult::GoBack; }
    if top_go_theory { return LessonsPanelResult::GoTheory; }
    LessonsPanelResult::None
}
 
// ─────────────────────────────────────────────────────────────────────────────
// ПАНЕЛЬ ТЕОРИИ
// ─────────────────────────────────────────────────────────────────────────────
pub fn draw_theory_panel(
    gc: &GraphicsContext, f: &Font,
    theory_file: &LessonsFile, state: &mut LessonsState,
    locale: &Localization, texture_cache: &HashMap<String, Texture2D>, theme: &ThemeState,
) -> LessonsPanelResult {
    let gold  = Color::new(0.95, 0.78, 0.18, 1.0);
    let goldh = Color::new(1.00, 0.92, 0.35, 1.0);
    let (pw, ph, px, py) = panel_rect(gc);
 
    let theory_bg = if theme.is_day {
        Color::new(0.95, 0.90, 0.75, 0.92)  // светло-золотой днём
    } else {
        Color::new(0.10, 0.08, 0.02, 0.98)  // тёмный ночью
    };
    let theory_border = Color::new(0.95, 0.78, 0.18, 0.90);
    draw_panel_bg(gc, px, py, pw, ph, theory_bg, theory_border);

 
    let sec = theory_file.section_title.as_deref().unwrap_or(&locale.lessons_theory_btn);
    let theory_title_col = if theme.is_day { Color::new(0.45, 0.30, 0.02, 1.0) }
    else            { Color::new(1.0,  0.88, 0.28, 1.0) };
    draw_sharp(gc, f, sec,
        px + (pw - msh(gc, f, sec, 26.0)) / 2.0,
        py + 36.0, 26.0,theory_title_col);
    draw_line(
        gc.sx(px + 18.0), gc.sy(py + 50.0),
        gc.sx(px + pw - 18.0), gc.sy(py + 50.0),
        gc.s(1.0), Color::new(0.80, 0.65, 0.10, 0.28));
 
    let (mx, my) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);
    let hbtn_y = py + 8.0;
    let hbtn_h = 26.0_f32;
    let back_w = 94.0_f32;
    let back_x = px + 10.0;
    let hb = mx >= back_x && mx <= back_x + back_w
          && my >= hbtn_y  && my <= hbtn_y + hbtn_h;
 
    // Кнопка «Назад» в золотом стиле
    {
        let bg2 = if hb { Color::new(0.70, 0.50, 0.05, 0.88) }
                  else  { Color::new(0.40, 0.30, 0.02, 0.75) };
        draw_rectangle(gc.sx(back_x), gc.sy(hbtn_y), gc.sx(back_w), gc.sy(hbtn_h), bg2);
        if hb { draw_glow_rect_lines(gc, back_x, hbtn_y, back_w, hbtn_h, goldh); }
        else  { draw_rectangle_lines(gc.sx(back_x), gc.sy(hbtn_y),
                    gc.sx(back_w), gc.sy(hbtn_h), gc.s(1.5), gold); }
        let fs_px = gc.s(13.0).round() as u16;
        let lw = measure_text(&locale.lessons_back, Some(f), fs_px, 1.0).width / gc.scale_x;
        draw_text_ex(&locale.lessons_back,
            gc.sx(back_x + (back_w - lw) / 2.0), gc.sy(hbtn_y + hbtn_h * 0.72),
            TextParams { font_size: fs_px, font: Some(f),
                color: Color::new(1.0, 0.92, 0.55, 1.0), ..Default::default() });
    }
 
    let back_btn_clicked2 = lmb && hb;
    let esc_pressed2      = is_key_pressed(KeyCode::Escape);
    let top_go_back       = !state.is_lesson_open() && (esc_pressed2 || back_btn_clicked2);
 
    // ── Открытый урок (теория) ────────────────────────────────────────────────
    if state.is_lesson_open() {
        if theory_file.is_empty() { return LessonsPanelResult::None; }
 
        let idx      = state.selected_lesson.min(theory_file.lessons.len() - 1);
        let lesson   = &theory_file.lessons[idx];
        let step_idx = state.open_step.unwrap_or(0)
                           .min(lesson.steps.len().saturating_sub(1));
 
        let link_action = draw_open_step(
            gc, f, lesson, step_idx, state, px, py, pw, ph,
            theory_title_col, texture_cache, locale, theme);
 
        match link_action {
            LessonLinkAction::GoTuner         => return LessonsPanelResult::GoTuner,
            LessonLinkAction::GoPractice      => return LessonsPanelResult::GoPractice,
            LessonLinkAction::PlaySong(path)  => return LessonsPanelResult::PlaySong(path),
            LessonLinkAction::Custom(_)       => {}
            LessonLinkAction::None            => {}
        }
 
        draw_step_nav(gc, f, lesson, state, px, py, pw, ph,
            gold, goldh, locale, mx, my, lmb, theme.is_day);
 
        draw_sharp(gc, f, &locale.lessons_nav_hint,
            px + (pw - msh(gc, f, &locale.lessons_nav_hint, 11.0)) / 2.0,
            py + ph - 12.0, 11.0, Color::new(0.65, 0.52, 0.10, 0.85));
 
        // Кнопка «Назад» в шапке или Esc — закрыть урок (в список теории)
        if back_btn_clicked2 || esc_pressed2 { state.close_lesson(); }
        return LessonsPanelResult::None;
    }
 
    // ── Список теории ─────────────────────────────────────────────────────────
    if theory_file.is_empty() {
        draw_sharp(gc, f, &locale.theory_empty,
            px + (pw - msh(gc, f, &locale.theory_empty, 17.0)) / 2.0,
            py + ph / 2.0, 17.0, Color::new(0.7, 0.55, 0.1, 1.0));
        if top_go_back { return LessonsPanelResult::GoBack; }
        return LessonsPanelResult::None;
    }
 
    draw_lesson_list(gc, f, theory_file, state, texture_cache,
        px, py, pw, ph, lmb, mx, my, theme.current().accent, theme.list_selected_bg(), theme.is_day);

    draw_sharp(gc, f, "Enter — открыть  |  Esc — назад",
        px + (pw - msh(gc, f, "Enter — открыть  |  Esc — назад", 11.0)) / 2.0,
        py + ph - 12.0, 11.0, Color::new(0.42, 0.36, 0.14, 0.82));
 
    if top_go_back { return LessonsPanelResult::GoBack; }
    LessonsPanelResult::None
}
 
// ─────────────────────────────────────────────────────────────────────────────
// Тело открытого шага — возвращает действие по ссылке (или None)
// ─────────────────────────────────────────────────────────────────────────────
fn draw_open_step(
    gc: &GraphicsContext, f: &Font,
    lesson: &super::lessons::Lesson, step_idx: usize,
    state: &mut super::lessons::LessonsState,  // ← ДОБАВЛЕНО
    px: f32, py: f32, pw: f32, ph: f32,        // ← ph теперь используется
    title_color: Color,
    texture_cache: &HashMap<String, Texture2D>,
    locale: &Localization,
    theme: &ThemeState,
) -> LessonLinkAction {
    // Заголовок урока (фиксированный, не скроллится)
    draw_sharp(gc, f, &lesson.title,
        px + (pw - msh(gc, f, &lesson.title, 20.0)) / 2.0,
        py + 74.0, 20.0, title_color);
    
    let step_text_col = if theme.is_day { Color::new(0.08, 0.12, 0.28, 1.0) }
    else      { Color::new(0.80, 0.85, 0.95, 1.0) };
    
    if lesson.steps.is_empty() {
        draw_wrapped_sharp(gc, f, &lesson.description,
            px + 28.0, py + 108.0, pw - 56.0, 16.0,
            step_text_col);
        return LessonLinkAction::None;
    }
    
    let step = &lesson.steps[step_idx];
    let has_imgs = draw_step_images(gc, step, texture_cache);
    
    // Индикатор шагов
    let step_str = locale.lessons_step_of
        .replace("{cur}",   &(step_idx + 1).to_string())
        .replace("{total}", &lesson.steps.len().to_string());
    draw_sharp(gc, f, &step_str,
        px + (pw - msh(gc, f, &step_str, 13.0)) / 2.0,
        py + 96.0, 13.0, Color::new(0.48, 0.58, 0.78, 0.85));
    
    // Точки прогресса
    let dots_w = lesson.steps.len() as f32 * 18.0;
    let dot_sx  = px + (pw - dots_w) / 2.0;
    for (di, _) in lesson.steps.iter().enumerate() {
        let dx = dot_sx + di as f32 * 18.0 + 7.0;
        let (r, col) = if di == step_idx {
            (5.0_f32, Color::new(0.28, 0.72, 1.0, 1.0))
        } else if di < step_idx {
            (3.0_f32, Color::new(0.20, 0.52, 0.80, 0.72))
        } else {
            (3.0_f32, Color::new(0.22, 0.25, 0.33, 0.72))
        };
        draw_circle(gc.sx(dx), gc.sy(py + 110.0), gc.s(r), col);
    }
    
    // ── Контентная зона со скроллом ────────────────────────────────────────
    let content_x = px + 28.0;
    let content_w = if has_imgs { pw * 0.50 } else { pw - 56.0 };
    let content_top = py + 128.0;
    let content_bottom = py + ph - 60.0; // Место для навигации
    let content_h = content_bottom - content_top;
    
    // Обработка скролла
    let (_, wy) = mouse_wheel();
    let (mx, my) = mouse_position_logical(gc);
    let is_hover = mx >= px && mx <= px + pw && my >= content_top && my <= content_bottom;
    
    if is_hover {
        state.content_scroll -= wy * 24.0;
        state.content_scroll = state.content_scroll.max(0.0);
    }
    
    // Первый проход: вычисляем общую высоту контента
    let mut total_h = 0.0_f32;
    
    // Высота текста
    if !step.text.is_empty() {
        total_h += super::lessons::calculate_text_height(&step.text, content_w, f, gc, 16.0);
        total_h += 12.0; // Отступ после текста
    }
    
    // Высота таблиц
    for _table in &step.tables {
        let table_max_h = (content_h * 0.6).min(400.0);
        total_h += table_max_h + 16.0;
    }
    
    // Высота ссылок
    let link_h = 15.0_f32 * 1.65;
    total_h += step.links.len() as f32 * link_h;
    
    // Ограничиваем скролл
    let max_scroll = (total_h - content_h).max(0.0);
    state.content_scroll = state.content_scroll.min(max_scroll);
    
    // Второй проход: отрисовка с учётом скролла
    let mut cursor_y = content_top - state.content_scroll;
    
    // Текст
    if !step.text.is_empty() {
        let text_h = super::lessons::draw_text_with_scroll(
            gc, f, &step.text,
            content_x, cursor_y,
            content_w, 16.0,
            step_text_col,
            0.0, // Скролл уже применён к cursor_y
            content_top, content_h,
        );
        cursor_y += text_h + 12.0;
    }
    
    // Таблицы
    for table in &step.tables {
        let table_max_h = (content_h * 0.6).min(400.0);
        
        // Проверяем видимость таблицы
        if cursor_y + table_max_h >= content_top && cursor_y <= content_bottom {
            let table_h = super::lessons::draw_step_table(
                gc, f, table,
                content_x, cursor_y,
                content_w, table_max_h,
                &mut state.table_scroll,
                theme,
            );
            cursor_y += table_h + 16.0;
        } else {
            cursor_y += table_max_h + 16.0;
        }
    }
    
    // Ссылки
    let mut link_action = LessonLinkAction::None;
    if !step.links.is_empty() {
        let links_y = cursor_y;
        
        // Проверяем видимость ссылок
        if links_y >= content_top && links_y <= content_bottom {
            link_action = draw_step_links_with_scroll(
                gc, f, &step.links,
                content_x, links_y,
                theme.is_day,
                state.content_scroll,
                content_top, content_h,
            );
        }
    }
    
    // Скроллбар страницы
    if max_scroll > 0.0 {
        let sb_x = px + pw - 12.0;
        let sb_ratio = state.content_scroll / max_scroll;
        let sb_h = ((content_h / (total_h + 1.0)) * content_h).max(24.0);
        let sb_y = content_top + sb_ratio * (content_h - sb_h);
        
        // Фон скроллбара
        draw_rectangle(
            gc.sx(sb_x), gc.sy(content_top),
            gc.s(6.0), gc.sy(content_h),
            Color::new(0.12, 0.13, 0.18, 0.7)
        );
        // Ползунок
        draw_rectangle(
            gc.sx(sb_x + 1.0), gc.sy(sb_y),
            gc.s(4.0), gc.sy(sb_h),
            Color::new(theme.current().accent.r * 0.8, 
                      theme.current().accent.g * 0.8, 
                      theme.current().accent.b * 0.6, 0.85)
        );
    }
    
    link_action
}

/// Рисует ссылки с учётом скролла
fn draw_step_links_with_scroll(
    gc: &GraphicsContext, f: &Font,
    links: &[super::lessons::StepLink],
    x: f32, start_y: f32,
    is_day: bool,
    scroll_offset: f32,
    content_top: f32,
    content_h: f32,
) -> LessonLinkAction {
    let (mx, my)  = mouse_position_logical(gc);
    let lmb       = is_mouse_button_pressed(MouseButton::Left);
    let fs_px     = gc.s(15.0).round() as u16;
    let line_h    = 15.0_f32 * 1.65;
    let (link_col, hover_col) = if is_day {
        (Color::new(0.05, 0.30, 0.65, 1.0), Color::new(0.0, 0.45, 0.85, 1.0))
    } else {
        (Color::new(0.35, 0.75, 1.0, 1.0), Color::new(0.60, 0.95, 1.0, 1.0))
    };
    
    for (i, link) in links.iter().enumerate() {
        let y      = start_y + i as f32 * line_h - scroll_offset;
        let label  = format!("→  {}", link.label);
        let lw     = measure_text(&label, Some(f), fs_px, 1.0).width / gc.scale_x;
        let top_y  = y - line_h * 0.82;
        
        // Проверяем видимость
        if y + line_h < content_top || y > content_top + content_h {
            continue;
        }
        
        let hov    = mx >= x && mx <= x + lw && my >= top_y && my <= y + 4.0;
        let col    = if hov { hover_col } else { link_col };
        
        // Фоновая подсветка при наведении
        if hov {
            draw_rectangle(
                gc.sx(x - 4.0), gc.sy(top_y),
                gc.sx(lw + 8.0), gc.sy(line_h),
                Color::new(0.35, 0.75, 1.0, 0.12));
        }
        
        // Текст ссылки
        draw_text_ex(&label, gc.sx(x), gc.sy(y),
            TextParams { font_size: fs_px, font: Some(f), color: col, ..Default::default() });
        
        // Подчёркивание
        draw_line(
            gc.sx(x),      gc.sy(y + 2.5),
            gc.sx(x + lw), gc.sy(y + 2.5),
            gc.s(if hov { 1.5 } else { 0.9 }),
            col);
        
        if hov && lmb {
            return LessonLinkAction::from_target(&link.target);
        }
    }
    LessonLinkAction::None
}
 
// ─────────────────────────────────────────────────────────────────────────────
// Ссылки шага — рисует кликабельный список под текстом
// ─────────────────────────────────────────────────────────────────────────────
fn draw_step_links(
    gc: &GraphicsContext, f: &Font,
    links: &[super::lessons::StepLink],
    x: f32, start_y: f32,
    is_day: bool
) -> LessonLinkAction {
    let (mx, my)  = mouse_position_logical(gc);
    let lmb       = is_mouse_button_pressed(MouseButton::Left);
    let fs_px     = gc.s(15.0).round() as u16;
    let line_h    = 15.0_f32 * 1.65;
    let (link_col, hover_col) = if is_day {
        (Color::new(0.05, 0.30, 0.65, 1.0), Color::new(0.0, 0.45, 0.85, 1.0))
    } else {
        (Color::new(0.35, 0.75, 1.0, 1.0), Color::new(0.60, 0.95, 1.0, 1.0))
    };
 
    for (i, link) in links.iter().enumerate() {
        let y      = start_y + i as f32 * line_h;
        let label  = format!("→  {}", link.label);
        let lw     = measure_text(&label, Some(f), fs_px, 1.0).width / gc.scale_x;
        let top_y  = y - line_h * 0.82;
        let hov    = mx >= x && mx <= x + lw && my >= top_y && my <= y + 4.0;
        let col    = if hov { hover_col } else { link_col };
 
        // Фоновая подсветка при наведении
        if hov {
            draw_rectangle(
                gc.sx(x - 4.0), gc.sy(top_y),
                gc.sx(lw + 8.0), gc.sy(line_h),
                Color::new(0.35, 0.75, 1.0, 0.12));
        }
 
        // Текст ссылки
        draw_text_ex(&label, gc.sx(x), gc.sy(y),
            TextParams { font_size: fs_px, font: Some(f), color: col, ..Default::default() });
 
        // Подчёркивание
        draw_line(
            gc.sx(x),      gc.sy(y + 2.5),
            gc.sx(x + lw), gc.sy(y + 2.5),
            gc.s(if hov { 1.5 } else { 0.9 }),
            col);
 
        if hov && lmb {
            return LessonLinkAction::from_target(&link.target);
        }
    }
 
    LessonLinkAction::None
}
 
// ─────────────────────────────────────────────────────────────────────────────
// Вспомогательная: считает примерное число строк после переноса
// ─────────────────────────────────────────────────────────────────────────────
fn estimate_line_count(
    text: &str, max_w: f32,
    f: &Font, gc: &GraphicsContext, lfs: f32,
) -> usize {
    let fs_px = gc.s(lfs).round() as u16;
    let mut lines = 0usize;
    for raw_line in text.split('\n') {
        let mut buf = String::new();
        for word in raw_line.split_whitespace() {
            let test = if buf.is_empty() {
                word.to_string()
            } else {
                format!("{} {}", buf, word)
            };
            let w = measure_text(&test, Some(f), fs_px, 1.0).width / gc.scale_x;
            if w > max_w && !buf.is_empty() {
                lines += 1;
                buf = word.to_string();
            } else {
                buf = test;
            }
        }
        lines += 1; // последняя строка параграфа
    }
    lines.max(1)
}
 
// ─────────────────────────────────────────────────────────────────────────────
// Навигация по шагам (Назад / Далее / Готово)
// ─────────────────────────────────────────────────────────────────────────────
fn draw_step_nav(
    gc: &GraphicsContext, f: &Font,
    lesson: &super::lessons::Lesson, state: &mut LessonsState,
    px: f32, py: f32, pw: f32, ph: f32,
    ac: Color, ac_hov: Color,
    locale: &Localization, mx: f32, my: f32, lmb: bool,
    is_day: bool,
) {
    let step_idx = state.open_step.unwrap_or(0);
    let nbw = 134.0_f32; let nbh = 36.0_f32;
    let nav_y = py + ph - 52.0;
    let prev_x = px + 22.0; let next_x = px + pw - nbw - 22.0;
    let hp = mx >= prev_x && mx <= prev_x + nbw && my >= nav_y && my <= nav_y + nbh;
    let has_next = !lesson.steps.is_empty() && step_idx + 1 < lesson.steps.len();
    let hn = has_next && mx >= next_x && mx <= next_x + nbw && my >= nav_y && my <= nav_y + nbh;
    let hd = !has_next && mx >= next_x && mx <= next_x + nbw && my >= nav_y && my <= nav_y + nbh;

    let prev_lbl = if step_idx == 0 { &locale.lessons_back } else { &locale.lessons_prev };
    small_btn(gc, f, prev_x, nav_y, nbw, nbh, prev_lbl, ac, ac_hov, hp, is_day);

    if has_next {
        small_btn(gc, f, next_x, nav_y, nbw, nbh, &locale.lessons_next, ac, ac_hov, hn, is_day);
    } else {
        // ── "Готово" — золотая кнопка, без изменений ───────────────────────
        let gold = Color::new(0.88, 0.75, 0.12, 1.0);
        let goldh= Color::new(1.00, 0.92, 0.25, 1.0);
        let bg = if hd { Color::new(0.40, 0.35, 0.04, 0.80) } else { Color::new(0.22, 0.18, 0.02, 0.65) };
        draw_rectangle(gc.sx(next_x), gc.sy(nav_y), gc.sx(nbw), gc.sy(nbh), bg);
        if hd { draw_glow_rect_lines(gc, next_x, nav_y, nbw, nbh, goldh); }
        else  { draw_rectangle_lines(gc.sx(next_x), gc.sy(nav_y), gc.sx(nbw), gc.sy(nbh), gc.s(1.0), gold); }
        let fs_px = gc.s(15.0).round() as u16;
        let lw = measure_text(&locale.lessons_done, Some(f), fs_px, 1.0).width / gc.scale_x;
        draw_text_ex(&locale.lessons_done, gc.sx(next_x + (nbw-lw)/2.0), gc.sy(nav_y + nbh*0.66),
            TextParams { font_size: fs_px, font: Some(f),
                color: if hd { WHITE } else { Color::new(1.0, 0.88, 0.30, 1.0) }, ..Default::default() });
        if (lmb && hd) || is_key_pressed(KeyCode::Enter) { state.close_lesson(); }
    }

    if (lmb && hp) || is_key_pressed(KeyCode::Left) || is_key_pressed(KeyCode::Backspace) {
        state.prev_step();
    }
    if (lmb && hn) || is_key_pressed(KeyCode::Right) { if has_next { state.next_step(lesson); } }
}
// ─────────────────────────────────────────────────────────────────────────────
// Список уроков/теории
// ─────────────────────────────────────────────────────────────────────────────
fn draw_lesson_list(
    gc: &GraphicsContext, f: &Font,
    file: &LessonsFile, state: &mut LessonsState,
    texture_cache: &HashMap<String, Texture2D>,
    px: f32, py: f32, pw: f32, ph: f32,
    lmb: bool, mx: f32, my: f32,
    accent: Color, sel_bg: Color, is_day: bool,
) {
    let (_, wy) = mouse_wheel();
    if wy > 0.0 && state.scroll_offset > 0 { state.scroll_offset -= 1; }
    else if wy < 0.0 { state.scroll_offset += 1; }
    if is_key_pressed(KeyCode::Up) && state.selected_lesson > 0 {
        state.selected_lesson -= 1;
        if state.selected_lesson < state.scroll_offset { state.scroll_offset = state.selected_lesson; }
    }
    if is_key_pressed(KeyCode::Down) && state.selected_lesson < file.lessons.len().saturating_sub(1) {
        state.selected_lesson += 1;
    }
    if is_key_pressed(KeyCode::Enter) { state.open_lesson(); }
 
    let list_top = py + 58.0; let item_h = 74.0_f32; let list_h = ph - 76.0;
    let visible = ((list_h / item_h).floor() as usize).max(1);
    let max_sc = file.lessons.len().saturating_sub(visible);
    state.scroll_offset = state.scroll_offset.min(max_sc);
    if state.selected_lesson >= state.scroll_offset + visible {
        state.scroll_offset = state.selected_lesson.saturating_sub(visible - 1);
    }
 
    for (li, lesson) in file.lessons.iter().enumerate().skip(state.scroll_offset).take(visible) {
            let vi = li - state.scroll_offset;
            let iy = list_top + vi as f32 * item_h;
            let ix = px + 14.0; let iw = pw - 28.0;
            let isel = li == state.selected_lesson;
            let ihov = mx >= ix && mx <= ix + iw && my >= iy && my <= iy + item_h - 4.0;
            if lmb && ihov { state.selected_lesson = li; state.open_lesson(); }

            let bg = if isel { sel_bg }
                    else if ihov { Color::new(sel_bg.r*0.45, sel_bg.g*0.45, sel_bg.b*0.45, 0.30) }
                    else if is_day { Color::new(1.0, 1.0, 1.0, 0.20) }
                    else { Color::new(0.06, 0.07, 0.09, 0.35) };
            draw_rectangle(gc.sx(ix), gc.sy(iy), gc.sx(iw), gc.sy(item_h-4.0), bg);
            if isel { draw_glow_rect_lines(gc, ix, iy, iw, item_h-4.0, accent); }
            else if ihov {
                draw_rectangle_lines(gc.sx(ix), gc.sy(iy), gc.sx(iw), gc.sy(item_h-4.0),
                    gc.s(1.0), Color::new(accent.r, accent.g, accent.b, 0.38));
            }
 
        let thumb_w = 56.0_f32; let thumb_h = item_h - 12.0;
        let thumb_x = ix + 10.0; let thumb_y = iy + 6.0;
        if let Some(ref ip) = lesson.image {
            if let Some(tex) = texture_cache.get(ip) {
                draw_texture_ex(tex, gc.sx(thumb_x), gc.sy(thumb_y),
                    Color::new(1.0, 1.0, 1.0, 0.88),
                    DrawTextureParams { dest_size: Some(Vec2::new(gc.sx(thumb_w), gc.sy(thumb_h))), ..Default::default() });
            } else {
                draw_rectangle(gc.sx(thumb_x), gc.sy(thumb_y), gc.sx(thumb_w), gc.sy(thumb_h),
                    Color::new(0.10, 0.10, 0.14, 0.55));
            }
        } else {
            draw_rectangle(gc.sx(thumb_x), gc.sy(thumb_y), gc.sx(thumb_w), gc.sy(thumb_h),
                Color::new(0.09, 0.09, 0.12, 0.55));
            let num = format!("{:02}", li+1);
            let nfs = gc.s(20.0).round() as u16;
            let nw  = measure_text(&num, Some(f), nfs, 1.0).width / gc.scale_x;
            draw_text_ex(&num, gc.sx(thumb_x + (thumb_w-nw)/2.0), gc.sy(thumb_y + thumb_h/2.0 + 8.0),
                TextParams { font_size: nfs, font: Some(f),
                    color: Color::new(accent.r, accent.g, accent.b, if isel { 1.0 } else { 0.52 }),
                    ..Default::default() });
        }
 
        let tx = thumb_x + thumb_w + 12.0; let avail = iw - thumb_w - 30.0;
        let tfs = gc.s(17.0).round() as u16;
        let tc  = if is_day {
            if isel { WHITE } else if ihov { Color::new(0.05, 0.08, 0.20, 1.0) } else { Color::new(0.15, 0.18, 0.28, 0.95) }
        } else {
            if isel { WHITE } else if ihov { Color::new(0.9, 0.95, 0.9, 1.0) } else { Color::new(0.7, 0.7, 0.7, 1.0) }
        };
        let tt  = truncate_str_px(&lesson.title, avail, f, 17, gc.s(1.0));
        draw_text_ex(&tt, gc.sx(tx), gc.sy(iy + item_h*0.42),
            TextParams { font_size: tfs, font: Some(f), color: tc, ..Default::default() });
        let dfs = gc.s(12.0).round() as u16;
        let desc_col = if is_day { Color::new(0.30, 0.34, 0.46, 0.90) } else { Color::new(0.50, 0.54, 0.64, 0.85) };
        let dt  = truncate_str_px(&lesson.description, avail, f, 12, gc.s(1.0));
        draw_text_ex(&dt, gc.sx(tx), gc.sy(iy + item_h*0.72),
            TextParams { font_size: dfs, font: Some(f), color: desc_col, ..Default::default() });
        if !lesson.steps.is_empty() {
            let ss = format!("{} шаг.", lesson.steps.len());
            let sfs2 = gc.s(11.0).round() as u16;
            draw_text_ex(&ss, gc.sx(ix + iw - 62.0), gc.sy(iy + item_h*0.60),
                TextParams { font_size: sfs2, font: Some(f),
                    color: Color::new(accent.r*0.8, accent.g*0.8, accent.b*0.5, 0.80), ..Default::default() });
        }
    }
 
    if file.lessons.len() > visible {
        let sb_x = px + pw - 11.0;
        let sb_r = state.scroll_offset as f32 / file.lessons.len().saturating_sub(visible) as f32;
        let sb_h = ((visible as f32 / file.lessons.len() as f32) * list_h).max(20.0);
        let sb_y = list_top + sb_r * (list_h - sb_h);
        draw_rectangle(gc.sx(sb_x), gc.sy(list_top), gc.s(8.0), gc.sy(list_h), Color::new(0.10, 0.11, 0.16, 0.6));
        draw_rectangle(gc.sx(sb_x+1.0), gc.sy(sb_y), gc.s(6.0), gc.sy(sb_h),
            Color::new(accent.r*0.9, accent.g*0.9, accent.b*0.6, 0.85));
    }
}
 
// ─────────────────────────────────────────────────────────────────────────────
// Общие вспомогательные
// ─────────────────────────────────────────────────────────────────────────────
 
fn panel_rect(gc: &GraphicsContext) -> (f32, f32, f32, f32) {
    let pw = gc.base_w * 0.88; let ph = gc.base_h * 0.86;
    let px = (gc.base_w - pw) / 2.0; let py = (gc.base_h - ph) / 2.0;
    (pw, ph, px, py)
}
 
fn draw_panel_bg(gc: &GraphicsContext, px: f32, py: f32, pw: f32, ph: f32, bg: Color, border: Color) {
    let is_light_bg = bg.r > 0.5;
    let overlay = if is_light_bg {
        Color::new(0.10, 0.18, 0.35, 0.30)
    } else {
         Color::new(0.03, 0.04, 0.08, 0.97)
    };
    draw_rectangle(gc.sx(0.0), gc.sy(0.0), gc.sx(gc.base_w), gc.sy(gc.base_h), overlay);
    draw_rectangle(gc.sx(px), gc.sy(py), gc.sx(pw), gc.sy(ph), bg);
    draw_glow_rect_lines(gc, px, py, pw, ph, border);
}
 
/// Резкий текст: fs округляется до целого пикселя
fn draw_sharp(gc: &GraphicsContext, f: &Font, text: &str, x: f32, y: f32, lfs: f32, color: Color) {
    let fs_px = gc.s(lfs).round() as u16;
    draw_text_ex(text, gc.sx(x), gc.sy(y),
        TextParams { font_size: fs_px, font: Some(f), color, ..Default::default() });
}
 
/// Measure с округлением
fn msh(gc: &GraphicsContext, f: &Font, text: &str, lfs: f32) -> f32 {
    let fs_px = gc.s(lfs).round() as u16;
    measure_text(text, Some(f), fs_px, 1.0).width / gc.scale_x
}
 
/// Маленькая кнопка с цветами акцента
fn small_btn(
    gc: &GraphicsContext, f: &Font,
    x: f32, y: f32, w: f32, h: f32,
    label: &str, ac: Color, ac_hov: Color, hov: bool, is_day: bool,
) {
    let bg = if is_day {
        if hov { Color::new(ac.r*0.30+0.55, ac.g*0.30+0.55, ac.b*0.30+0.55, 0.92) }
        else   { Color::new(0.90, 0.93, 0.97, 0.85) }
    } else {
        if hov { Color::new(ac.r*0.28, ac.g*0.28, ac.b*0.28, 0.88) }
        else   { Color::new(0.10, 0.11, 0.16, 0.75) }
    };
    draw_rectangle(gc.sx(x), gc.sy(y), gc.sx(w), gc.sy(h), bg);
    if hov { draw_glow_rect_lines(gc, x, y, w, h, ac_hov); }
    else   { draw_rectangle_lines(gc.sx(x), gc.sy(y), gc.sx(w), gc.sy(h),
                gc.s(1.0), Color::new(ac.r, ac.g, ac.b, if is_day { 0.60 } else { 0.46 })); }
    let fs_px = gc.s(13.0).round() as u16;
    let lw = measure_text(label, Some(f), fs_px, 1.0).width / gc.scale_x;
    let tc = if is_day {
        if hov { Color::new(0.05, 0.07, 0.14, 1.0) } else { Color::new(0.10, 0.14, 0.30, 0.95) }
    } else {
        if hov { WHITE } else { Color::new(ac.r*0.88+0.12, ac.g*0.88+0.12, ac.b*0.88+0.12, 0.95) }
    };
    draw_text_ex(label, gc.sx(x + (w-lw)/2.0), gc.sy(y + h*0.72),
        TextParams { font_size: fs_px, font: Some(f), color: tc, ..Default::default() });
}
 
/// Картинки шага урока
fn draw_step_images(gc: &GraphicsContext, step: &LessonStep, cache: &HashMap<String, Texture2D>) -> bool {
    let mut sorted = step.images.clone();
    sorted.sort_by_key(|i| i.z_order);
    let mut any = false;
    for img in &sorted {
        if let Some(tex) = cache.get(&img.path) {
            let ix = img.x * gc.base_w; let iy = img.y * gc.base_h;
            let iw = img.w * gc.base_w;
            let ih = if tex.width() > 0.0 { iw * tex.height() / tex.width() } else { iw };
            draw_texture_ex(tex, gc.sx(ix), gc.sy(iy), Color::new(1.0, 1.0, 1.0, img.alpha),
                DrawTextureParams { dest_size: Some(Vec2::new(gc.sx(iw), gc.sy(ih))), ..Default::default() });
            any = true;
        }
    }
    any
}
 
/// Перенос строк с резкими пикселями
fn draw_wrapped_sharp(
    gc: &GraphicsContext, f: &Font,
    text: &str, x: f32, y: f32, max_w: f32, lfs: f32, color: Color,
) {
    let fs_px = gc.s(lfs).round() as u16;
    let line_h = lfs * 1.58;
    let mut cur_y = y;
    for raw_line in text.split('\n') {
        let mut buf = String::new();
        for word in raw_line.split_whitespace() {
            let test = if buf.is_empty() { word.to_string() } else { format!("{} {}", buf, word) };
            let w = measure_text(&test, Some(f), fs_px, 1.0).width / gc.scale_x;
            if w > max_w && !buf.is_empty() {
                draw_text_ex(&buf, gc.sx(x), gc.sy(cur_y),
                    TextParams { font_size: fs_px, font: Some(f), color, ..Default::default() });
                cur_y += line_h; buf = word.to_string();
            } else { buf = test; }
        }
        if !buf.is_empty() {
            draw_text_ex(&buf, gc.sx(x), gc.sy(cur_y),
                TextParams { font_size: fs_px, font: Some(f), color, ..Default::default() });
        }
        cur_y += line_h;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ПАНЕЛЬ ЗАПИСЕЙ (Tuner → Records) — с переименованием по ПКМ и выделением
// ─────────────────────────────────────────────────────────────────────────────
pub fn draw_tuner_records_panel(
    gc: &GraphicsContext,
    records: &[String],
    selected: usize,
    is_playing: bool,
    renaming_idx: Option<usize>,
    rename_buffer: &str,
    rename_cursor: usize,
    rename_selection: Option<(usize, usize)>, // (start, end) позиции выделения
    f: &Font,
    theme: &ThemeState,
) -> (usize, bool, bool, bool, bool, usize, Option<(usize, usize)>) {
    // (new_sel, play_click, stop_click, rename_request, rename_cancel, new_cursor, new_selection)
    let is_day = theme.is_day;
    let list_x   = gc.base_w - 345.0;
    let list_y   = 150.0_f32;
    let list_w   = 300.0_f32;
    let list_h   = 780.0_f32; // УВЕЛИЧЕНО В 2 РАЗА (было 390.0)
    let item_h   = 28.0_f32;
    let header_h = 44.0_f32;
    let max_visible = ((list_h - header_h - 15.0) / item_h).floor() as usize;
    
    draw_rectangle(gc.sx(list_x), gc.sy(list_y), gc.sx(list_w), gc.sy(list_h), btn_panel_bg(is_day));
    draw_glow_rect_lines(gc, list_x, list_y, list_w, list_h,
        if is_playing { Color::new(0.2, 1.0, 0.5, 1.0) } else { Color::new(0.25, 0.45, 0.85, 0.9) });
    
    let title = "RECORDS";
    let title_color = if is_playing { Color::new(0.2, 1.0, 0.5, 1.0) }
        else if is_day { Color::new(0.10, 0.12, 0.25, 1.0) } else { WHITE };
    let tw = measure_text(title, Some(f), gc.s(17.0) as u16, 1.0).width / gc.scale_x;
    draw_glow_text(gc, title, list_x + (list_w - tw) / 2.0, list_y + 28.0, 17, title_color, f);
    
    draw_line(gc.sx(list_x + 5.0), gc.sy(list_y + header_h),
              gc.sx(list_x + list_w - 5.0), gc.sy(list_y + header_h),
              gc.s(1.0), btn_panel_border(is_day));
    
    let (mx_log, my_log) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);
    let rmb = is_mouse_button_pressed(MouseButton::Right);
    let mouse_over = mx_log >= list_x && mx_log <= list_x + list_w
                  && my_log >= list_y && my_log <= list_y + list_h;
    
    let mut new_sel = selected;
    let mut new_cursor = rename_cursor;
    let new_selection = rename_selection;
    
    if !records.is_empty() {
        let (_, wheel_y) = mouse_wheel();
        if mouse_over && renaming_idx.is_none() {
            if wheel_y > 0.0 && new_sel > 0 { new_sel -= 1; }
            else if wheel_y < 0.0 && new_sel < records.len().saturating_sub(1) { new_sel += 1; }
        }
    }
    
    let mut clicked_play = false;
    let mut clicked_stop = false;
    let mut rename_request = false;
    let mut rename_cancel = false;
    let empty_col = if is_day { Color::new(0.35, 0.38, 0.48, 1.0) } else { GRAY };
    
    if records.is_empty() {
        draw_text_custom(gc, "Записей нет", list_x + 10.0, list_y + header_h + 22.0, 14, empty_col, f);
        draw_text_custom(gc, "R - начать запись", list_x + 10.0, list_y + header_h + 42.0, 12,
            if is_day { Color::new(0.30, 0.34, 0.44, 0.8) } else { Color::new(0.4, 0.5, 0.7, 0.8) }, f);
        return (new_sel, false, false, false, false, new_cursor, new_selection);
    }
    
    let start_idx = if records.len() > max_visible {
        new_sel.saturating_sub(max_visible / 2).min(records.len().saturating_sub(max_visible))
    } else { 0 };
    
    for (i, record_name) in records.iter().enumerate().skip(start_idx).take(max_visible) {
        let vi = i - start_idx;
        let item_y = list_y + header_h + 5.0 + vi as f32 * item_h;
        let is_sel = i == new_sel;
        let is_now_playing = is_sel && is_playing;
        let is_renaming = renaming_idx == Some(i);
        
        // Кнопка Stop (только для играющей записи)
        let stop_btn_w = 24.0_f32;
        let stop_btn_h = item_h - 4.0;
        let stop_btn_x = list_x + list_w - stop_btn_w - 8.0;
        let stop_btn_y = item_y + 2.0;
        let stop_hover = is_now_playing
            && mx_log >= stop_btn_x && mx_log <= stop_btn_x + stop_btn_w
            && my_log >= stop_btn_y && my_log <= stop_btn_y + stop_btn_h;
        
        // Зона строки (без кнопки Stop)
        let ir_x = list_x + 5.0;
        let ir_w = list_w - 22.0 - (if is_now_playing { stop_btn_w + 6.0 } else { 0.0 });
        let is_hover = mx_log >= ir_x && mx_log <= ir_x + ir_w
                    && my_log >= item_y && my_log <= item_y + item_h;
        
        // ПКМ по строке — запрос переименования
        if rmb && is_hover && !is_renaming {
            new_sel = i;
            rename_request = true;
        }
        
        // ЛКМ по строке — воспроизведение (только если не в режиме переименования)
        if lmb && is_hover && !is_renaming {
            new_sel = i;
            clicked_play = true;
        }
        
        if stop_hover && lmb { clicked_stop = true; }
        
        let bg = if is_renaming { Color::new(0.30, 0.20, 0.55, 0.75) }
            else if is_now_playing { Color::new(0.05, 0.30, 0.15, 0.65) }
            else if is_sel         { Color::new(0.12, 0.25, 0.55, 0.55) }
            else if is_hover       { Color::new(0.08, 0.14, 0.30, 0.40) }
            else                   { Color::new(0.0,  0.0,  0.0,  0.0 ) };
        draw_rectangle(gc.sx(ir_x), gc.sy(item_y), gc.sx(ir_w), gc.sy(item_h - 2.0), bg);
        
        if is_renaming     { draw_glow_rect_lines(gc, ir_x, item_y, ir_w, item_h - 2.0, Color::new(0.8, 0.4, 1.0, 1.0)); }
        else if is_now_playing { draw_glow_rect_lines(gc, ir_x, item_y, ir_w, item_h - 2.0, Color::new(0.1, 0.9, 0.4, 0.9)); }
        else if is_sel    { draw_glow_rect_lines(gc, ir_x, item_y, ir_w, item_h - 2.0, Color::new(0.2, 0.6, 1.0, 0.8)); }
        else if is_hover  { draw_rectangle_lines(gc.sx(ir_x), gc.sy(item_y), gc.sx(ir_w), gc.sy(item_h - 2.0), gc.s(1.0), Color::new(0.3, 0.5, 1.0, 0.4)); }
        
        // ── Режим переименования: рисуем поле ввода с выделением ────────────
        if is_renaming {
            let input_x = ir_x + 4.0;
            let input_w = ir_w - 8.0;
            let input_y = item_y + 2.0;
            let input_h = item_h - 6.0;
            let input_bg = if is_day { Color::new(1.0, 1.0, 1.0, 0.95) }
                           else { Color::new(0.15, 0.18, 0.28, 0.95) };
            draw_rectangle(gc.sx(input_x), gc.sy(input_y),
                           gc.sx(input_w), gc.sy(input_h), input_bg);
            draw_rectangle_lines(gc.sx(input_x), gc.sy(input_y),
                gc.sx(input_w), gc.sy(input_h), gc.s(1.5), Color::new(0.8, 0.4, 1.0, 1.0));
            
            let text_col = if is_day { Color::new(0.05, 0.08, 0.16, 1.0) } else { WHITE };
            let display = if rename_buffer.is_empty() { "новое_имя".to_string() }
                          else { rename_buffer.to_string() };
            
            // Визуализация выделения
            if let Some((sel_start, sel_end)) = rename_selection {
                let sel_min = sel_start.min(sel_end);
                let sel_max = sel_start.max(sel_end);
                
                // Вычисляем ширину текста до выделения
                let before_sel: String = display.chars().take(sel_min).collect();
                let before_w = measure_text(&before_sel, Some(f), gc.s(12.0) as u16, 1.0).width / gc.scale_x;
                
                // Вычисляем ширину выделенного текста
                let selected_text: String = display.chars().skip(sel_min).take(sel_max - sel_min).collect();
                let sel_w = measure_text(&selected_text, Some(f), gc.s(12.0) as u16, 1.0).width / gc.scale_x;
                
                // Рисуем фон выделения (синий как в Windows)
                draw_rectangle(
                    gc.sx(input_x + 5.0 + before_w),
                    gc.sy(input_y + 2.0),
                    gc.sx(sel_w),
                    gc.sy(input_h - 4.0),
                    Color::new(0.2, 0.4, 0.8, 0.6)
                );
            }
            
            // Рисуем текст
            let dn = truncate_str_px(&display, input_w - 10.0, f, 12, gc.s(1.0));
            draw_text_ex(&dn, gc.sx(input_x + 5.0), gc.sy(input_y + input_h * 0.72),
                TextParams { font_size: gc.s(12.0) as u16, font: Some(f), color: text_col, ..Default::default() });
            
            // Мигающий курсор
            let cursor_blink = (get_time() as f32 * 2.0).sin() > 0.0;
            if cursor_blink && new_selection.is_none() {
                let cursor_text: String = display.chars().take(new_cursor).collect();
                let cw = measure_text(&cursor_text, Some(f), gc.s(12.0) as u16, 1.0).width / gc.scale_x;
                let cx = input_x + 5.0 + cw;
                draw_line(gc.sx(cx), gc.sy(input_y + 3.0),
                          gc.sx(cx), gc.sy(input_y + input_h - 3.0),
                          gc.s(1.5), text_col);
            }
            continue; // пропускаем обычный текст
        }
        
        // Обычный режим — рисуем имя записи
        let prefix = if is_now_playing { ">- " } else if is_sel { "> " } else { "  " };
        let text_color = if is_now_playing { Color::new(0.15, 1.0, 0.5, 1.0) }
            else if is_sel  { if is_day { Color::new(0.05, 0.08, 0.18, 1.0) } else { WHITE } }
            else if is_hover { if is_day { Color::new(0.10, 0.13, 0.25, 1.0) } else { Color::new(0.85, 0.88, 0.98, 1.0) } }
            else { if is_day { Color::new(0.35, 0.38, 0.48, 1.0) } else { Color::new(0.60, 0.62, 0.70, 1.0) } };
        
        let text_start_x = ir_x + 5.0;
        let text_avail_w = ir_w - 5.0 - 4.0;
        let prefix_w = measure_text(prefix, Some(f), gc.s(12.0) as u16, 1.0).width / gc.scale_x;
        let name_avail_w = (text_avail_w - prefix_w).max(10.0);
        let dn = truncate_str_px(record_name, name_avail_w, f, 12, gc.s(1.0));
        let label = format!("{}{}", prefix, dn);
        if is_now_playing { draw_glow_text(gc, &label, text_start_x, item_y + item_h * 0.72, 12, text_color, f); }
        else { draw_text_custom(gc, &label, text_start_x, item_y + item_h * 0.72, 12, text_color, f); }
        
        // Рисуем кнопку Stop
        if is_now_playing {
            let stop_bg = if stop_hover { Color::new(1.0, 0.2, 0.2, 0.95) }
                          else          { Color::new(0.7, 0.15, 0.15, 0.85) };
            draw_rectangle(gc.sx(stop_btn_x), gc.sy(stop_btn_y),
                           gc.sx(stop_btn_w), gc.sy(stop_btn_h), stop_bg);
            if stop_hover {
                draw_glow_rect_lines(gc, stop_btn_x, stop_btn_y, stop_btn_w, stop_btn_h,
                    Color::new(1.0, 0.4, 0.4, 1.0));
            } else {
                draw_rectangle_lines(gc.sx(stop_btn_x), gc.sy(stop_btn_y),
                    gc.sx(stop_btn_w), gc.sy(stop_btn_h), gc.s(1.0), Color::new(1.0, 0.3, 0.3, 0.7));
            }
            let sq = 8.0_f32;
            let sq_x = stop_btn_x + (stop_btn_w - sq) / 2.0;
            let sq_y = stop_btn_y + (stop_btn_h - sq) / 2.0;
            draw_rectangle(gc.sx(sq_x), gc.sy(sq_y), gc.sx(sq), gc.sy(sq), WHITE);
        }
    }
    
    if records.len() > max_visible {
        let sb_x = list_x + list_w - 14.0;
        let sb_area_y = list_y + header_h + 5.0;
        let sb_area_h = list_h - header_h - 20.0;
        let sb_ratio = start_idx as f32 / (records.len() - max_visible) as f32;
        let sb_h = ((max_visible as f32 / records.len() as f32) * sb_area_h).max(18.0);
        let sb_y = sb_area_y + sb_ratio * (sb_area_h - sb_h);
        draw_rectangle(gc.sx(sb_x), gc.sy(sb_area_y), gc.s(8.0), gc.sy(sb_area_h), Color::new(0.12, 0.12, 0.18, 0.75));
        draw_rectangle(gc.sx(sb_x + 1.0), gc.sy(sb_y), gc.s(6.0), gc.sy(sb_h), Color::new(0.35, 0.55, 0.95, 0.88));
    }
    
    draw_text_custom(gc, "ПКМ — переимен. | Клик — воспр.", list_x + 8.0, list_y + list_h - 8.0, 10,
        if is_day { Color::new(0.30, 0.34, 0.44, 0.85) } else { Color::new(0.35, 0.45, 0.65, 0.85) }, f);
    
    (new_sel, clicked_play, clicked_stop, rename_request, rename_cancel, new_cursor, new_selection)
}


// ─── Результат кликов по UI тюнера ───────────────────────────────────────────
#[derive(Debug, Clone, Default)]
pub struct TunerStringClicks {
    /// Клик по кнопке выбора струны (0..5)
    pub string_clicked: Option<usize>,
    /// Клик по кнопке открытия меню строев
    pub tuning_menu_toggle: bool,
    /// Выбор строя из меню (индекс)
    pub tuning_selected: Option<usize>,
}

/// Отрисовка кнопок выбора струн (слева экрана, вертикально)
pub fn draw_tuner_string_buttons(
    gc: &GraphicsContext,
    f: &Font,
    tuning: &GuitarTuning,
    selected_string: usize,
    detected_freq: Option<f32>,
    theme: &ThemeState,
) -> Option<usize> {
    let is_day = theme.is_day;
    let (mx, my) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);

    let btn_w = 110.0_f32;
    let btn_h = 44.0_f32;
    let gap = 6.0_f32;
    let start_x = 20.0_f32;
    let start_y = 200.0_f32;

    let mut clicked: Option<usize> = None;

    // Заголовок
    let title = "Струна";
    let tfs = gc.s(14.0).round() as u16;
    let tw = measure_text(title, Some(f), tfs, 1.0).width / gc.scale_x;
    draw_text_ex(title,
        gc.sx(start_x + (btn_w - tw) / 2.0), gc.sy(start_y - 22.0),
        TextParams { font_size: tfs, font: Some(f),
            color: if is_day { Color::new(0.10, 0.14, 0.30, 1.0) }
                   else { Color::new(0.7, 0.8, 1.0, 0.9) },
            ..Default::default() });

    for si in 0..6usize {
        let y = start_y + si as f32 * (btn_h + gap);
        let is_selected = si == selected_string;

        // Определяем, звучит ли сейчас эта струна (частота близка к целевой)
        let target_freq = tuning.string_freq(si);
        let is_in_tune = detected_freq.map_or(false, |df| {
            (df - target_freq).abs() < target_freq * 0.03
        });

        let hov = mx >= start_x && mx <= start_x + btn_w
               && my >= y && my <= y + btn_h;

        // Фон
        let bg = if is_selected {
            if is_day { Color::new(0.30, 0.55, 0.90, 0.92) }
            else      { Color::new(0.20, 0.45, 0.85, 0.85) }
        } else if hov {
            if is_day { Color::new(0.70, 0.78, 0.90, 0.70) }
            else      { Color::new(0.15, 0.22, 0.40, 0.60) }
        } else {
            if is_day { Color::new(0.92, 0.94, 0.97, 0.75) }
            else      { Color::new(0.08, 0.10, 0.16, 0.70) }
        };

        draw_rectangle(gc.sx(start_x), gc.sy(y), gc.sx(btn_w), gc.sy(btn_h), bg);

        // Рамка
        let border_col = if is_selected {
            Color::new(0.3, 0.85, 1.0, 0.95)
        } else if is_in_tune {
            Color::new(0.2, 1.0, 0.4, 0.90)
        } else if hov {
            Color::new(0.5, 0.7, 1.0, 0.70)
        } else {
            Color::new(0.4, 0.5, 0.7, 0.40)
        };
        draw_rectangle_lines(gc.sx(start_x), gc.sy(y), gc.sx(btn_w), gc.sy(btn_h),
            gc.s(if is_selected { 2.0 } else { 1.2 }), border_col);

        // Индикатор "в строю" (зелёная точка)
        if is_in_tune {
            let pulse = ((get_time() as f32 * 4.0).sin() * 0.3 + 0.7).clamp(0.0, 1.0);
            draw_circle(gc.sx(start_x + btn_w - 14.0), gc.sy(y + btn_h / 2.0),
                gc.s(5.0), Color::new(0.2, 1.0, 0.4, pulse));
        }

        // Номер струны
        let num_str = format!("{}", 6 - si);
        let nfs = gc.s(18.0).round() as u16;
        let nw = measure_text(&num_str, Some(f), nfs, 1.0).width / gc.scale_x;
        let num_col = if is_selected { WHITE }
                      else if is_day { Color::new(0.15, 0.20, 0.35, 1.0) }
                      else { Color::new(0.85, 0.88, 0.95, 1.0) };
        draw_text_ex(num_str,
            gc.sx(start_x + 10.0), gc.sy(y + btn_h / 2.0 + 6.0),
            TextParams { font_size: nfs, font: Some(f), color: num_col, ..Default::default() });

        // Название ноты
        let note = tuning.string_note(si);
        let ntf = gc.s(15.0).round() as u16;
        let note_col = if is_selected { WHITE }
                       else if is_day { Color::new(0.30, 0.35, 0.50, 1.0) }
                       else { Color::new(0.65, 0.72, 0.85, 1.0) };
        draw_text_ex(note,
            gc.sx(start_x + 36.0), gc.sy(y + btn_h / 2.0 + 5.0),
            TextParams { font_size: ntf, font: Some(f), color: note_col, ..Default::default() });

        // Частота (мелко)
        let freq_str = format!("{:.1}", target_freq);
        let ffs = gc.s(10.0).round() as u16;
        let freq_col = if is_selected { Color::new(1.0, 1.0, 1.0, 0.75) }
                       else if is_day { Color::new(0.45, 0.50, 0.60, 0.85) }
                       else { Color::new(0.50, 0.55, 0.65, 0.75) };
        draw_text_ex(&freq_str,
            gc.sx(start_x + btn_w - 42.0), gc.sy(y + btn_h / 2.0 + 4.0),
            TextParams { font_size: ffs, font: Some(f), color: freq_col, ..Default::default() });

        // Клик
        if lmb && hov { clicked = Some(si); }
    }

    clicked
}

/// Отрисовка кнопки выбора строя (над кнопками струн)
pub fn draw_tuner_tuning_selector(
    gc: &GraphicsContext,
    f: &Font,
    tunings: &[GuitarTuning],
    selected_idx: usize,
    menu_open: bool,
    theme: &ThemeState,
) -> (bool, Option<usize>) {
    // (toggle_menu, selected_new_idx)
    let is_day = theme.is_day;
    let (mx, my) = mouse_position_logical(gc);
    let lmb = is_mouse_button_pressed(MouseButton::Left);

    let btn_w = 110.0_f32;
    let btn_h = 34.0_f32;
    let btn_x = 20.0_f32;
    let btn_y = 150.0_f32;

    let hov = mx >= btn_x && mx <= btn_x + btn_w
           && my >= btn_y && my <= btn_y + btn_h;

    // Фон кнопки
    let bg = if menu_open || hov {
        if is_day { Color::new(0.75, 0.60, 0.10, 0.92) }
        else      { Color::new(0.55, 0.40, 0.05, 0.88) }
    } else {
        if is_day { Color::new(0.95, 0.88, 0.65, 0.85) }
        else      { Color::new(0.18, 0.14, 0.04, 0.80) }
    };
    draw_rectangle(gc.sx(btn_x), gc.sy(btn_y), gc.sx(btn_w), gc.sy(btn_h), bg);

    // Рамка
    let border_col = if menu_open || hov {
        Color::new(1.0, 0.85, 0.30, 0.95)
    } else {
        Color::new(0.80, 0.65, 0.15, 0.60)
    };
    draw_rectangle_lines(gc.sx(btn_x), gc.sy(btn_y), gc.sx(btn_w), gc.sy(btn_h),
        gc.s(1.5), border_col);

    // Заголовок "Строй"
    let title = "Строй";
    let tfs = gc.s(11.0).round() as u16;
    let tw = measure_text(title, Some(f), tfs, 1.0).width / gc.scale_x;
    draw_text_ex(title,
        gc.sx(btn_x + (btn_w - tw) / 2.0), gc.sy(btn_y - 6.0),
        TextParams { font_size: tfs, font: Some(f),
            color: if is_day { Color::new(0.45, 0.30, 0.02, 1.0) }
                   else { Color::new(1.0, 0.88, 0.28, 0.90) },
            ..Default::default() });

    // Текст выбранного строя
    let current_name = tunings.get(selected_idx)
        .map(|t| if is_day { t.name_ru.as_str() } else { t.name.as_str() })
        .unwrap_or("--");

    // Обрезаем если длинное
    let max_chars = 12;
    let display_name = if current_name.chars().count() > max_chars {
        let short: String = current_name.chars().take(max_chars - 1).collect();
        format!("{}…", short)
    } else {
        current_name.to_string()
    };

    let ntf = gc.s(13.0).round() as u16;
    let nw = measure_text(&display_name, Some(f), ntf, 1.0).width / gc.scale_x;
    let text_col = if is_day { Color::new(0.10, 0.08, 0.02, 1.0) }
                   else { Color::new(1.0, 0.92, 0.55, 1.0) };
    draw_text_ex(&display_name,
        gc.sx(btn_x + 8.0), gc.sy(btn_y + btn_h / 2.0 + 4.0),
        TextParams { font_size: ntf, font: Some(f), color: text_col, ..Default::default() });

    // Стрелка ▼
    let arrow = if menu_open { "▲" } else { "▼" };
    let afs = gc.s(11.0).round() as u16;
    draw_text_ex(arrow,
        gc.sx(btn_x + btn_w - 18.0), gc.sy(btn_y + btn_h / 2.0 + 4.0),
        TextParams { font_size: afs, font: Some(f), color: text_col, ..Default::default() });

    let toggle_menu = lmb && hov;

    // ── Выпадающее меню ───────────────────────────────────────────────
    let mut selected_new: Option<usize> = None;
    if menu_open {
        let item_h = 28.0_f32;
        let menu_w = 220.0_f32;
        let menu_h = (tunings.len() as f32 * item_h).min(400.0);
        let menu_x = btn_x + btn_w + 8.0;
        let menu_y = btn_y;

        // Фон меню
        draw_rectangle(gc.sx(menu_x), gc.sy(menu_y), gc.sx(menu_w), gc.sy(menu_h),
            if is_day { Color::new(0.98, 0.96, 0.90, 0.98) }
            else      { Color::new(0.08, 0.07, 0.03, 0.98) });
        draw_glow_rect_lines(gc, menu_x, menu_y, menu_w, menu_h,
            Color::new(1.0, 0.85, 0.30, 0.85));

        // Заголовок
        let m_title = "Выберите строй";
        let mtf = gc.s(12.0).round() as u16;
        draw_text_ex(m_title,
            gc.sx(menu_x + 10.0), gc.sy(menu_y + 16.0),
            TextParams { font_size: mtf, font: Some(f),
                color: if is_day { Color::new(0.45, 0.30, 0.02, 1.0) }
                       else { Color::new(1.0, 0.88, 0.28, 0.95) },
                ..Default::default() });

        // Элементы списка
        let list_y = menu_y + 24.0;
        for (i, t) in tunings.iter().enumerate() {
            let iy = list_y + i as f32 * item_h;
            if iy + item_h > menu_y + menu_h { break; }

            let is_cur = i == selected_idx;
            let ihov = mx >= menu_x && mx <= menu_x + menu_w
                    && my >= iy && my <= iy + item_h;

            // Фон элемента
            let ebg = if is_cur {
                if is_day { Color::new(0.85, 0.75, 0.30, 0.65) }
                else      { Color::new(0.40, 0.30, 0.05, 0.70) }
            } else if ihov {
                if is_day { Color::new(0.90, 0.85, 0.55, 0.45) }
                else      { Color::new(0.25, 0.20, 0.05, 0.55) }
            } else {
                Color::new(0.0, 0.0, 0.0, 0.0)
            };
            draw_rectangle(gc.sx(menu_x + 4.0), gc.sy(iy),
                gc.sx(menu_w - 8.0), gc.sy(item_h - 2.0), ebg);

            // Название
            let name = if is_day { &t.name_ru } else { &t.name };
            let ntf = gc.s(12.0).round() as u16;
            let name_col = if is_cur {
                if is_day { Color::new(0.10, 0.08, 0.02, 1.0) }
                else      { Color::new(1.0, 0.92, 0.55, 1.0) }
            } else if ihov {
                if is_day { Color::new(0.15, 0.12, 0.05, 1.0) }
                else      { Color::new(0.95, 0.88, 0.55, 1.0) }
            } else {
                if is_day { Color::new(0.30, 0.25, 0.10, 1.0) }
                else      { Color::new(0.70, 0.65, 0.45, 1.0) }
            };
            draw_text_ex(name,
                gc.sx(menu_x + 10.0), gc.sy(iy + item_h / 2.0 + 4.0),
                TextParams { font_size: ntf, font: Some(f), color: name_col, ..Default::default() });

            // Галочка для выбранного
            if is_cur {
                draw_text_ex("✓",
                    gc.sx(menu_x + menu_w - 22.0), gc.sy(iy + item_h / 2.0 + 4.0),
                    TextParams { font_size: ntf, font: Some(f), color: name_col, ..Default::default() });
            }

            // Клик
            if lmb && ihov {
                selected_new = Some(i);
            }
        }

        // Клик вне меню — закрыть
        let in_menu = mx >= menu_x && mx <= menu_x + menu_w
                   && my >= menu_y && my <= menu_y + menu_h;
        let in_btn = mx >= btn_x && mx <= btn_x + btn_w
                  && my >= btn_y && my <= btn_y + btn_h;
        if lmb && !in_menu && !in_btn {
            // Закрыть меню без выбора
            return (true, None);
        }
    }

    (toggle_menu, selected_new)
}

/// Отрисовка большой индикации целевой ноты/частоты выбранной струны
/// и текущей детектированной частоты с индикатором отклонения.
pub fn draw_tuner_target_indicator(
    gc: &GraphicsContext,
    f: &Font,
    tuning: &GuitarTuning,
    selected_string: usize,
    detected_freq: Option<f32>,
    theme: &ThemeState,
) {
    let is_day = theme.is_day;
    let target_freq = tuning.string_freq(selected_string);
    let target_note = tuning.string_note(selected_string);

    // Позиция — справа от кнопок струн, по центру верхней части
    let cx = 280.0_f32;
    let cy = 260.0_f32;

    // ── Целевая нота (большая, сверху) ────────────────────────────────
    let target_label = format!("Цель: {} ({:.2} Hz)", target_note, target_freq);
    let tfs = gc.s(16.0).round() as u16;
    let tw = measure_text(&target_label, Some(f), tfs, 1.0).width / gc.scale_x;
    draw_text_ex(&target_label,
        gc.sx(cx - tw / 2.0), gc.sy(cy),
        TextParams { font_size: tfs, font: Some(f),
            color: if is_day { Color::new(0.10, 0.14, 0.30, 1.0) }
                   else { Color::new(0.85, 0.90, 1.0, 0.95) },
            ..Default::default() });

    // ── Детектированная частота и отклонение ─────────────────────────
    if let Some(df) = detected_freq {
        // Вычисляем отклонение в центах
        let cents = 1200.0 * (df / target_freq).log2();
        let abs_cents = cents.abs();
        let in_tune = abs_cents < 5.0;

        // Цвет по точности
        let col = if in_tune {
            Color::new(0.2, 1.0, 0.4, 1.0)
        } else if abs_cents < 15.0 {
            Color::new(1.0, 0.9, 0.2, 1.0)
        } else {
            Color::new(1.0, 0.4, 0.3, 1.0)
        };

        // Детектированная нота
        let det_note = crate::get_note_name(df);
        let dfs = gc.s(32.0).round() as f32;
        let dnw = measure_text(&det_note, Some(f), gc.s(32.0).round() as u16, 1.0).width / gc.scale_x;
        draw_text_ex(&det_note,
            gc.sx(cx - dnw / 2.0), gc.sy(cy + 50.0),
            TextParams { font_size: gc.s(32.0).round() as u16, font: Some(f), color: col, ..Default::default() });

        // Частота
        let freq_str = format!("{:.2} Hz", df);
        let ffs = gc.s(14.0).round() as u16;
        let fw = measure_text(&freq_str, Some(f), ffs, 1.0).width / gc.scale_x;
        draw_text_ex(&freq_str,
            gc.sx(cx - fw / 2.0), gc.sy(cy + 80.0),
            TextParams { font_size: ffs, font: Some(f), color: col, ..Default::default() });

        // Отклонение в центах
        let sign = if cents > 0.5 { "+" } else if cents < -0.5 { "-" } else { "±" };
        let cents_str = format!("{}{:.1} cent", sign, abs_cents);
        let cfs = gc.s(13.0).round() as u16;
        let cw = measure_text(&cents_str, Some(f), cfs, 1.0).width / gc.scale_x;
        draw_text_ex(&cents_str,
            gc.sx(cx - cw / 2.0), gc.sy(cy + 100.0),
            TextParams { font_size: cfs, font: Some(f), color: col, ..Default::default() });

        // Стрелка-индикатор направления
        let arrow = if abs_cents < 3.0 { "✓" }
                    else if cents > 0.0 { "▲ (выше)" }
                    else { "▼ (ниже)" };
        let afs = gc.s(12.0).round() as u16;
        let aw = measure_text(arrow, Some(f), afs, 1.0).width / gc.scale_x;
        draw_text_ex(arrow,
            gc.sx(cx - aw / 2.0), gc.sy(cy + 120.0),
            TextParams { font_size: afs, font: Some(f), color: col, ..Default::default() });
    } else {
        let no_sig = "— нет сигнала —";
        let nfs = gc.s(16.0).round() as u16;
        let nw = measure_text(no_sig, Some(f), nfs, 1.0).width / gc.scale_x;
        draw_text_ex(no_sig,
            gc.sx(cx - nw / 2.0), gc.sy(cy + 60.0),
            TextParams { font_size: nfs, font: Some(f),
                color: if is_day { Color::new(0.50, 0.55, 0.65, 0.85) }
                       else { Color::new(0.45, 0.50, 0.60, 0.75) },
                ..Default::default() });
    }
}

/// Компактный нотный стан в правом верхнем углу (режим пианино).
/// Ноты движутся к центральной красной линии: пересечение = момент попадания.
pub fn draw_mini_notation_top_right(
    gc: &GraphicsContext, s: &GameState, f: &Font, theme: &ThemeState,
) {
    let l = &gc.layout;
    let is_day = theme.is_day;

    let panel_w = 420.0_f32;
    let panel_h = 150.0_f32;
    let panel_x = l.window_w - panel_w - 20.0;
    let panel_y = 64.0_f32; // под индикатором MIDI

    let bg        = if is_day { Color::new(0.72, 0.76, 0.84, 0.90) }
                    else      { Color::new(0.06, 0.06, 0.10, 0.85) };
    let border    = if is_day { Color::new(0.35, 0.45, 0.65, 0.55) }
                    else      { Color::new(0.4, 0.5, 0.7, 0.6) };
    let label_col = if is_day { Color::new(0.15, 0.20, 0.35, 0.95) }
                    else      { Color::new(0.7, 0.8, 1.0, 0.9) };
    let staff_col = if is_day { Color::new(0.30, 0.34, 0.45, 0.55) }
                    else      { Color::new(0.6, 0.6, 0.7, 0.4) };

    draw_rectangle(gc.sx(panel_x), gc.sy(panel_y), gc.sx(panel_w), gc.sy(panel_h), bg);
    draw_rectangle_lines(gc.sx(panel_x), gc.sy(panel_y), gc.sx(panel_w), gc.sy(panel_h),
                         gc.s(2.0), border);
    draw_text_custom(gc, if s.lang_ru { "Нотный стан" } else { "Notation" },
                     panel_x + 10.0, panel_y + 16.0, 13, label_col, f);

    // Пятилинейный стан
    let sc = panel_y + panel_h / 2.0 + 8.0;
    for i in -2..=2 {
        draw_line(gc.sx(panel_x + 6.0), gc.sy(sc + i as f32 * 12.0),
                  gc.sx(panel_x + panel_w - 6.0), gc.sy(sc + i as f32 * 12.0),
                  gc.s(1.0), staff_col);
    }

    // Ноты достигают центральной линии ровно в момент target_time
    let half_w  = panel_w / 2.0;
    let hit_x   = panel_x + half_w;
    let eff_spd = 250.0 * SPEED_MULTIPLIERS[s.speed_index];
    let eff_spd_track = half_w * eff_spd / HIGHWAY_H;

    let mut track_notes: Vec<GameNote> = s.notes_compat.iter()
        .filter(|n| !n.hit && !n.missed)
        .map(|n| {
            let tt = (n.target_time - s.song_time) as f32;
            let mut c = n.clone();
            c.x = hit_x - (tt * eff_spd_track);
            c
        })
        .collect();
    track_notes.retain(|n| n.x >= panel_x - 20.0 && n.x <= panel_x + panel_w + 20.0);

    for ch in group_notes_into_chords(&track_notes) {
        draw_chord_notation(gc, &ch, sc, panel_y + 24.0, panel_h - 32.0, f);
    }

    // Линия попадания и граница окна засчитывания
    draw_line(gc.sx(hit_x), gc.sy(panel_y - 4.0), gc.sx(hit_x), gc.sy(panel_y + panel_h + 4.0),
              gc.s(3.0), Color::new(1.0, 0.3, 0.3, 0.7));
    let tol_x = hit_x - HIT_TOLERANCE * half_w / HIGHWAY_H;
    draw_line(gc.sx(tol_x), gc.sy(panel_y - 4.0), gc.sx(tol_x), gc.sy(panel_y + panel_h + 4.0),
              gc.s(1.5), Color::new(1.0, 0.75, 0.2, 0.55));
}