//! parser.rs — парсер MusicXML для гитарных табулатур
//!
//! Изменения v3 (аккорды + уроки):
//!
//! 1. Добавлены типы ChordNote, SongEvent — аккорд как единица.
//! 2. ParsedSongData теперь содержит Vec<SongEvent> (основной список).
//! 3. Функция group_notes_into_events() — группирует RawNote по времени.
//! 4. Поле lesson_mode в ParsedSongData — для урочных файлов.
//! 5. Все прежние исправления (tied, chord timing, tempo map) сохранены.

use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

// ─── Публичные типы ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq)]
pub enum NoteTechnique {
    #[default]
    Normal,
    Staccato,
    HammerOn,
    PullOff,
    Bend(f32),
    Slide,
}

/// Одна нота внутри аккорда (или единственная нота события).
#[derive(Debug, Clone)]
pub struct ChordNote {
    pub string_idx: usize,
    pub fret: usize,
}

/// Музыкальное событие: один момент времени — одна нота ИЛИ аккорд.
/// Заменяет плоский SongNote как основную единицу данных.
#[derive(Debug, Clone)]
pub struct SongEvent {
    pub time: f64,
    pub duration: f64,
    pub sound_duration: f64,
    /// Все ноты события. Длина 1 = одиночная нота, >1 = аккорд.
    pub notes: Vec<ChordNote>,
    /// Название аккорда из chord-symbol (если есть в MusicXML).
    pub chord_name: Option<String>,
    pub technique: NoteTechnique,
}

/// Обратная совместимость: плоская нота для старых JSON и вспомогательных функций.
#[derive(Debug, Clone)]
pub struct SongNote {
    pub string_idx: usize,
    pub fret: usize,
    pub time: f64,
    pub duration: f64,
    pub sound_duration: f64,
    pub technique: NoteTechnique,
}

#[derive(Debug, Clone)]
pub struct Section {
    pub name: String,
    pub time_sec: f64,
}

#[derive(Debug, Clone)]
pub struct TempoEvent {
    pub time_sec: f64,
    pub bpm: f32,
}

#[derive(Debug, Clone)]
pub struct ParsedSongData {
    /// Основной список событий (аккорды сгруппированы).
    pub events: Vec<SongEvent>,
    /// Плоский список нот — для обратной совместимости и утилит.
    pub notes: Vec<SongNote>,
    pub tuning: [f32; 6],
    pub time_signature: (u8, u8),
    pub initial_tempo: f32,
    pub tempo_map: Vec<TempoEvent>,
    pub sections: Vec<Section>,
    /// true = файл предназначен для режима урока (пауза на хит-зоне).
    pub lesson_mode: bool,
}

// ─── Внутренние типы ─────────────────────────────────────────────────────────


/// Компенсирует погрешности квантизации в MusicXML.
/// Максимальный разрыв по времени между нотами одного аккорда (сек).
const CHORD_TIME_THRESHOLD: f64 = 0.010;

/// Максимальный разрыв для арпеджио-аккорда (сек).
/// Если ноты идут подряд с паузой меньше этого значения — это арпеджио.
const ARPEGGIO_TIME_THRESHOLD: f64 = 0.15;

#[derive(Debug, Clone)]
struct RawNote {
    string_idx: usize,
    fret: usize,
    time: f64,
    duration: f64,
    sound_duration: f64,
    technique: NoteTechnique,
    arpeggio_group_id: Option<u32>,
}

#[derive(Debug, Clone)]
struct PartInfo {
    id: String,
    instrument_name: String,
}

#[derive(Debug, Clone)]
struct TempoChange {
    tick: u32,
    tempo: f32,
    seconds_at_this_tick: f64,
}

struct ParserState {
    divisions: u32,
    current_tick: u32,
    current_time_sec: f64,
    tempo_changes: Vec<TempoChange>,
    notes: Vec<RawNote>,

    in_note: bool,
    is_chord: bool,
    in_backup: bool,
    in_forward: bool,
    in_technical: bool,
    in_metronome: bool,
    in_notations: bool,
    in_articulations: bool,

    current_part_id: Option<String>,
    parts_info: Vec<PartInfo>,
    target_instrument: Option<String>,

    note_duration: u32,
    note_start_tick: u32,
    note_start_time: f64,
    temp_string: Option<u8>,
    temp_fret: Option<u8>,

