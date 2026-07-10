// src/midi_parser.rs
//! Полный парсер MIDI файлов для gitar-приложения.
//! 
//! Использует крейт `midly` для корректного парсинга Standard MIDI Files (SMF).
//! Конвертирует MIDI ноты в SongEvent для гитары, группируя одновременные ноты в аккорды.

use midly::{Smf, TrackEventKind, MetaMessage, Format, Timing};
use std::fs;
use std::path::Path;

use crate::parser::{SongEvent, ChordNote, NoteTechnique, SongNote, ParsedSongData, TempoEvent, Section};

#[derive(Debug, Clone)]
pub struct MidiNote {
    pub pitch: u8,        // MIDI-номер ноты (0-127)
    pub velocity: u8,     // Громкость (0-127)
    pub time_sec: f64,    // Время начала (сек)
    pub duration_sec: f64,// Длительность (сек)
    pub channel: u8,      // MIDI-канал (0-15)
}

#[derive(Debug, Clone)]
pub struct ParsedMidi {
    pub notes: Vec<MidiNote>,
    pub tempo_bpm: f32,
    pub time_signature: (u8, u8),
    pub tempo_map: Vec<(f64, f32)>, // (time_sec, bpm)
}

/// Конвертирует MIDI pitch в (string_idx, fret) для стандартного гитарного строя.
/// Возвращает None если нота вне диапазона гитары.
pub fn pitch_to_guitar(pitch: u8, tuning: &[f32; 6]) -> Option<(usize, usize)> {
    // Стандартный строй: E2=40, A2=45, D3=50, G3=55, B3=59, E4=64
    // Но мы используем частоты для более точного определения
    const OPEN_STRINGS_MIDI: [u8; 6] = [40, 45, 50, 55, 59, 64];
    
    // Ищем струну, на которой можно сыграть эту ноту
    // Приоритет: более низкая струна (басовая)
    for (string_idx, &open_pitch) in OPEN_STRINGS_MIDI.iter().enumerate() {
        if pitch >= open_pitch && pitch <= open_pitch + 24 {
            let fret = (pitch - open_pitch) as usize;
            return Some((string_idx, fret));
        }
    }
    
    // Если нота ниже E2 или выше E4+24 ладов — вне диапазона
    None
}

/// Конвертирует MIDI pitch в частоту (Hz)
pub fn pitch_to_freq(pitch: u8) -> f32 {
    440.0 * 2.0_f32.powf((pitch as f32 - 69.0) / 12.0)
}

