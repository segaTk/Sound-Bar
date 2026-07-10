// src/lesson_game.rs
//
// Игровая логика режима урока (lesson_mode = true).
//
// В обычном режиме (Practice) время идёт непрерывно и ноты летят по таймеру.
// В режиме урока:
//   1. Аккорд летит к хит-зоне как обычно.
//   2. Достигнув хит-зоны, время замирает (lesson_paused = true).
//   3. Автоматически проигрывается звук аккорда, чтобы ученик слышал цель.
//   4. Детектор микрофона подтверждает ноты по одной (буфер confirmed[]).
//   5. Когда все ноты подтверждены → пауза снимается, игра продолжается.
//   6. При зависании (>HINT_DELAY кадров без прогресса) показывается подсказка.

use std::time::Instant;
use crate::{
    game_types::{GameEvent, LessonHitResult},
    parser::NoteTechnique,
    get_frequency, correct_octave_error, get_freq_tolerance, is_valid_guitar_freq,
    NUM_STRINGS, HIT_ZONE_Y, HIT_TOLERANCE,
};

// ─── Константы ───────────────────────────────────────────────────────────────

/// Расстояние от хит-зоны (в пикселях) при котором событие замораживается.
const LESSON_FREEZE_WINDOW: f32 = 30.0;

/// Число кадров без прогресса до показа подсказки (при 60fps ≈ 3 сек).
const HINT_DELAY_FRAMES: u32 = 180;

/// Максимальная длина истории попыток (для отображения статистики).
const ATTEMPT_HISTORY_LEN: usize = 64;

// ─── Состояние урочного режима ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LessonState {
    /// Сейчас ждём подтверждения от ученика.
    pub paused: bool,
    /// Индекс текущего ожидаемого события в GameState.events.
    pub current_event_idx: Option<usize>,
    /// Число кадров с момента последнего прогресса (для таймера подсказок).
    pub stall_frames: u32,
    /// Показывать ли подсказку (диаграмму аккорда).
    pub show_hint: bool,
    /// Число успешно завершённых событий.
    pub events_completed: u32,
    /// Суммарное число попыток (включая провальные кадры).
    pub total_attempts: u32,
    /// История: true = успех, false = промах (для статистики урока).
    pub attempt_history: Vec<bool>,
    /// Время начала текущей паузы (для таймаута).
    pub pause_start: Option<Instant>,
    /// Таймаут ожидания в секундах (0 = бесконечно).
    pub timeout_secs: f64,
    /// Звук подтверждения уже запланирован для текущей паузы.
    pub confirmation_queued: bool,
    /// Аккорд уже озвучен при заморозке.
    pub preview_played: bool,
}

impl LessonState {
    pub fn new() -> Self {
        Self {
            paused: false,
            current_event_idx: None,
            stall_frames: 0,
            show_hint: false,
            events_completed: 0,
            total_attempts: 0,
            attempt_history: Vec::new(),
            pause_start: None,
            timeout_secs: 0.0,
            confirmation_queued: false,
            preview_played: false,
        }
    }

    /// Замораживает игру на событии с данным индексом.
    pub fn freeze(&mut self, event_idx: usize) {
        self.paused = true;
        self.current_event_idx = Some(event_idx);
        self.stall_frames = 0;
        self.show_hint = false;
        self.pause_start = Some(Instant::now());
        self.confirmation_queued = false;
        self.preview_played = false;
    }

    /// Снимает заморозку после успешного подтверждения.
    pub fn unfreeze(&mut self) {
        self.paused = false;
        self.current_event_idx = None;
        self.stall_frames = 0;
        self.show_hint = false;
        self.events_completed += 1;
        self.pause_start = None;
        self.confirmation_queued = false;
    }

    /// Обновляет счётчик зависания; если превышен — показывает подсказку.
    pub fn tick_stall(&mut self, made_progress: bool) {
        if made_progress {
            self.stall_frames = 0;
            self.show_hint = false;
        } else {
            self.stall_frames += 1;
            if self.stall_frames >= HINT_DELAY_FRAMES {
                self.show_hint = true;
            }
        }
    }