    tab_staff_id: Option<u8>,
    current_staff: Option<u8>,

    pending_tied: Vec<RawNote>,

    is_tied_start: bool,
    is_tied_stop: bool,
    is_staccato: bool,
    is_hammer_on: bool,
    is_pull_off: bool,

    current_time_signature: (u8, u8),
    current_tuning: [f32; 6],
    invert_strings: bool,
    current_section_name: Option<String>,
    sections: Vec<Section>,

    /// Название аккорда из <harmony> (если встретилось перед нотой).
    pending_chord_name: Option<String>,
    in_harmony: bool,
    in_root: bool,
    harmony_root_step: Option<String>,
    harmony_kind: Option<String>,

    /// ID текущей группы арпеджио (увеличивается при встрече <arpeggiate/>)
    current_arpeggio_group: u32,
    /// Флаг: текущая нота помечена как арпеджио
    is_arpeggiate: bool,
}

impl ParserState {
    fn new(target_instrument: Option<String>, invert_strings: bool) -> Self {
        Self {
            divisions: 960,
            current_tick: 0,
            current_time_sec: 0.0,
            tempo_changes: vec![TempoChange { tick: 0, tempo: 120.0, seconds_at_this_tick: 0.0 }],
            notes: Vec::new(),
            in_note: false,
            is_chord: false,
            in_backup: false,
            in_forward: false,
            in_technical: false,
            in_metronome: false,
            in_notations: false,
            in_articulations: false,
            current_part_id: None,
            parts_info: Vec::new(),
            target_instrument,
            note_duration: 0,
            note_start_tick: 0,
            note_start_time: 0.0,
            temp_string: None,
            temp_fret: None,
            tab_staff_id: None,
            current_staff: None,
            pending_tied: Vec::new(),
            is_tied_start: false,
            is_tied_stop: false,
            is_staccato: false,
            is_hammer_on: false,
            is_pull_off: false,
            current_time_signature: (4, 4),
            current_tuning: [82.41, 110.00, 146.83, 196.00, 246.94, 329.63],
            invert_strings,
            current_section_name: None,
            sections: Vec::new(),
            pending_chord_name: None,
            in_harmony: false,
            in_root: false,
            harmony_root_step: None,
            harmony_kind: None,
            current_arpeggio_group: 0,
            is_arpeggiate: false,
        }
    }

    fn get_tempo_at_tick(&self, tick: u32) -> f32 {
        self.tempo_changes.iter().rev()
            .find(|tc| tc.tick <= tick)
            .map(|tc| tc.tempo)
            .unwrap_or(120.0)
    }

    fn ticks_to_seconds(&self, ticks: u32) -> f64 {
        if self.divisions == 0 { return 0.0; }
        let tempo = self.get_tempo_at_tick(self.current_tick);
        (ticks as f64 / self.divisions as f64) * (60.0 / tempo as f64)
    }

    fn advance_time(&mut self, ticks: u32) {
        if ticks == 0 { return; }
        let delta = self.ticks_to_seconds(ticks);
        self.current_time_sec += delta;
        self.current_tick += ticks;
    }

    fn add_tempo_change(&mut self, tick: u32, tempo: f32) {
        if let Some(last) = self.tempo_changes.last_mut() {
            if last.tick == tick {
                last.tempo = tempo;
                return;
            }
        }
        self.tempo_changes.push(TempoChange {
            tick,
            tempo,
            seconds_at_this_tick: self.current_time_sec,
        });
    }

    fn should_process_note(&self) -> bool {
        if let Some(target) = &self.target_instrument {
            let matches = self.current_part_id.as_ref().and_then(|pid| {
                self.parts_info.iter().find(|p| &p.id == pid)
            }).map_or(false, |p| p.instrument_name.contains(target.as_str()));
            if !matches { return false; }
        }
        if let Some(tab_id) = self.tab_staff_id {
            return self.current_staff == Some(tab_id);
        }
        true
    }

