//! audio_engine.rs
//!
//! Двухуровневый синтез гитарных нот:
//!
//! 1. ПРИОРИТЕТ — Сэмплер (как TuxGuitar/Guitar Pro RSE):
//!    Загружает WAV-записи открытых струн из assets/str_N.wav,
//!    транспонирует через pitch-shifting (speed * 2^(fret/12)).
//!    Даёт реалистичный звук реальной гитары.
//!
//! 2. FALLBACK — Karplus-Strong синтез:
//!    Физическое моделирование защипа струны через линию задержки.
//!    Работает без сэмплов — хороший гитарный тембр из математики.
//!
//! Все ноты планируются через ScheduledNote с абсолютным временем аудио-потока —
//! точность независит от framerate (как MIDI-секвенсор).

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    SampleFormat, Device, Stream,
};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use cpal::FromSample;

pub type AudioSample = f32;
pub const SAMPLE_RATE: u32 = 44100;
pub const CHANNELS: u16 = 2;

// ─────────────────────────────────────────────────────────────────────────────
// Публичные типы запросов
// ─────────────────────────────────────────────────────────────────────────────

/// Воспроизведение готового f32-буфера (WAV-запись из тюнера).
#[derive(Clone, Debug)]
pub struct PlayRequest {
    pub samples: Vec<AudioSample>,
    pub volume: f32,
    pub duration: f64,
}

/// Запрос на синтез/сэмплирование гитарной ноты.
#[derive(Clone, Debug)]
pub struct NoteRequest {
    pub freq: f32,
    pub duration: f32,
    /// Абсолютное время начала в секундах (stream_time микшера). None = немедленно.
    pub start_time: Option<f64>,
    /// 0=E2(бас)..5=E4(высокая) — влияет на тембр KS и выбор сэмпла
    pub string_idx: usize,
    pub volume: f32,
}

// ─────────────────────────────────────────────────────────────────────────────
// GuitarSampler — питч-шифтинг реальных WAV сэмплов
// ─────────────────────────────────────────────────────────────────────────────

/// Хранит загруженные сэмплы открытых струн и генерирует транспонированные ноты.
pub struct GuitarSampler {
    /// string_idx -> (samples_f32, original_sample_rate)
    strings: HashMap<usize, (Vec<f32>, u32)>,
}

impl GuitarSampler {
    pub fn new() -> Self {
        Self { strings: HashMap::new() }
    }

    /// Загружает WAV-файл открытой струны. Возвращает Ok если успешно.
    /// Ожидаемый путь: assets/str_0.wav ... assets/str_5.wav
    pub fn load_string(&mut self, string_idx: usize, path: &str) -> Result<(), String> {
        let file = std::fs::File::open(path)
            .map_err(|e| format!("Cannot open {}: {}", path, e))?;
        let reader = std::io::BufReader::new(file);

        // Используем rodio для декодирования любого WAV/OGG/MP3
        use rodio::Source;
        let source = rodio::Decoder::new(reader)
            .map_err(|e| format!("Decode error {}: {}", path, e))?;

        let sr = source.sample_rate();
        let channels = source.channels() as usize;
        let raw: Vec<f32> = source.convert_samples::<f32>().collect();

        // Стерео → моно усреднением
        let mono = if channels == 2 {
            raw.chunks(2)
                .map(|c| (c[0] + c.get(1).copied().unwrap_or(0.0)) * 0.5)
                .collect()
        } else {
            raw
        };

        println!("🎸 Loaded string {} from {} ({} samples @ {}Hz)", string_idx, path, mono.len(), sr);
        self.strings.insert(string_idx, (mono, sr));
        Ok(())
    }

    /// Пытается загрузить все 6 струн из стандартных путей.
    /// Возвращает количество успешно загруженных.
    pub fn try_load_all(&mut self) -> usize {
        let mut loaded = 0;
        for i in 0..6 {
            // Пробуем несколько вариантов пути
            let paths = [
                format!("assets/str_{}.wav", i),
                format!("assets/string_{}.wav", i),
                format!("assets/guitar_string_{}.wav", i),
            ];
            for path in &paths {
                if self.load_string(i, path).is_ok() {
                    loaded += 1;
                    break;
                }
            }
        }
        if loaded > 0 {
            println!("🎸 GuitarSampler: {}/6 strings loaded", loaded);
        } else {
            println!("🎸 GuitarSampler: no WAV samples found — using Karplus-Strong");
        }
        loaded
    }

    pub fn is_available(&self) -> bool {
        !self.strings.is_empty()
    }