    /// Записывает результат попытки в историю.
    pub fn record_attempt(&mut self, success: bool) {
        self.total_attempts += 1;
        self.attempt_history.push(success);
        if self.attempt_history.len() > ATTEMPT_HISTORY_LEN {
            self.attempt_history.remove(0);
        }
    }

    /// Точность за последние ATTEMPT_HISTORY_LEN попыток [0, 1].
    pub fn recent_accuracy(&self) -> f32 {
        if self.attempt_history.is_empty() { return 0.0; }
        let hits = self.attempt_history.iter().filter(|&&b| b).count();
        hits as f32 / self.attempt_history.len() as f32
    }

    /// Проверяет таймаут. Возвращает true если нужно пропустить событие.
    pub fn is_timed_out(&self) -> bool {
        if self.timeout_secs <= 0.0 { return false; }
        self.pause_start.map_or(false, |t| {
            t.elapsed().as_secs_f64() > self.timeout_secs
        })
    }
}

// ─── Детектирование попадания в аккорд ───────────────────────────────────────

/// Проверяет, совпадает ли обнаруженная частота с одной из нот аккорда.
///
/// Возвращает LessonHitResult с:
/// - newly_confirmed: индексы нот, подтверждённых в этом кадре.
/// - Обновляет confirmed-флаги в event.notes напрямую.
pub fn check_lesson_hit(
    event: &mut GameEvent,
    detected_freq: Option<f32>,
    tuning: &[f32; NUM_STRINGS],
) -> LessonHitResult {
    let freq = match detected_freq {
        Some(f) if is_valid_guitar_freq(f) => f,
        _ => return LessonHitResult {
            confirmed: event.confirmed_count(),
            total: event.notes.len(),
            all_done: event.all_confirmed(),
            newly_confirmed: Vec::new(),
        },
    };

    let mut newly = Vec::new();

    for (i, note) in event.notes.iter_mut().enumerate() {
        if note.confirmed { continue; }

        let corrected = correct_octave_error(freq, note.freq);
        let tolerance = get_freq_tolerance(note.freq);

        if (corrected - note.freq).abs() < tolerance {
            note.confirmed = true;
            newly.push(i);
        }
    }

    let confirmed = event.confirmed_count();
    let total = event.notes.len();

    LessonHitResult {
        confirmed,
        total,
        all_done: confirmed == total,
        newly_confirmed: newly,
    }
}

// ─── Определение момента заморозки ───────────────────────────────────────────

/// Возвращает true если событие достигло зоны заморозки и должно остановить время.
pub fn should_freeze(event: &GameEvent) -> bool {
    (event.y - crate::HIT_ZONE_Y).abs() < LESSON_FREEZE_WINDOW
        && !event.hit
        && !event.missed
}

// ─── Подсказки ───────────────────────────────────────────────────────────────

/// Данные для отрисовки подсказки: какие ноты подтверждены, какие нет.
#[derive(Debug, Clone)]
pub struct HintData {
    pub notes: Vec<HintNote>,
    pub chord_name: Option<String>,
    pub progress: f32,
}

#[derive(Debug, Clone)]
pub struct HintNote {
    pub string_idx: usize,
    pub fret: usize,
    pub note_name: String,
    pub confirmed: bool,
    pub freq: f32,
}

impl HintData {
    pub fn from_event(event: &GameEvent) -> Self {
        let notes = event.notes.iter().map(|n| HintNote {
            string_idx: n.string_idx,
            fret: n.fret,
            note_name: n.note_name.clone(),
            confirmed: n.confirmed,
            freq: n.freq,
        }).collect();

        HintData {
            notes,
            chord_name: event.chord_name.clone(),
            progress: event.confirm_progress(),
        }
    }
}