    fn finalize_note(&mut self) {
        let (s_raw, f_val) = match (self.temp_string, self.temp_fret) {
            (Some(s), Some(f)) => (s, f),
            _ => { self.reset_note_state(); return; }
        };

        if !self.should_process_note() {
            self.reset_note_state();
            return;
        }

        let string_idx = if self.invert_strings {
            (s_raw as usize).saturating_sub(1)
        } else {
            (6usize).saturating_sub(s_raw as usize)
        };

        let dur_sec = self.ticks_to_seconds(self.note_duration);

        let technique = if self.is_hammer_on {
            NoteTechnique::HammerOn
        } else if self.is_pull_off {
            NoteTechnique::PullOff
        } else if self.is_staccato {
            NoteTechnique::Staccato
        } else {
            NoteTechnique::Normal
        };

        let sound_dur = if self.is_staccato { dur_sec * 0.5 } else { dur_sec };

        let new_note = RawNote {
            string_idx,
            fret: f_val as usize,
            time: self.note_start_time,
            duration: dur_sec,
            sound_duration: sound_dur,
            technique,
            arpeggio_group_id: if self.is_arpeggiate {
                Some(self.current_arpeggio_group)
            } else {
                None
            },
        };

        if self.is_tied_stop {
            if let Some(idx) = self.pending_tied.iter().position(
                |p| p.string_idx == new_note.string_idx && p.fret == new_note.fret
            ) {
                let mut pending = self.pending_tied.remove(idx);
                pending.duration += new_note.duration;
                pending.sound_duration = pending.duration;

                if self.is_tied_start {
                    self.pending_tied.push(pending);
                } else {
                    self.notes.push(pending);
                }
            } else {
                if self.is_tied_start {
                    self.pending_tied.push(new_note);
                } else {
                    self.notes.push(new_note);
                }
            }
        } else if self.is_tied_start {
            self.pending_tied.push(new_note);
        } else {
            self.notes.push(new_note);
        }

        self.reset_note_state();
    }

    fn reset_note_state(&mut self) {
        self.temp_string = None;
        self.temp_fret = None;
        self.is_tied_start = false;
        self.is_tied_stop = false;
        self.is_staccato = false;
        self.is_hammer_on = false;
        self.is_pull_off = false;
        self.is_arpeggiate = false;
    }

    /// Формирует строку имени аккорда из harmony root + kind.
    fn flush_harmony(&mut self) {
        if let Some(root) = self.harmony_root_step.take() {
            let kind = self.harmony_kind.take().unwrap_or_default();
            let suffix = match kind.as_str() {
                "minor"            => "m",
                "dominant"         => "7",
                "minor-seventh"    => "m7",
                "major-seventh"    => "maj7",
                "diminished"       => "dim",
                "augmented"        => "aug",
                "suspended-fourth" => "sus4",
                "suspended-second" => "sus2",
                "dominant-ninth"   => "9",
                "minor-ninth"      => "m9",
                "major"            => "",
                _                  => "",
            };
            self.pending_chord_name = Some(format!("{}{}", root, suffix));
        }
        self.in_harmony = false;
        self.in_root = false;
    }
}



