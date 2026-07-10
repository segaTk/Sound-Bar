use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::fs::File;
use std::io::BufReader;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crate::is_valid_guitar_freq; // Импорт функций из main.rs, если они доступны в crate root
// Если прямой импорт невозможен из-за циклических зависимостей, 
// лучше скопировать логику get_note_name сюда или вынести в utils.
// Для данного примера предположим, что мы можем использовать логику определения ноты.

pub struct TunerState {
    pub detected_freq: Option<f32>,
    pub note_name: String,
    pub cents: f32, // Отклонение в центах
    pub is_in_tune: bool,
}

impl TunerState {
    pub fn new() -> Self {
        Self {
            detected_freq: None,
            note_name: "--".to_string(),
            cents: 0.0,
            is_in_tune: false,
        }
    }

    pub fn update(&mut self, freq: Option<f32>) {
        if let Some(f) = freq {
            if is_valid_guitar_freq(f) {
                self.detected_freq = Some(f);
                let note_info = calculate_note_info(f);
                self.note_name = note_info.name;
                self.cents = note_info.cents;
                self.is_in_tune = note_info.cents.abs() < 10.0; // Допуск +/- 10 центов
            } else {
                self.detected_freq = None;
                self.note_name = "--".to_string();
                self.cents = 0.0;
                self.is_in_tune = false;
            }
        } else {
            self.detected_freq = None;
            self.note_name = "--".to_string();
            self.cents = 0.0;
            self.is_in_tune = false;
        }
    }
}

struct NoteInfo {
    name: String,
    cents: f32,
}

fn calculate_note_info(freq: f32) -> NoteInfo {
    // Формула: 1200 * log2(freq / reference_freq)
    // Reference for A4 = 440Hz
    let semitones_from_a4 = 12.0 * (freq / 440.0).log2();
    let rounded_semitones = semitones_from_a4.round();
    let cents = (semitones_from_a4 - rounded_semitones) * 100.0;
    
    // Получаем имя ноты
    let note_names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    // A4 это 69-я нота в MIDI. 
    // midi_note = 69 + rounded_semitones
    let midi_note = 69.0 + rounded_semitones;
    let octave = (midi_note / 12.0).floor() as i32 - 1;
    let note_index = ((midi_note as i32) % 12 + 12) % 12;
    
    let name = format!("{}{}", note_names[note_index as usize], octave);
    
    NoteInfo {
        name,
        cents,
    }
}

pub struct GuitarPlayer {
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    sink: Arc<Mutex<Sink>>,  // Arc<Mutex> для shared ownership между потоками
    open_string_sources: HashMap<usize, Vec<f32>>,
}

impl GuitarPlayer {
    pub fn new() -> Self {
        // Создаём аудиопоток
        let (_stream, stream_handle) = OutputStream::try_default()
            .expect("Failed to create audio stream");
        
        // Создаём Sink для управления очередью воспроизведения
        let sink = Sink::try_new(&stream_handle)
            .expect("Failed to create audio sink");
        
        GuitarPlayer {
            _stream,
            stream_handle,
            sink: Arc::new(Mutex::new(sink)),
            open_string_sources: HashMap::new(),
        }
    }

    /// Загружает сэмплы струн в память
    pub fn load_string_samples(&mut self, string_idx: usize, file_path: &str) {
        // Здесь можно загрузить wav файл и сохранить в Vec<f32>
        // Для примера просто добавляем пустой вектор
        self.open_string_sources.insert(string_idx, Vec::new());
        println!("Loaded string {} from {}", string_idx, file_path);
    }

    /// Воспроизводит ноту, используя сэмпл открытой струны и меняя его высоту
    pub fn play_note(&self, string_idx: usize, fret: usize, volume: f32, _sound_duration: f64) {
        // Проверяем, загружена ли струна
        if !self.open_string_sources.contains_key(&string_idx) {
            println!("String {} not loaded!", string_idx);
            return;
        }

        // Формула изменения высоты тона: каждый лад = полутон
        let speed_multiplier = 2.0_f32.powf(fret as f32 / 12.0);
        
        // Путь к файлу (можно загружать из файлов или из памяти)
        let file_path = format!("assets/str_{}.wav", string_idx);
        
        // Открываем и декодируем файл
        match File::open(&file_path) {
            Ok(file) => {
                let source = Decoder::new(BufReader::new(file))
                    .expect("Failed to decode audio file");
                
                // Применяем эффекты: изменение скорости (высоты) и громкости
                let modified_source = source.speed(speed_multiplier).amplify(volume);
                
                // Блокируем Sink и добавляем источник в очередь
                let sink = self.sink.lock().unwrap();
                sink.append(modified_source);
                
                println!("Playing: String {}, Fret {} (speed: {:.3}x, volume: {:.2})", 
                         string_idx, fret, speed_multiplier, volume);
            }
            Err(e) => {
                println!("Failed to open file {}: {}", file_path, e);
            }
        }
    }

    /// Останавливает всё воспроизведение
    pub fn stop_all(&self) {
        let sink = self.sink.lock().unwrap();
        sink.stop();
        println!("Stopped all playback");
    }

    /// Пауза
    pub fn pause(&self) {
        let sink = self.sink.lock().unwrap();
        sink.pause();
        println!("Playback paused");
    }

    /// Продолжить воспроизведение
    pub fn resume(&self) {
        let sink = self.sink.lock().unwrap();
        sink.play();
        println!("Playback resumed");
    }

    /// Очищает очередь воспроизведения
    pub fn clear(&self) {
        let sink = self.sink.lock().unwrap();
        sink.clear();
        println!("Queue cleared");
    }

    /// Проверяет, есть ли ещё воспроизводимые звуки
    pub fn is_empty(&self) -> bool {
        let sink = self.sink.lock().unwrap();
        sink.empty()
    }

    /// Устанавливает громкость для всего Sink'а
    pub fn set_volume(&self, volume: f32) {
        let sink = self.sink.lock().unwrap();
        sink.set_volume(volume);
        println!("Global volume set to {}", volume);
    }
}

impl Default for GuitarPlayer {
    fn default() -> Self {
        Self::new()
    }
}