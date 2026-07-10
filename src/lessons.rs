// src/lessons.rs

use serde::Deserialize;
use std::fs;
use std::collections::HashMap;

// ─── Macroquad imports ─────────────────────────────────────────────────────
use macroquad::prelude::{
    Color, Font, MouseButton, TextParams,
    draw_line, draw_rectangle, draw_rectangle_lines, draw_text_ex,
    is_mouse_button_pressed, measure_text, mouse_wheel,
};
use macroquad::texture::{Texture2D, draw_texture_ex, DrawTextureParams};
use macroquad::math::Vec2;

// ─── Crate imports ─────────────────────────────────────────────────────────
use crate::graphics::{GraphicsContext, mouse_position_logical};
use crate::theme::ThemeState;
use crate::localization::Localization;

fn default_x()     -> f32 { 0.62 }
fn default_y()     -> f32 { 0.18 }
fn default_w()     -> f32 { 0.30 }
fn default_alpha() -> f32 { 1.0  }

#[derive(Deserialize, Debug, Clone)]
pub struct StepImage {
    pub path: String,
    #[serde(default = "default_x")]
    pub x: f32,
    #[serde(default = "default_y")]
    pub y: f32,
    #[serde(default = "default_w")]
    pub w: f32,
    #[serde(default = "default_alpha")]
    pub alpha: f32,
    #[serde(default)]
    pub z_order: i32,
}

