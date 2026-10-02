// src/midi_input.rs
//
// Контроллер MIDI-ввода: чтение Note On/Off с MIDI-клавиатуры (синтезатора)
// по USB/MIDI-кабелю. Работает параллельно с основным потоком через Arc<Mutex>.

use std::sync::{Arc, Mutex};
use midir::{MidiInput, MidiInputConnection};

// ─── Общие данные MIDI-ввода (доступны из callback-потока и main-потока) ────

pub struct MidiData {
    /// Последняя нажатая MIDI-нота (0–127).
    pub last_note: Option<u8>,
    /// Velocity последнего нажатия (1–127).
    pub last_velocity: u8,
    /// true = нота была нажата, но ещё не обработана игровым циклом.
    pub note_just_pressed: bool,
    /// Все текущие удерживаемые ноты.
    pub active_notes: Vec<u8>,
    /// Подключено ли устройство.
    pub is_connected: bool,
    /// Сообщение об ошибке (если есть).
    pub error_msg: Option<String>,
}

impl MidiData {
    fn new() -> Self {
        Self {
            last_note: None,
            last_velocity: 0,
            note_just_pressed: false,
            active_notes: Vec::new(),
            is_connected: false,
            error_msg: None,
        }
    }

    /// Сбрасывает флаг `note_just_pressed` — вызывается после обработки
    /// нажатия в игровом цикле.
    pub fn consume_press(&mut self) -> Option<u8> {
        if self.note_just_pressed {
            self.note_just_pressed = false;
            self.last_note
        } else {
            None
        }
    }

    /// Возвращает клон списка активных (удерживаемых) нот.
    pub fn get_active_notes(&self) -> Vec<u8> {
        self.active_notes.clone()
    }

}

// ─── Wrapper для MidiInputConnection, чтобы сделать его Send ─────────────────
// MidiInputConnection не реализует Send на некоторых платформах, но мы
// гарантируем, что он живёт только в main-потоке и никогда не перемещается.
// Callback работает исключительно через Arc<Mutex<MidiData>>, который Send+Sync.

// ─── Wrapper для сохранения и MidiInput, и MidiInputConnection ───────────────
struct MidiConnWrapper {
    connection: Option<MidiInputConnection<Arc<Mutex<MidiData>>>>,
}

impl MidiConnWrapper {
    fn new() -> Self {
        Self {connection: None }
    }
}

unsafe impl Send for MidiConnWrapper {}
// ─── Контроллер MIDI-ввода ──────────────────────────────────────────────────

pub struct MidiInputController {
    pub midi_data: Arc<Mutex<MidiData>>,
    /// Имена доступных MIDI-портов ввода.
    pub port_names: Vec<String>,
    /// Активное подключение (живёт в main-потоке).
    connection: MidiConnWrapper,
}

impl MidiInputController {
    pub fn new() -> Self {
        let mut ctrl = Self {
            midi_data: Arc::new(Mutex::new(MidiData::new())),
            port_names: Vec::new(),
            connection: MidiConnWrapper::new(),
        };
        ctrl.refresh_ports();
        ctrl
    }

    /// Автоматически подключается к первому доступному MIDI-порту
    pub fn auto_connect(&mut self) -> bool {
        if self.connection.connection.is_some() {
            return true; // Уже подключен
        }
        
        let midi_in = match MidiInput::new("piano_auto") {
            Ok(m) => m,
            Err(e) => {
                eprintln!("❌ MIDI init error: {}", e);
                return false;
            }
        };
        
        let ports = midi_in.ports();
        if ports.is_empty() {
            eprintln!("❌ No MIDI ports found");
            return false;
        }
        
        // Пытаемся подключиться к первому порту
        let port_name = midi_in.port_name(&ports[0])
            .unwrap_or_else(|_| "Unknown".into());
        
        match self.connect_port(0) {
            Ok(_) => {
                eprintln!("✅ MIDI auto-connected to: {}", port_name);
                true
            }
            Err(e) => {
                eprintln!("❌ MIDI auto-connect failed: {}", e);
                false
            }
        }
    }
    
    /// Возвращает true если MIDI подключен
    pub fn is_connected(&self) -> bool {
        self.connection.connection.is_some()
    }

    /// Перечисляет доступные MIDI-порты ввода.
    pub fn refresh_ports(&mut self) {
        self.port_names.clear();
        match MidiInput::new("piano_query") {
            Ok(midi_in) => {
                for port in midi_in.ports() {
                    if let Ok(name) = midi_in.port_name(&port) {
                        self.port_names.push(name);
                    }
                }
                if !self.port_names.is_empty() {
                    println!("🎹 MIDI ports found: {:?}", self.port_names);
                }
            }
            Err(e) => {
                eprintln!("⚠️ Cannot init MIDI input: {}", e);
            }
        }
    }

