// src/config.rs
use serde::Deserialize;
use std::fs;

#[derive(Deserialize, Debug, Clone)]
pub struct AppConfig {
    pub display: DisplayConfig,
    pub layout: LayoutConfig,
    pub fretboard: FretboardConfig,
    pub tracks: TracksConfig,
    pub widgets: WidgetsConfig,
    pub gameplay: GameplayConfig,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DisplayConfig {
    pub base_width: f32,
    pub base_height: f32,
}

#[derive(Deserialize, Debug, Clone)]
pub struct LayoutConfig {
    pub left_margin: f32,
    pub right_panel_width: f32,
    pub right_margin: f32,
    pub top_margin: f32,
}

#[derive(Deserialize, Debug, Clone)]
pub struct FretboardConfig {
    pub num_strings: usize,
    pub num_frets: usize,
    pub string_spacing: f32,
    pub fret_spacing_base: f32,
    pub fret_scale: f32,
    pub highway_height: f32,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TracksConfig {
    pub notation_height: f32,
    pub tab_height: f32,
    pub spacing_from_fretboard: f32,
    pub spacing_between_tracks: f32,
    pub track_visible_frets: f32,
}

#[derive(Deserialize, Debug, Clone)]
pub struct WidgetsConfig {
    pub waveform_width: f32,
    pub waveform_height: f32,
    pub freq_graph_width: f32,
    pub freq_graph_height: f32,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GameplayConfig {
    pub note_speed: f32,
    pub hit_tolerance: f32,
    pub hit_tolerance_freq: f32,
}

/// Структура, содержащая все рассчитанные координаты для отрисовки
#[derive(Debug, Clone)]
pub struct CalculatedLayout {
    pub window_w: f32,
    pub window_h: f32,
    pub fretboard_x: f32,
    pub fretboard_w: f32,
    pub fretboard_h: f32,
    pub fret_spacing: f32,
    pub hit_zone_y: f32,
    pub playhead_x: f32,
    
    pub track_base_x: f32,
    pub track_end_x: f32,
    pub notation_track_y: f32,
    pub tab_track_y: f32,
    
    pub waveform_x: f32,
    pub waveform_y: f32,
    pub freq_graph_x: f32,
    pub freq_graph_y: f32,
    
    pub right_panel_x: f32,
}

impl CalculatedLayout {
    /// X-координата центра ячейки лада на грифе (та же формула, что в
    /// draw_fretboard::fret_center_x и draw_string_fret_stars).
    /// fret = 0 → центр зоны открытых струн, fret = N → центр N-й ячейки.
    pub fn fret_center_x(&self, fret: usize) -> f32 {
        self.fretboard_x + fret as f32 * self.fret_spacing + self.fret_spacing / 2.0
    }
}

impl AppConfig {
    pub fn load() -> Self {
        let config_str = fs::read_to_string("config.toml").unwrap_or_else(|_| {
            eprintln!("config.toml not found, using defaults.");
            include_str!("../config.toml").to_string() // Fallback
        });
        toml::from_str(&config_str).expect("Failed to parse config.toml")
    }

    pub fn calculate_layout(&self) -> CalculatedLayout {
        let fret_spacing = self.fretboard.fret_spacing_base * self.fretboard.fret_scale;
        let fretboard_w = fret_spacing * (self.fretboard.num_frets as f32 + 1.0);
        let fretboard_h = self.fretboard.string_spacing * (self.fretboard.num_strings as f32 + 1.0);

        let window_w = self.layout.left_margin + fretboard_w + self.layout.right_panel_width + self.layout.right_margin;
        let window_h = self.display.base_height;

        let fretboard_x = self.layout.left_margin;
        let hit_zone_y = self.layout.top_margin + self.fretboard.highway_height;
        let playhead_x = fretboard_x + self.fretboard.highway_height;

        let track_base_x = fretboard_x;

        let notation_track_y = hit_zone_y + fretboard_h + self.tracks.spacing_from_fretboard;
        let tab_track_y = notation_track_y + self.tracks.notation_height + self.tracks.spacing_between_tracks;

        // Правая панель и виджеты — считаем раньше track_end_x, чтобы привязать
        // правый край нотных станов к правому краю графика FREQ vs TIME.
        let right_panel_x = fretboard_x + fretboard_w + self.layout.right_margin;
        let waveform_x = right_panel_x + (self.layout.right_panel_width - self.widgets.waveform_width) / 2.0;
        let waveform_y = hit_zone_y + 20.0;

        let freq_graph_x = right_panel_x + (self.layout.right_panel_width - self.widgets.freq_graph_width) / 2.0;
        let freq_graph_y = notation_track_y;

        // Станы тянутся от грифа до правого края графика FREQ vs TIME.
        let track_end_x = freq_graph_x - 50.0;

        CalculatedLayout {
            window_w, window_h, fretboard_x, fretboard_w, fretboard_h, fret_spacing,
            hit_zone_y, playhead_x, track_base_x, track_end_x, notation_track_y, tab_track_y,
            waveform_x, waveform_y, freq_graph_x, freq_graph_y, right_panel_x,
        }
    }
}