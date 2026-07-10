// src/info_screen.rs
//
// Формат locales/info_ru.toml:
//
//   title = "О приложении"
//   show_on_start_label = "Больше не показывать"   # подпись флага
//
//   [[sections]]
//   heading = "Как играть"
//   body    = "Текст раздела"
//
//   # Картинки — опционально
//   [[images]]
//   path  = "locales/img/info_banner.png"  # путь относительно exe
//   x     = 0.60   # позиция X как доля ширины панели (0.0–1.0)
//   y     = 0.12   # позиция Y как доля высоты панели (0.0–1.0)
//   w     = 0.35   # ширина как доля ширины панели
//   alpha = 1.0

use serde::Deserialize;
use std::fs;

// ─── Картинка внутри Info-экрана ─────────────────────────────────────────────

#[derive(Deserialize, Debug, Clone)]
pub struct InfoImage {
    pub path:  String,
    #[serde(default = "default_zero")]
    pub x:     f32,
    #[serde(default = "default_zero")]
    pub y:     f32,
    #[serde(default = "default_half")]
    pub w:     f32,
    #[serde(default = "default_one")]
    pub alpha: f32,
    #[serde(default)]
    pub z_order: i32,
}

fn default_zero() -> f32 { 0.0 }
fn default_half() -> f32 { 0.5 }
fn default_one()  -> f32 { 1.0 }

// ─── Секция текста ────────────────────────────────────────────────────────────

#[derive(Deserialize, Debug, Clone, Default)]
pub struct InfoSection {
    pub heading: String,
    pub body:    String,
}

// ─── Весь файл info_*.toml ────────────────────────────────────────────────────

#[derive(Deserialize, Debug, Clone, Default)]
pub struct InfoFile {
    pub title: String,
    /// Подпись флага «Больше не показывать» (берётся из TOML-файла)
    #[serde(default = "default_no_show_label")]
    pub show_on_start_label: String,
    #[serde(default)]
    pub sections: Vec<InfoSection>,
    #[serde(default)]
    pub images:   Vec<InfoImage>,
}

fn default_no_show_label() -> String {
    "Больше не показывать".to_string()
}

impl InfoFile {
    pub fn load(lang_ru: bool) -> Self {
        let path = if lang_ru {
            "locales/info_ru.toml"
        } else {
            "locales/info_en.toml"
        };
        match fs::read_to_string(path) {
            Ok(raw) => toml::from_str(&raw).unwrap_or_else(|e| {
                eprintln!("Info parse error ({}): {}", path, e);
                Self::placeholder(lang_ru)
            }),
            Err(_) => Self::placeholder(lang_ru),
        }
    }

    fn placeholder(lang_ru: bool) -> Self {
        if lang_ru {
            InfoFile {
                title:               "О приложении".to_string(),
                show_on_start_label: "Больше не показывать".to_string(),
                sections: vec![InfoSection {
                    heading: "Файл не найден".to_string(),
                    body:    "Создайте locales/info_ru.toml".to_string(),
                }],
                images: vec![],
            }
        } else {
            InfoFile {
                title:               "About".to_string(),
                show_on_start_label: "Don't show again".to_string(),
                sections: vec![InfoSection {
                    heading: "File not found".to_string(),
                    body:    "Create locales/info_en.toml".to_string(),
                }],
                images: vec![],
            }
        }
    }

    pub fn is_empty(&self) -> bool { self.sections.is_empty() }
}

// ─── Состояние экрана INFO ────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct InfoState {
    pub scroll_offset: f32,
}

impl InfoState {
    pub fn new() -> Self { Self { scroll_offset: 0.0 } }
}

// ─── Постоянные настройки (файл locales/info_prefs.toml) ─────────────────────
//
// Хранит: show_on_start = true/false
//
// Файл создаётся рядом с exe автоматически.

const PREFS_PATH: &str = "locales/info_prefs.toml";

#[derive(Deserialize, Debug, Clone)]
pub struct InfoPrefs {
    /// true = показывать INFO при каждом запуске (по умолчанию)
    pub show_on_start: bool,
}

impl Default for InfoPrefs {
    fn default() -> Self { Self { show_on_start: true } }
}

impl InfoPrefs {
    pub fn load() -> Self {
        fs::read_to_string(PREFS_PATH)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let content = format!(
            "# Настройки экрана INFO\n# true = показывать при запуске\nshow_on_start = {}\n",
            self.show_on_start
        );
        if let Err(e) = fs::write(PREFS_PATH, content) {
            eprintln!("Cannot save info prefs: {}", e);
        }
    }

    pub fn set_show_on_start(&mut self, value: bool) {
        self.show_on_start = value;
        self.save();
    }
}