/// Группирует плоский список нот в события (аккорды).
///
/// Логика группировки:
/// 1. Ноты с одинаковым `arpeggio_group_id` → один аккорд (явное арпеджио).
/// 2. Ноты с разницей во времени < CHORD_TIME_THRESHOLD → один аккорд (одновременные).
/// 3. Эвристика: ноты подряд на разных струнах с одинаковой длительностью
///    и паузой < ARPEGGIO_TIME_THRESHOLD → аккорд-арпеджио.
pub fn group_notes_into_events(notes: &[RawNote]) -> Vec<SongEvent> {
    if notes.is_empty() { return Vec::new(); }

    let mut events: Vec<SongEvent> = Vec::new();
    let mut i = 0;

    while i < notes.len() {
        let base_note = &notes[i];
        let base_time = base_note.time;
        let base_duration = base_note.duration;
        let base_arpeggio_id = base_note.arpeggio_group_id;
        
        let mut chord_notes: Vec<ChordNote> = vec![ChordNote {
            string_idx: base_note.string_idx,
            fret: base_note.fret,
        }];
        
        // Собираем все струны, задействованные в этом аккорде
        let mut used_strings: std::collections::HashSet<usize> = std::collections::HashSet::new();
        used_strings.insert(base_note.string_idx);

        let mut j = i + 1;
        
        while j < notes.len() {
            let next = &notes[j];
            let time_diff = next.time - base_time;
            
            // Критерий 1: явное арпеджио (тот же group_id)
            let same_arpeggio = base_arpeggio_id.is_some() 
                && next.arpeggio_group_id == base_arpeggio_id;
            
            // Критерий 2: одновременные ноты (разница < CHORD_TIME_THRESHOLD)
            let simultaneous = time_diff.abs() < CHORD_TIME_THRESHOLD;
            
            // Критерий 3: эвристика арпеджио
            // - пауза между нотами < ARPEGGIO_TIME_THRESHOLD
            // - нота на другой струне (не дубликат)
            // - длительность примерно одинаковая (±20%)
            let duration_similar = (next.duration - base_duration).abs() < base_duration * 0.3;
            let different_string = !used_strings.contains(&next.string_idx);
            let heuristic_arpeggio = time_diff < ARPEGGIO_TIME_THRESHOLD 
                && time_diff > 0.0
                && different_string 
                && duration_similar;
            
            if same_arpeggio || simultaneous || heuristic_arpeggio {
                chord_notes.push(ChordNote {
                    string_idx: next.string_idx,
                    fret: next.fret,
                });
                used_strings.insert(next.string_idx);
                j += 1;
            } else {
                break;
            }
        }

        // Дедупликация и сортировка
        chord_notes.sort_by_key(|n| (n.string_idx, n.fret));
        chord_notes.dedup_by_key(|n| (n.string_idx, n.fret));

        // Длительность аккорда = сумма длительностей всех нот (для арпеджио)
        // или длительность первой ноты (для одновременных)
        let total_duration = if chord_notes.len() > 1 && j > i + 1 {
            // Это арпеджио — суммируем длительности
            let last_note = &notes[j - 1];
            (last_note.time + last_note.duration) - base_time
        } else {
            base_duration
        };

        events.push(SongEvent {
            time: base_time,
            duration: total_duration,
            sound_duration: total_duration,
            notes: chord_notes,
            chord_name: None,
            technique: base_note.technique.clone(),
        });

        i = j;
    }

    events
}

/// Преобразует SongEvent обратно в плоские SongNote (для совместимости).
pub fn events_to_notes(events: &[SongEvent]) -> Vec<SongNote> {
    let mut result = Vec::new();
    for ev in events {
        for cn in &ev.notes {
            result.push(SongNote {
                string_idx: cn.string_idx,
                fret: cn.fret,
                time: ev.time,
                duration: ev.duration,
                sound_duration: ev.sound_duration,
                technique: ev.technique.clone(),
            });
        }
    }
    result
}

// ─── Публичные функции ────────────────────────────────────────────────────────

