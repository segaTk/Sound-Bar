// src/localization.rs
use serde::Deserialize;
use std::fs;

#[derive(Deserialize, Debug, Clone)]
pub struct Localization {
    pub title: String,
    pub author_label: String,  // напр. "by SjTk" / "автор: SjTk"
    pub song_label: String,
    pub score_label: String,
    pub speed_label: String,
    pub combo_label: String,
    pub hint_play: String,
    pub hint_mic: String,
    pub hint_record: String,
    pub hint_wave: String,
    pub hint_speed: String,
    pub hint_songs: String,
    pub hint_pause_on: String,
    pub hint_pause_off: String,
    pub song_complete: String,
    pub final_score: String,
    pub restart_space: String,
    pub paused_title: String,
    pub paused_hint: String,
    pub select_mic_title: String,
    pub select_mic_hint: String,
    pub select_mic_cancel: String,
    pub no_devices: String,
    pub select_song_title: String,
    pub select_song_hint: String,
    pub select_song_exit: String,
    pub no_songs_found: String,
    pub create_json: String,
    pub lang_label: String,
    pub stats_title: String,
    pub stats_hits: String,
    pub stats_misses: String,
    pub stats_accuracy: String,
    pub btn_restart: String,
    pub btn_songs: String,
    pub stats_col_note: String,
    pub stats_col_expected: String,
    pub stats_col_detected: String,
    pub stats_col_result: String,
    pub btn_clear_logs: String,
    pub clear_confirm: String,
    pub search_placeholder: String,
    pub search_label: String,
    pub select_instr_title: String,
    pub select_instr_hint: String,
    pub output_select_title: String,
    pub output_select_hint: String,
    pub no_output_devices: String,
    pub exit_confirm_title: String,
    pub exit_yes: String,
    pub exit_no: String,
    // ── Главное меню (новые поля) ──────────────────────────────────────────
    pub main_menu_subtitle: String,
    pub main_menu_lessons: String,
    pub main_menu_practice: String,
    // ── Уроки ─────────────────────────────────────────────────────────────
    pub lessons_back: String,
    pub lessons_step_of: String,
    pub lessons_prev: String,
    pub lessons_next: String,
    pub lessons_done: String,
    pub lessons_empty: String,
    pub lessons_nav_hint: String,
    // ── База (теория) ──────────────────────────────────────────────────────
    pub lessons_theory_btn: String,
    pub theory_empty: String,
    // ── UI-оверлей (язык / тема) ───────────────────────────────────────────
    /// Метка на кнопке переключения языка, когда активен русский ("EN")
    pub lang_switch_label: String,
    /// Метка, когда активен английский ("RU")
    pub lang_switch_label_alt: String,
    /// Метка кнопки выбора темы
    pub theme_btn_label: String,
    pub overlay_back: String,
    // ── Названия тем ──────────────────────────────────────────────────────────
    pub theme_blue:   String,
    pub theme_green:  String,
    pub theme_gold:   String,
    pub theme_purple: String,
    pub theme_sunset: String,
    pub theme_ice:    String,

    pub pregame_title: String,   // "Вы готовы?" / "Are you ready?"
    pub pregame_yes:   String,   // "Да" / "Yes"
    pub pregame_no:    String,   // "Нет" / "No"
    pub countdown_hint: String,  // "Приготовьтесь!" / "Get ready!"
}

impl Localization {
    /// Возвращает локализованное название темы по её ключу.
    pub fn theme_name<'a>(&'a self, key: &'a str) -> &'a str {
        match key {
            "theme_blue"   => &self.theme_blue,
            "theme_green"  => &self.theme_green,
            "theme_gold"   => &self.theme_gold,
            "theme_purple" => &self.theme_purple,
            "theme_sunset" => &self.theme_sunset,
            "theme_ice"    => &self.theme_ice,
            _              => key,
        }
    }

    pub fn load(lang_ru: bool) -> Self {
        let filename = if lang_ru { "ru.toml" } else { "en.toml" };
        let path = format!("locales/{}", filename);
        let content = fs::read_to_string(&path).unwrap_or_else(|_| {
            eprintln!("Warning: Could not load locale '{}', using fallback.", path);
            if lang_ru {
                include_str!("../locales/ru.toml").to_string()
            } else {
                include_str!("../locales/en.toml").to_string()
            }
        });
        toml::from_str(&content).expect("Failed to parse locale file")
    }
}