// src/main.rs
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod parser;
mod synth;
mod audio_engine;
mod graphics;
mod config;
mod localization;
mod lessons;
mod theme;
mod info_screen;
mod game_types;
mod lesson_game;
mod tuner_presets;
mod midi_parser;
mod midi_input;
use midi_input::MidiInputController;

use std::thread;
use std::sync::mpsc;
use std::path::PathBuf;
use std::fs;
use std::collections::{HashMap, VecDeque};
use std::time::{Instant, Duration};
use std::sync::{Arc, Mutex};

use macroquad::prelude::*;
use macroquad::window::request_new_screen_size;
use macroquad::window::set_fullscreen;
use macroquad::rand::gen_range;
use chrono::Local;
use cpal::{traits::{DeviceTrait, HostTrait, StreamTrait}, SampleFormat, Device, Stream};
use std::io::Write;
use std::path::Path;
use rustfft::{FftPlanner, num_complex::Complex};

use audio_engine::{OutputController, PlayRequest, NoteRequest, WaveType};
use config::AppConfig;
use graphics::{GraphicsContext, ExitDialogResult, MainMenuChoice, LessonsPanelResult};
use localization::Localization;
use lessons::{LessonsFile, LessonsState};
use info_screen::{InfoFile, InfoState, InfoPrefs};
use theme::ThemeState;
use game_types::{GameEvent, GameEventNote, GameNote, Instrument};
use lesson_game::{LessonState, HintData, check_lesson_hit, should_freeze};
use parser::{SongEvent, SongNote, ChordNote, NoteTechnique};
use tuner_presets::{get_tunings, all_tunings, GuitarTuning}; 

// ─── КОНСТАНТЫ ────────────────────────────────────────────────────────────────

pub const NOTE_SPEED: f32     = 250.0;
pub const HIGHWAY_H: f32      = 350.0;
pub const HIT_ZONE_Y: f32     = 400.0;
pub const PLAYHEAD_X: f32     = 150.0 + HIGHWAY_H;
pub const HIT_TOLERANCE: f32  = 80.0;
pub const HIT_TOLERANCE_FREQ: f32 = 40.0;
pub const WINDOW_H: f32       = 1000.0;
pub const NOTATION_TRACK_Y: f32 = HIT_ZONE_Y + 240.0 + 60.0;
pub const TAB_TRACK_Y: f32    = NOTATION_TRACK_Y + 100.0 + 15.0;
pub const TAB_TRACK_H: f32    = 80.0;
pub const FRETBOARD_X: f32    = 150.0;
pub const FRET_SPACING: f32   = 70.0 * 0.66;
pub const NOISE_GATE_THRESHOLD: f32 = 0.0003;
pub const TARGET_VOLUME: f32  = 0.5;
pub const MIC_SENSITIVITY: f32 = 2.8;
pub const NUM_STRINGS: usize  = 6;
pub const TOTAL_FRETS: usize  = 20;
pub const OPEN_STRINGS_FREQ: [f32; NUM_STRINGS] = [82.41, 110.00, 146.83, 196.00, 246.94, 329.63];
pub const STRING_NAMES: [&str; NUM_STRINGS]  = ["E2", "A2", "D3", "G3", "B3", "E4"];
pub const STRING_COLORS: [Color; NUM_STRINGS] = [
    Color::new(0.8, 0.2, 0.2, 1.0), Color::new(0.9, 0.5, 0.1, 1.0),
    Color::new(0.9, 0.9, 0.1, 1.0), Color::new(0.2, 0.9, 0.2, 1.0),
    Color::new(0.1, 0.5, 0.9, 1.0), Color::new(0.5, 0.1, 0.9, 1.0),
];
pub const NOTE_NAMES: [&str; 12]  = ["C","C#","D","D#","E","F","F#","G","G#","A","A#","B"];
pub const FONT_PATH: &str         = "fonts/Roboto-Regular.ttf";
pub const SPEED_MULTIPLIERS: [f32; 4] = [0.25, 0.5, 1.0, 2.0];
pub const SPEED_LABELS: [&str; 4]     = ["0.25x", "0.5x", "1x", "2x"];
pub const CHORD_X_TOLERANCE: f32  = 5.0;
pub const NOTE_HEAD_RADIUS: f32   = 5.0;

#[inline] pub fn safe_string_color(idx: usize) -> Color {
    *STRING_COLORS.get(idx).unwrap_or(&STRING_COLORS[0])
}
#[inline] pub fn safe_string_name(idx: usize) -> &'static str {
    STRING_NAMES.get(idx).unwrap_or(&STRING_NAMES[0])
}
#[inline] pub fn safe_open_freq(idx: usize) -> f32 {
    *OPEN_STRINGS_FREQ.get(idx).unwrap_or(&OPEN_STRINGS_FREQ[0])
}
#[inline] pub fn freq_to_pitch_step(freq: f32) -> i32 {
    (12.0 * (freq / 440.0).log2()).round() as i32
}

pub fn is_valid_guitar_freq(f: f32) -> bool { (70.0..=700.0).contains(&f) }

pub fn correct_octave_error(det: f32, exp: f32) -> f32 {
    let mut c = det;
    while c < exp * 0.75 && c * 2.0 <= exp * 1.25 { c *= 2.0; }
    while c > exp * 1.25 && c / 2.0 >= exp * 0.75 { c /= 2.0; }
    c
}

pub fn get_freq_tolerance(exp: f32) -> f32 {
    if exp < 110.0 { 15.0 } else { (exp * 0.05).max(8.0) }
}

pub fn get_frequency(tuning: &[f32; NUM_STRINGS], si: usize, f: usize) -> f32 {
    let base = tuning.get(si.min(NUM_STRINGS - 1)).unwrap_or(&OPEN_STRINGS_FREQ[0]);
    base * 2.0_f32.powf(f as f32 / 12.0)
}

pub fn get_note_name(freq: f32) -> String {
    let st  = 12.0 * (freq / 440.0).log2();
    let idx = ((st + 69.0).round() as i32) % 12;
    let name = NOTE_NAMES[((idx + 12) % 12) as usize];
    let oct = ((st + 69.0) / 12.0).floor() as i32;
    format!("{}{}", name, oct)
}

/// Возвращает цвет для ноты по MIDI-номеру (радужная схема "rainbow notes").
/// C=красный, D=оранжевый, E=жёлтый, F=зелёный, G=голубой, A=синий, B=фиолетовый.
/// Для диезов/бемолов — промежуточный оттенок между соседними нотами.
pub fn note_color_from_midi(midi_note: u8) -> Color {
    let note_in_octave = midi_note % 12;
    match note_in_octave {
        0  => Color::new(0.95, 0.20, 0.20, 1.0),  // C  = красный
        1  => Color::new(1.00, 0.55, 0.10, 1.0),  // C# = красно-оранжевый
        2  => Color::new(1.00, 0.65, 0.15, 1.0),  // D  = оранжевый
        3  => Color::new(1.00, 0.85, 0.20, 1.0),  // D# = жёлто-оранжевый
        4  => Color::new(1.00, 0.95, 0.25, 1.0),  // E  = жёлтый
        5  => Color::new(0.30, 0.85, 0.30, 1.0),  // F  = зелёный
        6  => Color::new(0.20, 0.75, 0.60, 1.0),  // F# = сине-зелёный
        7  => Color::new(0.30, 0.80, 0.95, 1.0),  // G  = голубой
        8  => Color::new(0.25, 0.55, 0.95, 1.0),  // G# = сине-голубой
        9  => Color::new(0.20, 0.30, 0.95, 1.0),  // A  = синий
        10 => Color::new(0.55, 0.30, 0.90, 1.0),  // A# = сине-фиолетовый
        11 => Color::new(0.70, 0.30, 0.95, 1.0),  // B  = фиолетовый
        _  => Color::new(0.80, 0.80, 0.80, 1.0),
    }
}

// Для draw_notation_track / draw_tab_track (принимают &[GameNote])
pub fn group_notes_into_chords(notes: &[GameNote]) -> Vec<Vec<&GameNote>> {
    let mut sorted: Vec<&GameNote> = notes.iter().filter(|n| !n.hit && !n.missed).collect();
    sorted.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
    let mut chords = Vec::new();
    let mut i = 0;
    while i < sorted.len() {
        let bx = sorted[i].x;
        let mut ch = vec![sorted[i]];
        let mut j = i + 1;
        while j < sorted.len() && (sorted[j].x - bx).abs() < CHORD_X_TOLERANCE {
            ch.push(sorted[j]); j += 1;
        }
        chords.push(ch); i = j;
    }
    chords
}

// ─── СТРУКТУРЫ ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Song {
    pub name: String,
    /// Основной список: аккорды сгруппированы.
    pub events: Vec<SongEvent>,
    /// Плоский список нот — для совместимости с draw_notation/tab_track.
    pub notes: Vec<SongNote>,
    pub tuning: [f32; NUM_STRINGS],
    pub time_signature: (u8, u8),
    pub initial_tempo: f32,
    pub tempo_map: Vec<parser::TempoEvent>,
    pub sections: Vec<parser::Section>,
    /// true → запускать в режиме урока (пауза на хит-зоне).
    pub lesson_mode: bool,
}

#[derive(Debug, Clone)]
pub struct TimeEvent {
    pub timestamp: f64,
    pub freq: Option<f32>,
    pub expected_freq: Option<f32>,
    pub width: f32,
    pub is_target: bool,
    pub string_idx: usize,
    pub hit_result: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct NoteLogEntry {
    pub note_name: String,
    pub expected_freq: f32,
    pub detected_freq: Option<f32>,
    pub is_hit: bool,
}

#[derive(Debug, Clone)]
pub struct GameLogger {
    pub events: VecDeque<TimeEvent>,
    pub max_history_seconds: f64,
    pub last_detected_freq: Option<f32>,
    pub log_file_path: String,
    pub note_history: Vec<NoteLogEntry>,
}

pub struct Particle {
    pub x: f32, pub y: f32,
    pub vx: f32, pub vy: f32,
    pub life: f32, pub color: Color, pub size: f32,
}

#[derive(Debug, Clone)]
pub struct SongStats {
    pub hits: u32, pub misses: u32,
    pub accuracy: f32,
    pub details: Vec<NoteLogEntry>,
}

pub struct FastPitchDetector {
    fft_fwd: Arc<dyn rustfft::Fft<f32>>,
    fft_inv: Arc<dyn rustfft::Fft<f32>>,
    buffer_size: usize,
    time_buf: Vec<f32>,
    complex_buf: Vec<Complex<f32>>,
    autocorr: Vec<f32>,
    energy: Vec<f32>,
}

pub struct AudioData {
    pub detected_freq: Option<f32>,
    pub last_detection_time: f64,
    pub wave_buffer: Vec<f32>,
    pub current_volume: f32,
    pub is_stream_active: bool,
    pub error_msg: Option<String>,
    pub is_recording: bool,
    pub recording_buffer: Vec<i16>,
    pub sample_rate: u32,
    pub channels: u16,
    pub current_device_name: String,
    pub pitch_detector: FastPitchDetector,
}

pub struct MicController {
    pub audio_data: Arc<Mutex<AudioData>>,
    pub current_stream: Option<Stream>,
    pub host: cpal::Host,
    pub available_devices: Vec<Device>,
}

/// Фаза перед началом игры: подтверждение готовности → обратный отсчёт → игра.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PreStartPhase {
    Confirm,
    Countdown(f32), // секунд осталось
    Playing,
}

pub struct GameState {
    pub instrument: Instrument,
    /// Летящие события (каждое может быть аккордом).
    pub events: Vec<GameEvent>,
    /// Плоский список для draw_notation_track / draw_tab_track.
    pub notes_compat: Vec<GameNote>,
    pub pre_start: PreStartPhase,
    pub particles: Vec<Particle>,
    pub score: u32,
    pub combo: u32,
    pub max_combo: u32,
    /// Исходные данные песни — события (аккорды).
    pub song_data: Vec<SongEvent>,
    pub song_time: f64,
    pub last_spawn_idx: usize,
    pub game_over: bool,
    pub paused: bool,
    pub speed_index: usize,
    pub wave_type: WaveType,
    pub flash_timer: f32,
    pub flash_color: Color,
    pub tuning: [f32; NUM_STRINGS],
    pub output: Arc<Mutex<OutputController>>,
    pub sound_cache: HashMap<(usize, usize), Vec<f32>>,
    pub audio_data: Arc<Mutex<AudioData>>,
    pub last_hit_time: f64,
    pub current_playing_freq: Option<f32>,
    pub message: Option<(String, f32)>,
    pub active_hints: [Option<Color>; TOTAL_FRETS + 1],
    pub auto_play_mode: bool,
    pub current_song_name: String,
    pub locale: Localization,
    pub logger: GameLogger,
    pub lang_ru: bool,
    pub last_valid_freq: Option<f32>,
    pub stats_scroll_offset: usize,
    pub manual_track_offset: f32,
    pub is_track_dragging: bool,
    pub drag_start_x: f32,
    pub drag_start_offset: f32,
    pub time_signature: (u8, u8),
    pub initial_tempo: f32,
    pub audio_stream_offset: f64,
    pub auto_play_scheduled: bool,
    /// Состояние урочного режима.
    pub lesson: LessonState,
    /// true = песня запущена в режиме урока.
    pub lesson_mode: bool,
    /// Текущая подсказка (обновляется lesson_tick).
    pub lesson_hint: Option<HintData>,
    pub layout: config::CalculatedLayout,
    pub midi_data: Option<Arc<Mutex<midi_input::MidiData>>>,
}

// ─── Вспомогательные типы игровой логики ─────────────────────────────────────

pub struct HitCandidate {
    pub event_idx: usize,
    pub note_idx: usize,
    pub detected_freq: f32,
    pub expected_freq: f32,
}

pub struct LessonTickResult {
    pub play_preview: Option<Vec<(usize, usize, f32)>>,
    pub play_confirmation: bool,
    pub hint: Option<HintData>,
    pub newly_confirmed: Vec<(usize, f32)>,
    pub unfreeze: bool,
    pub timeout: bool,
}

// ─── Поисковое состояние ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SearchState {
    pub active: bool,
    pub text: String,
    pub cursor_blink: f32,
}
impl SearchState {
    pub fn new() -> Self { Self { active: false, text: String::new(), cursor_blink: 0. } }
    pub fn update(&mut self, dt: f32) {
        self.cursor_blink += dt;
        if self.cursor_blink > 0.5 { self.cursor_blink = 0.; }
    }
    pub fn add_char(&mut self, c: char) { if !c.is_control() { self.text.push(c); } }
    pub fn backspace(&mut self) { self.text.pop(); }
    pub fn clear(&mut self) { self.text.clear(); }
    pub fn is_empty(&self) -> bool { self.text.is_empty() }
}

pub fn filter_and_sort_songs(songs: &[Song], query: &str) -> Vec<(usize, bool)> {
    if query.is_empty() {
        return songs.iter().enumerate().map(|(i, _)| (i, false)).collect();
    }
    let q = query.to_lowercase();
    let mut res: Vec<(usize, bool, i32)> = songs.iter().enumerate()
        .filter_map(|(i, s)| {
            let nl = s.name.to_lowercase();
            if nl.contains(&q) {
                Some((i, true, if nl.starts_with(&q) { 1 } else { 0 }))
            } else { None }
        }).collect();
    res.sort_by(|a, b| b.2.cmp(&a.2)
        .then(songs[a.0].name.to_lowercase().cmp(&songs[b.0].name.to_lowercase())));
    res.into_iter().map(|(i, _, _)| (i, true)).collect()
}

// ─── IMPL FastPitchDetector ───────────────────────────────────────────────────

impl FastPitchDetector {
    pub fn new() -> Self {
        let buffer_size = 2048usize;
        let mut planner = FftPlanner::new();
        Self {
            fft_fwd: planner.plan_fft_forward(buffer_size),
            fft_inv: planner.plan_fft_inverse(buffer_size),
            buffer_size,
            time_buf: vec![0.0; buffer_size],
            complex_buf: vec![Complex::new(0.0, 0.0); buffer_size],
            autocorr: vec![0.0; buffer_size],
            energy: vec![0.0; buffer_size + 1],
        }
    }
    pub fn detect_pitch(&mut self, samples: &[f32], sample_rate: u32) -> Option<f32> {
        let n = samples.len().min(self.buffer_size);
        self.time_buf[..n].copy_from_slice(&samples[..n]);
        self.time_buf[n..].fill(0.0);
        let mut sum = 0.0;
        self.energy[0] = 0.0;
        for i in 0..n { sum += self.time_buf[i]; self.energy[i+1] = self.energy[i] + self.time_buf[i]*self.time_buf[i]; }
        let mean = sum / n as f32;
        for x in &mut self.time_buf[..n] { *x -= mean; }
        self.energy[0] = 0.0;
        for i in 0..n { self.energy[i+1] = self.energy[i] + self.time_buf[i]*self.time_buf[i]; }
        for i in 0..self.buffer_size { self.complex_buf[i] = Complex::new(self.time_buf[i], 0.0); }
        self.fft_fwd.process(&mut self.complex_buf);
        for c in &mut self.complex_buf { *c = Complex::new(c.norm_sqr(), 0.0); }
        self.fft_inv.process(&mut self.complex_buf);
        let scale = 1.0 / self.buffer_size as f32;
        for i in 0..self.buffer_size { self.autocorr[i] = self.complex_buf[i].re * scale; }
        let min_tau = (sample_rate as f32 / 1200.0) as usize;
        let max_tau = (sample_rate as f32 / 50.0) as usize;
        let eff_max = max_tau.min(n / 2);
        if eff_max <= min_tau || self.autocorr[0] < 1e-6 { return None; }
        let cutoff = 0.55;
        let mut best_tau = 0; let mut best_val = -1.0f32;
        for tau in min_tau..eff_max {
            let ac = self.autocorr[tau];
            let e1 = self.energy[n - tau];
            let e2 = self.energy[n] - self.energy[tau];
            let m = 0.5 * (e1 + e2);
            if m < 1e-6 { continue; }
            let val = ac / m;
            if val > cutoff && val > best_val { best_val = val; best_tau = tau; }
        }
        if best_val < cutoff { return None; }
        let t = best_tau as f32;
        let get_val = |tau: usize| -> f32 {
            if tau == 0 || tau >= n { return best_val; }
            let ac = self.autocorr[tau];
            let e1 = self.energy[n - tau]; let e2 = self.energy[n] - self.energy[tau];
            let m = 0.5*(e1+e2); if m < 1e-6 { return best_val; } ac/m
        };
        let (x1, x2, x3) = (get_val(best_tau-1), best_val, get_val(best_tau+1));
        let denom = x1 - 2.0*x2 + x3;
        let refined = if denom.abs() > 1e-6 { t + 0.5*(x1-x3)/denom } else { t };
        if refined <= 0.0 { return None; }
        Some(sample_rate as f32 / refined)
    }
}