pub fn get_available_instruments(content: &[u8]) -> Result<Vec<String>, String> {
    let mut reader = Reader::from_reader(content);
    let mut buf = Vec::new();
    let mut instruments = Vec::new();
    let mut in_part_list = false;
    let mut in_score_instrument = false;
    let mut current_name = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_string();
                match tag.as_str() {
                    "part-list" => in_part_list = true,
                    "score-instrument" if in_part_list => {
                        in_score_instrument = true;
                        current_name.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) if in_score_instrument => {
                let text = std::str::from_utf8(&e).unwrap_or("").trim().to_string();
                if !text.is_empty() { current_name = text; }
            }
            Ok(Event::End(e)) => {
                let tag = std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_string();
                match tag.as_str() {
                    "part-list" => in_part_list = false,
                    "score-instrument" => {
                        if in_score_instrument && !current_name.is_empty()
                            && !instruments.contains(&current_name)
                        {
                            instruments.push(current_name.clone());
                        }
                        in_score_instrument = false;
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML Error: {:?}", e)),
            _ => {}
        }
        buf.clear();
    }
    if instruments.is_empty() { Err("No instruments found".into()) } else { Ok(instruments) }
}

/// Основная функция парсинга.
///
/// `invert_strings`: false = стандарт MusicXML (string 1 = E4 тонкая),
///                   true  = инверсия (string 1 = E2 бас).
/// `lesson_mode`:    true  = файл урока, игровой движок делает паузу на хит-зоне.
pub fn parse_musicxml_to_song_notes(
    content: &[u8],
    target_instrument_name: Option<&str>,
    invert_strings: bool,
) -> Result<ParsedSongData, String> {
    parse_musicxml_internal(content, target_instrument_name, invert_strings, false)
}

/// Версия для урочных файлов — выставляет lesson_mode = true.
pub fn parse_musicxml_lesson(
    content: &[u8],
    target_instrument_name: Option<&str>,
    invert_strings: bool,
) -> Result<ParsedSongData, String> {
    parse_musicxml_internal(content, target_instrument_name, invert_strings, true)
}

fn parse_musicxml_internal(
    content: &[u8],
    target_instrument_name: Option<&str>,
    invert_strings: bool,
    lesson_mode: bool,
) -> Result<ParsedSongData, String> {
    let target = target_instrument_name.map(|s| s.to_string());
    let mut state = ParserState::new(target, invert_strings);
    let mut reader = Reader::from_reader(content);
    let mut buf = Vec::new();
    let mut tag_stack: Vec<String> = Vec::new();

    let mut parsing_part_list = false;
    let mut current_parsing_part_id: Option<String> = None;
    let mut current_parsing_instr_name: Option<String> = None;

    let mut in_clef = false;
    let mut in_staff_tuning = false;
    let mut in_time = false;
    let mut in_beats = false;
    let mut in_beat_type = false;
    let mut in_direction_type = false;
    let mut in_rehearsal = false;

    let mut current_clef_number: Option<u8> = None;
    let mut current_tuning_line: Option<u8> = None;
    let mut current_tuning_step: Option<String> = None;
    let mut current_tuning_octave: Option<i32> = None;
    let mut current_beats: Option<u8> = None;
    let mut current_beat_type_val: Option<u8> = None;
    let mut current_tie_type: Option<String> = None;
    let mut current_beat_unit: Option<String> = None;
    let mut current_per_minute: Option<f32> = None;

    // Карта chord_name по тику: заполняется из <harmony>
    let mut chord_names: std::collections::HashMap<u32, String> = std::collections::HashMap::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_string();
                tag_stack.push(tag.clone());

                match tag.as_str() {
                    "part" => {
                        state.current_part_id = e.attributes().find_map(|a| {
                            let attr = a.ok()?;
                            if attr.key.as_ref() == b"id" {
                                Some(String::from_utf8_lossy(&attr.value).to_string())
                            } else { None }
                        });
                        state.current_tick = 0;
                        state.current_time_sec = 0.0;
                    }
                    "part-list" => parsing_part_list = true,
                    "score-part" if parsing_part_list => {
                        current_parsing_part_id = e.attributes().find_map(|a| {
                            let attr = a.ok()?;
                            if attr.key.as_ref() == b"id" {
                                Some(String::from_utf8_lossy(&attr.value).to_string())
                            } else { None }
                        });
                    }
                    "sound" => {
                        if let Some(tempo) = e.attributes().find_map(|a| {
                            let attr = a.ok()?;
                            if attr.key.as_ref() == b"tempo" {
                                String::from_utf8_lossy(&attr.value).parse::<f32>().ok()
                            } else { None }
                        }) {
                            state.add_tempo_change(state.current_tick, tempo);
                        }
                    }
                    "metronome" => {
                        state.in_metronome = true;
                        current_beat_unit = None;
                        current_per_minute = None;
                    }
                    "note" => {
                        state.in_note = true;
                        if !state.is_chord {
                            state.note_start_tick = state.current_tick;
                            state.note_start_time = state.current_time_sec;
                        }
                        state.is_chord = false;
                        state.note_duration = 0;
                        state.current_staff = None;
                        state.is_tied_start = false;
                        state.is_tied_stop = false;
                        state.is_staccato = false;
                        state.is_hammer_on = false;
                        state.is_pull_off = false;
                    }
                    "chord" => {
                        state.is_chord = true;
                        state.note_start_tick = state.current_tick
                            .saturating_sub(state.note_duration);
                        state.note_start_time = self_time_at_tick(
                            state.note_start_tick, &state.tempo_changes, state.divisions
                        );
                    }
                    "backup" => state.in_backup = true,
                    "forward" => state.in_forward = true,
                    "technical" => state.in_technical = true,
                    "notations" => state.in_notations = true,
                    "articulations" if state.in_notations => state.in_articulations = true,
                    "arpeggiate" if state.in_notations => {
                        state.is_arpeggiate = true;
                        state.current_arpeggio_group += 1;
                    }
                    "clef" => {
                        in_clef = true;
                        current_clef_number = e.attributes().find_map(|a| {
                            let attr = a.ok()?;
                            if attr.key.as_ref() == b"number" {
                                String::from_utf8_lossy(&attr.value).parse::<u8>().ok()
                            } else { None }
                        });
                    }
                    "staff-tuning" => {
                        in_staff_tuning = true;
                        current_tuning_line = e.attributes().find_map(|a| {
                            let attr = a.ok()?;
                            if attr.key.as_ref() == b"line" {
                                String::from_utf8_lossy(&attr.value).parse::<u8>().ok()
                            } else { None }
                        });
                        current_tuning_step = None;
                        current_tuning_octave = None;
                    }
                    "time" => {
                        in_time = true;
                        current_beats = None;
                        current_beat_type_val = None;
                    }
                    "beats" if in_time => in_beats = true,
                    "beat-type" if in_time => in_beat_type = true,
                    "tie" => {
                        current_tie_type = e.attributes().find_map(|a| {
                            let attr = a.ok()?;
                            if attr.key.as_ref() == b"type" {
                                Some(String::from_utf8_lossy(&attr.value).to_string())
                            } else { None }
                        });
                    }
                    "direction-type" => in_direction_type = true,
                    "rehearsal" if in_direction_type => {
                        in_rehearsal = true;
                        state.current_section_name = Some(String::new());
                    }
                    "hammer-on" => state.is_hammer_on = true,
                    "pull-off"  => state.is_pull_off = true,
                    // ── Аккордные символы (harmony) ──────────────────────────
                    "harmony" => {
                        state.in_harmony = true;
                        state.harmony_root_step = None;
                        state.harmony_kind = None;
                    }
                    "root" if state.in_harmony => state.in_root = true,
                    _ => {}
                }
            }

            Ok(Event::Text(e)) => {
                let text = std::str::from_utf8(&e).unwrap_or("").trim().to_string();
                if text.is_empty() { continue; }
                let parent = tag_stack.last().cloned().unwrap_or_default();

                if parsing_part_list {
                    if parent == "instrument-name" {
                        current_parsing_instr_name = Some(text);
                    }
                    continue;
                }

                match parent.as_str() {
                    "divisions" => {
                        if let Ok(d) = text.parse::<u32>() { state.divisions = d; }
                    }
                    "duration" => {
                        if let Ok(d) = text.parse::<u32>() {
                            if state.in_note {
                                state.note_duration = d;
                            } else if state.in_backup {
                                let delta = time_delta(d, state.divisions, state.get_tempo_at_tick(state.current_tick));
                                state.current_time_sec = (state.current_time_sec - delta).max(0.0);
                                state.current_tick = state.current_tick.saturating_sub(d);
                            } else if state.in_forward {
                                state.advance_time(d);
                            }
                        }
                    }
                    "beat-unit" if state.in_metronome => {
                        current_beat_unit = Some(text);
                    }
                    "per-minute" if state.in_metronome => {
                        if let Ok(t) = text.parse::<f32>() { current_per_minute = Some(t); }
                    }
                    "string" if state.in_technical => {
                        if let Ok(s) = text.parse::<u8>() { state.temp_string = Some(s); }
                    }
                    "fret" if state.in_technical => {
                        if let Ok(f) = text.parse::<u8>() { state.temp_fret = Some(f); }
                    }
                    "sign" if in_clef => {
                        if text == "TAB" {
                            state.tab_staff_id = current_clef_number.or(Some(1));
                        }
                    }
                    "tuning-step" if in_staff_tuning => {
                        current_tuning_step = Some(text);
                    }
                    "tuning-octave" if in_staff_tuning => {
                        if let Ok(o) = text.parse::<i32>() { current_tuning_octave = Some(o); }
                    }
                    "beats" if in_beats => {
                        if let Ok(b) = text.parse::<u8>() { current_beats = Some(b); }
                    }
                    "beat-type" if in_beat_type => {
                        if let Ok(bt) = text.parse::<u8>() { current_beat_type_val = Some(bt); }
                    }
                    "staff" if state.in_note => {
                        if let Ok(s) = text.parse::<u8>() { state.current_staff = Some(s); }
                    }
                    "rehearsal" if in_rehearsal => {
                        if let Some(name) = &mut state.current_section_name {
                            name.push_str(&text);
                            name.push(' ');
                        }
                    }
                    // Harmony parsing
                    "root-step" if state.in_root => {
                        state.harmony_root_step = Some(text);
                    }
                    "kind" if state.in_harmony => {
                        state.harmony_kind = Some(text);
                    }
                    _ => {}
                }
            }

            Ok(Event::End(e)) => {
                let tag = std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_string();

                if parsing_part_list && tag == "score-part" {
                    if let (Some(pid), Some(iname)) = (&current_parsing_part_id, &current_parsing_instr_name) {
                        state.parts_info.push(PartInfo {
                            id: pid.clone(),
                            instrument_name: iname.clone(),
                        });
                    }
                    current_parsing_part_id = None;
                    current_parsing_instr_name = None;
                }

                match tag.as_str() {
                    "part-list" => parsing_part_list = false,
                    "metronome" => {
                        state.in_metronome = false;
                        if let (Some(bu), Some(pm)) = (&current_beat_unit, &current_per_minute) {
                            let tempo = match bu.as_str() {
                                "eighth" => pm / 2.0,
                                "half"   => pm * 2.0,
                                "whole"  => pm * 4.0,
                                "16th"   => pm / 4.0,
                                _        => *pm,
                            };
                            state.add_tempo_change(state.current_tick, tempo);
                        }
                        current_beat_unit = None;
                        current_per_minute = None;
                    }
                    "staccato" if state.in_articulations => state.is_staccato = true,
                    "note" => {
                        let dur = state.note_duration;
                        state.finalize_note();
                        state.in_note = false;
                        if !state.is_chord {
                            state.advance_time(dur);
                        }
                    }
                    "backup"       => state.in_backup = false,
                    "forward"      => state.in_forward = false,
                    "technical"    => state.in_technical = false,
                    "notations"    => state.in_notations = false,
                    "articulations" => state.in_articulations = false,
                    "clef" => { in_clef = false; current_clef_number = None; }
                    "staff-tuning" => {
                        if let (Some(line), Some(step), Some(oct)) =
                            (current_tuning_line, &current_tuning_step, current_tuning_octave)
                        {
                            let idx = line.saturating_sub(1) as usize;
                            if idx < 6 {
                                let semitone = note_step_to_semitone(step);
                                let total_midi = semitone + (oct + 1) * 12;
                                state.current_tuning[idx] =
                                    440.0 * 2.0_f32.powf((total_midi as f32 - 69.0) / 12.0);
                            }
                        }
                        in_staff_tuning = false;
                        current_tuning_line = None;
                        current_tuning_step = None;
                        current_tuning_octave = None;
                    }
                    "time" => {
                        if let (Some(b), Some(bt)) = (current_beats, current_beat_type_val) {
                            state.current_time_signature = (b, bt);
                        }
                        in_time = false;
                        in_beats = false;
                        in_beat_type = false;
                        current_beats = None;
                        current_beat_type_val = None;
                    }
                    "beats"     => in_beats = false,
                    "beat-type" => in_beat_type = false,
                    "tie" => {
                        if let Some(ref tt) = current_tie_type {
                            match tt.as_str() {
                                "start" => state.is_tied_start = true,
                                "stop"  => state.is_tied_stop = true,
                                _ => {}
                            }
                        }
                        current_tie_type = None;
                    }
                    "direction-type" => in_direction_type = false,
                    "rehearsal" => {
                        if let Some(name) = &state.current_section_name {
                            let trimmed = name.trim().to_string();
                            if !trimmed.is_empty() {
                                state.sections.push(Section {
                                    name: trimmed,
                                    time_sec: state.current_time_sec,
                                });
                            }
                        }
                        state.current_section_name = None;
                        in_rehearsal = false;
                    }
                    // Harmony завершён — сохраняем имя аккорда на текущий тик
                    "harmony" => {
                        state.flush_harmony();
                        if let Some(name) = state.pending_chord_name.take() {
                            chord_names.insert(state.current_tick, name);
                        }
                    }
                    "root" => state.in_root = false,
                    _ => {}
                }

                if tag_stack.last() == Some(&tag) { tag_stack.pop(); }
            }

            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error: {:?}", e)),
            _ => {}
        }
        buf.clear();
    }

    // Дописываем оставшиеся pending tied ноты
    for pending in state.pending_tied.drain(..) {
        state.notes.push(pending);
    }

    if state.notes.is_empty() {
        return Err("No guitar notes found. Check instrument selection and string/fret data.".into());
    }

    // Нормализация времени
    let min_time = state.notes.iter().map(|n| n.time).fold(f64::INFINITY, f64::min);
    let min_time = if min_time.is_infinite() { 0.0 } else { min_time };

    // Извлекаем до частичного перемещения state
    let initial_tempo = state.tempo_changes.first().map(|tc| tc.tempo).unwrap_or(120.0);
    let saved_divisions = state.divisions;
    let saved_tempo_changes = state.tempo_changes.clone();

    let tempo_map = saved_tempo_changes.iter().map(|tc| TempoEvent {
        time_sec: (tc.seconds_at_this_tick - min_time).max(0.0),
        bpm: tc.tempo,
    }).collect();

    // Нормализуем и сортируем raw ноты
    let mut raw_notes = state.notes;
    for n in &mut raw_notes {
        n.time -= min_time;
    }
    raw_notes.sort_by(|a, b| a.time.partial_cmp(&b.time)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then(a.string_idx.cmp(&b.string_idx)));

    // Группируем в события
    let mut events = group_notes_into_events(&raw_notes);

    // Прикрепляем имена аккордов (по ближайшему тику)
    // chord_names хранит тики до нормализации — пересчитываем через tempo_map
    // Упрощённый вариант: ищем по времени события
    for ev in &mut events {
        // Ищем ближайшее имя аккорда в окне ±beat
        let beat_dur = 60.0 / initial_tempo as f64;
        for (&tick, name) in &chord_names {
            let chord_time_raw = time_from_tick(tick, &saved_tempo_changes, saved_divisions);
            let chord_time = (chord_time_raw - min_time).max(0.0);
            if (ev.time - chord_time).abs() < beat_dur * 0.5 {
                ev.chord_name = Some(name.clone());
                break;
            }
        }
    }

    // Плоский список нот для обратной совместимости
    let notes: Vec<SongNote> = events_to_notes(&events);

    Ok(ParsedSongData {
        events,
        notes,
        tuning: state.current_tuning,
        time_signature: state.current_time_signature,
        initial_tempo,
        tempo_map,
        sections: state.sections,
        lesson_mode,
    })
}

