[Setup]
; === Основные настройки ===
AppName=Ranrock
AppVersion=0.1
AppVerName=Bomb Notes 1.0
AppPublisher=Sergey Telenyak
AppPublisherURL=https://your-website.com
AppSupportURL=https://your-website.com/support
AppUpdatesURL=https://your-website.com/updates

; === Пути установки ===
DefaultDirName={pf}\BombNotes
DefaultGroupName=BombNotes
; Не создавать подпапку в меню Пуск, если имя группы совпадает с именем приложения
DisableProgramGroupPage=yes

; === Выходные данные ===
OutputDir=installer_output
OutputBaseFilename=Sound_Bar
SetupIconFile=note.ico

; === Сжатие и права ===
Compression=lzma2/ultra64
SolidCompression=yes
PrivilegesRequired=admin
ArchitecturesInstallIn64BitMode=x64

; === ЗАЩИТА ДАННЫХ (Ресурсов) ===
; Включает шифрование файлов внутри архива установщика.
; Это защитит ваши JSON с песнями от простого извлечения.
Encryption=yes
;Password=1

; Запретить извлечение без установки
DisableDirPage=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
;Name: "quicklaunchicon"; Description: "{cm:CreateQuickLaunchIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked; Tasks: !desktopicon

[Files]
; Основной исполняемый файл
Source: "target\release\guitara-sound-mod.exe"; DestDir: "{app}"; Flags: ignoreversion

; Папка со шрифтами (обязательно для работы игры)
Source: "fonts\*"; DestDir: "{app}\fonts"; Flags: ignoreversion recursesubdirs createallsubdirs

; Папка с песнями (если есть демо-файлы, раскомментируйте строку ниже)
Source: "songs\*"; DestDir: "{app}\songs"; Flags: ignoreversion recursesubdirs createallsubdirs

; Все остальные файлы из папки dist (картинки, конфиги и т.д.)
Source: "*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs; Excludes: "play_notes.exe,fonts,fonts\*,songs,songs\*,icon.ico"

Source: "note.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
; Ярлык в меню Пуск
Name: "{group}\Bomb Notes"; Filename: "{app}\play_notes.exe"; IconFilename: "{app}\note.ico"
; Ярлык на рабочем столе
Name: "{autodesktop}\Bomb Notes"; Filename: "{app}\play_notes.exe"; Tasks: desktopicon; IconFilename: "{app}\note.ico"
; Ярлык в панели быстрого запуска
;Name: "{userappdata}\Microsoft\Internet Explorer\Quick Launch\Bomb Notes"; Filename: "{app}\play_notes.exe"; Tasks: quicklaunchicon

[Run]
; Запуск приложения сразу после установки (с чекбоксом в финальном окне)
Filename: "{app}\play_notes.exe";  Description: "{cm:LaunchProgram,Bomb Notes}"; Flags: nowait postinstall skipifsilent