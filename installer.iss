[Setup]
; === Основные настройки ===
AppName=Sound Bar
AppVersion=0.1
AppVerName=Sound Notes
AppPublisher=Sergey Naumenko
AppPublisherURL=https://your-website.com
AppSupportURL=https://your-website.com/support
AppUpdatesURL=https://your-website.com/updates

; === Пути установки ===
DefaultDirName={pf}\Sound-Bar
DefaultGroupName=Sound-Bar
; Не создавать подпапку в меню Пуск, если имя группы совпадает с именем приложения
DisableProgramGroupPage=yes

; === Выходные данные ===
OutputDir=installer
OutputBaseFilename=Sound-Bar_setup
SetupIconFile=note.ico

; === Сжатие и права ===
Compression=lzma2/ultra64
SolidCompression=yes
PrivilegesRequired=admin
ArchitecturesInstallIn64BitMode=x64

; === ЗАЩИТА ДАННЫХ (Ресурсов) ===
; Включает шифрование файлов внутри архива установщика.
; Это защитит ваши JSON с песнями от простого извлечения.
;Encryption=yes
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
Source: "target\release\SoundBar.exe"; DestDir: "{app}"; Flags: ignoreversion

; Папка со шрифтами (обязательно для работы игры)
Source: "fonts\*"; DestDir: "{app}\fonts"; Flags: ignoreversion recursesubdirs createallsubdirs

; Папка с песнями (если есть демо-файлы, раскомментируйте строку ниже)
Source: "songs\*"; DestDir: "{app}\songs"; Flags: ignoreversion recursesubdirs createallsubdirs

; Все остальные файлы из папки dist (картинки, конфиги и т.д.)
Source: "*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs; 

;Excludes: "SoundBar.exe,fonts,fonts\*,songs,songs\*,icon.ico"

Source: "note.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
; Ярлык в меню Пуск
Name: "{group}\Sound-Bar"; Filename: "{app}\Sound-Bar.exe"; IconFilename: "{app}\note.ico"
; Ярлык на рабочем столе
Name: "{autodesktop}\Sound-Bar"; Filename: "{app}\Sound-Bar.exe"; Tasks: desktopicon; IconFilename: "{app}\note.ico"
; Ярлык в панели быстрого запуска
;Name: "{userappdata}\Microsoft\Internet Explorer\Quick Launch\SoundBar"; Filename: "{app}\sound-bar.exe"; Tasks: quicklaunchicon

[Run]
; Запуск приложения сразу после установки (с чекбоксом в финальном окне)
Filename: "{app}\Sound-Bar.exe";  Description: "{cm:LaunchProgram,SoundBar}"; Flags: nowait postinstall skipifsilent