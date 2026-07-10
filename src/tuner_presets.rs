// src/tuner_presets.rs

use serde::Deserialize;

/// Один гитарный строй: 6 частот (от 6-й толстой к 1-й тонкой)
#[derive(Debug, Clone, Deserialize)]
pub struct GuitarTuning {
    pub name: String,
    pub name_ru: String,
    /// Частоты открытых струн: [6-я (бас), 5, 4, 3, 2, 1-я (тонкая)]
    pub strings: [f32; 6],
    /// Названия нот открытых струн (для UI)
    pub note_names: [String; 6],
}

impl GuitarTuning {
    pub fn new(name: &str, name_ru: &str, strings: [f32; 6], note_names: [&str; 6]) -> Self {
        Self {
            name: name.to_string(),
            name_ru: name_ru.to_string(),
            strings,
            note_names: note_names.map(|s| s.to_string()),
        }
    }

    /// Получить целевую частоту для конкретной струны (0..5, где 0 = 6-я басовая)
    pub fn string_freq(&self, string_idx: usize) -> f32 {
        self.strings.get(string_idx).copied().unwrap_or(82.41)
    }

    pub fn string_note(&self, string_idx: usize) -> &str {
        self.note_names.get(string_idx).map(|s| s.as_str()).unwrap_or("--")
    }
}

/// Все доступные пресеты строев
pub fn all_tunings() -> Vec<GuitarTuning> {
    vec![
        // ── Стандартные ──────────────────────────────────────────────
        GuitarTuning::new("Standard E", "Стандартный (E)",
            [82.41, 110.00, 146.83, 196.00, 246.94, 329.63],
            ["E2", "A2", "D3", "G3", "B3", "E4"]),
        GuitarTuning::new("Half-Step Down (Eb)", "Полтона ниже (Eb)",
            [77.78, 103.83, 138.59, 185.00, 233.08, 311.13],
            ["Eb2", "Ab2", "Db3", "Gb3", "Bb3", "Eb4"]),
        GuitarTuning::new("Whole-Step Down (D)", "Тон ниже (D)",
            [73.42, 98.00, 130.81, 174.61, 220.00, 293.66],
            ["D2", "G2", "C3", "F3", "A3", "D4"]),
        GuitarTuning::new("1.5 Steps Down (C#)", "Полтора тона ниже (C#)",
            [69.30, 92.50, 123.47, 164.81, 207.65, 277.18],
            ["C#2", "F#2", "B2", "E3", "Ab3", "C#4"]),
        GuitarTuning::new("Drop D", "Drop D",
            [73.42, 110.00, 146.83, 196.00, 246.94, 329.63],
            ["D2", "A2", "D3", "G3", "B3", "E4"]),
        GuitarTuning::new("Drop C", "Drop C",
            [65.41, 98.00, 130.81, 174.61, 220.00, 293.66],
            ["C2", "G2", "C3", "F3", "A3", "D4"]),
        GuitarTuning::new("C Standard", "Строй C",
            [65.41, 87.31, 116.54, 155.56, 196.00, 261.63],
            ["C2", "F2", "A#2", "D#3", "G3", "C4"]),
        GuitarTuning::new("B Standard", "Строй B",
            [61.74, 82.41, 110.00, 146.83, 185.00, 246.94],
            ["B1", "E2", "A2", "D3", "F#3", "B3"]),
        GuitarTuning::new("A Standard", "Строй A",
            [55.00, 73.42, 98.00, 130.81, 164.81, 220.00],
            ["A1", "D2", "G2", "C3", "E3", "A3"]),

        // ── Открытые строи ───────────────────────────────────────────
        GuitarTuning::new("Open D", "Открытый D",
            [73.42, 110.00, 146.83, 185.00, 220.00, 293.66],
            ["D2", "A2", "D3", "F#3", "A3", "D4"]),
        GuitarTuning::new("Open G", "Открытый G",
            [73.42, 98.00, 146.83, 196.00, 246.94, 293.66],
            ["D2", "G2", "D3", "G3", "B3", "D4"]),
        GuitarTuning::new("Open E", "Открытый E",
            [82.41, 123.47, 164.81, 207.65, 246.94, 329.63],
            ["E2", "B2", "E3", "G#3", "B3", "E4"]),
        GuitarTuning::new("Open A", "Открытый A",
            [82.41, 110.00, 138.59, 164.81, 220.00, 329.63],
            ["E2", "A2", "C#3", "E3", "A3", "E4"]),
        GuitarTuning::new("Open C", "Открытый C",
            [65.41, 98.00, 130.81, 196.00, 261.63, 329.63],
            ["C2", "G2", "C3", "G3", "C4", "E4"]),

        // ── Альтернативные ───────────────────────────────────────────
        GuitarTuning::new("DADGAD", "DADGAD (кельтский)",
            [73.42, 110.00, 146.83, 196.00, 220.00, 293.66],
            ["D2", "A2", "D3", "G3", "A3", "D4"]),
        GuitarTuning::new("Double Drop D", "Double Drop D",
            [73.42, 110.00, 146.83, 196.00, 246.94, 293.66],
            ["D2", "A2", "D3", "G3", "B3", "D4"]),
        GuitarTuning::new("All Fourths", "Все кварты",
            [82.41, 110.00, 146.83, 196.00, 261.63, 349.23],
            ["E2", "A2", "D3", "G3", "C4", "F4"]),
    ]
}

/// Загрузка строев из TOML (опционально, если есть файл `tuner_presets.toml`)
pub fn load_tunings_from_toml() -> Option<Vec<GuitarTuning>> {
    let path = "tuner_presets.toml";
    let content = std::fs::read_to_string(path).ok()?;
    #[derive(Deserialize)]
    struct Wrapper { tunings: Vec<GuitarTuning> }
    toml::from_str::<Wrapper>(&content).ok().map(|w| w.tunings)
}

/// Возвращает список строев: из TOML если есть, иначе встроенные
pub fn get_tunings() -> Vec<GuitarTuning> {
    load_tunings_from_toml().unwrap_or_else(all_tunings)
}