    /// Генерирует транспонированный сэмпл для (string_idx, fret).
    ///
    /// Алгоритм (как rodio source.speed() но в f32):
    /// speed = 2^(fret/12) — каждый лад = полутон вверх.
    /// Линейная интерполяция при чтении с шагом speed.
    ///
    /// target_sr — sample_rate устройства вывода (для ресэмплинга).
    pub fn render_note(
        &self,
        string_idx: usize,
        fret: usize,
        duration_secs: f32,
        target_sr: u32,
        volume: f32,
    ) -> Vec<f32> {
        // Находим ближайшую загруженную струну
        let (base_samples, base_sr) = if let Some(d) = self.strings.get(&string_idx) {
            d
        } else if let Some(d) = self.strings.values().next() {
            d
        } else {
            return vec![];
        };

        // Скорость воспроизведения для транспонирования (каждый лад = 2^(1/12))
        let pitch_speed = 2.0_f32.powf(fret as f32 / 12.0);

        // Количество выходных сэмплов
        let out_samples = (target_sr as f32 * duration_secs) as usize;

        // Шаг чтения входного буфера с учётом:
        // - pitch_speed (транспонирование)
        // - соотношения sample_rate (ресэмплинг base_sr → target_sr)
        let read_step = pitch_speed * (*base_sr as f32 / target_sr as f32);

        // Огибающая: быстрая атака + плавный релиз (как щипок струны)
        let attack = (target_sr as f32 * 0.004) as usize; // 4 мс
        let release_start = (out_samples as f32 * 0.75) as usize;

        let mut output = Vec::with_capacity(out_samples);
        let mut read_pos = 0.0_f32;
        let base_len = base_samples.len();

        for i in 0..out_samples {
            // Линейная интерполяция
            let idx = read_pos as usize;
            let frac = read_pos - idx as f32;

            let s = if idx + 1 < base_len {
                base_samples[idx] * (1.0 - frac) + base_samples[idx + 1] * frac
            } else if idx < base_len {
                base_samples[idx]
            } else {
                // Конец сэмпла: затухание
                break;
            };

            // Огибающая
            let env = if i < attack {
                i as f32 / attack as f32
            } else if i >= release_start {
                let rel = out_samples - release_start;
                if rel > 0 { 1.0 - (i - release_start) as f32 / rel as f32 } else { 0.0 }
            } else {
                1.0
            };

            output.push(s * env * volume);
            read_pos += read_step;
        }

        // Дополняем тишиной если короче запрошенного (конец WAV-сэмпла)
        while output.len() < out_samples {
            output.push(0.0);
        }

        output
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Karplus-Strong — физическое моделирование защипа струны (fallback)
// ─────────────────────────────────────────────────────────────────────────────
pub fn karplus_strong(freq: f32, duration: f32, string_idx: usize, sample_rate: u32) -> Vec<f32> {
    let sr = sample_rate as f32;
    let n_samples = (sr * duration) as usize;
    let delay_len = (sr / freq).round() as usize;
    if delay_len == 0 || n_samples == 0 { return vec![0.0; n_samples]; }

    let string_norm = string_idx.min(5) as f32 / 5.0;
    let decay = 0.998 - string_norm * 0.006;
    let noise_blend = 1.0 - string_norm * 0.4;

    let mut delay_buf = vec![0.0f32; delay_len];
    {
        let mut rng: u64 = 0x123456789ABCDEF0 ^ (freq as u64).wrapping_mul(6364136223846793005);
        let next = |s: &mut u64| -> f32 {
            *s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (*s >> 33) as f32 / u32::MAX as f32 * 2.0 - 1.0
        };
        for i in 0..delay_len {
            let white = next(&mut rng);
            let smooth = if i > 0 { (delay_buf[i-1] + white) * 0.5 } else { white };
            delay_buf[i] = white * noise_blend + smooth * (1.0 - noise_blend);
        }
    }

    let mut out = Vec::with_capacity(n_samples);
    let mut pos = 0usize;
    let attack = (sr * 0.003) as usize;
    let release_start = ((duration - 0.06).max(0.0) * sr) as usize;

    for i in 0..n_samples {
        let next_pos = (pos + 1) % delay_len;
        let s = (delay_buf[pos] + delay_buf[next_pos]) * 0.5 * decay;
        delay_buf[pos] = s;
        pos = next_pos;
        let env = if i < attack { i as f32 / attack as f32 }
            else if i >= release_start {
                let r = n_samples - release_start;
                if r > 0 { 1.0 - (i - release_start) as f32 / r as f32 } else { 0.0 }
            } else { 1.0 };
        out.push(s * env);
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// SampleMixer — микшер с планировщиком нот
// ─────────────────────────────────────────────────────────────────────────────

struct ActiveVoice {
    samples: Vec<f32>,
    pos: usize,
    volume: f32,
    start_time: f64,
    end_time: f64,
}

struct ScheduledNote {
    freq: f32,
    duration: f32,
    string_idx: usize,
    fret: usize,
    volume: f32,
    start_time: f64,
}

pub struct SampleMixer {
    active_voices: Vec<ActiveVoice>,
    scheduled_notes: Vec<ScheduledNote>,
    stream_time: f64,
    sampler: Arc<Mutex<GuitarSampler>>,
    device_sample_rate: u32,
}

impl SampleMixer {
    pub fn new(sampler: Arc<Mutex<GuitarSampler>>) -> Self {
        Self {
            active_voices: Vec::with_capacity(64),
            scheduled_notes: Vec::with_capacity(512),
            stream_time: 0.0,
            sampler,
            device_sample_rate: SAMPLE_RATE,
        }
    }

    pub fn stop_all(&mut self) {
        self.active_voices.clear();
        self.scheduled_notes.clear();
    }

    /// Воспроизвести готовый f32-буфер (WAV из тюнера).
    pub fn queue(&mut self, request: PlayRequest) {
        if request.samples.is_empty() || request.volume <= 0.0 { return; }
        let dur = request.samples.len() as f64 / self.device_sample_rate as f64;
        self.active_voices.push(ActiveVoice {
            samples: request.samples,
            pos: 0,
            volume: request.volume,
            start_time: self.stream_time,
            end_time: self.stream_time + dur + 0.1,
        });
    }

    /// Запланировать гитарную ноту на абсолютный момент stream_time.
    pub fn schedule_note(&mut self, note: NoteRequest, fret: usize) {
        let start = note.start_time.unwrap_or(self.stream_time);
        self.scheduled_notes.push(ScheduledNote {
            freq: note.freq,
            duration: note.duration,
            string_idx: note.string_idx,
            fret,
            volume: note.volume,
            start_time: start,
        });
    }

    fn synthesize(&self, sn: &ScheduledNote) -> Vec<f32> {
        let sr = self.device_sample_rate;
        // Пробуем сэмплер
        if let Ok(sampler) = self.sampler.try_lock() {
            if sampler.is_available() {
                let rendered = sampler.render_note(
                    sn.string_idx, sn.fret, sn.duration, sr, sn.volume,
                );
                if !rendered.is_empty() {
                    return rendered;
                }
            }
        }
        // Fallback: Karplus-Strong
        let mut ks = karplus_strong(sn.freq, sn.duration, sn.string_idx, sr);
        for s in &mut ks { *s *= sn.volume; }
        ks
    }

    pub fn mix(&mut self, output: &mut [AudioSample]) {
        let buf_dur = output.len() as f64 / (self.device_sample_rate as f64 * CHANNELS as f64);
        output.fill(0.0);
        let end_of_buf = self.stream_time + buf_dur;

        // Активируем запланированные ноты
        let mut i = 0;
        while i < self.scheduled_notes.len() {
            if self.scheduled_notes[i].start_time <= end_of_buf {
                let sn = self.scheduled_notes.swap_remove(i);
                let samples = self.synthesize(&sn);
                let dur = samples.len() as f64 / self.device_sample_rate as f64;
                self.active_voices.push(ActiveVoice {
                    samples,
                    pos: 0,
                    volume: 1.0, // уже учтён в synthesize
                    start_time: sn.start_time,
                    end_time: sn.start_time + dur,
                });
            } else { i += 1; }
        }

        // Микшируем активные голоса
        self.active_voices.retain_mut(|voice| {
            if voice.end_time < self.stream_time { return false; }
            for frame in 0..(output.len() / 2) {
                let t = self.stream_time + frame as f64 / self.device_sample_rate as f64;
                if t < voice.start_time { continue; }
                if voice.pos >= voice.samples.len() { return false; }
                let s = voice.samples[voice.pos] * voice.volume;
                voice.pos += 1;
                let oi = frame * 2;
                if oi + 1 < output.len() {
                    output[oi]     += s;
                    output[oi + 1] += s;
                }
            }
            voice.pos < voice.samples.len()
        });

        // Soft clip
        for s in output.iter_mut() { *s = s.tanh() * 0.9; }
        self.stream_time += buf_dur;
    }

    pub fn stream_time(&self) -> f64 { self.stream_time }
}

// ─────────────────────────────────────────────────────────────────────────────
// OutputController
// ─────────────────────────────────────────────────────────────────────────────

pub struct OutputController {
    host: cpal::Host,
    devices: Vec<Device>,
    stream: Option<Stream>,
    mixer: Arc<Mutex<SampleMixer>>,
    sampler: Arc<Mutex<GuitarSampler>>,
    pub active_sample_rate: u32,
}

impl OutputController {
    pub fn new() -> Self {
        let host = cpal::default_host();
        let devices: Vec<_> = host.output_devices()
            .map(|it| it.collect())
            .unwrap_or_default();
        println!("🔊 Audio Engine: {} output devices", devices.len());
        for (i, d) in devices.iter().enumerate() {
            if let Ok(name) = d.name() { println!("   [{}] {}", i, name); }
        }
        let sampler = Arc::new(Mutex::new(GuitarSampler::new()));
        let mixer = Arc::new(Mutex::new(SampleMixer::new(sampler.clone())));
        Self { host, devices, stream: None, mixer, sampler, active_sample_rate: SAMPLE_RATE }
    }

    /// Загружает WAV-сэмплы струн. Вызовите после new(), до start().
    /// Возвращает сколько струн загружено.
    pub fn load_guitar_samples(&self) -> usize {
        if let Ok(mut s) = self.sampler.lock() {
            s.try_load_all()
        } else { 0 }
    }

    /// Загружает конкретный файл для конкретной струны (для кастомных путей).
    pub fn load_string_sample(&self, string_idx: usize, path: &str) -> Result<(), String> {
        self.sampler.lock()
            .map_err(|_| "Sampler lock failed".to_string())?
            .load_string(string_idx, path)
    }

    pub fn sampler_available(&self) -> bool {
        self.sampler.lock().map(|s| s.is_available()).unwrap_or(false)
    }

    pub fn list_devices(&self) -> Vec<String> {
        self.devices.iter().filter_map(|d| d.name().ok()).collect()
    }

    /// Немедленно останавливает все звуки (очищает микшер).
    /// try_lock с повтором — аудио-поток удерживает lock только ~1мс на mix().
    pub fn stop_playback(&self) {
        for _ in 0..20 {
            if let Ok(mut m) = self.mixer.try_lock() {
                m.stop_all();
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        // Крайний случай: блокирующий lock
        if let Ok(mut m) = self.mixer.lock() { m.stop_all(); }
    }

    pub fn start(&mut self, device_idx: usize) -> Result<(), String> {
        if device_idx >= self.devices.len() {
            return Err(format!("Invalid device index {}", device_idx));
        }
        self.stop();
        let device = &self.devices[device_idx];
        let config = device.default_output_config().map_err(|e| e.to_string())?;
        self.active_sample_rate = config.sample_rate().0;

        // Обновляем device_sample_rate в микшере
        if let Ok(mut m) = self.mixer.lock() {
            m.device_sample_rate = self.active_sample_rate;
        }
        println!("🚀 Audio on: {:?} @ {} Hz (sampler: {})",
            device.name(), self.active_sample_rate,
            if self.sampler_available() { "WAV" } else { "KS" });

        let mixer_clone = self.mixer.clone();
        let stream = match config.sample_format() {
            SampleFormat::F32 => device.build_output_stream(
                &config.into(),
                move |out: &mut [f32], _| {
                    if let Ok(mut m) = mixer_clone.lock() { m.mix(out); }
                },
                |e| eprintln!("❌ Audio: {}", e), None,
            ).map_err(|e| e.to_string())?,
            SampleFormat::I16 => device.build_output_stream(
                &config.into(),
                move |out: &mut [i16], _| {
                    if let Ok(mut m) = mixer_clone.lock() {
                        let mut tmp = vec![0.0f32; out.len()];
                        m.mix(&mut tmp);
                        for (o, s) in out.iter_mut().zip(tmp.iter()) {
                            *o = <i16 as FromSample<f32>>::from_sample_(*s);
                        }
                    }
                },
                |e| eprintln!("❌ Audio: {}", e), None,
            ).map_err(|e| e.to_string())?,
            _ => return Err("Unsupported sample format".into()),
        };
        stream.play().map_err(|e| e.to_string())?;
        self.stream = Some(stream);
        println!("✅ Audio stream started.");
        Ok(())
    }

    pub fn stop(&mut self) { self.stream = None; }

    /// Воспроизвести WAV-буфер (для тюнера).
    pub fn play(&self, request: PlayRequest) {
        if let Ok(mut m) = self.mixer.try_lock() { m.queue(request); }
        else { eprintln!("⚠️ Mixer busy"); }
    }

    /// Запланировать гитарную ноту.
    pub fn play_note(&self, note: NoteRequest) {
        // Для NoteRequest нужен fret — кодируем через freq/tuning
        // Определяем fret по частоте (приближённо через log2)
        // Если freq дана напрямую, используем её через KS/sampler с fret=0 и speed
        // Но лучший вариант: передавать fret явно через расширенный метод
        let fret = note_freq_to_fret_approx(note.freq, note.string_idx);
        if let Ok(mut m) = self.mixer.try_lock() {
            m.schedule_note(note, fret);
        }
    }

    /// Запланировать гитарную ноту с явным fret (предпочтительный метод).
    pub fn play_note_fret(&self, note: NoteRequest, fret: usize) {
        if let Ok(mut m) = self.mixer.try_lock() {
            m.schedule_note(note, fret);
        }
    }

    pub fn stream_time(&self) -> f64 {
        self.mixer.lock().map(|m| m.stream_time()).unwrap_or(0.0)
    }

    pub fn get_mixer(&self) -> Arc<Mutex<SampleMixer>> { self.mixer.clone() }
}

/// Приближённое определение лада по частоте и индексу струны.
/// Используется только когда fret не передаётся явно.
fn note_freq_to_fret_approx(freq: f32, string_idx: usize) -> usize {
    const OPEN: [f32; 6] = [82.41, 110.00, 146.83, 196.00, 246.94, 329.63];
    let open = OPEN[string_idx.min(5)];
    let semitones = 12.0 * (freq / open).log2();
    semitones.round().max(0.0) as usize
}

// ─────────────────────────────────────────────────────────────────────────────
// Утилиты
// ─────────────────────────────────────────────────────────────────────────────

pub fn save_wav_file(path: &str, samples: &[i16], sr: u32) -> std::io::Result<()> {
    use std::fs::File;
    use std::io::{Write, BufWriter};
    let f = File::create(path)?;
    let mut w = BufWriter::new(f);
    let ds = samples.len() as u32 * 2;
    w.write_all(b"RIFF")?; w.write_all(&(36 + ds).to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?; w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?; w.write_all(&1u16.to_le_bytes())?;
    w.write_all(&sr.to_le_bytes())?; w.write_all(&(sr * 2).to_le_bytes())?;
    w.write_all(&2u16.to_le_bytes())?; w.write_all(&16u16.to_le_bytes())?;
    w.write_all(b"data")?; w.write_all(&ds.to_le_bytes())?;
    for s in samples { w.write_all(&s.to_le_bytes())?; }
    w.flush()
}

pub fn save_test_wav(filename: &str, samples: &[f32], sample_rate: u32) {
    use std::fs::File;
    use std::io::{Write, BufWriter};
    let mut pcm = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let v = (s * 32767.0).clamp(-32768.0, 32767.0) as i16;
        pcm.extend_from_slice(&v.to_le_bytes());
    }
    let data_len = pcm.len() as u32;
    let mut w = BufWriter::new(File::create(filename).expect("create failed"));
    w.write_all(b"RIFF").unwrap(); w.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    w.write_all(b"WAVEfmt ").unwrap(); w.write_all(&16u32.to_le_bytes()).unwrap();
    w.write_all(&1u16.to_le_bytes()).unwrap(); w.write_all(&1u16.to_le_bytes()).unwrap();
    w.write_all(&sample_rate.to_le_bytes()).unwrap();
    w.write_all(&(sample_rate * 2).to_le_bytes()).unwrap();
    w.write_all(&2u16.to_le_bytes()).unwrap(); w.write_all(&16u16.to_le_bytes()).unwrap();
    w.write_all(b"data").unwrap(); w.write_all(&data_len.to_le_bytes()).unwrap();
    w.write_all(&pcm).unwrap(); w.flush().unwrap();
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum WaveType { Triangle, Sawtooth, Square, Sine }

pub fn generate_wav(freq: f32, dur: f32, _wt: WaveType) -> Vec<u8> {
    let samples = karplus_strong(freq, dur, 3, SAMPLE_RATE);
    let mut pcm = Vec::with_capacity(samples.len() * 2);
    for &s in &samples {
        let v = (s * 32767.0).clamp(-32768.0, 32767.0) as i16;
        pcm.extend_from_slice(&v.to_le_bytes());
    }
    let mut w = Vec::with_capacity(44 + pcm.len());
    w.extend(b"RIFF"); w.extend(&(36 + pcm.len() as u32).to_le_bytes());
    w.extend(b"WAVEfmt "); w.extend(&16u32.to_le_bytes());
    w.extend(&1u16.to_le_bytes()); w.extend(&1u16.to_le_bytes());
    w.extend(&SAMPLE_RATE.to_le_bytes()); w.extend(&(SAMPLE_RATE * 2).to_le_bytes());
    w.extend(&2u16.to_le_bytes()); w.extend(&16u16.to_le_bytes());
    w.extend(b"data"); w.extend(&(pcm.len() as u32).to_le_bytes());
    w.extend(pcm); w
}

// ─────────────────────────────────────────────────────────────────────────────
// Рендеринг полной песни в WAV файл
// ─────────────────────────────────────────────────────────────────────────────

/// Рендерит всю песню в WAV файл
/// 
/// # Arguments
/// * `song_events` - список событий песни (аккордов)
/// * `tuning` - настройка струн [f32; 6]
/// * `output_path` - путь для сохранения WAV файла
/// * `sampler` - сэмплер (если доступен, использует WAV сэмплы)
/// 
/// # Returns
/// * `Ok(())` при успешном рендеринге
/// * `Err(String)` при ошибке
pub fn render_song_to_wav(
    song_events: &[crate::parser::SongEvent],
    tuning: &[f32; 6],
    output_path: &str,
    sampler: Option<&GuitarSampler>,
) -> Result<(), String> {
    
    // Находим максимальное время окончания ноты
    let mut max_end_time = 0.0_f64;
    for event in song_events {
        let event_end = event.time + event.duration.max(event.sound_duration);
        if event_end > max_end_time {
            max_end_time = event_end;
        }
    }
    
    // Добавляем небольшой хвост для затухания
    let total_duration = max_end_time + 0.5;
    let total_samples = (SAMPLE_RATE as f64 * total_duration) as usize;
    
    // Создаем буфер для микширования (стерео)
    let mut mix_buffer = vec![0.0_f32; total_samples * 2];
    
    println!("🎵 Rendering song: {} events, {:.2}s duration", 
             song_events.len(), total_duration);
    
    // Рендерим каждое событие
    for event in song_events {
        for note in &event.notes {
            // Вычисляем частоту ноты
            let base_freq = tuning[note.string_idx.min(5)];
            let freq = base_freq * 2.0_f32.powf(note.fret as f32 / 12.0);
            
            // Длительность ноты
            let note_duration = event.sound_duration.max(event.duration).max(0.1) as f32;
            
            // Синтезируем ноту
            let note_samples = if let Some(sampler) = sampler {
                if sampler.is_available() {
                    sampler.render_note(note.string_idx, note.fret, note_duration, SAMPLE_RATE, 0.7)
                } else {
                    karplus_strong(freq, note_duration, note.string_idx, SAMPLE_RATE)
                }
            } else {
                karplus_strong(freq, note_duration, note.string_idx, SAMPLE_RATE)
            };
            
            // Вычисляем позицию начала ноты в буфере
            let start_sample = (event.time * SAMPLE_RATE as f64) as usize;
            
            // Микшируем ноту в общий буфер
            for (i, &sample) in note_samples.iter().enumerate() {
                let buf_idx = (start_sample + i) * 2;
                if buf_idx + 1 < mix_buffer.len() {
                    mix_buffer[buf_idx] += sample;     // Левый канал
                    mix_buffer[buf_idx + 1] += sample; // Правый канал
                }
            }
        }
    }
    
    // Нормализация для предотвращения клиппинга
    let max_amplitude = mix_buffer.iter()
        .map(|s| s.abs())
        .fold(0.0_f32, f32::max);
    
    if max_amplitude > 0.9 {
        let scale = 0.9 / max_amplitude;
        for s in mix_buffer.iter_mut() {
            *s *= scale;
        }
        println!("🔊 Normalized audio (peak: {:.2})", max_amplitude);
    }
    
    // Применяем soft clip
    for s in mix_buffer.iter_mut() {
        *s = s.tanh() * 0.95;
    }
    
    // Конвертируем в i16 и сохраняем
    let mut pcm = Vec::with_capacity(mix_buffer.len());
    for &s in &mix_buffer {
        let v = (s * 32767.0).clamp(-32768.0, 32767.0) as i16;
        pcm.extend_from_slice(&v.to_le_bytes());
    }
    
    // Записываем WAV файл
    std::fs::create_dir_all(std::path::Path::new(output_path).parent().unwrap())
        .map_err(|e| format!("Failed to create directory: {}", e))?;
    
    let mut file = std::fs::File::create(output_path)
        .map_err(|e| format!("Failed to create file: {}", e))?;
    
    use std::io::Write;
    let data_len = pcm.len() as u32;
    
    // WAV заголовок
    file.write_all(b"RIFF").map_err(|e| e.to_string())?;
    file.write_all(&(36 + data_len).to_le_bytes()).map_err(|e| e.to_string())?;
    file.write_all(b"WAVEfmt ").map_err(|e| e.to_string())?;
    file.write_all(&16u32.to_le_bytes()).map_err(|e| e.to_string())?;
    file.write_all(&1u16.to_le_bytes()).map_err(|e| e.to_string())?; // PCM
    file.write_all(&2u16.to_le_bytes()).map_err(|e| e.to_string())?; // Стерео
    file.write_all(&SAMPLE_RATE.to_le_bytes()).map_err(|e| e.to_string())?;
    file.write_all(&(SAMPLE_RATE * 4).to_le_bytes()).map_err(|e| e.to_string())?; // Byte rate
    file.write_all(&4u16.to_le_bytes()).map_err(|e| e.to_string())?; // Block align
    file.write_all(&16u16.to_le_bytes()).map_err(|e| e.to_string())?; // Bits per sample
    file.write_all(b"data").map_err(|e| e.to_string())?;
    file.write_all(&data_len.to_le_bytes()).map_err(|e| e.to_string())?;
    file.write_all(&pcm).map_err(|e| e.to_string())?;
    
    println!("✅ Saved WAV: {} ({} samples)", output_path, total_samples);
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Генератор случайных мелодий
// ─────────────────────────────────────────────────────────────────────────────

/// Генерирует случайную мелодию из 88 клавиш фортепиано
/// 
/// # Arguments
/// * `max_duration` - максимальная длительность в секундах (обычно 60.0)
/// 
/// # Returns
/// * `Vec<SongEvent>` - список событий для сохранения в JSON
pub fn generate_random_melody(max_duration: f64) -> Vec<crate::parser::SongEvent> {
    use crate::parser::{SongEvent, ChordNote, NoteTechnique};
    use macroquad::rand::gen_range;
    
    // 88 клавиш фортепиано: A0 (MIDI 21) до C8 (MIDI 108)
    // Для гитары используем диапазон E2 (MIDI 40) до E6 (MIDI 88) - 49 нот
    // Но пользователь просит 88 нот, поэтому используем весь диапазон
    
    let midi_min = 21; // A0
    let midi_max = 108; // C8
    
    let mut events: Vec<SongEvent> = Vec::new();
    let mut current_time = 0.0;
    
    // Генерируем ноты пока не достигнем лимита времени
    while current_time < max_duration {
        // Случайная длительность ноты (от 0.25 до 2.0 секунд)
        let duration = match gen_range(0, 10) {
            0..=2 => 0.25,  // шестнадцатая
            3..=6 => 0.5,   // восьмая
            7..=8 => 1.0,   // четверть
            _ => 2.0,       // половина
        };
        
        // Случайная нота (MIDI номер)
        let midi_note = gen_range(midi_min, midi_max + 1);
        
        // Конвертируем MIDI в частоту
        let freq = 440.0 * 2.0_f32.powf((midi_note as f32 - 69.0) / 12.0);
        
        // Определяем струну и лад для гитары (приближённо)
        // Используем стандартный строй гитары
        let open_strings = [82.41, 110.00, 146.83, 196.00, 246.94, 329.63]; // E2, A2, D3, G3, B3, E4
        
        let mut best_string = 0;
        let mut best_fret = 0;
        let mut min_diff = f32::MAX;
        
        for (string_idx, &open_freq) in open_strings.iter().enumerate() {
            // Вычисляем лад
            let semitones = 12.0 * (freq / open_freq).log2();
            let fret = semitones.round() as i32;
            
            if fret >= 0 && fret <= 20 {
                let actual_freq = open_freq * 2.0_f32.powf(fret as f32 / 12.0);
                let diff = (actual_freq - freq).abs();
                
                if diff < min_diff {
                    min_diff = diff;
                    best_string = string_idx;
                    best_fret = fret as usize;
                }
            }
        }
        
        // Иногда добавляем аккорды (2-3 ноты одновременно)
        let num_notes = if gen_range(0, 10) < 3 { gen_range(2, 4) } else { 1 };
        
        let mut chord_notes = Vec::new();
        for i in 0..num_notes {
            let note_midi = if i == 0 {
                midi_note
            } else {
                // Добавляем ноты из аккорда (терция, квинта)
                midi_note + [3, 7, 12][i % 3]
            };
            
            let note_freq = 440.0 * 2.0_f32.powf((note_midi as f32 - 69.0) / 12.0);
            
            // Находим струну и лад для этой ноты
            let mut note_string = 0;
            let mut note_fret = 0;
            let mut note_min_diff = f32::MAX;
            
            for (string_idx, &open_freq) in open_strings.iter().enumerate() {
                let semitones = 12.0 * (note_freq / open_freq).log2();
                let fret = semitones.round() as i32;
                
                if fret >= 0 && fret <= 20 {
                    let actual_freq = open_freq * 2.0_f32.powf(fret as f32 / 12.0);
                    let diff = (actual_freq - note_freq).abs();
                    
                    if diff < note_min_diff {
                        note_min_diff = diff;
                        note_string = string_idx;
                        note_fret = fret as usize;
                    }
                }
            }
            
            // Проверяем, что струна не уже используется в этом аккорде
            if !chord_notes.iter().any(|n: &ChordNote| n.string_idx == note_string) {
                chord_notes.push(ChordNote {
                    string_idx: note_string,
                    fret: note_fret,
                });
            }
        }
        
        // Создаём событие
        events.push(SongEvent {
            time: current_time,
            duration: duration,
            sound_duration: duration * 0.9, // чуть короче для чёткости
            notes: chord_notes,
            chord_name: None,
            technique: NoteTechnique::Normal,
        });
        
        current_time += duration;
        
        // Иногда добавляем паузу
        if gen_range(0, 10) < 2 {
            current_time += gen_range(0.25, 1.0);
        }
    }
    
    println!("🎹 Generated melody: {} events, {:.1}s duration", events.len(), current_time);
    events
}

/// Конвертирует MIDI номер в название ноты (например, "C4", "A#3")
pub fn midi_to_note_name(midi: u8) -> String {
    const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let note_idx = (midi % 12) as usize;
    let octave = (midi / 12).saturating_sub(1);
    format!("{}{}", NOTE_NAMES[note_idx], octave)
}

/// Конвертирует MIDI номер в частоту
pub fn midi_to_freq(midi: u8) -> f32 {
    440.0 * 2.0_f32.powf((midi as f32 - 69.0) / 12.0)
}

/// Склейка нескольких WAV файлов в один
/// 
/// # Arguments
/// * `note_names` - список названий нот (например, ["C4", "D#4", "E5"])
/// * `output_path` - путь для сохранения итогового WAV файла
/// 
/// # Returns
/// * `Ok(())` при успешной склейке
/// * `Err(String)` при ошибке
pub fn concatenate_piano_wavs(note_names: &[String], output_path: &str) -> Result<(), String> {
    use std::fs::File;
    use std::io::{Write, BufWriter, Read};
    
    if note_names.is_empty() {
        return Err("No notes to concatenate".into());
    }
    
    let mut all_samples: Vec<i16> = Vec::new();
    let mut sample_rate = SAMPLE_RATE;
    
    for note_name in note_names {
        let wav_path = format!("assets/piano/{}.wav", note_name);
        
        // Читаем WAV файл
        let mut file = File::open(&wav_path)
            .map_err(|e| format!("Cannot open {}: {}", wav_path, e))?;
        
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .map_err(|e| format!("Cannot read {}: {}", wav_path, e))?;
        
        // Парсим WAV заголовок
        if buffer.len() < 44 {
            return Err(format!("Invalid WAV file: {}", wav_path));
        }
        
        // Извлекаем sample rate из заголовка (байты 24-27)
        if note_names.iter().position(|n| n == note_name) == Some(0) {
            sample_rate = u32::from_le_bytes([buffer[24], buffer[25], buffer[26], buffer[27]]);
        }
        
        // Извлекаем данные (после заголовка 44 байта)
        let data_start = 44;
        let data_len = buffer.len() - data_start;
        
        // Конвертируем байты в i16 сэмплы
        for i in (data_start..buffer.len()).step_by(2) {
            if i + 1 < buffer.len() {
                let sample = i16::from_le_bytes([buffer[i], buffer[i + 1]]);
                all_samples.push(sample);
            }
        }
        
        // Добавляем небольшую паузу между нотами (50мс)
        let pause_samples = (sample_rate as f32 * 0.05) as usize;
        for _ in 0..pause_samples {
            all_samples.push(0);
        }
    }
    
    // Записываем итоговый WAV файл
    std::fs::create_dir_all(std::path::Path::new(output_path).parent().unwrap())
        .map_err(|e| format!("Failed to create directory: {}", e))?;
    
    let mut file = File::create(output_path)
        .map_err(|e| format!("Failed to create file: {}", e))?;
    
    let mut w = BufWriter::new(file);
    let data_len = all_samples.len() as u32 * 2;
    
    // WAV заголовок
    w.write_all(b"RIFF").map_err(|e| e.to_string())?;
    w.write_all(&(36 + data_len).to_le_bytes()).map_err(|e| e.to_string())?;
    w.write_all(b"WAVEfmt ").map_err(|e| e.to_string())?;
    w.write_all(&16u32.to_le_bytes()).map_err(|e| e.to_string())?;
    w.write_all(&1u16.to_le_bytes()).map_err(|e| e.to_string())?; // PCM
    w.write_all(&1u16.to_le_bytes()).map_err(|e| e.to_string())?; // Mono
    w.write_all(&sample_rate.to_le_bytes()).map_err(|e| e.to_string())?;
    w.write_all(&(sample_rate * 2).to_le_bytes()).map_err(|e| e.to_string())?; // Byte rate
    w.write_all(&2u16.to_le_bytes()).map_err(|e| e.to_string())?; // Block align
    w.write_all(&16u16.to_le_bytes()).map_err(|e| e.to_string())?; // Bits per sample
    w.write_all(b"data").map_err(|e| e.to_string())?;
    w.write_all(&data_len.to_le_bytes()).map_err(|e| e.to_string())?;
    
    // Записываем сэмплы
    for sample in &all_samples {
        w.write_all(&sample.to_le_bytes()).map_err(|e| e.to_string())?;
    }
    
    w.flush().map_err(|e| e.to_string())?;
    
    println!("✅ Concatenated {} notes into {}", note_names.len(), output_path);
    Ok(())
}