// ─── IMPL GameLogger ──────────────────────────────────────────────────────────

impl GameLogger {
    fn new(song_name: &str) -> Self {
        let now = Local::now();
        let safe = song_name.chars()
            .map(|c| if c.is_alphanumeric() || c=='-' || c=='_' { c } else { '_' })
            .collect::<String>();
        let path = format!("logs/{}_{}.csv", now.format("%Y.%m.%d_%H.%M.%S"), safe);
        if fs::create_dir_all("logs").is_ok() {
            let _ = fs::write(&path, "Time,Type,StringIdx,ExpectedFreq,DetectedFreq,HitResult\n");
        }
        Self { events: VecDeque::new(), max_history_seconds: 5.0,
            last_detected_freq: None, log_file_path: path, note_history: Vec::new() }
    }
    fn add_target_note(&mut self, time: f64, freq: f32, tolerance: f32, string_idx: usize) {
        if self.events.iter().any(|e| e.is_target && e.string_idx == string_idx && (e.timestamp - time).abs() < 0.01) { return; }
        self.events.push_back(TimeEvent {
            timestamp: time, freq: None, expected_freq: Some(freq),
            width: tolerance * 2.0, is_target: true, string_idx, hit_result: None });
    }
    fn add_mic_detection(&mut self, time: f64, freq: f32) {
        self.events.push_back(TimeEvent {
            timestamp: time, freq: Some(freq), expected_freq: None,
            width: 0.0, is_target: false, string_idx: 0, hit_result: None });
        self.last_detected_freq = Some(freq);
    }
    fn register_hit(&mut self, time: f64, string_idx: usize, detected_freq: f32, expected_freq: f32) {
        self.note_history.push(NoteLogEntry {
            note_name: get_note_name(expected_freq), expected_freq,
            detected_freq: Some(detected_freq), is_hit: true });
        for ev in self.events.iter_mut().rev() {
            if ev.is_target && ev.string_idx == string_idx
                && ev.hit_result.is_none() && (time - ev.timestamp).abs() < 1.0
            {
                ev.hit_result = Some(true);
                let line = format!("{:.4},Target,{},{:.2},{:.2},Hit\n",
                    ev.timestamp, ev.string_idx, ev.expected_freq.unwrap_or(0.0), detected_freq);
                if let Ok(mut f) = fs::OpenOptions::new().append(true).open(&self.log_file_path) {
                    let _ = f.write_all(line.as_bytes()); }
                break;
            }
        }
    }
    fn register_miss(&mut self, time: f64, string_idx: usize, expected_freq: f32, detected_freq: Option<f32>) {
        self.note_history.push(NoteLogEntry {
            note_name: get_note_name(expected_freq), expected_freq,
            detected_freq: detected_freq.filter(|&f| is_valid_guitar_freq(f)), is_hit: false });
        for ev in self.events.iter_mut().rev() {
            if ev.is_target && ev.string_idx == string_idx
                && ev.hit_result.is_none() && (time - ev.timestamp).abs() < 2.0
            {
                ev.hit_result = Some(false);
                let ds = detected_freq.filter(|&f| is_valid_guitar_freq(f))
                    .map(|f| format!("{:.2}", f)).unwrap_or_default();
                let line = format!("{:.4},Target,{},{:.2},{},Miss\n",
                    ev.timestamp, ev.string_idx, expected_freq, ds);
                if let Ok(mut f) = fs::OpenOptions::new().append(true).open(&self.log_file_path) {
                    let _ = f.write_all(line.as_bytes()); }
                break;
            }
        }
    }
    fn update(&mut self, current_time: f64, _dt: f32) {
        while self.events.front().map_or(false, |e| current_time - e.timestamp > self.max_history_seconds) {
            self.events.pop_front();
        }
    }
}

// ─── SongLoader ───────────────────────────────────────────────────────────────

struct SongLoader { songs: Vec<Song>, error: Option<String> }

impl SongLoader {
    fn new() -> Self {
        let mut l = Self { songs: Vec::new(), error: None };
        l.load_songs_from_folder("songs");
        l
    }

    fn load_songs_from_folder(&mut self, folder: &str) {
        if !Path::new(folder).exists() {
            if let Err(e) = fs::create_dir_all(folder) {
                self.error = Some(format!("Failed to create songs folder: {}", e)); return;
            }
            // Стартовый демо-файл в старом формате — parse_json_song его поддержит
            let _ = fs::write(
                format!("{}/demo_scale.json", folder),
                r#"{"name":"Demo Scale","notes":[{"string":0,"fret":0,"time":2.0,"duration":0.5},{"string":1,"fret":0,"time":2.5,"duration":0.5}]}"#
            );
        }
        if let Ok(entries) = fs::read_dir(folder) {
            for entry in entries.flatten() {
                if entry.path().extension().map_or(false, |e| e == "json") {
                    if let Ok(content) = fs::read_to_string(&entry.path()) {
                        if let Some(song) = self.parse_json_song(&content) {
                            self.songs.push(song);
                        }
                    }
                }
            }
        }
        if self.songs.is_empty() { self.error = Some("No songs found.".into()); }
        else { println!("Loaded {} songs.", self.songs.len()); }
    }

    /// Парсит JSON в Song.
    /// Поддерживает оба формата:
    ///   - новый: `"events": [{time, duration, notes:[{string,fret},...]}]`
    ///   - старый: `"notes": [{string, fret, time, duration}]`
    fn parse_json_song(&self, json: &str) -> Option<Song> {
        let name = {
            let p = json.find("\"name\"")?;
            let r = &json[p + 6..];
            let q1 = r.find('"')? + 1;
            let q2 = r[q1..].find('"')? + q1;
            r[q1..q2].to_string()
        };
        let lesson_mode = json.contains("\"lesson_mode\": true") || json.contains("\"lesson_mode\":true");

        // Новый формат events[]
        if let Some(events) = self.parse_events_array(json) {
            let notes = parser::events_to_notes(&events);
            let tuning = self.parse_tuning(json);
            let ts     = self.parse_time_sig(json);
            let tempo  = self.extract_float(json, "initial_tempo").unwrap_or(120.0) as f32;
            return Some(Song { name, events, notes, tuning, time_signature: ts,
                initial_tempo: tempo, tempo_map: vec![], sections: vec![], lesson_mode });
        }

        // Старый формат notes[]
        let notes = self.parse_flat_notes(json)?;
        let events: Vec<SongEvent> = notes.iter().map(|n| SongEvent {
            time: n.time, duration: n.duration, sound_duration: n.sound_duration,
            notes: vec![ChordNote { string_idx: n.string_idx, fret: n.fret }],
            chord_name: None, technique: NoteTechnique::Normal,
        }).collect();
        let tuning = self.parse_tuning(json);
        let ts     = self.parse_time_sig(json);
        let tempo  = self.extract_float(json, "initial_tempo").unwrap_or(120.0) as f32;
        Some(Song { name, events, notes, tuning, time_signature: ts,
            initial_tempo: tempo, tempo_map: vec![], sections: vec![], lesson_mode })
    }

    fn parse_events_array(&self, json: &str) -> Option<Vec<SongEvent>> {
        let p   = json.find("\"events\"")?;
        let rest = &json[p + 8..];
        let a0  = rest.find('[')? + 1;
        let a1  = bracket_end(rest, a0)?;
        let arr = &rest[a0..a1];
        let mut events = Vec::new();
        let mut i = 0;
        while let Some(os) = arr[i..].find('{') {
            let abs_s = i + os;
            let abs_e = brace_end(arr, abs_s + 1)?;
            let obj   = &arr[abs_s..=abs_e];
            let time  = self.extract_float(obj, "time").unwrap_or(0.0);
            let dur   = self.extract_float(obj, "duration").unwrap_or(0.25);
            let sdur  = self.extract_float(obj, "sound_duration").unwrap_or(dur);
            let chord_name = extract_str_field(obj, "chord_name");
            if let Some(notes) = self.parse_chord_notes_inner(obj) {
                if !notes.is_empty() {
                    events.push(SongEvent {
                        time, duration: dur, sound_duration: sdur,
                        notes, chord_name, technique: NoteTechnique::Normal,
                    });
                }
            }
            i = abs_e + 1;
        }
        if events.is_empty() { None } else { Some(events) }
    }

    fn parse_chord_notes_inner(&self, obj: &str) -> Option<Vec<ChordNote>> {
        let p   = obj.find("\"notes\"")?;
        let rest = &obj[p + 7..];
        let a0  = rest.find('[')? + 1;
        let a1  = bracket_end(rest, a0)?;
        let arr = &rest[a0..a1];
        let mut notes = Vec::new();
        let mut i = 0;
        while let Some(os) = arr[i..].find('{') {
            let abs_s = i + os;
            let abs_e = brace_end(arr, abs_s + 1)?;
            let no    = &arr[abs_s..=abs_e];
            let s = self.extract_number(no, "string").unwrap_or(-1);
            let f = self.extract_number(no, "fret").unwrap_or(-1);
            if s >= 0 && (s as usize) < NUM_STRINGS && f >= 0 && (f as usize) <= TOTAL_FRETS {
                notes.push(ChordNote { string_idx: s as usize, fret: f as usize });
            }
            i = abs_e + 1;
        }
        Some(notes)
    }

    fn parse_flat_notes(&self, json: &str) -> Option<Vec<SongNote>> {
        let p   = json.find("\"notes\"")?;
        let rest = &json[p + 7..];
        let a0  = rest.find('[')? + 1;
        let a1  = bracket_end(rest, a0)?;
        let arr = &rest[a0..a1];
        let mut notes = Vec::new();
        let mut i = 0;
        while let Some(os) = arr[i..].find('{') {
            let abs_s = i + os;
            let abs_e = match brace_end(arr, abs_s + 1) { Some(e) => e, None => break };
            let obj   = &arr[abs_s..=abs_e];
            let s  = self.extract_number(obj, "string").unwrap_or(-1);
            let f  = self.extract_number(obj, "fret").unwrap_or(-1);
            let t  = self.extract_float(obj, "time").unwrap_or(0.0);
            if s < 0 || (s as usize) >= NUM_STRINGS || f < 0 || (f as usize) > TOTAL_FRETS {
                i = abs_e + 1; continue;
            }
            let dur  = self.extract_float(obj, "duration").unwrap_or(0.25);
            let sdur = self.extract_float(obj, "sound_duration").unwrap_or(dur);
            notes.push(SongNote {
                string_idx: s as usize, fret: f as usize, time: t,
                duration: dur, sound_duration: sdur, technique: NoteTechnique::Normal,
            });
            i = abs_e + 1;
        }
        if notes.is_empty() { None } else { Some(notes) }
    }

    fn parse_tuning(&self, json: &str) -> [f32; NUM_STRINGS] {
        if let Some(p) = json.find("\"tuning\"") {
            let r = &json[p..];
            if let (Some(b), Some(c)) = (r.find('['), r.find(']')) {
                let mut t = OPEN_STRINGS_FREQ;
                for (i, v) in r[b+1..c].split(',').enumerate() {
                    if i < 6 { if let Ok(f) = v.trim().parse::<f32>() { t[i] = f; } }
                }
                return t;
            }
        }
        OPEN_STRINGS_FREQ
    }

    fn parse_time_sig(&self, json: &str) -> (u8, u8) {
        if let Some(p) = json.find("\"time_signature\"") {
            let r = &json[p..];
            if let (Some(b), Some(c)) = (r.find('['), r.find(']')) {
                let parts: Vec<&str> = r[b+1..c].split(',').collect();
                if parts.len() == 2 {
                    return (parts[0].trim().parse().unwrap_or(4),
                            parts[1].trim().parse().unwrap_or(4));
                }
            }
        }
        (4, 4)
    }

    fn extract_number(&self, s: &str, key: &str) -> Option<i32> {
        let p = s.find(key)?;
        let r = &s[p + key.len()..];
        let c = r.find(':')?;
        let num = r[c+1..].trim_start();
        let end = num.find([',', '}'].as_ref()).unwrap_or(num.len());
        num[..end].trim().parse().ok()
    }

    fn extract_float(&self, s: &str, key: &str) -> Option<f64> {
        let p = s.find(key)?;
        let r = &s[p + key.len()..];
        let c = r.find(':')?;
        let num = r[c+1..].trim_start();
        let end = num.find([',', '}'].as_ref()).unwrap_or(num.len());
        num[..end].trim().parse().ok()
    }