// ─── Вспомогательные функции ─────────────────────────────────────────────────

fn note_step_to_semitone(step: &str) -> i32 {
    match step {
        "C" => 0, "D" => 2, "E" => 4, "F" => 5,
        "G" => 7, "A" => 9, "B" => 11, _ => 0
    }
}

fn time_delta(ticks: u32, divisions: u32, tempo: f32) -> f64 {
    if divisions == 0 { return 0.0; }
    (ticks as f64 / divisions as f64) * (60.0 / tempo as f64)
}

fn self_time_at_tick(tick: u32, tempo_changes: &[TempoChange], divisions: u32) -> f64 {
    if tempo_changes.is_empty() || divisions == 0 { return 0.0; }
    let tc = tempo_changes.iter().rev().find(|tc| tc.tick <= tick).unwrap_or(&tempo_changes[0]);
    let delta_ticks = tick.saturating_sub(tc.tick);
    tc.seconds_at_this_tick + time_delta(delta_ticks, divisions, tc.tempo)
}

/// Вычисляет абсолютное время тика без привязки к состоянию парсера.
fn time_from_tick(tick: u32, tempo_changes: &[TempoChange], divisions: u32) -> f64 {
    self_time_at_tick(tick, tempo_changes, divisions)
}

pub fn read_file_content(path: &str) -> Result<Vec<u8>, String> {
    let p = Path::new(path);
    if p.extension().and_then(|s| s.to_str()) == Some("mxl") {
        let file = File::open(p).map_err(|e| e.to_string())?;
        let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;
        for i in 0..archive.len() {
            let mut f = archive.by_index(i).map_err(|e| e.to_string())?;
            let name = f.name().to_string();
            if name.ends_with(".xml") && !name.starts_with("META-INF") {
                let mut buffer = Vec::new();
                f.read_to_end(&mut buffer).map_err(|e| e.to_string())?;
                return Ok(buffer);
            }
        }
        Err("No XML file found in MXL archive".into())
    } else {
        let mut file = File::open(p).map_err(|e| e.to_string())?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).map_err(|e| e.to_string())?;
        Ok(buffer)
    }
}