    /// Подключается к MIDI-порту по индексу.
    pub fn connect_port(&mut self, port_idx: usize) -> Result<(), String> {
        // Сначала отключаем существующее подключение
        self.disconnect();

        let midi_in = MidiInput::new("piano_input")
            .map_err(|e| format!("MIDI init error: {}", e))?;
        let ports = midi_in.ports();

        if port_idx >= ports.len() {
            return Err(format!("Port index {} out of range ({})", port_idx, ports.len()));
        }

        let port_name = midi_in.port_name(&ports[port_idx])
            .unwrap_or_else(|_| "Unknown".into());

        let data = self.midi_data.clone();

        match midi_in.connect(
            &ports[port_idx],
            "piano_game_input",
            midi_callback,
            data,
        ) {
            Ok(conn) => {
                // Сохраняем только connection. midi_in успешно выполнил свою задачу 
                // и будет автоматически удалён (dropped), что абсолютно корректно.
                self.connection = MidiConnWrapper{connection: Some(conn)};
                
                if let Ok(mut d) = self.midi_data.lock() {
                    d.is_connected = true;
                    d.error_msg = None;
                }
                eprintln!("🎹 Connected to MIDI: {}", port_name);
                Ok(())
            }
            Err(e) => {
                let msg = format!("MIDI connect error: {}", e);
                if let Ok(mut d) = self.midi_data.lock() {
                    d.is_connected = false;
                    d.error_msg = Some(msg.clone());
                }
                Err(msg)
            }
        }
    }

    /// Отключает текущий MIDI-порт.
    pub fn disconnect(&mut self) {
        // ✅ ИСПРАВЛЕНО: обращаемся к полю `connection`, а не к `.0`
        if let Some(conn) = self.connection.connection.take() {
            
            // ✅ ИСПРАВЛЕНО: явная аннотация типов решает проблему вывода (E0282)
            let (_midi_in, data): (midir::MidiInput, Arc<Mutex<MidiData>>) = conn.close();
            self.midi_data = data;
            
            if let Ok(mut d) = self.midi_data.lock() {
                d.is_connected = false;
                d.active_notes.clear();
                // d.note_just_pressed = false; // если это поле есть в MidiData
            }
            eprintln!("🎹 MIDI disconnected");
        }
    }

    /// Возвращает клон Arc для передачи в GameState.
    pub fn get_midi_state(&self) -> Arc<Mutex<MidiData>> {
        self.midi_data.clone()
    }

    /// true = есть хотя бы один MIDI-порт.
    pub fn has_ports(&self) -> bool {
        !self.port_names.is_empty()
    }
}

impl Drop for MidiInputController {
    fn drop(&mut self) {
        self.disconnect();
    }
}

fn midi_callback(_timestamp: u64, message: &[u8], data: &mut Arc<Mutex<MidiData>>) {
    // ─── ДОБАВЛЕНО: лог ВСЕХ входящих MIDI-сообщений ───
    eprintln!("🎹 MIDI RAW: {:02X?} (len={})", message, message.len());
    
    if message.len() < 3 { 
        eprintln!("⚠️ MIDI: message too short, skipping");
        return; 
    }

    let status = message[0] & 0xF0;
    let channel = message[0] & 0x0F;
    let note = message[1];
    let velocity = message[2];

    if let Ok(mut d) = data.lock() {
        match status {
            0x90 if velocity > 0 => {
                d.last_note = Some(note);
                d.last_velocity = velocity;
                d.note_just_pressed = true;
                if !d.active_notes.contains(&note) {
                    d.active_notes.push(note);
                }
                let note_name = crate::get_note_name(440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0));
                eprintln!("🎹 MIDI Note ON:  {} (midi={}, vel={}, ch={})", note_name, note, velocity, channel);
            }
            0x80 | 0x90 => {
                d.active_notes.retain(|&n| n != note);
                let note_name = crate::get_note_name(440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0));
                eprintln!("🎹 MIDI Note OFF: {} (midi={})", note_name, note);
            }
            _ => {
                eprintln!("🎹 MIDI other: status=0x{:02X} note={} vel={}", status, note, velocity);
            }
        }
    } else {
        eprintln!("❌ MIDI: failed to lock MidiData!");
    }
}