    pub fn import_song(&mut self, file_path: &str, instr: &str, invert: bool, lesson: bool) -> Result<usize, String> {
        let content = parser::read_file_content(file_path)?;
        let parsed  = if lesson {
            parser::parse_musicxml_lesson(&content, Some(instr), invert)?
        } else {
            parser::parse_musicxml_to_song_notes(&content, Some(instr), invert)?
        };
        if parsed.events.is_empty() { return Err("No notes found.".into()); }

        let stem     = PathBuf::from(file_path).file_stem().unwrap_or_default().to_string_lossy().to_string();
        let json_path = PathBuf::from("songs").join(format!("{}.json", stem));

        use serde_json::json;
        let jdata = json!({
            "name": stem,
            "tuning": parsed.tuning,
            "time_signature": [parsed.time_signature.0, parsed.time_signature.1],
            "initial_tempo": parsed.initial_tempo,
            "lesson_mode": parsed.lesson_mode,
            "events": parsed.events.iter().map(|ev| json!({
                "time": ev.time, "duration": ev.duration, "sound_duration": ev.sound_duration,
                "chord_name": ev.chord_name,
                "notes": ev.notes.iter().map(|n| json!({"string": n.string_idx, "fret": n.fret})).collect::<Vec<_>>()
            })).collect::<Vec<_>>(),
            // Обратная совместимость
            "notes": parsed.notes.iter().map(|n| json!({
                "string": n.string_idx, "fret": n.fret,
                "time": n.time, "duration": n.duration, "sound_duration": n.sound_duration
            })).collect::<Vec<_>>()
        });

        fs::create_dir_all("songs").map_err(|e| e.to_string())?;
        fs::write(&json_path, serde_json::to_string_pretty(&jdata).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        println!("✅ Saved {:?} ({} events, lesson={})", json_path, parsed.events.len(), parsed.lesson_mode);

        let notes = parsed.notes.clone();
        self.songs.push(Song {
            name: stem, events: parsed.events, notes, tuning: parsed.tuning,
            time_signature: parsed.time_signature, initial_tempo: parsed.initial_tempo,
            tempo_map: parsed.tempo_map, sections: parsed.sections, lesson_mode: parsed.lesson_mode,
        });
        Ok(self.songs.len() - 1)
    }

    pub fn load_single_song(&self, path: &str) -> Result<Song, String> {
        let content = fs::read_to_string(path).map_err(|e| format!("Cannot read '{}': {}", path, e))?;
        self.parse_json_song(&content).ok_or_else(|| format!("Cannot parse '{}'", path))
    }
}

// ─── JSON helpers ─────────────────────────────────────────────────────────────

fn bracket_end(s: &str, start: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut depth = 1i32; let mut i = start;
    while i < b.len() {
        match b[i] {
            b'[' => depth += 1,
            b']' => { depth -= 1; if depth == 0 { return Some(i); } }
            b'"' => { i += 1; while i < b.len() && b[i] != b'"' { if b[i] == b'\\' { i += 1; } i += 1; } }
            _ => {}
        }
        i += 1;
    }
    None
}

fn brace_end(s: &str, start: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut depth = 1i32; let mut i = start;
    while i < b.len() {
        match b[i] {
            b'{' => depth += 1,
            b'}' => { depth -= 1; if depth == 0 { return Some(i); } }
            b'"' => { i += 1; while i < b.len() && b[i] != b'"' { if b[i] == b'\\' { i += 1; } i += 1; } }
            _ => {}
        }
        i += 1;
    }
    None
}

fn extract_str_field(s: &str, key: &str) -> Option<String> {
    let pat = format!("\"{}\"", key);
    let p   = s.find(&pat)?;
    let r   = &s[p + pat.len()..];
    let c   = r.find(':')?;
    let v   = r[c+1..].trim_start();
    if v.starts_with("null") || v.starts_with("false") { return None; }
    if v.starts_with('"') {
        let q = v[1..].find('"')? + 1;
        return Some(v[1..q].to_string());
    }
    None
}

// ─── IMPL MicController & AudioData ──────────────────────────────────────────

impl AudioData {
    fn new() -> Self {
        Self { detected_freq: None, last_detection_time: 0.0, wave_buffer: vec![0.0; 512],
            current_volume: 0.0, is_stream_active: false, error_msg: None,
            is_recording: false, recording_buffer: Vec::new(),
            sample_rate: 44100, channels: 1, current_device_name: "None".into(),
            pitch_detector: FastPitchDetector::new() }
    }
}

impl MicController {
    fn new() -> Self {
        Self { audio_data: Arc::new(Mutex::new(AudioData::new())),
            current_stream: None, host: cpal::default_host(), available_devices: Vec::new() }
    }
    pub fn refresh_devices(&mut self) {
        self.available_devices = self.host.input_devices().map(|d| d.collect()).unwrap_or_default();
    }
    pub fn get_audio_state(&self) -> Arc<Mutex<AudioData>> { self.audio_data.clone() }
    fn stop_stream(&mut self) {
        self.current_stream = None;
        if let Ok(mut d) = self.audio_data.lock() { d.is_stream_active = false; d.detected_freq = None; }
    }
    pub fn start_stream_with_device_index(&mut self, idx: usize) -> Result<(), String> {
        self.stop_stream(); self.refresh_devices();
        if idx >= self.available_devices.len() { return Err("Index out of range".into()); }
        let dev = &self.available_devices[idx];
        let cfg = dev.default_input_config().map_err(|e| e.to_string())?;
        let sr = cfg.sample_rate().0; let ch = cfg.channels();
        if let Ok(mut d) = self.audio_data.lock() {
            d.sample_rate = sr; d.channels = ch;
            d.current_device_name = dev.name().unwrap_or_default();
        }
        let stream = match cfg.sample_format() {
            SampleFormat::F32 => self.build_f32(dev, &cfg.into(), sr)?,
            SampleFormat::I16 => self.build_i16(dev, &cfg.into(), sr)?,
            _ => return Err("Unsupported sample format".into()),
        };
        stream.play().map_err(|e| e.to_string())?;
        self.current_stream = Some(stream);
        if let Ok(mut d) = self.audio_data.lock() { d.is_stream_active = true; d.error_msg = None; }
        Ok(())
    }
    fn build_f32(&self, dev: &Device, cfg: &cpal::StreamConfig, sr: u32) -> Result<Stream, String> {
        let ad = self.audio_data.clone();
        dev.build_input_stream(cfg, move |d: &[f32], _| process_audio_data(d, sr, &ad),
            |e| eprintln!("Audio: {}", e), None).map_err(|e| e.to_string())
    }
    fn build_i16(&self, dev: &Device, cfg: &cpal::StreamConfig, sr: u32) -> Result<Stream, String> {
        let ad = self.audio_data.clone();
        dev.build_input_stream(cfg, move |d: &[i16], _| {
            let s: Vec<f32> = d.iter().map(|&x| x as f32 / 32768.0).collect();
            process_audio_data(&s, sr, &ad);
        }, |e| eprintln!("Audio: {}", e), None).map_err(|e| e.to_string())
    }
    fn save_and_clear_recording(d: &mut AudioData, song_name: Option<&str>) -> Option<String> {
        if d.recording_buffer.is_empty() { return Some("Empty recording.".into()); }
        let _ = fs::create_dir_all("records");
        let sn = song_name.unwrap_or("unknown")
            .replace(|c: char| !c.is_alphanumeric() && c != '_' && c != '-', "");
        let path = format!("records/{}_{}.wav", Local::now().format("%Y.%m.%d_%H.%M.%S"), sn);
        if audio_engine::save_wav_file(&path, &d.recording_buffer, d.sample_rate).is_ok() {
            d.recording_buffer.clear(); Some(format!("Saved {}", path))
        } else { Some("Failed to save.".into()) }
    }
    pub fn stop_recording_if_active(&self, song_name: Option<&str>) -> Option<String> {
        if let Ok(mut d) = self.audio_data.lock() {
            if d.is_recording { d.is_recording = false; return Self::save_and_clear_recording(&mut d, song_name); }
        }
        None
    }
    fn toggle_recording(&self, song_name: Option<&str>) -> Option<String> {
        if let Ok(mut d) = self.audio_data.lock() {
            if d.is_recording {
                d.is_recording = false; Self::save_and_clear_recording(&mut d, song_name)
            } else {
                d.is_recording = true; d.recording_buffer.clear();
                let _ = fs::create_dir_all("records");
                Some("Recording started...".into())
            }
        } else { None }
    }
}

// ─── IMPL GameState ───────────────────────────────────────────────────────────

impl GameState {
    async fn new(output: Arc<Mutex<OutputController>>, song: &Song, lang_ru: bool, layout: config::CalculatedLayout, instrument: Instrument) -> Self {
        let mut song_data = song.events.clone();
        song_data.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
        Self {
            instrument,
            midi_data: None,
            events: Vec::new(), notes_compat: Vec::new(), particles: Vec::new(),
            score: 0, combo: 0, max_combo: 0,
            song_data, song_time: 0.0, last_spawn_idx: 0,
            game_over: false, paused: false, speed_index: 2,
            wave_type: WaveType::Triangle, flash_timer: 0.0, flash_color: WHITE,
            output, sound_cache: HashMap::new(),
            audio_data: Arc::new(Mutex::new(AudioData::new())),
            last_hit_time: 0.0, current_playing_freq: None, message: None,
            active_hints: [None; TOTAL_FRETS + 1],
            auto_play_mode: false, current_song_name: song.name.clone(),
            logger: GameLogger::new(&song.name), lang_ru,
            locale: Localization::load(lang_ru), last_valid_freq: None,
            stats_scroll_offset: 0, manual_track_offset: 0.0,
            is_track_dragging: false, drag_start_x: 0.0, drag_start_offset: 0.0,
            tuning: song.tuning, time_signature: song.time_signature,
            initial_tempo: song.initial_tempo, audio_stream_offset: 0.0,
            auto_play_scheduled: false,
            lesson: LessonState::new(),
            lesson_mode: song.lesson_mode,
            lesson_hint: None,
            layout,
            pre_start: PreStartPhase::Confirm,
            
        }
    }

    fn play_note_sound(&self, si: usize, fret: usize, vol: f32, dur: f64, _sdur: f64) {
        let freq = get_frequency(&self.tuning, si, fret);
        if let Ok(o) = self.output.lock() {
            o.play_note_fret(NoteRequest { freq, duration: dur.max(0.15) as f32,
                start_time: None, string_idx: si, volume: vol }, fret);
        }
    }

    fn schedule_all_notes(&mut self) {
        self.audio_stream_offset = self.output.lock()
            .map(|o| o.stream_time()).unwrap_or(0.0) - self.song_time;
        self.auto_play_scheduled = true;
        if let Ok(o) = self.output.lock() { o.stop_playback(); }
        let offset = self.audio_stream_offset;
        let stream_now = self.output.lock().map(|o| o.stream_time()).unwrap_or(0.0);
        for ev in &self.song_data {
            let audio_start = offset + ev.time;
            if audio_start < stream_now - 0.1 { continue; }
            let dur = ev.sound_duration.max(ev.duration).max(0.1) as f32;
            for cn in &ev.notes {
                let freq = get_frequency(&self.tuning, cn.string_idx, cn.fret);
                if let Ok(o) = self.output.lock() {
                    o.play_note_fret(NoteRequest {
                        freq, duration: dur, start_time: Some(audio_start),
                        string_idx: cn.string_idx, volume: 0.85,
                    }, cn.fret);
                }
            }
        }
        println!("Auto Play: scheduled {} events", self.song_data.len());
    }