/// Читает и парсит MIDI файл.
/// Возвращает ParsedMidi с нотами, темпом и размером такта.
pub fn parse_midi_file(path: &str) -> Result<ParsedMidi, String> {
    let content = fs::read(path).map_err(|e| format!("Cannot read file: {}", e))?;
    let smf = Smf::parse(&content).map_err(|e| format!("MIDI parse error: {:?}", e))?;
    
    let mut notes: Vec<MidiNote> = Vec::new();
    let mut tempo_bpm: f32 = 120.0;
    let mut time_signature: (u8, u8) = (4, 4);
    let mut tempo_map: Vec<(f64, f32)> = vec![(0.0, 120.0)];
    
    // Определяем ticks per quarter note
    let ticks_per_quarter = match smf.header.timing {
        Timing::Metrical(tpq) => tpq.as_int() as f64,
        Timing::Timecode(_, _) => {
            return Err("MIDI Timecode timing not supported. Use metrical timing.".into());
        }
    };
    
    // Обрабатываем каждый трек
    for track in &smf.tracks {
        let mut current_tick: u32 = 0;
        let mut current_time_sec: f64 = 0.0;
        let mut current_tempo_us_per_quarter: f64 = 500_000.0; // 120 BPM по умолчанию
        
        // Для сопоставления note-on и note-off
        // Ключ: (channel, pitch), Значение: (start_time_sec, velocity)
        let mut active_notes: std::collections::HashMap<(u8, u8), (f64, u8)> = 
            std::collections::HashMap::new();
        
        for event in track {
            current_tick = current_tick.wrapping_add(event.delta.as_int());
            
            // Обновляем время в секундах
            let seconds_per_tick = current_tempo_us_per_quarter / (ticks_per_quarter * 1_000_000.0);
            current_time_sec += event.delta.as_int() as f64 * seconds_per_tick;
            
            match event.kind {
                TrackEventKind::Meta(meta) => {
                    match meta {
                        MetaMessage::Tempo(tempo) => {
                            // tempo — микросекунды на четвертную ноту
                            current_tempo_us_per_quarter = tempo.as_int() as f64;
                            tempo_bpm = 60_000_000.0 / current_tempo_us_per_quarter as f32;
                            tempo_map.push((current_time_sec, tempo_bpm));
                        }
                        MetaMessage::TimeSignature(num, den, _, _) => {
                            // num и den уже являются u8 в midly 0.5
                            // den — степень двойки: 2 = четвертная, 3 = восьмая и т.д.
                            let denominator = 1u8.checked_shl(den as u32).unwrap_or(4);
                            time_signature = (num, denominator);
                        }
                        _ => {}
                    }
                }
                TrackEventKind::Midi { channel, message } => {
                    use midly::MidiMessage;
                    match message {
                        MidiMessage::NoteOn { key, vel } => {
                            let pitch = key.as_int();
                            let velocity = vel.as_int();
                            
                            if velocity == 0 {
                                // NoteOn с velocity=0 = NoteOff
                                if let Some((start_time, start_vel)) = active_notes.remove(&(channel.as_int(), pitch)) {
                                    let duration = current_time_sec - start_time;
                                    if duration > 0.0 {
                                        notes.push(MidiNote {
                                            pitch,
                                            velocity: start_vel,
                                            time_sec: start_time,
                                            duration_sec: duration,
                                            channel: channel.as_int(),
                                        });
                                    }
                                }
                            } else {
                                // Новая нота началась
                                active_notes.insert((channel.as_int(), pitch), (current_time_sec, velocity));
                            }
                        }
                        MidiMessage::NoteOff { key, .. } => {
                            let pitch = key.as_int();
                            if let Some((start_time, start_vel)) = active_notes.remove(&(channel.as_int(), pitch)) {
                                let duration = current_time_sec - start_time;
                                if duration > 0.0 {
                                    notes.push(MidiNote {
                                        pitch,
                                        velocity: start_vel,
                                        time_sec: start_time,
                                        duration_sec: duration,
                                        channel: channel.as_int(),
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
    
    // Сортируем ноты по времени
    notes.sort_by(|a, b| a.time_sec.partial_cmp(&b.time_sec).unwrap_or(std::cmp::Ordering::Equal));
    
    Ok(ParsedMidi {
        notes,
        tempo_bpm,
        time_signature,
        tempo_map,
    })
}

// src/midi_parser.rs

/// Группирует MIDI ноты в SongEvent (аккорды) с учётом арпеджио.
/// 
/// Логика:
/// 1. Если ноты перекрываются по времени (следующая начинается до конца предыдущей) - это аккорд
/// 2. Если ноты идут подряд с маленьким интервалом (< 50ms) и имеют похожую длительность - это арпеджио
/// 3. Если интервал между нотами большой (> 100ms) - это разные аккорды
pub fn midi_to_song_events(
    midi: &ParsedMidi,
    tuning: &[f32; 6],
) -> Vec<SongEvent> {
    let mut events: Vec<SongEvent> = Vec::new();
    
    if midi.notes.is_empty() {
        return events;
    }
    
    let mut i = 0;
    
    while i < midi.notes.len() {
        let base_note = &midi.notes[i];
        let base_time = base_note.time_sec;
        let base_end = base_time + base_note.duration_sec;
        
        // Собираем ноты в аккорд
        let mut chord_notes: Vec<ChordNote> = Vec::new();
        let mut max_end_time = base_end;
        let mut last_note_end = base_end;
        
        // Первая нота аккорда
        if let Some((string_idx, fret)) = pitch_to_guitar(base_note.pitch, tuning) {
            chord_notes.push(ChordNote { string_idx, fret });
        }
        
        let mut j = i + 1;
        
        // Проверяем следующие ноты
        while j < midi.notes.len() {
            let next = &midi.notes[j];
            let next_start = next.time_sec;
            let next_end = next_start + next.duration_sec;
            
            // Критерий 1: ноты перекрываются (аккорд)
            let overlapping = next_start < last_note_end;
            
            // Критерий 2: ноты идут подряд с маленьким интервалом (арпеджио)
            let interval = next_start - last_note_end;
            let is_arpeggio = interval >= 0.0 && interval < 0.08; // 80ms максимум
            
            // Критерий 3: похожая длительность (для арпеджио)
            let duration_ratio = if base_note.duration_sec > 0.0 {
                next.duration_sec / base_note.duration_sec
            } else {
                1.0
            };
            let similar_duration = duration_ratio > 0.5 && duration_ratio < 2.0;
            
            // Критерий 4: не слишком большой разрыв между нотами
            let gap = next_start - base_time;
            let not_too_far = gap < 0.5; // максимум 500ms на весь аккорд
            
            if (overlapping || (is_arpeggio && similar_duration)) && not_too_far {
                // Проверяем, что эта струна ещё не занята в аккорде
                if let Some((string_idx, fret)) = pitch_to_guitar(next.pitch, tuning) {
                    if !chord_notes.iter().any(|cn| cn.string_idx == string_idx) {
                        chord_notes.push(ChordNote { string_idx, fret });
                    }
                }
                
                if next_end > max_end_time {
                    max_end_time = next_end;
                }
                last_note_end = next_end;
                j += 1;
            } else {
                break;
            }
        }
        
        if !chord_notes.is_empty() {
            // Сортируем ноты по струне (от басовой к высокой)
            chord_notes.sort_by_key(|cn| cn.string_idx);
            
            // Длительность аккорда = от начала первой ноты до конца последней
            let duration = max_end_time - base_time;
            
            events.push(SongEvent {
                time: base_time,
                duration,
                sound_duration: duration,
                notes: chord_notes,
                chord_name: None,
                technique: NoteTechnique::Normal,
            });
        }
        
        i = j;
    }
    
    events
}

/// Полная конвертация MIDI файла в ParsedSongData.
pub fn parse_midi_to_song(
    path: &str,
    tuning: [f32; 6],
) -> Result<ParsedSongData, String> {
    let midi = parse_midi_file(path)?;
    
    if midi.notes.is_empty() {
        return Err("No notes found in MIDI file".into());
    }
    
    // Группируем в аккорды с улучшенной логикой
    let events = midi_to_song_events(&midi, &tuning);
    
    if events.is_empty() {
        return Err("No guitar-playable notes found in MIDI file".into());
    }
    
    // Создаём плоский список нот для обратной совместимости
    let notes: Vec<SongNote> = events.iter().flat_map(|ev| {
        ev.notes.iter().map(|cn| SongNote {
            string_idx: cn.string_idx,
            fret: cn.fret,
            time: ev.time,
            duration: ev.duration,
            sound_duration: ev.sound_duration,
            technique: ev.technique.clone(),
        })
    }).collect();
    
    // Конвертируем tempo_map
    let tempo_map: Vec<TempoEvent> = midi.tempo_map.iter().map(|(t, bpm)| TempoEvent {
        time_sec: *t,
        bpm: *bpm,
    }).collect();
    
    Ok(ParsedSongData {
        events,
        notes,
        tuning,
        time_signature: midi.time_signature,
        initial_tempo: midi.tempo_bpm,
        tempo_map,
        sections: Vec::new(),
        lesson_mode: false,
    })
}

/// Сохраняет ParsedSongData в JSON файл.
pub fn save_midi_as_json(
    song_data: &ParsedSongData,
    name: &str,
    output_path: &str,
) -> Result<(), String> {
    use serde_json::json;
    
    let jdata = json!({
        "name": name,
        "tuning": song_data.tuning,
        "time_signature": [song_data.time_signature.0, song_data.time_signature.1],
        "initial_tempo": song_data.initial_tempo,
        "lesson_mode": song_data.lesson_mode,
        "events": song_data.events.iter().map(|ev| json!({
            "time": ev.time,
            "duration": ev.duration,
            "sound_duration": ev.sound_duration,
            "chord_name": ev.chord_name,
            "notes": ev.notes.iter().map(|n| json!({
                "string": n.string_idx,
                "fret": n.fret
            })).collect::<Vec<_>>()
        })).collect::<Vec<_>>()
    });
    
    let json_str = serde_json::to_string_pretty(&jdata)
        .map_err(|e| format!("JSON serialize error: {}", e))?;
    
    fs::write(output_path, json_str)
        .map_err(|e| format!("Cannot write file: {}", e))?;
    
    Ok(())
}