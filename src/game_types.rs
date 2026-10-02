use crate::config::CalculatedLayout;
// src/game_types.rs
//
// Типы игровых объектов v3: аккорды как единица.
// GameNote оставлен как type alias для совместимости с draw_* функциями,
// которые ещё не обновлены.

use macroquad::prelude::Color;
use crate::{
    parser::{NoteTechnique, SongEvent, ChordNote},
    safe_string_color, get_frequency, freq_to_pitch_step, get_note_name,
    SPEED_MULTIPLIERS, HIT_ZONE_Y, PLAYHEAD_X, NUM_STRINGS,
};


#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Instrument {
    Guitar,
    Piano,
}

// ─── Нота внутри летящего события ────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct GameEventNote {
    pub string_idx: usize,
    pub fret: usize,
    pub midi_note: u8,
    /// Y-координата на грифе (зависит от string_idx).
    pub y: f32,
    /// Частота ноты (Hz).
    pub freq: f32,
    /// Отображаемое имя ноты ("D4", "F#3" …).
    pub note_name: String,
    /// MIDI-шаг для нотной дорожки.
    pub pitch_step: i32,
    /// Цвет струны.
    pub color: Color,
    /// [lesson_mode] true = ученик уже подтвердил эту ноту микрофоном.
    pub confirmed: bool,
}

impl GameEventNote {
    pub fn new(cn: &ChordNote, tuning: &[f32; NUM_STRINGS], layout: &CalculatedLayout, instrument: Instrument) -> Self {
        // Для пианино частота вычисляется напрямую из MIDI-ноты (предполагаем, что в JSON для пианино fret = midi_note)
        // Для гитары используется стандартная формула с тюнингом.
        let freq = get_frequency(tuning, cn.string_idx, cn.fret);

        let st = 12.0 * (freq / 440.0).log2();
        let midi = ((st + 69.0).round() as i32).clamp(21, 108) as u8;

        GameEventNote {
            string_idx: cn.string_idx,
            fret: cn.fret,
            midi_note: midi,
            y: layout.fret_center_x(cn.fret), // сохраняется для совместимости с логикой гитары
            freq,
            note_name: get_note_name(freq),
            pitch_step: freq_to_pitch_step(freq),
            color: if instrument == Instrument::Piano {
                // Радужная раскраска по имени ноты (C=красный, D=оранжевый, ...)
                crate::note_color_from_midi(midi)
            } else {
                safe_string_color(cn.string_idx)
            },
            confirmed: false,
        }
    }
}
// ─── Летящее событие (одна нота или аккорд) ──────────────────────────────────

#[derive(Clone, Debug)]
pub struct GameEvent {
    /// Все ноты события (1 = одиночная, >1 = аккорд).
    pub notes: Vec<GameEventNote>,
    /// X-позиция в игровых координатах (все ноты летят вместе).
    pub y: f32,
    /// Целевое время (сек от начала песни).
    pub target_time: f64,
    pub duration: f64,
    pub sound_duration: f64,
    pub technique: NoteTechnique,
    /// Название аккорда для отображения (если есть).
    pub chord_name: Option<String>,
    /// Событие обработано игроком.
    pub hit: bool,
    /// Событие пропущено.
    pub missed: bool,
}

impl GameEvent {
    pub fn from_song_event(ev: &SongEvent, tuning: &[f32; NUM_STRINGS], current_time: f64, speed_idx: usize, layout: &CalculatedLayout, instrument: Instrument) -> Self {
        let eff_spd = 250.0 * SPEED_MULTIPLIERS[speed_idx];
        let tt = (ev.time - current_time) as f32;
        let y = HIT_ZONE_Y - (tt * eff_spd);

        let notes = ev.notes.iter()
            .map(|cn| GameEventNote::new(cn, tuning, layout, instrument))
            .collect();

        GameEvent {
            notes,
            y,
            target_time: ev.time,
            duration: ev.duration,
            sound_duration: ev.sound_duration,
            technique: ev.technique.clone(),
            chord_name: ev.chord_name.clone(),
            hit: false,
            missed: false,
        }
    }

    /// Обновляет X-позицию на основе текущего времени и скорости.
    pub fn update_position(&mut self, current_time: f64, speed_idx: usize) {
        let eff_spd = 250.0 * SPEED_MULTIPLIERS[speed_idx];
        let tt = (self.target_time - current_time) as f32;
        self.y = HIT_ZONE_Y - (tt * eff_spd);
        // Y каждой ноты фиксирован (зависит только от string_idx)
    }

    /// Все частоты события для звукового движка.
    pub fn freqs(&self) -> Vec<(usize, usize, f32)> {
        self.notes.iter()
            .map(|n| (n.string_idx, n.fret, n.freq))
            .collect()
    }

    /// true = аккорд (более одной ноты).
    pub fn is_chord(&self) -> bool { self.notes.len() > 1 }

    /// Число подтверждённых нот (только в lesson_mode).
    pub fn confirmed_count(&self) -> usize {
        self.notes.iter().filter(|n| n.confirmed).count()
    }

    /// true = все ноты события подтверждены.
    pub fn all_confirmed(&self) -> bool {
        self.notes.iter().all(|n| n.confirmed)
    }

    /// Прогресс подтверждения [0.0, 1.0] для UI.
    pub fn confirm_progress(&self) -> f32 {
        if self.notes.is_empty() { return 1.0; }
        self.confirmed_count() as f32 / self.notes.len() as f32
    }
}

// ─── Совместимость с draw_* функциями ────────────────────────────────────────
// GameNote как проекция одной ноты из GameEvent для старых draw-функций.

#[derive(Clone, Debug)]
pub struct GameNote {
    pub string_idx: usize,
    pub fret: usize,
    pub midi_note: u8,
    pub x: f32,
    pub y: f32,
    pub target_time: f64,
    pub duration: f64,
    pub sound_duration: f64,
    pub hit: bool,
    pub missed: bool,
    pub color: Color,
    pub note_name: String,
    pub pitch_step: i32,
    pub technique: NoteTechnique,
}

impl GameNote {
    /// Создаёт GameNote из GameEvent (берёт первую/единственную ноту).
    /// Используется для совместимости с функциями, ожидающими GameNote.
    pub fn from_event_first(ev: &GameEvent) -> Option<Self> {
        ev.notes.first().map(|n| GameNote {
            string_idx: n.string_idx,
            fret: n.fret,
            midi_note: n.midi_note,
            x: n.y,
            y: ev.y,
            target_time: ev.target_time,
            duration: ev.duration,
            sound_duration: ev.sound_duration,
            hit: ev.hit,
            missed: ev.missed,
            color: n.color,
            note_name: n.note_name.clone(),
            pitch_step: n.pitch_step,
            technique: ev.technique.clone(),
        })
    }
}

// ─── LessonHitResult ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LessonHitResult {
    pub confirmed: usize,
    pub total: usize,
    pub all_done: bool,
    /// Индексы нот, подтверждённых в этом кадре (новые).
    pub newly_confirmed: Vec<usize>,
}

impl LessonHitResult {
    pub fn none() -> Self {
        Self { confirmed: 0, total: 0, all_done: false, newly_confirmed: Vec::new() }
    }
}