/// Таблица внутри шага урока/теории.
/// В TOML:
///   [[lessons.steps.tables]]
///   headers = ["№", "Нота", "Частота"]
///   align   = ["center", "center", "right"]
///   rows    = [
///       ["1", "A0", "27.50"],
///       ["2", "A#0", "29.14"],
///   ]
#[derive(Deserialize, Debug, Clone)]
pub struct StepTable {
    /// Заголовки колонок
    pub headers: Vec<String>,
    /// Строки данных (каждая строка — вектор ячеек)
    pub rows: Vec<Vec<String>>,
    /// Выравнивание колонок: "left" | "center" | "right"
    /// Если не указано или меньше колонок — по умолчанию "center"
    #[serde(default)]
    pub align: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct StepLink {
    pub label: String,
    pub target: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct LessonStep {
    pub text: String,
    #[serde(default)]
    pub images: Vec<StepImage>,
    #[serde(default)]
    pub links: Vec<StepLink>,
    /// Таблицы внутри шага
    #[serde(default)]
    pub tables: Vec<StepTable>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LessonLinkAction {
    None,
    GoTuner,
    GoPractice,
    PlaySong(String),
    Custom(String),
}

impl LessonLinkAction {
    pub fn from_target(target: &str) -> Self {
        match target {
            "tuner"    => LessonLinkAction::GoTuner,
            "practice" => LessonLinkAction::GoPractice,
            t if t.ends_with(".json") => LessonLinkAction::PlaySong(t.to_string()),
            other => LessonLinkAction::Custom(other.to_string()),
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Lesson {
    pub id: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub steps: Vec<LessonStep>,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct LessonsFile {
    #[serde(default)]
    pub lessons: Vec<Lesson>,
    #[serde(default)]
    pub section_title: Option<String>,
}

impl LessonsFile {
    pub fn load(lang_ru: bool) -> Self {
        let filename = if lang_ru { "ru.toml" } else { "en.toml" };
        let path = format!("lessons/{}", filename);
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Warning: Could not load lessons '{}': {}", path, e);
                return Self::default();
            }
        };
        toml::from_str(&content).unwrap_or_else(|e| {
            eprintln!("Warning: Failed to parse lessons '{}': {}", path, e);
            Self::default()
        })
    }
    pub fn is_empty(&self) -> bool { self.lessons.is_empty() }
}

#[derive(Debug, Clone)]
pub struct LessonsState {
    pub selected_lesson: usize,
    pub open_step: Option<usize>,
    /// Скролл списка уроков (индекс первого видимого урока)
    pub scroll_offset: usize,
    /// Скролл содержимого открытого урока (в пикселях)
    pub content_scroll: f32,
    /// Скролл для таблиц внутри урока
    pub table_scroll: f32,
}

impl LessonsState {
    pub fn new() -> Self {
        Self { 
            selected_lesson: 0, 
            open_step: None, 
            scroll_offset: 0,
            content_scroll: 0.0,
            table_scroll: 0.0,
        }
    }
    pub fn open_lesson(&mut self) { 
        self.open_step = Some(0);
        self.content_scroll = 0.0;
        self.table_scroll = 0.0;
    }
    pub fn next_step(&mut self, lesson: &Lesson) {
        if let Some(s) = self.open_step {
            if s + 1 < lesson.steps.len() { 
                self.open_step = Some(s + 1);
                self.content_scroll = 0.0;
                self.table_scroll = 0.0;
            }
        }
    }
    pub fn prev_step(&mut self) {
        if let Some(s) = self.open_step {
            self.open_step = if s > 0 { Some(s - 1) } else { None };
            self.content_scroll = 0.0;
            self.table_scroll = 0.0;
        }
    }
    pub fn close_lesson(&mut self) { 
        self.open_step = None;
        self.content_scroll = 0.0;
        self.table_scroll = 0.0;
    }
    pub fn is_lesson_open(&self) -> bool { self.open_step.is_some() }
}

impl LessonsFile {
    pub fn load_theory(lang_ru: bool) -> Self {
        let filename = if lang_ru { "ru.toml" } else { "en.toml" };
        let path = format!("theory/{}", filename);
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Warning: Could not load theory '{}': {}", path, e);
                return Self::default();
            }
        };
        toml::from_str(&content).unwrap_or_else(|e| {
            eprintln!("Warning: Failed to parse theory '{}': {}", path, e);
            Self::default()
        })
    }
}

// ─── Отрисовка таблиц ──────────────────────────────────────────────────────

/// Отрисовывает одну таблицу с вертикальным скроллом.
/// Возвращает высоту занятой области.
pub fn draw_step_table(
    gc: &GraphicsContext,
    f: &Font,
    table: &StepTable,
    x: f32,
    y: f32,
    max_width: f32,
    max_height: f32,
    scroll_y: &mut f32,
    theme: &ThemeState,
) -> f32 {
    let th = theme.current();
    let header_h = 28.0_f32;
    let row_h = 24.0_f32;
    let padding = 8.0_f32;
    
    let num_cols = table.headers.len();
    if num_cols == 0 { return 0.0; }
    
    let col_width = max_width / num_cols as f32;
    let total_height = header_h + (table.rows.len() as f32 * row_h);
    
    // Обработка скролла колесом мыши
    let (_, wy) = mouse_wheel();
    let (mx, my) = mouse_position_logical(gc);
    
    let is_hover = mx >= x && mx <= x + max_width && my >= y && my <= y + max_height;
    
    if is_hover {
        *scroll_y -= wy * 20.0;
        *scroll_y = scroll_y.max(0.0);
    }
    
    let max_scroll_y = (total_height - max_height).max(0.0);
    *scroll_y = scroll_y.min(max_scroll_y);
    
    // Фон таблицы
    draw_rectangle(
        gc.sx(x), gc.sy(y),
        gc.sx(max_width), gc.sy(max_height),
        Color::new(0.08, 0.09, 0.12, 0.85)
    );
    
    // Заголовок (фиксированный, не скроллится)
    draw_rectangle(
        gc.sx(x), gc.sy(y),
        gc.sx(max_width), gc.sy(header_h),
        Color::new(th.accent.r * 0.3, th.accent.g * 0.3, th.accent.b * 0.3, 0.95)
    );
    
    // Заголовки колонок
    let header_fs = gc.s(13.0).round() as u16;
    for (i, header) in table.headers.iter().enumerate() {
        let col_x = x + i as f32 * col_width;
        let align = table.align.get(i).map(|s| s.as_str()).unwrap_or("center");
        
        let text_w = measure_text(header, Some(f), header_fs, 1.0).width / gc.scale_x;
        let text_x = match align {
            "left" => col_x + padding,
            "right" => col_x + col_width - text_w - padding,
            _ => col_x + (col_width - text_w) / 2.0,
        };
        
        draw_text_ex(
            header,
            gc.sx(text_x),
            gc.sy(y + header_h / 2.0 + 5.0),
            TextParams {
                font_size: header_fs,
                font: Some(f),
                color: theme.section_title(),
                ..Default::default()
            }
        );
    }
    
    // Разделитель под заголовком
    draw_line(
        gc.sx(x), gc.sy(y + header_h),
        gc.sx(x + max_width), gc.sy(y + header_h),
        gc.s(2.0),
        Color::new(th.accent.r, th.accent.g, th.accent.b, 0.6)
    );
    
    // Строки с учётом скролла
    let content_y = y + header_h;
    let content_h = max_height - header_h;
    
    for (row_idx, row) in table.rows.iter().enumerate() {
        let row_y = content_y + row_idx as f32 * row_h - *scroll_y;
        
        // Пропускаем строки вне видимой области
        if row_y + row_h < content_y || row_y > content_y + content_h {
            continue;
        }
        
        // Фон строки (чередование)
        let bg_alpha = if row_idx % 2 == 0 { 0.0 } else { 0.05 };
        draw_rectangle(
            gc.sx(x), gc.sy(row_y),
            gc.sx(max_width), gc.sy(row_h),
            Color::new(1.0, 1.0, 1.0, bg_alpha)
        );
        
        // Ячейки
        let cell_fs = gc.s(12.0).round() as u16;
        for (col_idx, cell) in row.iter().enumerate() {
            if col_idx >= num_cols { break; }
            
            let col_x = x + col_idx as f32 * col_width;
            let align = table.align.get(col_idx).map(|s| s.as_str()).unwrap_or("center");
            
            let text_w = measure_text(cell, Some(f), cell_fs, 1.0).width / gc.scale_x;
            let text_x = match align {
                "left" => col_x + padding,
                "right" => col_x + col_width - text_w - padding,
                _ => col_x + (col_width - text_w) / 2.0,
            };
            
            draw_text_ex(
                cell,
                gc.sx(text_x),
                gc.sy(row_y + row_h / 2.0 + 4.0),
                TextParams {
                    font_size: cell_fs,
                    font: Some(f),
                    color: theme.panel_text(),
                    ..Default::default()
                }
            );
        }
        
        // Разделитель между строками
        if row_idx < table.rows.len() - 1 {
            draw_line(
                gc.sx(x), gc.sy(row_y + row_h),
                gc.sx(x + max_width), gc.sy(row_y + row_h),
                gc.s(0.5),
                Color::new(0.3, 0.3, 0.4, 0.3)
            );
        }
    }
    
    // Скроллбар таблицы
    if max_scroll_y > 0.0 {
        let sb_w = 6.0_f32;
        let sb_x = x + max_width - sb_w - 2.0;
        let sb_ratio = *scroll_y / max_scroll_y;
        let sb_h = ((content_h / total_height) * content_h).max(20.0);
        let sb_y = content_y + sb_ratio * (content_h - sb_h);
        
        draw_rectangle(
            gc.sx(sb_x), gc.sy(content_y),
            gc.s(sb_w), gc.sy(content_h),
            Color::new(0.12, 0.13, 0.18, 0.7)
        );
        draw_rectangle(
            gc.sx(sb_x + 1.0), gc.sy(sb_y),
            gc.s(sb_w - 2.0), gc.sy(sb_h),
            Color::new(th.accent.r * 0.8, th.accent.g * 0.8, th.accent.b * 0.6, 0.85)
        );
    }
    
    // Рамка таблицы
    draw_rectangle_lines(
        gc.sx(x), gc.sy(y),
        gc.sx(max_width), gc.sy(max_height),
        gc.s(1.5),
        Color::new(th.accent.r, th.accent.g, th.accent.b, 0.4)
    );
    
    max_height
}

/// Вычисляет высоту текстового контента с переносом строк.
pub fn calculate_text_height(
    text: &str,
    max_width: f32,
    f: &Font,
    gc: &GraphicsContext,
    font_size: f32,
) -> f32 {
    let fs_px = gc.s(font_size).round() as u16;
    let line_h = font_size * 1.58;
    let mut total_h = 0.0_f32;
    
    for raw_line in text.split('\n') {
        let mut buf = String::new();
        for word in raw_line.split_whitespace() {
            let test = if buf.is_empty() { word.to_string() } else { format!("{} {}", buf, word) };
            let w = measure_text(&test, Some(f), fs_px, 1.0).width / gc.scale_x;
            
            if w > max_width && !buf.is_empty() {
                total_h += line_h;
                buf = word.to_string();
            } else {
                buf = test;
            }
        }
        if !buf.is_empty() {
            total_h += line_h;
        }
    }
    
    total_h
}

/// Отрисовывает текст с переносом строк, применяя смещение скролла.
/// Возвращает высоту занятой области.
pub fn draw_text_with_scroll(
    gc: &GraphicsContext,
    f: &Font,
    text: &str,
    x: f32,
    y: f32,
    max_width: f32,
    font_size: f32,
    color: Color,
    scroll_offset: f32,
    content_top: f32,
    content_h: f32,
) -> f32 {
    let fs_px = gc.s(font_size).round() as u16;
    let line_h = font_size * 1.58;
    let mut cur_y = y - scroll_offset;
    let mut total_h = 0.0_f32;
    
    for raw_line in text.split('\n') {
        let mut buf = String::new();
        for word in raw_line.split_whitespace() {
            let test = if buf.is_empty() { word.to_string() } else { format!("{} {}", buf, word) };
            let w = measure_text(&test, Some(f), fs_px, 1.0).width / gc.scale_x;
            
            if w > max_width && !buf.is_empty() {
                // Проверяем видимость
                if cur_y + line_h >= content_top && cur_y <= content_top + content_h {
                    draw_text_ex(&buf, gc.sx(x), gc.sy(cur_y),
                        TextParams { font_size: fs_px, font: Some(f), color, ..Default::default() });
                }
                cur_y += line_h;
                total_h += line_h;
                buf = word.to_string();
            } else {
                buf = test;
            }
        }
        if !buf.is_empty() {
            if cur_y + line_h >= content_top && cur_y <= content_top + content_h {
                draw_text_ex(&buf, gc.sx(x), gc.sy(cur_y),
                    TextParams { font_size: fs_px, font: Some(f), color, ..Default::default() });
            }
            cur_y += line_h;
            total_h += line_h;
        }
    }
    
    total_h
}