    fn update(&mut self, dt: f32) {
        if self.game_over || self.paused { return; }
        if self.pre_start != PreStartPhase::Playing { return; } // пока не подтверждено

        // ── Урочный режим ──────────────────────────────────────────────────────
        if self.lesson_mode {
            let detected = self.audio_data.lock().ok().and_then(|d| d.detected_freq);
            let tick = self.lesson_tick(detected);

            if let Some(ref freqs) = tick.play_preview {
                if !self.lesson.preview_played {
                    for &(si, fret, _) in freqs { self.play_note_sound(si, fret, 0.65, 1.5, 1.5); }
                    self.lesson.preview_played = true;
                }
            }
            for (si, _freq) in &tick.newly_confirmed {
                let x = FRETBOARD_X + 50.0;
                let y = HIT_ZONE_Y + *si as f32 * 40.0 + 20.0;
                self.spawn_particles(x, y, safe_string_color(*si));
                self.flash_timer = 0.1; self.flash_color = safe_string_color(*si);
            }
            if tick.play_confirmation {
                self.score += 100 * ((self.combo / 10 + 1) as u32).min(5);
                self.combo += 1; self.max_combo = self.max_combo.max(self.combo);
                self.flash_timer = 0.2; self.flash_color = GREEN;
                self.set_message("✓".into());
            }
            self.lesson_hint = tick.hint;

            // Время идёт только когда не заморожены
            if !self.lesson.paused {
                self.song_time += dt as f64;
                self.spawn_events();
                self.update_positions();
            }

            // Логируем цели для freq-графика
            for ev in &self.events {
                if ev.hit || ev.missed { continue; }
                if (ev.y - PLAYHEAD_X).abs() < HIT_TOLERANCE * 3.0 {
                    for n in &ev.notes {
                        self.logger.add_target_note(ev.target_time, n.freq, HIT_TOLERANCE_FREQ, n.string_idx);
                    }
                }
            }
        } else {
            // ── Обычный режим ──────────────────────────────────────────────────
            self.song_time += dt as f64;
            self.spawn_events();
            for h in self.active_hints.iter_mut() { *h = None; }
            self.update_positions();

            for ev in &self.events {
                if ev.hit || ev.missed { continue; }
                if (ev.y - HIT_ZONE_Y).abs() < HIT_TOLERANCE * 3.0 {
                    for n in &ev.notes {
                        self.logger.add_target_note(ev.target_time, n.freq, HIT_TOLERANCE_FREQ, n.string_idx);
                    }
                }
            }

            if self.auto_play_mode {
                for ev in self.events.iter_mut() {
                    if !ev.hit && !ev.missed && (ev.target_time - self.song_time).abs() < 0.08 {
                        ev.hit = true;
                        self.flash_timer = 0.1;
                        self.flash_color = ev.notes.first().map(|n| n.color).unwrap_or(WHITE);
                        self.score += 100 * ((self.combo / 10 + 1) as u32).min(5);
                        self.combo += 1; self.max_combo = self.max_combo.max(self.combo);
                    }
                }
            }
        }

        // ── Пропущенные (оба режима) ───────────────────────────────────────────
        let mut missed_info: Vec<(usize, f32)> = Vec::new();
        for ev in self.events.iter_mut() {
            if !ev.hit && !ev.missed && ev.y > HIT_ZONE_Y + HIT_TOLERANCE * 2.0 {
                ev.missed = true;
                if !self.auto_play_mode {
                    if let Some(n) = ev.notes.first() {
                        missed_info.push((n.string_idx, n.freq));
                    }
                }
            }
        }
        for (si, ef) in missed_info {
            self.combo = 0;
            let det = self.audio_data.lock().ok().and_then(|d| d.detected_freq);
            self.logger.register_miss(self.song_time, si, ef, det);
        }
        self.events.retain(|ev| !ev.hit && !ev.missed && ev.y > -200.0 && ev.y < 2000.0);

        // Совместимость со старыми draw-функциями
        self.notes_compat = self.events.iter().flat_map(|ev| {
            ev.notes.iter().map(|n| GameNote {
                string_idx: n.string_idx,
                fret: n.fret,
                midi_note: n.midi_note, // <-- ДОБАВЛЕНО
                y: n.y,
                x: ev.y,
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
        }).collect();

        // Конец песни
        if self.last_spawn_idx >= self.song_data.len() && self.events.is_empty() {
            self.game_over = true;
            if let Ok(o) = self.output.lock() { o.stop_playback(); }
            self.auto_play_mode = false; self.auto_play_scheduled = false;
        }

        // Частицы, таймеры
        for p in &mut self.particles {
            p.x += p.vx * dt; p.y += p.vy * dt;
            p.vy += 20.0 * dt; p.life -= dt; p.size *= 0.98;
        }
        self.particles.retain(|p| p.life > 0.0);
        if self.flash_timer > 0.0 { self.flash_timer -= dt; }
        if let Some((_, t)) = &mut self.message { *t -= dt; if *t <= 0.0 { self.message = None; } }
        if self.current_playing_freq.is_some() && (self.song_time - self.last_hit_time) > 0.3 {
            self.current_playing_freq = None;
        }
        self.logger.update(self.song_time, dt);

        // Drag трека
        let (mx, my) = mouse_position();
        let in_track = my >= NOTATION_TRACK_Y && my <= TAB_TRACK_Y + TAB_TRACK_H;
        if is_mouse_button_pressed(MouseButton::Left) && in_track {
            self.is_track_dragging = true; self.drag_start_x = mx;
            self.drag_start_offset = self.manual_track_offset;
        }
        if is_mouse_button_released(MouseButton::Left) { self.is_track_dragging = false; }
        if self.is_track_dragging {
            self.manual_track_offset = (self.drag_start_offset + (mx - self.drag_start_x)).clamp(-400.0, 400.0);
        } else { self.manual_track_offset *= 0.9; }
    }

    fn spawn_events(&mut self) {
        let eff_spd = NOTE_SPEED * SPEED_MULTIPLIERS[self.speed_index];
        let travel  = HIGHWAY_H as f64 / eff_spd as f64;
        while self.last_spawn_idx < self.song_data.len() {
            let ev = &self.song_data[self.last_spawn_idx];
            if self.song_time >= ev.time - travel {
                self.events.push(GameEvent::from_song_event(ev, &self.tuning, self.song_time, self.speed_index, &self.layout, self.instrument,));
                self.last_spawn_idx += 1;
            } else { break; }
        }
    }

    fn update_positions(&mut self) {
        let eff_spd = NOTE_SPEED * SPEED_MULTIPLIERS[self.speed_index];
        for ev in self.events.iter_mut() {
            if !ev.hit && !ev.missed {
                let tt = (ev.target_time - self.song_time) as f32;
                ev.y = PLAYHEAD_X - (tt * eff_spd);
            }
        }
    }

    /// Один тик логики урочного режима (возвращает команды для main loop).
    fn lesson_tick(&mut self, detected_freq: Option<f32>) -> LessonTickResult {
        let mut result = LessonTickResult {
            play_preview: None, play_confirmation: false, hint: None,
            newly_confirmed: Vec::new(), unfreeze: false, timeout: false,
        };

        if self.lesson.paused {
            let idx = match self.lesson.current_event_idx {
                Some(i) => i,
                None => { self.lesson.unfreeze(); result.unfreeze = true; return result; }
            };
            if idx >= self.events.len() { self.lesson.unfreeze(); result.unfreeze = true; return result; }
            if self.lesson.is_timed_out() { self.lesson.unfreeze(); result.timeout = true; return result; }

            let hit = check_lesson_hit(&mut self.events[idx], detected_freq, &self.tuning);
            let progress = !hit.newly_confirmed.is_empty();
            self.lesson.tick_stall(progress);

            if progress {
                self.lesson.record_attempt(true);
                for &ni in &hit.newly_confirmed {
                    let n = &self.events[idx].notes[ni];
                    result.newly_confirmed.push((n.string_idx, n.freq));
                }
            } else if detected_freq.map_or(false, is_valid_guitar_freq) {
                self.lesson.record_attempt(false);
            }

            if hit.all_done {
                self.lesson.unfreeze();
                result.play_confirmation = true;
                result.unfreeze = true;
                return result;
            }
            if self.lesson.show_hint {
                result.hint = Some(HintData::from_event(&self.events[idx]));
            }
        } else {
            // Ищем событие, достигшее зоны заморозки
            for (i, ev) in self.events.iter().enumerate() {
                if ev.hit || ev.missed { continue; }
                if should_freeze(ev) {
                    let preview = ev.notes.iter().map(|n| (n.string_idx, n.fret, n.freq)).collect();
                    self.lesson.freeze(i);
                    result.play_preview = Some(preview);
                    break;
                }
            }
        }
        result
    }

    /// Проверяет попадание ноты через MIDI-клавиатуру.
    /// Вызывается вместо check_microphone_hit когда подключена MIDI-клавиатура.
    fn check_midi_hit(&mut self) {
        if self.paused || self.lesson_mode { return; }
        
        let midi_data = match &self.midi_data {
            Some(d) => d,
            None => {
                eprintln!("⚠️ check_midi_hit: midi_data is None!");
                return;
            }
        };
        
        let mut md = match midi_data.lock() {
            Ok(d) => d,
            Err(_) => return,
        };
        
        let pressed_note = match md.consume_press() {
            Some(n) => {
                let freq = 440.0 * 2.0_f32.powf((n as f32 - 69.0) / 12.0);
                let name = crate::get_note_name(freq);
                eprintln!("🎵 MIDI note pressed: {} ({} Hz)", name, freq);
                n
            },
            None => return,
        };
        drop(md); // освобождаем мьютекс перед мутацией events

        // Ищем событие в зоне попадания с совпадающей MIDI-нотой.
        // Сохраняем данные для спавна частиц в локальные переменные,
        // чтобы не конфликтовать с borrow checker при вызове spawn_particles.
        let mut hit_info: Option<(f32, f32, Color)> = None;

        for ev in &mut self.events {
            if ev.hit || ev.missed { continue; }
            if (ev.y - HIT_ZONE_Y).abs() > HIT_TOLERANCE { continue; }

            for note in &mut ev.notes {
                if note.midi_note == pressed_note {
                    note.confirmed = true;
                    if !ev.hit {
                        ev.hit = true;
                        self.flash_timer = 0.15;
                        self.flash_color = note.color;
                        self.score += 100 * ((self.combo / 10 + 1) as u32).min(5);
                        self.combo += 1;
                        self.max_combo = self.max_combo.max(self.combo);
                        self.last_hit_time = self.song_time;
                        self.current_playing_freq = Some(note.freq);

                        // Вычисляем координаты клавиши пианино для спавна частиц
                        let piano_h = 180.0_f32;
                        let piano_y = self.layout.window_h - 82.0 - 10.0 - piano_h;
                        let (kx, ky, kw, kh) = graphics::get_piano_key_rect_game(
                            note.midi_note, 0.0, piano_y, self.layout.window_w, piano_h
                        );
                        hit_info = Some((kx + kw / 2.0, ky + kh * 0.5, note.color));
                    }
                    break;
                }
            }
            if hit_info.is_some() { break; }
        }

        // Спавн частиц — вызываем ПОСЛЕ выхода из заимствования self.events
        if let Some((px, py, color)) = hit_info {
            self.spawn_particles(px, py, color);
        }
    }

    /// Проверка попаданий микрофона в обычном режиме.
    fn check_microphone_hit(&mut self) {
        if self.paused || self.lesson_mode { return; }
        let (det, vol) = match self.audio_data.lock() {
            Ok(a) => (a.detected_freq, a.current_volume),
            Err(_) => return,
        };
        let det_f = match det { Some(f) if is_valid_guitar_freq(f) => f, _ => return };
        if vol < NOISE_GATE_THRESHOLD { return; }

        // Сглаживание
        let smoothed = if let Some(prev) = self.last_valid_freq {
            if (prev - det_f).abs() < 15.0 { prev * 0.7 + det_f * 0.3 } else { det_f }
        } else { det_f };
        self.last_valid_freq = Some(smoothed);

        // Собираем кандидатов
        let mut candidates: Vec<(usize, usize, f32, f32)> = Vec::new(); // (ei, si, smoothed, ef)
        for (ei, ev) in self.events.iter().enumerate() {
            if ev.hit || ev.missed { continue; }
            
            // ─── Используем HIT_ZONE_Y вместо PLAYHEAD_X ───
            if (ev.y - HIT_ZONE_Y).abs() > HIT_TOLERANCE { continue; }
            // ──────────────────────────────────────────────────────────
            
            for n in &ev.notes {
                let corr = correct_octave_error(smoothed, n.freq);
                if (corr - n.freq).abs() < get_freq_tolerance(n.freq) {
                    candidates.push((ei, n.string_idx, smoothed, n.freq));
                }
            }
        }
        if candidates.is_empty() { return; }

        // Применяем (одно событие засчитывается одним хитом)
        let mut seen = std::collections::HashSet::new();
        for (ei, si, smo, ef) in candidates {
            if seen.contains(&ei) { continue; }
            if ei >= self.events.len() { continue; }
            let ev = &mut self.events[ei];
            if ev.hit || ev.missed { continue; }
            ev.hit = true; seen.insert(ei);
            self.logger.register_hit(self.song_time, si, smo, ef);
            let hx = FRETBOARD_X + 50.0;
            let hy = HIT_ZONE_Y + si as f32 * 40.0 + 20.0;
            self.score += 100 * ((self.combo / 10 + 1) as u32).min(5);
            self.combo += 1; self.max_combo = self.max_combo.max(self.combo);
            self.flash_timer = 0.15; self.flash_color = safe_string_color(si);
            self.last_hit_time = self.song_time; self.current_playing_freq = Some(ef);
            self.spawn_particles(hx, hy, safe_string_color(si));
        }
        if seen.is_empty() && self.combo > 0 { self.combo = 0; }
    }

    fn spawn_particles(&mut self, x: f32, y: f32, c: Color) {
        for _ in 0..12 {
            let a  = gen_range(0.0, std::f32::consts::PI * 2.0);
            let sp = 50.0 + gen_range(0.0, 100.0);
            self.particles.push(Particle {
                x, y, vx: a.cos() * sp, vy: a.sin() * sp - 50.0,
                life: 0.4 + gen_range(0.0, 0.3), color: c, size: 3.0 + gen_range(0.0, 4.0) });
        }
    }

    async fn toggle_wave_type(&mut self) {
        self.wave_type = match self.wave_type {
            WaveType::Triangle => WaveType::Sawtooth, WaveType::Sawtooth => WaveType::Square,
            WaveType::Square   => WaveType::Sine,     WaveType::Sine     => WaveType::Triangle,
        };
    }
    fn set_message(&mut self, m: String) { self.message = Some((m, 3.0)); }

    fn toggle_automode(&mut self) {
        self.auto_play_mode = !self.auto_play_mode;
        if self.auto_play_mode {
            self.song_time = 0.0; self.last_spawn_idx = 0; self.events.clear(); self.notes_compat.clear();
            self.score = 0; self.combo = 0; self.auto_play_scheduled = false;
            self.schedule_all_notes();
            let synth = if self.output.lock().map(|o| o.sampler_available()).unwrap_or(false) { "WAV Sampler" } else { "Karplus-Strong" };
            self.set_message(format!("Auto Play ON [{}]", synth));
        } else {
            if let Ok(o) = self.output.lock() { o.stop_playback(); }
            self.auto_play_scheduled = false; self.set_message("Auto Play OFF".into());
        }
    }

    fn change_speed(&mut self, delta: i32) {
        let new_idx = (self.speed_index as i32 + delta)
            .clamp(0, SPEED_MULTIPLIERS.len() as i32 - 1) as usize;
        if new_idx != self.speed_index {
            self.speed_index = new_idx;
            self.set_message(format!("Speed: {}", SPEED_LABELS[self.speed_index]));
        }
    }

    fn toggle_pause(&mut self) { self.paused = !self.paused; }

    fn toggle_language(&mut self) {
        self.lang_ru = !self.lang_ru;
        self.locale  = Localization::load(self.lang_ru);
    }
}

// ─── Утилиты ──────────────────────────────────────────────────────────────────

pub fn analyze_song_logs_from_state(s: &GameState) -> SongStats {
    let (hits, misses) = s.logger.note_history.iter()
        .fold((0u32, 0u32), |(h, m), e| if e.is_hit { (h+1, m) } else { (h, m+1) });
    let tot = hits + misses;
    SongStats { hits, misses,
        accuracy: if tot > 0 { hits as f32 / tot as f32 * 100.0 } else { 0.0 },
        details: s.logger.note_history.clone() }
}

fn process_audio_data(samples: &[f32], sr: u32, ad: &Arc<Mutex<AudioData>>) {
    if samples.is_empty() { return; }
    let rms = (samples.iter().map(|&s| s*s).sum::<f32>() / samples.len() as f32).sqrt();
    let gain = if rms < 0.001 { 1.0 } else { (TARGET_VOLUME / rms).min(10.0) } * MIC_SENSITIVITY;
    let mut proc: Vec<f32> = samples.iter().map(|&s| (s * gain).clamp(-1.0, 1.0)).collect();
    for i in (1..proc.len()).rev() { proc[i] -= 0.95 * proc[i-1]; }
    let prms = (proc.iter().map(|&s| s*s).sum::<f32>() / proc.len() as f32).sqrt();
    if let Ok(mut d) = ad.lock() {
        let len = d.wave_buffer.len();
        if proc.len() >= len { d.wave_buffer.copy_from_slice(&proc[proc.len()-len..]); }
        else { d.wave_buffer.drain(0..len-proc.len()); d.wave_buffer.extend_from_slice(&proc); }
        d.current_volume = prms;
        if prms > NOISE_GATE_THRESHOLD {
            let sz = ((sr as f32 / 82.41 * 6.0) as usize).min(samples.len()).max(1024);
            let st = samples.len().saturating_sub(sz);
            let det = d.pitch_detector.detect_pitch(&samples[st..], sr);
            d.detected_freq = det.filter(|&f| [50.0f32,60.0,100.0,120.0].iter().all(|&n| (f-n).abs() >= 4.0));
        } else { d.detected_freq = None; }
        if d.is_recording {
            if d.channels == 2 {
                for chunk in samples.chunks(2) {
                    let avg = (chunk[0] + chunk.get(1).copied().unwrap_or(0.0)) / 2.0;
                    d.recording_buffer.push((avg * 32767.0).clamp(-32768.0, 32767.0) as i16);
                }
            } else {
                for &s in samples {
                    d.recording_buffer.push((s * 32767.0).clamp(-32768.0, 32767.0) as i16);
                }
            }
        }
    }
}

pub fn get_logs_folder_size() -> u64 {
    fs::read_dir("logs").map(|e| e.flatten()
        .filter_map(|e| e.metadata().ok().filter(|m| m.is_file()).map(|m| m.len()))
        .sum()).unwrap_or(0)
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 { format!("{} B", bytes) }
    else if bytes < 1024*1024 { format!("{:.1} KB", bytes as f64/1024.0) }
    else { format!("{:.1} MB", bytes as f64/1024.0/1024.0) }
}

fn clear_logs_folder() -> std::io::Result<()> {
    if let Ok(entries) = fs::read_dir("logs") {
        for e in entries.flatten() {
            if e.path().extension().map_or(false, |x| x == "csv") { fs::remove_file(e.path())?; }
        }
    }
    Ok(())
}

fn load_wav_samples(path: &str) -> Result<(Vec<f32>, u32), String> {
    use std::io::BufReader; use rodio::Source;
    let file   = fs::File::open(path).map_err(|e| e.to_string())?;
    let source = rodio::Decoder::new(BufReader::new(file)).map_err(|e| e.to_string())?;
    let sr     = source.sample_rate();
    let ch     = source.channels() as usize;
    let raw: Vec<f32> = source.convert_samples().collect();
    let samples = if ch == 2 {
        raw.chunks(2).map(|c| (c[0] + c.get(1).copied().unwrap_or(0.0)) / 2.0).collect()
    } else { raw };
    Ok((samples, sr))
}

fn resample_linear(samples: &[f32], src: u32, dst: u32) -> Vec<f32> {
    if src == dst || samples.is_empty() { return samples.to_vec(); }
    let ratio = src as f64 / dst as f64;
    let len   = ((samples.len() as f64) / ratio).ceil() as usize;
    (0..len).map(|i| {
        let p = i as f64 * ratio;
        let idx = p as usize;
        let frac = (p - idx as f64) as f32;
        let a = *samples.get(idx).unwrap_or(&0.0);
        let b = *samples.get(idx+1).unwrap_or(&0.0);
        a + frac * (b - a)
    }).collect()
}

fn scan_records_folder() -> Vec<String> {
    let mut r: Vec<String> = fs::read_dir("records").map(|e| e.flatten()
        .filter(|e| e.path().extension().map_or(false, |x| x == "wav"))
        .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
        .collect()).unwrap_or_default();
    r.sort(); r
}

fn tuner_play_record(idx: usize, records: &[String], output: &OutputController) -> (bool, String) {
    if idx >= records.len() { return (false, "Нет записи".into()); }
    let path = format!("records/{}", records[idx]);
    match load_wav_samples(&path) {
        Ok((samples, file_sr)) => {
            let dev_sr    = output.active_sample_rate;
            let resampled = resample_linear(&samples, file_sr, dev_sr);
            let dur       = resampled.len() as f64 / dev_sr as f64;
            output.play(PlayRequest { samples: resampled, volume: 1.0, duration: dur });
            (true, format!("{} ({}→{} Hz)", records[idx], file_sr, dev_sr))
        }
        Err(e) => (false, format!("Ошибка загрузки: {}", e)),
    }
}

fn collect_missing_textures(
    lf: &LessonsFile, state: &LessonsState,
    cache: &HashMap<String, macroquad::texture::Texture2D>,
    attempted: &std::collections::HashSet<String>,
) -> Vec<String> {
    let new = |p: &String| !cache.contains_key(p) && !attempted.contains(p);
    let mut paths = Vec::new();
    if lf.is_empty() { return paths; }
    let idx    = state.selected_lesson.min(lf.lessons.len() - 1);
    let lesson = &lf.lessons[idx];
    if let Some(ref p) = lesson.image { if new(p) { paths.push(p.clone()); } }
    if let Some(si) = state.open_step {
        if let Some(step) = lesson.steps.get(si) {
            for img in &step.images { if new(&img.path) { paths.push(img.path.clone()); } }
        }
    }
    for l in &lf.lessons { if let Some(ref p) = l.image { if new(p) { paths.push(p.clone()); } } }
    paths.sort(); paths.dedup(); paths
}

// ─── MAIN ─────────────────────────────────────────────────────────────────────

#[macroquad::main("GuiTAR Sound")]
async fn main() {
    let app_config = AppConfig::load();
    let layout     = app_config.calculate_layout();
    request_new_screen_size(layout.window_w, layout.window_h);
    set_fullscreen(true);

    let mut output_controller = OutputController::new();
    let devices = output_controller.list_devices();
    let mut selected_idx = 0usize;
    if !devices.is_empty() {
        for (i, name) in devices.iter().enumerate() {
            let n = name.to_lowercase();
            if (n.contains("динамик") || n.contains("headphone")) && !n.contains("nvidia") {
                selected_idx = i; break;
            }
        }
        if output_controller.start(selected_idx).is_ok() {
            if output_controller.load_guitar_samples() == 0 {
                println!("ℹ️  No WAV samples — Karplus-Strong active");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    let output_arc   = Arc::new(Mutex::new(output_controller));
    let mut tuner_output = OutputController::new();
    let _ = tuner_output.start(selected_idx);

    let _ = std::fs::create_dir_all("sounds");

    let font      = load_ttf_font(FONT_PATH).await
        .unwrap_or_else(|_| panic!("Font '{}' not found.", FONT_PATH));
    let icon_font: Option<Font> = load_ttf_font("fonts/fa-solid-900.otf").await.ok();
    if icon_font.is_some() { println!("✅ Font Awesome loaded"); }
    else { println!("ℹ️  Font Awesome not found — text fallback active"); }

    let mut sl = SongLoader::new();
    let mut mc = MicController::new();
    let mut midi_ctrl = MidiInputController::new();

    if mc.host.input_devices().map(|d| d.count()).unwrap_or(0) > 0 {
        let _ = mc.start_stream_with_device_index(0);
    }

    let mut lt = Instant::now();
    let mut gc = GraphicsContext::new(layout.window_w, layout.window_h, layout.clone());

    #[derive(Clone, Copy, PartialEq, Debug)]
    enum AppState {
        MainMenu, Lessons, Theory, SongSelection, Info, Game,
        MicSelection, Stats, OutputSelection,
        InstrumentSelection, StringInversion, Tuner, TunerOutputSelection,
        TunerMicSelection, Studio, StudioOutputSelection,
    }

    let mut app_state  = AppState::MainMenu;
    let mut nav_stack: VecDeque<AppState> = VecDeque::new();
    let max_nav        = 8usize;
    macro_rules! push_nav {
        () => { nav_stack.push_back(app_state); if nav_stack.len() > max_nav { nav_stack.pop_front(); } };
    }
    macro_rules! pop_nav {
        ($default:expr) => { nav_stack.pop_back().unwrap_or($default) };
    }

    let mut ssi = 0usize;
    let mut msi = 0usize;
    let mut osi = 0usize;
    let mut game_state: Option<GameState> = None;
    let mut game_from_lesson = false;
    let mut lang_ru    = true;
    let mut locale     = Localization::load(lang_ru);
    let mut search     = SearchState::new();
    let mut clear_confirm: Option<Instant> = None;
    let mut pending_import: Option<(String, Vec<String>)> = None;
    let mut instr_idx  = 0usize;
    let mut last_lang_toggle = 0.0f64;
    let mut show_exit  = false;
    let mut exit_just  = false;
    let mut theme      = ThemeState::new();
    let mut dbg_day    = theme.is_day;

    let mut lessons_file  = LessonsFile::load(lang_ru);
    let mut lessons_state = LessonsState::new();
    let mut tex_cache: HashMap<String, macroquad::texture::Texture2D> = HashMap::new();
    let mut tex_tried: std::collections::HashSet<String>              = Default::default();
    let mut theory_file  = LessonsFile::load_theory(lang_ru);
    let mut theory_state = LessonsState::new();
    let mut theory_tried: std::collections::HashSet<String>           = Default::default();
    let mut info_file    = InfoFile::load(lang_ru);
    let mut info_state   = InfoState::new();
    let mut info_prefs   = InfoPrefs::load();
    let mut show_on_start = info_prefs.show_on_start;
    let mut info_tex: HashMap<String, macroquad::texture::Texture2D>  = HashMap::new();
    let mut info_tried: std::collections::HashSet<String>             = Default::default();

    let mut tuner_note   = "--".to_string();
    let mut tuner_freq: Option<f32> = None;
    let mut tuner_hold   = 0.0f32;
    let mut tuner_frozen = false;
    let mut tuner_msg: Option<(String, f32)> = None;
    let mut tuner_records = scan_records_folder();
    let mut tuner_rec_sel = 0usize;
    let mut tuner_playing = false;
    let mut tuner_osi     = selected_idx;

    let mut studio_song_idx = 0usize;
    let mut studio_generated_files: Vec<String> = Vec::new();
    let mut studio_selected_file = 0usize;
    let mut studio_playing = false;
    let mut studio_rendering = false;
    //let mut studio_playback_thread: Option<std::thread::JoinHandle<()>> = None;
    let mut studio_is_playing = false;
    let mut studio_current_playing_file: Option<String> = None;
    let mut studio_is_playing = false;
    let mut studio_message: Option<(String, f64)> = None;
    
    let mut studio_generate_clicked = false;
    let mut studio_generation_message: Option<(String, f64)> = None;
    let mut studio_osi = selected_idx;
    let mut studio_loaded_notes: Vec<String> = Vec::new();  // Последовательность загруженных нот

    let mut tuner_renaming: Option<usize> = None;          // индекс записи в режиме переименования
    let mut tuner_rename_buffer: String = String::new();
    let mut tuner_rename_cursor: usize = 0;
    let mut tuner_rename_selection: Option<(usize, usize)> = None;
     let mut practice_module = graphics::PracticeModuleChoice::Guitar;


    use tuner_presets::get_tunings;
    let  tuner_tunings = get_tunings();
    let mut tuner_selected_tuning: usize = 0;  // Standard E по умолчанию
    let mut tuner_selected_string: usize = 0;  // 6-я струна по умолчанию
    let mut tuner_tuning_menu_open: bool = false;

    let (tx, rx) = mpsc::channel::<Option<PathBuf>>();
    let mut importing  = false;
    let mut fullscreen = true;

    if show_on_start { push_nav!(); app_state = AppState::Info; }

    // Вспомогательный макрос для переключения языка (встречается в каждом экране)
    macro_rules! reload_lang {
        () => {
            lang_ru = !lang_ru; locale = Localization::load(lang_ru);
            lessons_file = LessonsFile::load(lang_ru);
            theory_file  = LessonsFile::load_theory(lang_ru);
            info_file    = InfoFile::load(lang_ru);
            tex_tried.clear(); theory_tried.clear(); info_tried.clear(); info_tex.clear();
        };
    }
    macro_rules! go_info {
        () => { push_nav!(); info_state = InfoState::new(); app_state = AppState::Info; };
    }

    fn scan_generated_sounds() -> Vec<String> {
        let mut files: Vec<String> = std::fs::read_dir("sounds")
            .map(|entries| {
                entries.flatten()
                    .filter(|e| e.path().extension().map_or(false, |ext| ext == "wav"))
                    .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        files.sort();
        files
    }

    loop {
        gc.update();
        theme.tick_auto();
        theme.is_day = false; // TO DO - Полный сутточный цикл солнца
        if theme.is_day != dbg_day {
            println!("🌅 is_day → {} at {:.1}s", theme.is_day, get_time()); dbg_day = theme.is_day;
        }
        let dt = lt.elapsed().as_secs_f32(); lt = Instant::now();
        search.update(dt);

        if is_key_pressed(KeyCode::F12) {
            fullscreen = !fullscreen; set_fullscreen(fullscreen);
            if !fullscreen { request_new_screen_size(layout.window_w, layout.window_h); }
        }

        // Фоновый импорт
        if importing {
            if let Ok(res) = rx.try_recv() {
                importing = false;
                if let Some(pb) = res {
                    let ps = pb.to_string_lossy().to_string();
                    let ext = pb.extension()
                        .and_then(|e| e.to_str())
                        .map(|s| s.to_lowercase())
                        .unwrap_or_default();
                    
                    // Определяем формат по расширению
                    let is_midi = matches!(ext.as_str(), "mid" | "midi");
                    
                    if is_midi {
                        // ── MIDI: парсим сразу, без выбора инструмента ────────────
                        let stem = pb.file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        let json_path = PathBuf::from("songs").join(format!("{}.json", stem));
                        
                        // Стандартный гитарный строй
                        let tuning = [82.41, 110.00, 146.83, 196.00, 246.94, 329.63];
                        
                        match midi_parser::parse_midi_to_song(&ps, tuning) {
                            Ok(song_data) => {
                                match midi_parser::save_midi_as_json(&song_data, &stem, json_path.to_str().unwrap()) {
                                    Ok(_) => {
                                        println!("✅ MIDI imported: {}.json ({} events)", stem, song_data.events.len());
                                        sl.load_songs_from_folder("songs");
                                    }
                                    Err(e) => eprintln!("❌ MIDI save error: {}", e),
                                }
                            }
                            Err(e) => eprintln!("❌ MIDI parse error: {}", e),
                        }
                    } else {
                        // ── MusicXML: показываем выбор инструмента ────────────────
                        match parser::read_file_content(&ps) {
                            Ok(c) => match parser::get_available_instruments(&c) {
                                Ok(instr) => { 
                                    pending_import = Some((ps, instr)); 
                                    app_state = AppState::InstrumentSelection; 
                                    instr_idx = 0; 
                                }
                                Err(e) => eprintln!("Instruments error: {}", e),
                            },
                            Err(e) => eprintln!("File error: {}", e),
                        }
                    }
                }
            }
        }

        match app_state {

            // ── ГЛАВНОЕ МЕНЮ ──────────────────────────────────────────────────
            AppState::MainMenu => {
                clear_background(Color::new(0.05, 0.05, 0.08, 1.));
                let choice = graphics::draw_main_menu(&gc, &font, icon_font.as_ref(), &locale, &theme, get_time());
                let (_, lv, _, iv) = graphics::draw_overlay_buttons_with_locale(&gc, &font, &locale, &mut theme, None);
                if lv { reload_lang!(); } if iv { go_info!(); }
                match choice {
                    MainMenuChoice::Lessons  => { lessons_file = LessonsFile::load(lang_ru); lessons_state = LessonsState::new(); tex_tried.clear(); push_nav!(); app_state = AppState::Lessons; }
                    MainMenuChoice::Practice => { push_nav!(); app_state = AppState::SongSelection; }
                    MainMenuChoice::Studio   => { push_nav!(); app_state = AppState::Studio; }
                    MainMenuChoice::None     => {}
                }
                if is_key_pressed(KeyCode::Escape) && !show_exit { show_exit = true; exit_just = true; }
                if show_exit {
                    let r = graphics::draw_exit_confirm_dialog(&gc, &font, &locale, &theme);
                    if !exit_just && is_key_pressed(KeyCode::Escape) { show_exit = false; }
                    else { match r {
                        ExitDialogResult::Confirm => break,
                        ExitDialogResult::Cancel  => { show_exit = false; }
                        ExitDialogResult::None    => {}
                    }}
                }
                if exit_just { exit_just = false; }
            }

            // ── УРОКИ ─────────────────────────────────────────────────────────
            AppState::Lessons => {
                clear_background(Color::new(0.05, 0.05, 0.08, 1.)); theme.draw_background(get_time());
                for p in collect_missing_textures(&lessons_file, &lessons_state, &tex_cache, &tex_tried) {
                    tex_tried.insert(p.clone());
                    match macroquad::texture::load_texture(&p).await {
                        Ok(t) => { tex_cache.insert(p, t); } Err(e) => { eprintln!("Tex '{}': {}", p, e); }
                    }
                }
                let r = graphics::draw_lessons_panel(&gc, &font, &lessons_file, &mut lessons_state, &locale, &tex_cache, &theme);
                let (bv, lv, _, iv) = graphics::draw_overlay_buttons_with_locale(&gc, &font, &locale, &mut theme, Some(&locale.overlay_back));
                if lv { reload_lang!(); } if iv { go_info!(); }
                match r {
                    LessonsPanelResult::GoBack      => { lessons_state = LessonsState::new(); app_state = pop_nav!(AppState::MainMenu); }
                    LessonsPanelResult::GoTheory    => { theory_file = LessonsFile::load_theory(lang_ru); theory_state = LessonsState::new(); theory_tried.clear(); push_nav!(); app_state = AppState::Theory; }
                    LessonsPanelResult::GoTuner     => { if mc.current_stream.is_none() { mc.refresh_devices(); if !mc.available_devices.is_empty() { let _ = mc.start_stream_with_device_index(msi); } } tuner_frozen = false; tuner_msg = Some(("← из Уроков".into(), 2.5)); push_nav!(); app_state = AppState::Tuner; }
                    LessonsPanelResult::GoPractice  => { push_nav!(); app_state = AppState::SongSelection; }
                    LessonsPanelResult::PlaySong(ref path) => {
                        let sp = path.trim_start_matches('/').to_string();
                        match sl.load_single_song(&sp) {
                            Ok(song) => { 
                                push_nav!(); 
                                tuner_output.stop_playback(); 
                                // ДОБАВЛЕНО: Instrument::Guitar
                                let mut ns = GameState::new(output_arc.clone(), &song, lang_ru, layout.clone(), Instrument::Guitar).await; 
                                ns.audio_data = mc.get_audio_state(); 
                                game_state = Some(ns); 
                                game_from_lesson = true; 
                                app_state = AppState::Game; 
                            }
                            Err(e) => eprintln!("Cannot load lesson song '{}': {}", sp, e),
                        }
                    }
                    LessonsPanelResult::None => { if bv { lessons_state = LessonsState::new(); app_state = pop_nav!(AppState::MainMenu); } }
                }
            }

            // ── СТУДИЯ (ГЕНЕРАЦИЯ WAV) ────────────────────────────────────────────────
            AppState::Studio => {
                clear_background(Color::new(0.05, 0.05, 0.08, 1.));
                theme.draw_background(get_time());
                
                if studio_generated_files.is_empty() || is_key_pressed(KeyCode::R) {
                    studio_generated_files = scan_generated_sounds();
                    if studio_selected_file >= studio_generated_files.len() {
                        studio_selected_file = studio_generated_files.len().saturating_sub(1);
                    }
                }
                
                if let Some((_, ref mut t)) = studio_message {
                    *t -= dt as f64;
                    if *t <= 0.0 { studio_message = None; }
                }
                
                if is_key_pressed(KeyCode::W) && studio_song_idx > 0 { studio_song_idx -= 1; }
                if is_key_pressed(KeyCode::S) && studio_song_idx < sl.songs.len().saturating_sub(1) { studio_song_idx += 1; }
                
                if is_key_pressed(KeyCode::Up) && studio_selected_file > 0 {
                    studio_selected_file -= 1;
                    if studio_is_playing {
                        if let Ok(mut o) = output_arc.lock() { o.stop_playback(); }
                        studio_is_playing = false;
                        studio_current_playing_file = None;
                    }
                }
                if is_key_pressed(KeyCode::Down) && studio_selected_file < studio_generated_files.len().saturating_sub(1) {
                    studio_selected_file += 1;
                    if studio_is_playing {
                        if let Ok(mut o) = output_arc.lock() { o.stop_playback(); }
                        studio_is_playing = false;
                        studio_current_playing_file = None;
                    }
                }
                
                // Воспроизведение по Space
                if is_key_pressed(KeyCode::Space) && !studio_generated_files.is_empty() {
                    let file_path = format!("sounds/{}", studio_generated_files[studio_selected_file]);
                    if studio_is_playing {
                        if let Ok(mut o) = output_arc.lock() { o.stop_playback(); }
                        studio_is_playing = false;
                        studio_current_playing_file = None;
                        studio_message = Some(("⏹ Stopped".into(), 2.0));
                    } else {
                        match load_wav_samples(&file_path) {
                            Ok((samples, file_sr)) => {
                                if let Ok(mut o) = output_arc.lock() {
                                    let dev_sr = o.active_sample_rate;
                                    let resampled = resample_linear(&samples, file_sr, dev_sr);
                                    let dur = resampled.len() as f64 / dev_sr as f64;
                                    o.play(PlayRequest { samples: resampled, volume: 1.0, duration: dur });
                                    studio_is_playing = true;
                                    studio_current_playing_file = Some(file_path);
                                    studio_message = Some((format!("Playing: {}", studio_generated_files[studio_selected_file]), 3.0));
                                }
                            }
                            Err(e) => {
                                studio_message = Some((format!("❌ Load error: {}", e), 4.0));
                            }
                        }
                    }
                }
                
                // 1. Сначала рисуем UI (это создаёт временные иммутабельные заимствования)
                let ui_result = graphics::draw_studio_screen(
                    &gc, &font, &sl.songs, studio_song_idx,
                    &studio_generated_files, studio_selected_file,
                    studio_is_playing, studio_rendering,
                    studio_message.as_ref().map(|(m, _)| m.as_str()),
                    &theme,
                );
                
                // 2. Обрабатываем клики (после того как UI отрисован и заимствования сняты)
                if let Some(idx) = ui_result.song_click { studio_song_idx = idx; }
                if let Some(idx) = ui_result.file_click {
                    studio_selected_file = idx;
                    if studio_is_playing {
                        if let Ok(mut o) = output_arc.lock() { o.stop_playback(); }
                        studio_is_playing = false;
                        studio_current_playing_file = None;
                    }
                }
                
                // 3. Генерация случайной мелодии (кнопка в нижней правой панели)
                if ui_result.generate_melody_click && !studio_rendering {
                    studio_rendering = true;
                    
                    // Генерируем мелодию
                    let events = audio_engine::generate_random_melody(60.0);
                    
                    // Создаём JSON файл
                    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
                    let song_name = format!("random_melody_{}", timestamp);
                    let json_path = format!("songs/{}.json", song_name);
                    
                    use serde_json::json;
                    let jdata = json!({
                        "name": song_name,
                        "tuning": [82.41, 110.00, 146.83, 196.00, 246.94, 329.63],
                        "time_signature": [4, 4],
                        "initial_tempo": 120.0,
                        "lesson_mode": false,
                        "events": events.iter().map(|ev| json!({
                            "time": ev.time,
                            "duration": ev.duration,
                            "sound_duration": ev.sound_duration,
                            "notes": ev.notes.iter().map(|n| json!({
                                "string": n.string_idx,
                                "fret": n.fret
                            })).collect::<Vec<_>>()
                        })).collect::<Vec<_>>()
                    });
                    
                    match std::fs::write(&json_path, serde_json::to_string_pretty(&jdata).unwrap()) {
                        Ok(_) => {
                            // Перезагружаем песни
                            sl.load_songs_from_folder("songs");
                            
                            // Генерируем WAV
                            let wav_path = format!("sounds/{}.wav", song_name);
                            let mut sampler = audio_engine::GuitarSampler::new();
                            sampler.try_load_all();
                            
                            match audio_engine::render_song_to_wav(&events, &[82.41, 110.00, 146.83, 196.00, 246.94, 329.63], &wav_path, Some(&sampler)) {
                                Ok(_) => {
                                    studio_generated_files = scan_generated_sounds();
                                    studio_message = Some((format!("✅ Generated: {}.wav", song_name), 4.0));
                                }
                                Err(e) => {
                                    studio_message = Some((format!("❌ WAV Error: {}", e), 4.0));
                                }
                            }
                        }
                        Err(e) => {
                            studio_message = Some((format!("❌ JSON Error: {}", e), 4.0));
                        }
                    }
                    
                    studio_rendering = false;
                }

                // Обработка клика по клавише пианино
                if let Some(note_name) = ui_result.piano_key_click {
                    // Загружаем WAV файл и воспроизводим
                    let wav_path = format!("assets/piano/{}.wav", note_name);
                    match load_wav_samples(&wav_path) {
                        Ok((samples, file_sr)) => {
                            if let Ok(mut o) = output_arc.lock() {
                                let dev_sr = o.active_sample_rate;
                                let resampled = resample_linear(&samples, file_sr, dev_sr);
                                let dur = resampled.len() as f64 / dev_sr as f64;
                                o.play(PlayRequest { 
                                    samples: resampled, 
                                    volume: 1.0, 
                                    duration: dur 
                                });
                                studio_message = Some((format!("🎹 {}", note_name), 1.5));
                            }
                            // Добавляем ноту в последовательность
                            studio_loaded_notes.push(note_name);
                        }
                        Err(e) => {
                            studio_message = Some((format!("❌ Нет файла: {}.wav", note_name), 3.0));
                        }
                    }
                }
                
                // Генерация мелодии из загруженных нот
                if ui_result.generate_sequence_click && !studio_loaded_notes.is_empty() {
                    studio_rendering = true;
                    
                    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
                    let song_name = format!("piano_melody_{}", timestamp);
                    let wav_path = format!("sounds/{}.wav", song_name);
                    
                    match audio_engine::concatenate_piano_wavs(&studio_loaded_notes, &wav_path) {
                        Ok(_) => {
                            studio_generated_files = scan_generated_sounds();
                            studio_message = Some((format!("✅ Generated: {}.wav ({} notes)", song_name, studio_loaded_notes.len()), 4.0));
                            // Очищаем последовательность после генерации
                            studio_loaded_notes.clear();
                        }
                        Err(e) => {
                            studio_message = Some((format!("❌ Error: {}", e), 4.0));
                        }
                    }
                    
                    studio_rendering = false;
                }
                
                // 4. Генерация WAV из выбранной песни (кнопка под списком песен слева)
                if ui_result.generate_wav_click && !studio_rendering && !sl.songs.is_empty() {
                    studio_rendering = true;
                    let song = &sl.songs[studio_song_idx];
                    let output_path = format!("sounds/{}.wav", song.name);
                    
                    let mut sampler = audio_engine::GuitarSampler::new();
                    sampler.try_load_all();
                    
                    match audio_engine::render_song_to_wav(&song.events, &song.tuning, &output_path, Some(&sampler)) {
                        Ok(_) => {
                            studio_generated_files = scan_generated_sounds();
                            studio_message = Some((format!("✅ Generated: {}.wav", song.name), 3.0));
                        }
                        Err(e) => studio_message = Some((format!("❌ Error: {}", e), 4.0)),
                    }
                    studio_rendering = false;
                }

                // 5. Выбор выходного устройства (кнопка в правом верхнем углу или клавиша O)
                if ui_result.output_select || is_key_pressed(KeyCode::O) {
                    push_nav!();
                    app_state = AppState::StudioOutputSelection;
                }
                
                // 6. Оверлей-кнопки и навигация
                let (bv, lv, _, iv) = graphics::draw_overlay_buttons_with_locale(
                    &gc, &font, &locale, &mut theme, Some(&locale.overlay_back)
                );
                if lv { reload_lang!(); }
                if iv { go_info!(); }
                if bv { 
                    if studio_is_playing {
                        if let Ok(mut o) = output_arc.lock() { o.stop_playback(); }
                        studio_is_playing = false;
                        studio_current_playing_file = None;
                    }
                    app_state = pop_nav!(AppState::MainMenu); 
                }
                

                if is_key_pressed(KeyCode::Escape) {
                    if studio_is_playing {
                        if let Ok(mut o) = output_arc.lock() { o.stop_playback(); }
                        studio_is_playing = false;
                        studio_current_playing_file = None;
                    }
                    app_state = pop_nav!(AppState::MainMenu);
                }
            }

            // ── ВЫБОР ВЫХОДНОГО УСТРОЙСТВА (STUDIO) ─────────────────────────────────
            AppState::StudioOutputSelection => {
                if is_key_pressed(KeyCode::Escape) { app_state = AppState::Studio; }
                if is_key_pressed(KeyCode::Up) && studio_osi > 0 { studio_osi -= 1; }
                if is_key_pressed(KeyCode::Down) {
                    let cnt = output_arc.lock().map(|o| o.list_devices().len()).unwrap_or(0);
                    if studio_osi < cnt.saturating_sub(1) { studio_osi += 1; }
                }
                clear_background(Color::new(0.05, 0.05, 0.08, 1.));
                theme.draw_background(get_time());
                
                let devs = output_arc.lock().map(|o| o.list_devices()).unwrap_or_default();
                let (new_osi, clicked) = graphics::draw_device_list_menu(
                    &gc, &devs, studio_osi, &font, lang_ru, &locale, &theme
                );
                studio_osi = new_osi;
                
                if clicked || is_key_pressed(KeyCode::Enter) {
                    let res = output_arc.lock().map_err(|_| "lock failed".to_string()).and_then(|mut o| o.start(studio_osi));
                    match res {
                        Ok(_) => {
                            let n = output_arc.lock().map(|o| o.list_devices().get(studio_osi).cloned().unwrap_or_default()).unwrap_or_default();
                            studio_message = Some((format!("✅ Выход: {}", n), 3.0));
                        }
                        Err(e) => {
                            studio_message = Some((format!("❌ Ошибка: {}", e), 4.0));
                        }
                    }
                    app_state = AppState::Studio;
                }
                
                let (bv, lv, _, iv) = graphics::draw_overlay_buttons_with_locale(
                    &gc, &font, &locale, &mut theme, Some(&locale.overlay_back)
                );
                if lv { reload_lang!(); }
                if iv { go_info!(); }
                if bv { app_state = AppState::Studio; }
                
                draw_text_ex("Enter / клик по строке — выбрать | Esc — назад",
                    gc.sx(gc.layout.window_w / 2.0 - 160.0), gc.sy(gc.layout.window_h - 30.0),
                    TextParams { font_size: gc.s(16.0) as u16, font: Some(&font), color: Color::new(0.7,0.7,0.8,1.0), ..Default::default() });
            }

            // ── ТЕОРИЯ ────────────────────────────────────────────────────────
            AppState::Theory => {
                clear_background(Color::new(0.06, 0.05, 0.03, 1.)); theme.draw_background(get_time());
                for p in collect_missing_textures(&theory_file, &theory_state, &tex_cache, &theory_tried) {
                    theory_tried.insert(p.clone());
                    match macroquad::texture::load_texture(&p).await {
                        Ok(t) => { tex_cache.insert(p, t); } Err(e) => { eprintln!("Tex '{}': {}", p, e); }
                    }
                }
                let r = graphics::draw_theory_panel(&gc, &font, &theory_file, &mut theory_state, &locale, &tex_cache, &theme);
                let (bv, lv, _, iv) = graphics::draw_overlay_buttons_with_locale(&gc, &font, &locale, &mut theme, Some(&locale.overlay_back));
                if lv { reload_lang!(); } if iv { go_info!(); }
                match r {
                    LessonsPanelResult::GoBack     => { theory_state = LessonsState::new(); app_state = pop_nav!(AppState::Lessons); }
                    LessonsPanelResult::GoTuner    => { if mc.current_stream.is_none() { mc.refresh_devices(); if !mc.available_devices.is_empty() { let _ = mc.start_stream_with_device_index(msi); } } tuner_frozen = false; tuner_msg = Some(("← из Базы".into(), 2.5)); push_nav!(); app_state = AppState::Tuner; }
                    LessonsPanelResult::GoPractice => { push_nav!(); app_state = AppState::SongSelection; }
                    LessonsPanelResult::PlaySong(ref path) => {
                        let sp = path.trim_start_matches('/').to_string();
                        match sl.load_single_song(&sp) {
                            Ok(song) => { 
                                push_nav!(); 
                                tuner_output.stop_playback(); 
                                let mut ns = GameState::new(output_arc.clone(), &song, lang_ru, layout.clone(), Instrument::Guitar).await; 
                                ns.audio_data = mc.get_audio_state(); 
                                game_state = Some(ns); 
                                app_state = AppState::Game; 
                            }
                            Err(e) => eprintln!("Cannot load theory song '{}': {}", sp, e),
                        }
                    }
                    _ => { if bv { theory_state = LessonsState::new(); app_state = pop_nav!(AppState::Lessons); } }
                }
            }

            // ── ВЫБОР ПЕСНИ ───────────────────────────────────────────────────
            // ── ПРАКТИКА (выбор модуля + список песен для Guitar) ─────────────────
            AppState::SongSelection => {
                if (is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Backspace)) && !search.active {
                    app_state = AppState::MainMenu;
                }
                if is_key_pressed(KeyCode::T) {
                    push_nav!(); app_state = AppState::Tuner;
                    if mc.current_stream.is_none() {
                        mc.refresh_devices();
                        if !mc.available_devices.is_empty() { let _ = mc.start_stream_with_device_index(msi); }
                    }
                }
                if !search.active {
                    if is_key_pressed(KeyCode::Up)   && ssi > 0 { ssi -= 1; }
                    if is_key_pressed(KeyCode::Down)  && ssi < sl.songs.len().saturating_sub(1) { ssi += 1; }
                }
                clear_background(Color::new(0.05, 0.05, 0.08, 1.));
                theme.draw_background(get_time());
                
                let (new_ssi, actual, enter, _tuner_click, import_click, del_idx) =
                    graphics::draw_song_selection_menu(&gc, &sl.songs, ssi, &font, lang_ru, &mut search, &locale, &mut theme);
                ssi = new_ssi;

                //Отрисовка и обработка выбора модуля практики ───
                let module_choice = graphics::draw_practice_modules(&gc, &font, &theme, practice_module);
                if module_choice != graphics::PracticeModuleChoice::None {
                    practice_module = module_choice;
                }

                if let Some(di) = del_idx {
                    if di < sl.songs.len() {
                        let jpath = PathBuf::from("songs").join(format!("{}.json", sl.songs[di].name));
                        if fs::remove_file(&jpath).is_ok() { println!("🗑 {:?}", jpath); }
                        sl.songs.remove(di);
                        let nl = filter_and_sort_songs(&sl.songs, &search.text).len();
                        if ssi >= nl && ssi > 0 { ssi = nl.saturating_sub(1); }
                    }
                }

                if import_click && !importing {
                    importing = true;
                    let tx2 = tx.clone();
                    thread::spawn(move || {
                        let f = rfd::FileDialog::new()
                            //.add_filter("MIDI", &["mid", "midi"])
                            //.add_filter("MusicXML", &["xml", "musicxml", "mxl"])
                            .add_filter("Все музыкальные файлы", &["xml", "musicxml", "mxl", "mid", "midi"])
                            .pick_file();
                        let _ = tx2.send(f);
                    });
                }

                // Обработка клика по TUNER в правом верхнем углу (старая кнопка)
                //if tuner_click {
                //    push_nav!(); app_state = AppState::Tuner;
                //    if mc.current_stream.is_none() {
                //        mc.refresh_devices();
                //        if !mc.available_devices.is_empty() { let _ = mc.start_stream_with_device_index(msi); }
                //    }
                //}

                // Запуск игры по Enter (Guitar-режим по умолчанию)
                if enter || (is_key_pressed(KeyCode::Enter) && !search.active) {
                    if !sl.songs.is_empty() {
                        tuner_output.stop_playback();
                        let instr = match practice_module {
                            graphics::PracticeModuleChoice::Piano => Instrument::Piano,
                            _ => Instrument::Guitar,
                        };
                        let mut ns = GameState::new(output_arc.clone(), &sl.songs[actual], lang_ru, layout.clone(), instr).await;
                        ns.audio_data = mc.get_audio_state();

                        game_state = Some(ns);
                        game_from_lesson = false;
                        app_state = AppState::Game;
                    }
                }

                // ── Обработка выбора модуля ─────────────────────────────────────────
                match module_choice {
                    graphics::PracticeModuleChoice::Guitar => {
                        // Просто переключаем активный модуль — игра запустится по Enter
                        practice_module = graphics::PracticeModuleChoice::Guitar;
                    }
                    graphics::PracticeModuleChoice::Piano => {
                        // Переключаем инструмент на пианино, оставаясь в Практике
                        practice_module = graphics::PracticeModuleChoice::Piano;
                    }
                    graphics::PracticeModuleChoice::Tuner => {
                        push_nav!(); app_state = AppState::Tuner;
                        if mc.current_stream.is_none() {
                            mc.refresh_devices();
                            if !mc.available_devices.is_empty() { let _ = mc.start_stream_with_device_index(msi); }
                        }
                    }
                    graphics::PracticeModuleChoice::None => {}
                }

                let (bv, lv, _, iv) = graphics::draw_overlay_buttons_with_locale(&gc, &font, &locale, &mut theme, Some(&locale.overlay_back));
                if lv { reload_lang!(); } if iv { go_info!(); } if bv { app_state = pop_nav!(AppState::MainMenu); }
            }

            // ── INFO ──────────────────────────────────────────────────────────
            AppState::Info => {
                clear_background(Color::new(0.03, 0.04, 0.08, 1.)); theme.draw_background(get_time());
                for img in &info_file.images {
                    if !info_tried.contains(&img.path) {
                        info_tried.insert(img.path.clone());
                        match macroquad::texture::load_texture(&img.path).await {
                            Ok(t) => { info_tex.insert(img.path.clone(), t); }
                            Err(e) => { eprintln!("Info img '{}': {}", img.path, e); }
                        }
                    }
                }
                let sections: Vec<(&str,&str)> = info_file.sections.iter().map(|s| (s.heading.as_str(), s.body.as_str())).collect();
                let imgs: Vec<graphics::InfoImageData> = info_file.images.iter()
                    .filter_map(|i| info_tex.get(&i.path).map(|t| graphics::InfoImageData { texture: t, x: i.x, y: i.y, w: i.w, alpha: i.alpha }))
                    .collect();
                let (close, pref) = graphics::draw_info_screen(&gc, &font, &info_file.title, &info_file.show_on_start_label, &sections, &imgs, show_on_start, &mut info_state.scroll_offset, &locale, &theme);
                if let Some(v) = pref { show_on_start = v; info_prefs.set_show_on_start(v); }
                let (bv, lv, _, _) = graphics::draw_overlay_buttons_with_locale(&gc, &font, &locale, &mut theme, Some(&locale.overlay_back));
                if lv { lang_ru = !lang_ru; locale = Localization::load(lang_ru); lessons_file = LessonsFile::load(lang_ru); theory_file = LessonsFile::load_theory(lang_ru); info_file = InfoFile::load(lang_ru); info_tried.clear(); info_tex.clear(); }
                if close || bv { app_state = pop_nav!(AppState::MainMenu); }
            }

            // ── ТЮНЕР ─────────────────────────────────────────────────────────
            AppState::Tuner => {
                if is_key_pressed(KeyCode::Escape) { if let Some(m) = mc.stop_recording_if_active(Some("Tuner")) { println!("{}", m); } tuner_playing = false; tuner_output.stop_playback(); app_state = pop_nav!(AppState::SongSelection); tuner_frozen = false; tuner_msg = None; }
                if is_key_pressed(KeyCode::O) { tuner_playing = false; tuner_output.stop_playback(); app_state = AppState::TunerOutputSelection; }
                if is_key_pressed(KeyCode::R) { let was = mc.audio_data.lock().map(|d| d.is_recording).unwrap_or(false); if let Some(m) = mc.toggle_recording(Some("Tuner")) { tuner_msg = Some((m, 3.0)); } if was { tuner_records = scan_records_folder(); } }
                if is_key_pressed(KeyCode::W) && msi > 0 { msi -= 1; let _ = mc.start_stream_with_device_index(msi); }
                if is_key_pressed(KeyCode::S) { mc.refresh_devices(); if msi < mc.available_devices.len().saturating_sub(1) { msi += 1; let _ = mc.start_stream_with_device_index(msi); } }
                if !tuner_records.is_empty() {
                    if is_key_pressed(KeyCode::Up)   && tuner_rec_sel > 0 { tuner_rec_sel -= 1; if tuner_playing { tuner_playing = false; tuner_output.stop_playback(); } }
                    if is_key_pressed(KeyCode::Down)  && tuner_rec_sel < tuner_records.len().saturating_sub(1) { tuner_rec_sel += 1; if tuner_playing { tuner_playing = false; tuner_output.stop_playback(); } }
                }
                if is_key_pressed(KeyCode::Enter) && !tuner_records.is_empty() {
                    if tuner_playing { tuner_playing = false; tuner_output.stop_playback(); tuner_msg = Some(("Стоп".into(), 2.0)); }
                    else { let (ok, m) = tuner_play_record(tuner_rec_sel, &tuner_records, &tuner_output); tuner_playing = ok; tuner_msg = Some((m, if ok { 4.0 } else { 3.0 })); }
                }
                if is_key_pressed(KeyCode::F) { tuner_frozen = !tuner_frozen; if !tuner_frozen { tuner_hold = 2.0; } }
                // ── Переименование записей ──────────────────────────────────────────────
                if let Some(idx) = tuner_renaming {
                    // Активен режим переименования
                    if is_key_pressed(KeyCode::Escape) {
                        tuner_renaming = None;
                        tuner_rename_buffer.clear();
                        tuner_rename_cursor = 0;
                        tuner_rename_selection = None;
                    }
                    else if is_key_pressed(KeyCode::Enter) && !tuner_rename_buffer.is_empty() {
                        // Подтверждение переименования
                        if idx < tuner_records.len() {
                            let old_name = tuner_records[idx].clone();
                            let mut new_name = tuner_rename_buffer.trim().to_string();
                            new_name = new_name.chars()
                                .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' { c } else { '_' })
                                .collect::<String>();
                            if !new_name.ends_with(".wav") { new_name.push_str(".wav"); }
                            if !new_name.is_empty() && new_name != old_name {
                                let old_path = format!("records/{}", old_name);
                                let new_path = format!("records/{}", new_name);
                                match std::fs::rename(&old_path, &new_path) {
                                    Ok(_) => {
                                        tuner_records[idx] = new_name.clone();
                                        tuner_records.sort();
                                        tuner_msg = Some((format!("✅ {}", new_name), 2.5));
                                    }
                                    Err(e) => {
                                        tuner_msg = Some((format!("❌ Ошибка: {}", e), 3.0));
                                    }
                                }
                            }
                        }
                        tuner_renaming = None;
                        tuner_rename_buffer.clear();
                        tuner_rename_cursor = 0;
                        tuner_rename_selection = None;
                    }
                    else {
                        // Обработка выделения и ввода
                        let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
                        
                        // Управление курсором и выделением
                        if is_key_pressed(KeyCode::Left) {
                            if shift {
                                // Расширение выделения влево
                                if tuner_rename_selection.is_none() {
                                    tuner_rename_selection = Some((tuner_rename_cursor, tuner_rename_cursor));
                                }
                                if let Some((start, end)) = tuner_rename_selection {
                                    if end > 0 {
                                        tuner_rename_selection = Some((start, end - 1));
                                        tuner_rename_cursor = end - 1;
                                    }
                                }
                            } else {
                                tuner_rename_selection = None;
                                if tuner_rename_cursor > 0 { tuner_rename_cursor -= 1; }
                            }
                        }
                        if is_key_pressed(KeyCode::Right) {
                            if shift {
                                if tuner_rename_selection.is_none() {
                                    tuner_rename_selection = Some((tuner_rename_cursor, tuner_rename_cursor));
                                }
                                if let Some((start, end)) = tuner_rename_selection {
                                    if end < tuner_rename_buffer.len() {
                                        tuner_rename_selection = Some((start, end + 1));
                                        tuner_rename_cursor = end + 1;
                                    }
                                }
                            } else {
                                tuner_rename_selection = None;
                                if tuner_rename_cursor < tuner_rename_buffer.len() { tuner_rename_cursor += 1; }
                            }
                        }
                        if is_key_pressed(KeyCode::Home) {
                            if shift {
                                tuner_rename_selection = Some((tuner_rename_cursor, 0));
                            } else {
                                tuner_rename_selection = None;
                            }
                            tuner_rename_cursor = 0;
                        }
                        if is_key_pressed(KeyCode::End) {
                            if shift {
                                tuner_rename_selection = Some((tuner_rename_cursor, tuner_rename_buffer.len()));
                            } else {
                                tuner_rename_selection = None;
                            }
                            tuner_rename_cursor = tuner_rename_buffer.len();
                        }
                        
                        // Ctrl+A — выделить всё
                        if is_key_down(KeyCode::LeftControl) && is_key_pressed(KeyCode::A) {
                            tuner_rename_selection = Some((0, tuner_rename_buffer.len()));
                            tuner_rename_cursor = tuner_rename_buffer.len();
                        }
                        
                        // Ввод символов
                        while let Some(c) = get_char_pressed() {
                            if !c.is_control() {
                                // Если есть выделение — заменяем его
                                if let Some((sel_start, sel_end)) = tuner_rename_selection {
                                    let sel_min = sel_start.min(sel_end);
                                    let sel_max = sel_start.max(sel_end);
                                    let before: String = tuner_rename_buffer.chars().take(sel_min).collect();
                                    let after: String = tuner_rename_buffer.chars().skip(sel_max).collect();
                                    tuner_rename_buffer = format!("{}{}{}", before, c, after);
                                    tuner_rename_cursor = sel_min + 1;
                                    tuner_rename_selection = None;
                                } else {
                                    tuner_rename_buffer.insert(tuner_rename_cursor, c);
                                    tuner_rename_cursor += 1;
                                }
                            }
                        }
                        
                        // Backspace
                        if is_key_pressed(KeyCode::Backspace) {
                            if let Some((sel_start, sel_end)) = tuner_rename_selection {
                                let sel_min = sel_start.min(sel_end);
                                let sel_max = sel_start.max(sel_end);
                                let before: String = tuner_rename_buffer.chars().take(sel_min).collect();
                                let after: String = tuner_rename_buffer.chars().skip(sel_max).collect();
                                tuner_rename_buffer = format!("{}{}", before, after);
                                tuner_rename_cursor = sel_min;
                                tuner_rename_selection = None;
                            } else if tuner_rename_cursor > 0 {
                                tuner_rename_buffer.remove(tuner_rename_cursor - 1);
                                tuner_rename_cursor -= 1;
                            }
                        }
                        
                        // Delete
                        if is_key_pressed(KeyCode::Delete) {
                            if let Some((sel_start, sel_end)) = tuner_rename_selection {
                                let sel_min = sel_start.min(sel_end);
                                let sel_max = sel_start.max(sel_end);
                                let before: String = tuner_rename_buffer.chars().take(sel_min).collect();
                                let after: String = tuner_rename_buffer.chars().skip(sel_max).collect();
                                tuner_rename_buffer = format!("{}{}", before, after);
                                tuner_rename_cursor = sel_min;
                                tuner_rename_selection = None;
                            } else if tuner_rename_cursor < tuner_rename_buffer.len() {
                                tuner_rename_buffer.remove(tuner_rename_cursor);
                            }
                        }
                    }
                } else {
                    // Режим переименования НЕ активен — обрабатываем F2
                    if is_key_pressed(KeyCode::F2) && !tuner_records.is_empty() {
                        tuner_renaming = Some(tuner_rec_sel);
                        let cur = &tuner_records[tuner_rec_sel];
                        tuner_rename_buffer = cur.strip_suffix(".wav").unwrap_or(cur).to_string();
                        tuner_rename_cursor = tuner_rename_buffer.len();
                        tuner_rename_selection = Some((0, tuner_rename_buffer.len())); // Выделяем всё
                    }
                }
                let cur_f = mc.audio_data.lock().ok().and_then(|d| d.detected_freq);
                if !tuner_frozen {
                    if let Some(f) = cur_f { if is_valid_guitar_freq(f) { tuner_freq = Some(f); tuner_note = get_note_name(f); tuner_hold = 2.0; } }
                    else { tuner_hold -= dt; if tuner_hold <= 0.0 { tuner_note = "--".into(); tuner_freq = None; } }
                }
                if let Some((_, ref mut t)) = tuner_msg { *t -= dt; if *t <= 0.0 { tuner_msg = None; } }
                let rec_active = mc.audio_data.lock().map(|d| d.is_recording).unwrap_or(false);
                let dn    = if tuner_hold > 0.0 || tuner_frozen { tuner_note.clone() } else { "--".into() };
                let nc    = if tuner_frozen { YELLOW } else if tuner_hold > 0.0 { GREEN } else { GRAY };
                let wdata = mc.audio_data.lock().map(|d| d.wave_buffer.clone()).unwrap_or_default();
                let mnames: Vec<String> = mc.available_devices.iter().map(|d| d.name().unwrap_or_else(|_| "Unknown".into())).collect();
                let tout_name = tuner_output.list_devices().get(tuner_osi).cloned().unwrap_or_else(|| "—".into());
                clear_background(Color::new(0.05, 0.05, 0.08, 1.)); theme.draw_background(get_time());
                
                // ─── В блоке AppState::Tuner заменить вызов draw_tuner_screen ────────────────
                let cur_tuning = tuner_tunings.get(tuner_selected_tuning)
                    .cloned()
                    .unwrap_or_else(|| tuner_presets::all_tunings()[0].clone());

                let tuner_clicks = graphics::draw_tuner_screen(
                    &gc, &font, &dn, nc, tuner_freq, tuner_frozen, rec_active,
                    &wdata, &mnames, msi,
                    tuner_msg.as_ref().map(|(m,_)| m.as_str()),
                    &locale, &theme,
                    &cur_tuning,
                    tuner_selected_string,
                    tuner_tuning_menu_open,
                );

                // ─── Обработка кликов по новым элементам ─────────────────────────────────────
                // Клик по струне
                if let Some(si) = tuner_clicks.string_clicked {
                    tuner_selected_string = si;
                    let note = cur_tuning.string_note(si);
                    let freq = cur_tuning.string_freq(si);
                    tuner_msg = Some((format!("🎸 Струна {}: {} ({:.2} Hz)", 6 - si, note, freq), 2.5));
                }

                // Клик по кнопке меню строев
                if tuner_clicks.tuning_menu_toggle {
                    tuner_tuning_menu_open = !tuner_tuning_menu_open;
                }

                // Выбор строя из меню
                if let Some(new_idx) = tuner_clicks.tuning_selected {
                    if new_idx < tuner_tunings.len() {
                        tuner_selected_tuning = new_idx;
                        tuner_tuning_menu_open = false;
                        let t = &tuner_tunings[new_idx];
                        let name = if lang_ru { &t.name_ru } else { &t.name };
                        tuner_msg = Some((format!("🎼 Строй: {}", name), 2.5));
                    }
                }

                // Автоматический выбор струны по детектированной частоте (опционально)
                // Если частота ближе к другой струне текущего строя — переключаемся
                if !tuner_frozen {
                    if let Some(df) = tuner_freq {
                        let mut best_si = tuner_selected_string;
                        let mut best_diff = f32::MAX;
                        for si in 0..6 {
                            let target = cur_tuning.string_freq(si);
                            let diff = (df - target).abs() / target;
                            if diff < best_diff && diff < 0.08 {
                                best_diff = diff;
                                best_si = si;
                            }
                        }
                        if best_si != tuner_selected_string {
                            tuner_selected_string = best_si;
                        }
                    }
                }
                
                let out_label = { let s = &tout_name; if s.chars().count() > 38 { format!("Выход (O): {}…", s.chars().take(37).collect::<String>()) } else { format!("Выход (O): {}", s) } };
                graphics::draw_tuner_output_label(&gc, &font, &out_label, &theme);
                
                let tuner_controls = graphics::draw_tuner_controls(&gc, &font, rec_active, &theme);
                if tuner_controls.mic_select    { push_nav!(); app_state = AppState::TunerMicSelection; }
                if tuner_controls.output_select { tuner_playing = false; tuner_output.stop_playback(); app_state = AppState::TunerOutputSelection; }
                if tuner_controls.record_toggle {
                    let was = rec_active;
                    if let Some(m) = mc.toggle_recording(Some("Tuner")) { tuner_msg = Some((m, 3.0)); }
                    if was { tuner_records = scan_records_folder(); }
                }

                let (new_sel, play_click, stop_click, rename_request, _rename_cancel, new_cursor, new_selection) =
                    graphics::draw_tuner_records_panel(
                        &gc, &tuner_records, tuner_rec_sel, tuner_playing,
                        tuner_renaming, &tuner_rename_buffer, tuner_rename_cursor, tuner_rename_selection,
                        &font, &theme
                    );

                if new_sel != tuner_rec_sel && tuner_playing {
                    tuner_playing = false;
                    tuner_output.stop_playback();
                }
                tuner_rec_sel = new_sel;
                tuner_rename_cursor = new_cursor;
                tuner_rename_selection = new_selection;

                // Клик по остальному экрану — завершение переименования
                let (mx_log, my_log) = (mouse_position().0 / gc.scale_x, mouse_position().1 / gc.scale_y);
                let list_x = gc.base_w - 345.0;
                let list_y = 150.0_f32;
                let list_w = 300.0_f32;
                let list_h = 780.0_f32;
                let click_outside = is_mouse_button_pressed(MouseButton::Left) 
                    && !(mx_log >= list_x && mx_log <= list_x + list_w && my_log >= list_y && my_log <= list_y + list_h);

                if click_outside && tuner_renaming.is_some() && !tuner_rename_buffer.is_empty() {
                    // Сохраняем переименование
                    let idx = tuner_renaming.unwrap();
                    if idx < tuner_records.len() {
                        let old_name = tuner_records[idx].clone();
                        let mut new_name = tuner_rename_buffer.trim().to_string();
                        new_name = new_name.chars()
                            .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' { c } else { '_' })
                            .collect::<String>();
                        if !new_name.ends_with(".wav") { new_name.push_str(".wav"); }
                        if !new_name.is_empty() && new_name != old_name {
                            let old_path = format!("records/{}", old_name);
                            let new_path = format!("records/{}", new_name);
                            if std::fs::rename(&old_path, &new_path).is_ok() {
                                tuner_records[idx] = new_name;
                                tuner_records.sort();
                            }
                        }
                    }
                    tuner_renaming = None;
                    tuner_rename_buffer.clear();
                    tuner_rename_cursor = 0;
                    tuner_rename_selection = None;
                }

                // ПКМ запрос переименования из UI
                if rename_request && tuner_renaming.is_none() && !tuner_records.is_empty() {
                    tuner_renaming = Some(tuner_rec_sel);
                    let cur = &tuner_records[tuner_rec_sel];
                    tuner_rename_buffer = cur.strip_suffix(".wav").unwrap_or(cur).to_string();
                    tuner_rename_cursor = tuner_rename_buffer.len();
                    tuner_rename_selection = Some((0, tuner_rename_buffer.len())); // Выделяем всё
                }

                tuner_rename_cursor = new_cursor;
                tuner_rename_selection = new_selection;

                // ── Логика воспроизведения (ЛКМ = toggle Play/Stop) ─────────────────────
                if play_click && !stop_click && tuner_renaming.is_none() && !tuner_records.is_empty() {
                    let is_same_track = new_sel == tuner_rec_sel;
                    
                    if tuner_playing && is_same_track {
                        // 🔹 Повторный клик по той же записи → Остановить
                        tuner_playing = false;
                        tuner_output.stop_playback();
                        tuner_msg = Some(("⏹ Стоп".into(), 2.0));
                    } else {
                        // 🔹 Клик по новой записи (или клик после остановки) → Запустить
                        tuner_output.stop_playback(); // Гарантированный стоп перед запуском
                        let (ok, m) = tuner_play_record(new_sel, &tuner_records, &tuner_output);
                        tuner_playing = ok;
                        tuner_msg = Some((m, if ok { 4.0 } else { 3.0 }));
                    }
                    // Обновляем индекс выбранной записи
                    tuner_rec_sel = new_sel;
                } else {
                    // Если выбор изменился не кликом (скролл/стрелки), а что-то играет → стоп
                    if new_sel != tuner_rec_sel && tuner_playing {
                        tuner_playing = false;
                        tuner_output.stop_playback();
                    }
                    tuner_rec_sel = new_sel;
                }

                // Явная кнопка Stop (красный квадратик)
                if stop_click && tuner_playing {
                    tuner_playing = false;
                    tuner_output.stop_playback();
                    tuner_msg = Some(("Стоп".into(), 2.0));
                }


                let (bv, lv, _, iv) = graphics::draw_overlay_buttons_with_locale(&gc, &font, &locale, &mut theme, Some(&locale.overlay_back));
                if lv { reload_lang!(); } if iv { go_info!(); } if bv { tuner_playing = false; tuner_output.stop_playback(); app_state = pop_nav!(AppState::SongSelection); }
            }

            AppState::TunerMicSelection => {
                if is_key_pressed(KeyCode::Escape) { app_state = AppState::Tuner; }
                if is_key_pressed(KeyCode::Up)   && msi > 0 { msi -= 1; }
                if is_key_pressed(KeyCode::Down)  { mc.refresh_devices(); if msi < mc.available_devices.len().saturating_sub(1) { msi += 1; } }
                clear_background(Color::new(0.05, 0.05, 0.08, 1.)); theme.draw_background(get_time());
                let (new_msi, clicked) = graphics::draw_mic_selection_menu(&gc, &mut mc, msi, &font, lang_ru, &locale, &theme);
                msi = new_msi;
                if clicked || is_key_pressed(KeyCode::Enter) {
                    match mc.start_stream_with_device_index(msi) {
                        Ok(_)  => { tuner_msg = Some((format!("Mic #{}", msi), 2.5)); }
                        Err(e) => { tuner_msg = Some((format!("Mic error: {}", e), 3.0)); }
                    }
                    app_state = AppState::Tuner;
                }
                draw_text_ex("Enter / клик по строке — выбрать | Esc — назад",
                    gc.sx(gc.layout.window_w / 2.0 - 160.0), gc.sy(gc.layout.window_h - 30.0),
                    TextParams { font_size: gc.s(16.0) as u16, font: Some(&font), color: Color::new(0.7,0.7,0.8,1.0), ..Default::default() });
            }

            // ── ИНВЕРСИЯ СТРУН ────────────────────────────────────────────────
            AppState::StringInversion => {
                clear_background(Color::new(0.05, 0.05, 0.08, 1.)); theme.draw_background(get_time());
                let (confirmed, inv) = graphics::draw_string_inversion_menu(&gc, &font, &locale);
                if is_key_pressed(KeyCode::Escape) { app_state = AppState::InstrumentSelection; }
                else if confirmed {
                    if let Some((path, instrs)) = pending_import.take() {
                        match sl.import_song(&path, &instrs[instr_idx], inv, false) {
                            Ok(_)  => { app_state = AppState::SongSelection; }
                            Err(e) => { eprintln!("Import failed: {}", e); pending_import = Some((path, instrs)); app_state = AppState::InstrumentSelection; }
                        }
                    } else { app_state = AppState::SongSelection; }
                }
            }

            // ── ВЫБОР УСТРОЙСТВА ТЮНЕРА ───────────────────────────────────────
            AppState::TunerOutputSelection => {
                if is_key_pressed(KeyCode::Escape) { app_state = AppState::Tuner; }
                if is_key_pressed(KeyCode::Up)   && tuner_osi > 0 { tuner_osi -= 1; }
                if is_key_pressed(KeyCode::Down)  && tuner_osi < tuner_output.list_devices().len().saturating_sub(1) { tuner_osi += 1; }
                clear_background(Color::new(0.05, 0.05, 0.08, 1.)); theme.draw_background(get_time());
                let (new_osi, clicked) = graphics::draw_output_selection_menu(&gc, &tuner_output, tuner_osi, &font, lang_ru, &locale, &theme);
                tuner_osi = new_osi;
                if clicked || is_key_pressed(KeyCode::Enter) {
                    match tuner_output.start(tuner_osi) {
                        Ok(_)  => { let n = tuner_output.list_devices().get(tuner_osi).cloned().unwrap_or_default(); tuner_msg = Some((format!("Выход: {}", n), 3.0)); }
                        Err(e) => { tuner_msg = Some((format!("Ошибка: {}", e), 4.0)); }
                    }
                    app_state = AppState::Tuner;
                }
                draw_text_ex("Enter / клик по строке — выбрать | Esc — назад",
                    gc.sx(gc.layout.window_w / 2.0 - 160.0), gc.sy(gc.layout.window_h - 30.0),
                    TextParams { font_size: gc.s(18.0) as u16, font: Some(&font), color: Color::new(0.7,0.7,0.8,1.0), ..Default::default() });
            }

            // ── ВЫБОР ИНСТРУМЕНТА ─────────────────────────────────────────────
            AppState::InstrumentSelection => {
                if is_key_pressed(KeyCode::Escape) { app_state = AppState::SongSelection; pending_import = None; }
                if let Some((path, instrs)) = pending_import.take() {
                    if is_key_pressed(KeyCode::Up)   && instr_idx > 0 { instr_idx -= 1; }
                    if is_key_pressed(KeyCode::Down)  && instr_idx < instrs.len().saturating_sub(1) { instr_idx += 1; }
                    clear_background(Color::new(0.05, 0.05, 0.08, 1.)); theme.draw_background(get_time());
                    let (ni, _, confirmed) = graphics::draw_instrument_selection_menu(&gc, &instrs, instr_idx, &font, &locale, lang_ru);
                    instr_idx = ni;
                    if confirmed || is_key_pressed(KeyCode::Enter) {
                        pending_import = Some((path, instrs)); app_state = AppState::StringInversion;
                    } else { pending_import = Some((path, instrs)); }
                } else { app_state = AppState::SongSelection; }
            }

            // ── ИГРА ──────────────────────────────────────────────────────────
            AppState::Game => {
                let mut go_stats = false; let mut go_menu = false;

                // ── Пред-старт: "Вы готовы?" → отсчёт 3-2-1 → игра ──────────────────
                let phase = game_state.as_ref().map(|g| g.pre_start);
                if let Some(phase) = phase {
                    if phase != PreStartPhase::Playing {
                        if let Some(g) = &mut game_state {
                            clear_background(Color::new(0.06, 0.07, 0.10, 1.));
                            theme.draw_background(g.song_time);
                            
                            // ─── Условная отрисовка ───
                            if g.instrument == Instrument::Piano {
                                graphics::draw_piano_game_board(&gc, g, &font, &theme);
                            } else {
                                graphics::draw_fretboard(&gc, g, &font, &theme);
                            }
                            // ─────────────────────────────────────
                            
                            graphics::draw_ui(&gc, g, &font);

                            match phase {
                                PreStartPhase::Confirm => {
                                    match graphics::draw_pregame_confirm_dialog(&gc, &font, &locale, &theme) {
                                        graphics::PreGameConfirmResult::Yes  => { g.pre_start = PreStartPhase::Countdown(2.0); }
                                        graphics::PreGameConfirmResult::No   => { go_menu = true; }
                                        graphics::PreGameConfirmResult::None => {}
                                    }
                                }
                                PreStartPhase::Countdown(remaining) => {
                                    graphics::draw_countdown_overlay(&gc, &font, remaining, &locale);
                                    let next = remaining - dt;
                                    g.pre_start = if next <= 0.0 { PreStartPhase::Playing } else { PreStartPhase::Countdown(next) };
                                }
                                PreStartPhase::Playing => {}
                            }
                        }
                        if go_menu {
                            if game_from_lesson {
                                // Запуск был по ссылке из раздела «Уроки» — при отказе
                                // возвращаемся туда же, а не в общий список песен.
                                game_state = None;
                                game_from_lesson = false;
                                app_state = pop_nav!(AppState::Lessons);
                            } else {
                                app_state = AppState::SongSelection;
                                game_state = None;
                            }
                        }
                        next_frame().await;
                        continue;
                    }
                }


                //--------
                if let Some(g) = &mut game_state {
                    if is_key_pressed(KeyCode::Escape) {
                        lang_ru = g.lang_ru;
                        if g.auto_play_mode || g.auto_play_scheduled {
                            if let Ok(o) = g.output.lock() { o.stop_playback(); }
                            g.auto_play_mode = false; g.auto_play_scheduled = false;
                        }
                        if let Some(m) = mc.stop_recording_if_active(Some(&g.current_song_name)) { g.set_message(m); }
                        go_stats = true;
                    }
                    
                    if is_key_pressed(KeyCode::N) {
                        if let Some(so) = sl.songs.iter().find(|s| s.name == g.current_song_name) {
                            // ДОБАВЛЕНО: g.instrument
                            let mut ng = GameState::new(output_arc.clone(), so, g.lang_ru, layout.clone(), g.instrument).await;
                            ng.lang_ru = g.lang_ru;
                            ng.audio_data = mc.get_audio_state();
                            *g = ng;
                            g.set_message("Restart".into());
                        }
                    }
                                        
                    // ── Перемотка стрелками (работает на паузе и во время игры) ─────────
                    if is_key_pressed(KeyCode::Left) {
                        g.song_time = (g.song_time - 2.0).max(0.0);
                        g.last_spawn_idx = 0;
                        g.events.clear();
                        g.notes_compat.clear();
                        if g.auto_play_mode {
                            g.auto_play_scheduled = false;
                            g.schedule_all_notes();
                        }
                        g.set_message(" -2 сек".into());
                    }
                    if is_key_pressed(KeyCode::Right) {
                        g.song_time += 2.0;
                        g.last_spawn_idx = 0;
                        g.events.clear();
                        g.notes_compat.clear();
                        if g.auto_play_mode {
                            g.auto_play_scheduled = false;
                            g.schedule_all_notes();
                        }
                        g.set_message(" +2 сек".into());
                    }
                    
                    if is_key_pressed(KeyCode::Z) {
                        g.auto_play_mode = !g.auto_play_mode;
                        if g.auto_play_mode {
                            g.song_time = 0.0; g.last_spawn_idx = 0; g.events.clear(); g.notes_compat.clear();
                            g.score = 0; g.combo = 0; g.auto_play_scheduled = false;
                            g.schedule_all_notes();
                            let synth = if g.output.lock().map(|o| o.sampler_available()).unwrap_or(false) { "WAV Sampler" } else { "Karplus-Strong" };
                            g.set_message(format!("Auto Play ON [{}]", synth));
                        } else {
                            if let Ok(o) = g.output.lock() { o.stop_playback(); }
                            g.auto_play_scheduled = false; g.set_message("Auto Play OFF".into());
                        }
                    }
                    if is_key_pressed(KeyCode::O) { app_state = AppState::OutputSelection; }
                    if is_key_pressed(KeyCode::W) && g.speed_index < SPEED_MULTIPLIERS.len()-1 { g.speed_index += 1; g.set_message(format!("Speed: {}", SPEED_LABELS[g.speed_index])); }
                    if is_key_pressed(KeyCode::S) && g.speed_index > 0 { g.speed_index -= 1; g.set_message(format!("Speed: {}", SPEED_LABELS[g.speed_index])); }
                    if is_key_pressed(KeyCode::M) { app_state = AppState::MicSelection; }
                    
                    // ── ПАУЗА (Space) - работает всегда, кроме game_over ─────────────────
                    if !g.game_over && is_key_pressed(KeyCode::Space) { 
                        g.toggle_pause();
                        // Если включена пауза - останавливаем автопроигрывание
                        if g.paused && g.auto_play_mode {
                            if let Ok(o) = g.output.lock() { o.stop_playback(); }
                            g.auto_play_scheduled = false;
                        }
                    }
                    
                    let now = get_time();
                    if is_key_pressed(KeyCode::L) && now - last_lang_toggle > 0.3 {
                        g.toggle_language(); lang_ru = g.lang_ru;
                        locale = Localization::load(lang_ru);
                        lessons_file = LessonsFile::load(lang_ru); theory_file = LessonsFile::load_theory(lang_ru);
                        tex_tried.clear(); theory_tried.clear(); last_lang_toggle = now;
                    }
                    if is_key_pressed(KeyCode::R) { if let Some(m) = mc.toggle_recording(Some(&g.current_song_name)) { g.set_message(m); } }

                    if !g.game_over && !g.paused {
                        if !g.lesson_mode {
                            // ─── Детекция нот: MIDI (если подключён) или микрофон ───
                            if g.midi_data.is_some() {
                                g.check_midi_hit();
                            } else {
                                g.check_microphone_hit();
                            }
                        }
                        if is_key_pressed(KeyCode::Tab) { g.toggle_wave_type().await; }
                        if g.last_spawn_idx >= g.song_data.len() && g.events.is_empty() {
                            g.game_over = true;
                            if let Ok(o) = g.output.lock() { o.stop_playback(); }
                            g.auto_play_mode = false; g.auto_play_scheduled = false;
                            if let Some(m) = mc.stop_recording_if_active(Some(&g.current_song_name)) { g.set_message(m); }
                        }
                    } else if g.game_over {
                        // Когда игра окончена - Space переходит к статистике
                        if is_key_pressed(KeyCode::Space)     { go_stats = true; }
                        if is_key_pressed(KeyCode::Backspace) { lang_ru = g.lang_ru; go_menu = true; }
                    }

                    g.update(dt);
                    clear_background(Color::new(0.06, 0.07, 0.10, 1.));
                    theme.draw_background(g.song_time);

                    if g.instrument == Instrument::Piano {
                        graphics::draw_piano_game_board(&gc, g, &font, &theme);
                    } else {
                        graphics::draw_fretboard(&gc, g, &font, &theme);
                    }

                    graphics::draw_ui(&gc, g, &font);
                    graphics::draw_waveform_and_mic_info(&gc, &mc, g, &font, &theme);
                    graphics::draw_frequency_graph(&gc, &g.logger, g.song_time, &font, &theme);
                    if g.paused { graphics::draw_pause_overlay(&gc, g, &font); }
                    
                    if g.lesson_mode && g.lesson.paused {
                        let cur_ev = g.lesson.current_event_idx.and_then(|i| g.events.get(i));
                        let hint   = g.lesson_hint.as_ref();
                        graphics::draw_lesson_pause_overlay(&gc, &font, &g.lesson, hint, cur_ev);
                    }
                    if g.lesson_mode {
                        let lbl = if g.lesson.paused { "II УРОК — сыграйте аккорд" } else { "-> УРОК" };
                        let lfs = gc.s(16.0) as u16;
                        let lw  = measure_text(lbl, Some(&font), lfs, 1.0).width / gc.scale_x;
                        draw_text_ex(lbl,
                            gc.sx(gc.layout.window_w / 2.0 - lw / 2.0), gc.sy(gc.layout.window_h - 60.0),
                            TextParams { font_size: lfs, font: Some(&font),
                                color: if g.lesson.paused { Color::new(1.0, 0.75, 0.2, 1.0) }
                                    else { Color::new(0.4, 0.85, 0.4, 0.9) },
                                ..Default::default() });
                    }

                    let is_rec = mc.audio_data.lock().map(|d| d.is_recording).unwrap_or(false);
                    let controls = graphics::draw_game_controls(&gc, g, &font, is_rec, &theme);
                    if controls.mic        { app_state = AppState::MicSelection; }
                    if controls.output     { app_state = AppState::OutputSelection; }
                    if controls.pause      { 
                        g.toggle_pause();
                        if g.paused && g.auto_play_mode {
                            if let Ok(o) = g.output.lock() { o.stop_playback(); }
                            g.auto_play_scheduled = false;
                        }
                    }
                    if controls.speed_up   { g.change_speed(1); }
                    if controls.speed_down { g.change_speed(-1); }
                    if controls.automode   { g.toggle_automode(); }
                    if controls.record_toggle {
                        if let Some(m) = mc.toggle_recording(Some(&g.current_song_name)) { g.set_message(m); }
                    }

                    // ── Обработка клика по кнопке Restart (когда game_over) ─────────────
                    if g.game_over {
                        let btn_w = 180.0_f32;
                        let btn_h = 44.0_f32;
                        let btn_x = gc.layout.window_w / 2.0 - btn_w / 2.0;
                        let btn_y = gc.layout.window_h / 2.0 + 120.0;
                        
                        let (mx, my) = macroquad::input::mouse_position();
                        let mx_log = mx / gc.scale_x;
                        let my_log = my / gc.scale_y;
                        let lmb = is_mouse_button_pressed(MouseButton::Left);
                        let hover = mx_log >= btn_x && mx_log <= btn_x + btn_w && my_log >= btn_y && my_log <= btn_y + btn_h;
                        
                        if lmb && hover {
                            if let Some(so) = sl.songs.iter().find(|s| s.name == g.current_song_name) {
                                let mut ng = GameState::new(output_arc.clone(), so, g.lang_ru, layout.clone(), g.instrument).await;
                                ng.lang_ru = g.lang_ru;
                                ng.audio_data = mc.get_audio_state();
                                *g = ng;
                            }
                        }
                    }

                    let (back_ov, lang_ov, _theme_ov, info_ov) =
                        graphics::draw_overlay_buttons_with_locale(&gc, &font, &locale, &mut theme, Some(&locale.overlay_back));
                    if lang_ov && now - last_lang_toggle > 0.3 {
                        g.toggle_language(); lang_ru = g.lang_ru;
                        locale = Localization::load(lang_ru);
                        lessons_file = LessonsFile::load(lang_ru); theory_file = LessonsFile::load_theory(lang_ru);
                        tex_tried.clear(); theory_tried.clear(); last_lang_toggle = now;
                    }
                    if info_ov { go_info!(); }
                    if back_ov {
                        lang_ru = g.lang_ru;
                        go_stats = true;
                    }
                }
                if go_stats { app_state = AppState::Stats; }
                else if go_menu { app_state = AppState::SongSelection; game_state = None; }
            }

            // ── СТАТИСТИКА ────────────────────────────────────────────────────
            AppState::Stats => {
                let (stats, scroll, stat_lang, song_name) = match &game_state {
                    Some(g) => (analyze_song_logs_from_state(g), g.stats_scroll_offset, g.lang_ru, g.current_song_name.clone()),
                    None    => { app_state = AppState::SongSelection; next_frame().await; continue; }
                };
                let mut ns   = scroll;
                let te       = stats.details.len(); let mvr = 12;
                let log_size = format_bytes(get_logs_folder_size());
                let show_confirm = clear_confirm.map_or(false, |t| {
                    if t.elapsed() > Duration::from_secs(3) { clear_confirm = None; false } else { true }
                });
                if te > mvr {
                    if is_key_pressed(KeyCode::Up)       { ns = ns.saturating_sub(1); }
                    if is_key_pressed(KeyCode::Down)      { ns = (ns+1).min(te.saturating_sub(mvr)); }
                    if is_key_pressed(KeyCode::PageUp)   { ns = ns.saturating_sub(mvr); }
                    if is_key_pressed(KeyCode::PageDown) { ns = (ns+mvr).min(te.saturating_sub(mvr)); }
                }
                if is_key_pressed(KeyCode::K) {
                    if show_confirm { let _ = clear_logs_folder(); clear_confirm = None; }
                    else { clear_confirm = Some(Instant::now()); }
                }
                let (new_ns, action) = graphics::draw_stats_menu(&gc, &stats, &font, stat_lang, ns, &log_size, show_confirm, game_from_lesson, &locale, &theme);
                ns = new_ns;
                if let Some(ref mut g) = game_state { g.stats_scroll_offset = ns; }
                let (bv, lv, _, iv) = graphics::draw_overlay_buttons_with_locale(&gc, &font, &locale, &mut theme, Some(&locale.overlay_back));
                if lv { reload_lang!(); } if iv { go_info!(); }
                if bv {
                    if game_from_lesson { game_state = None; game_from_lesson = false; app_state = pop_nav!(AppState::Lessons); }
                    else { app_state = AppState::SongSelection; game_state = None; }
                }
                use graphics::StatsAction;
                match action {
                    StatsAction::Restart => {
                        if let Some(so) = sl.songs.iter().find(|s| s.name == song_name) {
                            // ДОБАВЛЕНО: получение инструмента из текущего состояния
                            let instr = game_state.as_ref().map(|gs| gs.instrument).unwrap_or(Instrument::Guitar);
                            let mut ng = GameState::new(output_arc.clone(), so, stat_lang, layout.clone(), instr).await;
                            ng.lang_ru = stat_lang; 
                            ng.audio_data = mc.get_audio_state();
                            game_state = Some(ng); 
                            app_state = AppState::Game;
                        }
                    }
                    StatsAction::BackToSongs => { app_state = AppState::SongSelection; game_state = None; game_from_lesson = false; }
                    StatsAction::ClearLogs   => { if show_confirm { let _ = clear_logs_folder(); clear_confirm = None; } else { clear_confirm = Some(Instant::now()); } }
                    StatsAction::None        => {
                        if is_key_pressed(KeyCode::Space) {
                            if let Some(so) = sl.songs.iter().find(|s| s.name == song_name) {
                                let instr = game_state.as_ref().map(|gs| gs.instrument).unwrap_or(Instrument::Guitar);
                                let mut ng = GameState::new(output_arc.clone(), so, stat_lang, layout.clone(), instr).await;
                                ng.lang_ru = stat_lang; 
                                ng.audio_data = mc.get_audio_state();
                                game_state = Some(ng); 
                                app_state = AppState::Game;
                            }
                        }
                        if is_key_pressed(KeyCode::Escape) {
                            if game_from_lesson { game_state = None; game_from_lesson = false; app_state = pop_nav!(AppState::Lessons); }
                            else { app_state = AppState::SongSelection; game_state = None; }
                        }
                    }
                }
            }

            // ── ВЫБОР МИКРОФОНА ───────────────────────────────────────────────
            AppState::MicSelection => {
                if is_key_pressed(KeyCode::Escape) { app_state = AppState::Game; }
                if is_key_pressed(KeyCode::Up)   && msi > 0 { msi -= 1; }
                if is_key_pressed(KeyCode::Down)  { mc.refresh_devices(); if msi < mc.available_devices.len().saturating_sub(1) { msi += 1; } }
                if let Some(g) = &game_state {
                    clear_background(Color::new(0.06, 0.07, 0.10, 1.)); theme.draw_background(get_time());
                    graphics::draw_waveform_and_mic_info(&gc, &mc, g, &font, &theme);
                    // ─── Индикатор MIDI ───
                    if let Some(ref midi) = g.midi_data {
                        if let Ok(md) = midi.lock() {
                            let (label, color) = if md.is_connected {
                                ("MIDI ON", macroquad::prelude::Color::new(0.2, 1.0, 0.4, 1.0))
                            } else {
                                ("MIDI OFF", macroquad::prelude::Color::new(1.0, 0.3, 0.3, 0.7))
                            };
                            draw_text_ex(label, gc.sx(10.0), gc.sy(20.0),
                                TextParams { font_size: gc.s(16.0) as u16, font: Some(&font), color, ..Default::default() });

                            // Показать активные (удерживаемые) ноты
                            if !md.active_notes.is_empty() {
                                let notes_str: Vec<String> = md.active_notes.iter()
                                    .map(|&n| crate::get_note_name(440.0 * 2.0_f32.powf((n as f32 - 69.0) / 12.0)))
                                    .collect();
                                let active_str = format!("♪ {}", notes_str.join(", "));
                                draw_text_ex(&active_str, gc.sx(10.0), gc.sy(40.0),
                                    TextParams { font_size: gc.s(14.0) as u16, font: Some(&font),
                                        color: macroquad::prelude::Color::new(1.0, 0.9, 0.3, 1.0), ..Default::default() });
                            }

                            // Показать последнюю нажатую ноту (крупно)
                            if let Some(last) = md.last_note {
                                let last_name = crate::get_note_name(440.0 * 2.0_f32.powf((last as f32 - 69.0) / 12.0));
                                let last_str = format!("Last: {} (midi={})", last_name, last);
                                draw_text_ex(&last_str, gc.sx(10.0), gc.sy(60.0),
                                    TextParams { font_size: gc.s(12.0) as u16, font: Some(&font),
                                        color: macroquad::prelude::Color::new(0.7, 0.8, 1.0, 0.9), ..Default::default() });
                            }
                        }
                    }
                    
                    graphics::draw_fretboard(&gc, g, &font, &theme);
                    graphics::draw_frequency_graph(&gc, &g.logger, g.song_time, &font, &theme);
                }
                let lm = game_state.as_ref().map(|g| g.lang_ru).unwrap_or(lang_ru);
                let (new_msi, clicked) = graphics::draw_mic_selection_menu(&gc, &mut mc, msi, &font, lm, &locale, &theme);
                msi = new_msi;
                if clicked || is_key_pressed(KeyCode::Enter) {
                    match mc.start_stream_with_device_index(msi) {
                        Ok(_)  => { if let Some(g) = &mut game_state { g.set_message(format!("Mic #{}", msi)); } app_state = AppState::Game; }
                        Err(e) => { if let Some(g) = &mut game_state { g.set_message(format!("Mic error: {}", e)); } }
                    }
                }
                let (bv, lv, _, iv) = graphics::draw_overlay_buttons_with_locale(&gc, &font, &locale, &mut theme, Some(&locale.overlay_back));
                if lv { reload_lang!(); } if iv { go_info!(); } if bv { app_state = AppState::Game; }
            }

            // ── ВЫБОР АУДИОВЫХОДА ─────────────────────────────────────────────
            AppState::OutputSelection => {
                if is_key_pressed(KeyCode::Escape) { app_state = AppState::Game; }
                if is_key_pressed(KeyCode::Up)   && osi > 0 { osi -= 1; }
                if is_key_pressed(KeyCode::Down)  {
                    let cnt = output_arc.lock().map(|o| o.list_devices().len()).unwrap_or(0);
                    if osi < cnt.saturating_sub(1) { osi += 1; }
                }
                if let Some(g) = &game_state {
                    clear_background(Color::new(0.06, 0.07, 0.10, 1.)); theme.draw_background(get_time());
                    graphics::draw_fretboard(&gc, g, &font, &theme);
                }
                let lm = game_state.as_ref().map(|g| g.lang_ru).unwrap_or(lang_ru);
                let devs = output_arc.lock().map(|o| o.list_devices()).unwrap_or_default();
                let (new_osi, clicked) = graphics::draw_device_list_menu(&gc, &devs, osi, &font, lm, &locale, &theme);
                osi = new_osi;
                if clicked || is_key_pressed(KeyCode::Enter) {
                    let res = output_arc.lock().map_err(|_| "lock failed".to_string()).and_then(|mut o| o.start(osi));
                    if let Some(g) = &mut game_state {
                        match res {
                            Ok(_)  => { let n = output_arc.lock().map(|o| o.list_devices().get(osi).cloned().unwrap_or_default()).unwrap_or_default(); g.set_message(format!("Выход: {}", n)); }
                            Err(e) => g.set_message(format!("Ошибка: {}", e)),
                        }
                    }
                    app_state = AppState::Game;
                }
                let (bv, lv, _, iv) = graphics::draw_overlay_buttons_with_locale(&gc, &font, &locale, &mut theme, Some(&locale.overlay_back));
                if lv { reload_lang!(); } if iv { go_info!(); } if bv { app_state = AppState::Game; }
            }
        }

        next_frame().await;
    }
}