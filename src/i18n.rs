//! Localise application-owned labels. Project data and command identifiers stay verbatim.
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    Ru,
    En,
}

static ENGLISH: AtomicBool = AtomicBool::new(false);

impl Language {
    pub fn current() -> Self {
        if ENGLISH.load(Ordering::Relaxed) {
            Self::En
        } else {
            Self::Ru
        }
    }
    pub fn activate(self) {
        ENGLISH.store(self == Self::En, Ordering::Relaxed);
    }
    pub fn text(self, key: &str) -> &str {
        if self == Self::En { english(key) } else { key }
    }
}

pub fn tr(key: &str) -> &str {
    Language::current().text(key)
}

fn english(key: &str) -> &str {
    match key {
        "Разделы справки" => "Help topics",
        "Поиск по справке" => "Search help",
        "Ничего не найдено. Измените запрос или очистите поле поиска." => {
            "No matching topics. Change or clear the search text."
        }
        " · выкл." => " · off",
        " — копия" => " — copy",
        "--data-dir требует путь" => "--data-dir requires a path",
        "--port требует номер" => "--port requires a number",
        "; результат печати неизвестен" => {
            ". The print outcome is unknown"
        }
        "Bluetooth-адаптер не найден. Включите Bluetooth в Windows" => {
            "No Bluetooth adapter found. Enable Bluetooth in Windows"
        }
        "Bluetooth-адрес" => "Bluetooth address",
        "Bluetooth-соединение разорвано" => "Bluetooth disconnected",
        "Bluetooth-соединение сохраняется до закрытия приложения. Проверка связи — каждые 5 секунд; при обрыве выполняется переподключение." => {
            "Bluetooth stays connected while the app is open. The app checks every 5 seconds and reconnects after a disconnect."
        }
        "Bluetooth: тайм-аут записи" => "Bluetooth write timed out",
        "CSV превышает 4 МБ" => "CSV exceeds 4 MB",
        "D101 · симуляция" => "D101 · simulation",
        "D101 не поддерживает команду. Проверьте режим протокола" => {
            "D101 does not support this command. Check the protocol setting"
        }
        "D101 не подтвердил завершение печати" => {
            "D101 did not confirm completion"
        }
        "D101 поддерживает этикетки с зазором и прозрачные" => {
            "D101 supports gap labels and transparent labels"
        }
        "D101 подтвердил завершение" => "D101 confirmed completion",
        "IPP-порт занят. Закройте другой экземпляр D101 Label Studio" => {
            "IPP port is busy. Close the other instance of D101 Label Studio"
        }
        "PNG сохранён в разрешении печати 203 dpi" => {
            "PNG saved at the print resolution of 203 dpi"
        }
        "PWG не содержит страниц" => "PWG contains no pages",
        "RFID-метка рулона не прочитана. Тип и остаток этикеток неизвестны." => {
            "Could not read the RFID tag. Label type and remaining quantity are unknown."
        }
        "Автоматически" => "Automatic",
        "Атрибуты IPP превышают 64 КБ" => "IPP attributes exceed 64 KB",
        "Батарея" => "Battery",
        "Блок выходит за пределы этикетки" => {
            "Block extends outside the label"
        }
        "Блок на холсте" => "Block on canvas",
        "Блок относительно этикетки" => "Align block to label",
        "Блоки за границами нового холста будут сдвинуты или уменьшены. Изменение можно отменить." => {
            "Blocks outside the new canvas will be moved or resized. You can undo this change."
        }
        "Больше 1000 сочетаний; сократите диапазоны или списки" => {
            "More than 1,000 combinations. Shorten the ranges or lists"
        }
        "В диапазоне больше 1000 значений" => {
            "The range contains more than 1,000 values"
        }
        "В задании должно быть 1–1000 этикеток" => {
            "A job must contain 1–1,000 labels"
        }
        "В наборе" => "Include in set",
        "В наборе получится больше 1000 экземпляров; уменьшите количество или исключите прежние этикетки из печати" => {
            "The set would exceed 1,000 copies. Reduce copies or exclude existing labels"
        }
        "В очереди" => "Queued",
        "В проекте может быть не больше 1000 этикеток, включая шаблон" => {
            "A project can contain up to 1,000 labels including the template"
        }
        "В симуляции принтер не опрашивается." => {
            "Simulation does not query the printer."
        }
        "В списке больше 1000 значений" => {
            "List contains more than 1,000 values"
        }
        "Введите текст этикетки" => "Enter label text",
        "Вертикальная поправка в точках" => "Vertical offset in dots",
        "Вид" => "View",
        "Включите хотя бы одну этикетку в набор" => {
            "Include at least one label in the set"
        }
        "Вместимость получена из RFID принтера." => {
            "The printer supplied roll capacity via RFID."
        }
        "Вписать" => "Fit to window",
        "Все сочетания" => "All combinations",
        "Выберите Bluetooth-принтер в приложении" => {
            "Select a Bluetooth printer in the app"
        }
        "Выберите блок на холсте или добавьте новый через верхнее меню." => {
            "Select a block on the canvas or add one from the top menu."
        }
        "Выберите от 1 до 1000 экземпляров" => "Select 1–1,000 copies",
        "Выберите поддерживаемый размер" => "Select a supported size",
        "Выберите поддерживаемый размер D101 в меню «Настройки → Размер этикетки»" => {
            "Choose a supported D101 size in Settings → Label size"
        }
        "Выравнивание текста" => "Text alignment",
        "Генератор этикеток" => "Label generator",
        "Генератор этикеток…" => "Label generator…",
        "Готово" => "Completed",
        "Данные рулона изменились или связь потеряна. Обновите статус принтера." => {
            "The roll changed or the connection was lost. Refresh printer status."
        }
        "Данные считываются с принтера каждые 30 секунд и после печати." => {
            "The app reads the printer every 30 seconds and after printing."
        }
        "Диапазон чисел" => "Number range",
        "Длина этикетки должна быть 1–1600 точек" => {
            "Label length must be 1–1,600 dots"
        }
        "Длина: 10–200 мм" => "Length must be 10–200 mm",
        "До включительно" => "To (inclusive)",
        "Добавить" => "Insert",
        "Добавить этикетку" => "Add label",
        "Добавьте блок и перетащите его на этикетке" => {
            "Add a block and drag it onto the label"
        }
        "Добавьте в текст {номер} или {название} и задайте значения в генераторе" => {
            "Insert {number} or {name} in the text and set its values in the generator"
        }
        "Добавьте хотя бы одну непустую строку" => {
            "Add at least one non-empty line"
        }
        "Документ уже принят или задание завершено" => {
            "The document has already been received or the job has finished"
        }
        "Дублировать блок" => "Duplicate block",
        "Дублировать этикетку" => "Duplicate label",
        "Если отпечаток выше макета, сдвиньте его вниз. Направление указано для этикетки с выключенным поворотом на 180°. Значение 0 отключает поправку." => {
            "If the print sits too high, move it down. Directions assume 180° rotation is off. Set the offset to 0 to disable calibration."
        }
        "Жирный" => "Bold",
        "Задание не найдено" => "Job not found",
        "Задание превышает 32 МБ растра" => "Job raster data exceeds 32 MB",
        "Задание превышает лимит 32 млн точек" => {
            "Job exceeds 32 million dots"
        }
        "Задание уже завершено" => "Job already finished",
        "Задания приложения не изменяют остаток." => {
            "Local print jobs do not change the roll count."
        }
        "Закрыть" => "Close",
        "Заливка" => "Fill",
        "Запуск IPP…" => "Starting IPP…",
        "Запустите scripts/Install-Printer.ps1 от имени администратора. После установки D101 появится в списке принтеров Windows. Приложение должно оставаться запущенным во время печати." => {
            "Run scripts/Install-Printer.ps1 as administrator. D101 will appear in the Windows printer list. Keep the application running while printing."
        }
        "Зачёркнутый" => "Strikethrough",
        "Значения переменной" => "Variable values",
        "Изображение" => "Image",
        "Изображение не помещается в область D101" => {
            "Image does not fit the D101 print area"
        }
        "Изображение…" => "Image…",
        "Изображения" => "Images",
        "Изображения в создаваемом наборе превысят 16 МБ; уменьшите количество этикеток или размер изображения" => {
            "Generated images would exceed 16 MB. Reduce the label count or image size"
        }
        "Изображения превышают лимит памяти" => {
            "Images exceed the memory limit"
        }
        "Импорт CSV…" => "Import CSV…",
        "Имя в фигурных скобках должно начинаться с буквы или _ и содержать только буквы, цифры и _" => {
            "A variable name must begin with a letter or underscore and contain only letters, digits and underscores"
        }
        "Истекло время подключения к D101" => {
            "Connection to D101 timed out"
        }
        "Истекло время поиска Bluetooth" => "Bluetooth scan timed out",
        "Источник значений" => "Value source",
        "Исходный шаблон сохранится в наборе и будет исключён из печати. Новые этикетки можно редактировать. Создание отменяется одним действием." => {
            "The template stays in the set and is excluded from printing. You can edit the generated labels or undo the entire operation."
        }
        "Исчерпаны идентификаторы блоков" => "No block IDs remain",
        "Кабель USB-C" => "USB-C cable",
        "Кабельная 74 + 35 мм" => "Cable flag, 74 + 35 mm",
        "Кабельный контур рассчитан на 12,5 × 109 мм" => {
            "This cable outline requires 12.5 × 109 mm stock"
        }
        "Калибровка печати сохранена" => "Print calibration saved",
        "Калибровка печати…" => "Print calibration…",
        "Каталог вернул другой код рулона" => {
            "The catalog returned a different roll code"
        }
        "Каталог не нашёл этот расходник" => {
            "The catalog could not find this stock"
        }
        "Каталог не подтверждает совместимость с D101" => {
            "The catalog does not confirm D101 compatibility"
        }
        "Каталог пока недоступен; повтор через минуту" => {
            "Catalog temporarily unavailable. Retrying in one minute"
        }
        "Количество копий в CSV" => "Number of copies in the CSV",
        "Количество копий: 1–1000" => "Copies must be 1–1,000",
        "Количество копий: от 1 до 1000" => "Choose 1–1,000 copies",
        "Контур не соответствует пропорциям этикетки" => {
            "Outline proportions do not match the label"
        }
        "Контур относится к другому размеру этикетки" => {
            "Outline belongs to another label size"
        }
        "Контур рулона NIIMBOT" => "NIIMBOT roll outline",
        "Копий каждой этикетки" => "Copies per label",
        "Копия" => "Duplicate",
        "Круглая / овальная" => "Round / oval",
        "Курсив" => "Italic",
        "Лента" => "Media",
        "Линия" => "Line",
        "Линия сгиба флажка" => "Flag fold line",
        "Локальный редактор" => "Local editor",
        "Максимум 100 блоков и 1000 копий на этикетку" => {
            "Maximum 100 blocks and 1,000 copies per label"
        }
        "Максимум 1000 этикеток в задании" => {
            "A job supports up to 1,000 labels"
        }
        "Масштаб" => "Zoom",
        "Масштаб 100%" => "Zoom to 100%",
        "Минимум цифр" => "Minimum digits",
        "На задний план" => "Send to back",
        "На передний план" => "Bring to front",
        "Набор из редактора" => "Set from the editor",
        "Набор этикеток" => "Label set",
        "Название" => "Name",
        "Найденные устройства" => "Discovered devices",
        "Найти Niimbot D101" => "Find Niimbot D101",
        "Настройки" => "Settings",
        "Настройки сохранены" => "Settings saved",
        "Начертание" => "Font style",
        "Не более 1000 этикеток" => "A project can contain up to 1,000 labels",
        "Не более 12 цифр с ведущими нулями" => {
            "Use up to 12 digits with leading zeros"
        }
        "Не более 32 переменных" => "Use up to 32 variables",
        "Не более 32 переменных в шаблоне" => {
            "A template supports up to 32 variables"
        }
        "Не найден шрифт Segoe UI" => "Segoe UI font not found",
        "Не определена" => "Unknown",
        "Не определены" => "Unknown",
        "Не удалось загрузить шрифт Windows" => {
            "Could not load the Windows font"
        }
        "Не удалось открыть IPP-порт: возможно, приложение уже запущено" => {
            "Could not open the IPP port. The app may already be running"
        }
        "Не удалось подключиться. Отключите D101 от приложения на телефоне" => {
            "Could not connect. Disconnect D101 from the phone app"
        }
        "Не удалось прочитать шрифт" => "Could not read the font",
        "Не удалось создать изображение текста" => {
            "Could not create the text image"
        }
        "Не удалось создать контекст шрифта Windows" => {
            "Could not create a Windows font context"
        }
        "Неверная глубина цвета" => "Invalid colour depth",
        "Неверная группа IPP" => "Invalid IPP group",
        "Неверная длина растра" => "Invalid raster length",
        "Неверная длина строки PWG" => "Invalid PWG row length",
        "Неверное завершение коллекции IPP" => {
            "Invalid IPP collection end"
        }
        "Неверные параметры D101" => "Invalid D101 settings",
        "Неверный файл набора этикеток" => "Invalid label project file",
        "Недопустимая поправка печати" => "Invalid print offset",
        "Недопустимое разрешение PWG Raster" => {
            "Invalid PWG Raster resolution"
        }
        "Недопустимый размер PWG Raster" => "Invalid PWG Raster dimensions",
        "Недопустимый размер текстового блока" => {
            "Invalid text block size"
        }
        "Незакрытая коллекция IPP" => "Unclosed IPP collection",
        "Неизвестная версия файла редактора" => {
            "Unknown editor file version"
        }
        "Неизвестная ориентация шаблона" => {
            "Unknown template orientation"
        }
        "Неизвестно" => "Unknown",
        "Неизвестный ответ версии протокола; выберите B1 или V4 вручную" => {
            "Unknown protocol version reply. Choose B1 or V4 manually"
        }
        "Неизвестный формат кода рулона" => "Unknown roll code format",
        "Неизвестный формат размера рулона" => {
            "Unknown roll size format"
        }
        "Неизвестный формат счётчиков RFID" => {
            "Unknown RFID counter format"
        }
        "Некорректная длина кабельной этикетки" => {
            "Invalid cable label length"
        }
        "Некорректное или повторяющееся имя переменной" => {
            "Invalid or duplicate variable name"
        }
        "Некорректные параметры текста" => "Invalid text settings",
        "Некорректный код рулона" => "Invalid roll code",
        "Некорректный ответ заряда батареи" => {
            "Invalid battery response"
        }
        "Некорректный размер блока" => "Invalid block size",
        "Некорректный размер каталога" => "Invalid catalog size",
        "Некорректный растр контура" => "Invalid outline bitmap",
        "Некорректный статус печати" => "Invalid print status",
        "Неполная строка RFID" => "Incomplete RFID string",
        "Неполный ответ RFID" => "Incomplete RFID response",
        "Непрерывная лента" => "Continuous stock",
        "Несколько переменных" => "Multiple variables",
        "Нет высоты шаблона" => "Missing template height",
        "Нет данных" => "No data",
        "Нет длины строки RFID" => "Missing RFID string length",
        "Нет длины хвостика" => "Missing cable wrap length",
        "Нет названия расходника" => "Missing stock name",
        "Нет ориентации шаблона" => "Template orientation missing",
        "Нет связи · повтор через 5 с" => "Offline · retry in 5 s",
        "Нет такого образца" => "Sample not found",
        "Нет ширины шаблона" => "Missing template width",
        "Неточный размер каталога" => "Imprecise catalog dimensions",
        "Новый проект" => "New project",
        "Новый текст" => "New text",
        "Номер образца" => "Sample number",
        "Нужно 1–1000 этикеток" => "A project needs 1–1,000 labels",
        "Области вне контура не печатаются. Серые края рулона 25 мм — вне печатающей головки. «По формату» использует типовой контур; точный контур загружается по коду рулона при подключении." => {
            "Areas outside the outline are masked. Grey edges on 25 mm stock are outside the printhead. Stock default uses the standard shape. A roll-specific outline can be loaded when connected."
        }
        "Обновить состояние принтера" => "Refresh printer status",
        "Обновление…" => "Updating…",
        "Образец №" => "Sample #",
        "Обрезанный IPP-пакет" => "Truncated IPP packet",
        "Обрезанный PWG Raster" => "Truncated PWG Raster",
        "Обычные этикетки" => "Standard labels",
        "Одна строка — одно значение; пустые строки пропускаются" => {
            "Enter one value per line. Empty lines are skipped."
        }
        "Ожидался PWG Raster (RaS2). PDF/XPS напрямую не поддерживаются" => {
            "Expected PWG Raster (RaS2). Direct PDF/XPS input is unsupported"
        }
        "Ожидание данных" => "Waiting for data",
        "Ожидание завершения" => "Waiting for shutdown",
        "Оставить текущий" => "Keep current",
        "Оставьте хотя бы одну этикетку" => "Keep at least one label",
        "Остаток рулона" => "Roll remaining",
        "От" => "From",
        "Открыть…" => "Open…",
        "Отмена" => "Cancel",
        "Отменено" => "Cancelled",
        "Отменено пользователем" => "Cancelled by the user",
        "Отменить" => "Undo",
        "Отменить задание" => "Cancel job",
        "Очередь заполнена (32 задания)" => "Queue is full (32 jobs)",
        "Очередь печати" => "Print queue",
        "Очередь превышает 64 МБ" => "Queue exceeds 64 MB",
        "Очередь пуста" => "The queue is empty",
        "Ошибка" => "Error",
        "ПВХ-бирки" => "PVC tags",
        "Перед печатью задайте размер этикетки и выберите Bluetooth-принтер в меню «Настройки»." => {
            "Before printing, choose a label size and a Bluetooth printer in Settings."
        }
        "Переменная" => "Variable",
        "Переподключение…" => "Reconnecting…",
        "Переполнение RLE строки PWG" => "PWG run length exceeds row width",
        "Переполнение literal строки PWG" => "PWG literal run exceeds row width",
        "Перфорированные" => "Perforated",
        "Печатать набор" => "Print set",
        "Печатать текущую" => "Print current label",
        "Печать" => "Print",
        "Печать отменена" => "Printing cancelled",
        "Печать…" => "Printing…",
        "Питание\nLAN\nРезерв" => "Power\nLAN\nSpare",
        "Плотность" => "Density",
        "Плотность D101: 1–3" => "D101 density must be 1–3",
        "По вертикали" => "Vertical",
        "По горизонтали" => "Horizontal",
        "По строкам" => "Match by row",
        "По формату" => "Stock default",
        "Повтор строки выходит за страницу PWG" => {
            "Row repeat exceeds the PWG page"
        }
        "Повторить" => "Redo",
        "Повторяющийся идентификатор блока" => "Duplicate block ID",
        "Поддерживается только chunky PWG Raster" => {
            "Only chunky PWG Raster is supported"
        }
        "Подключение…" => "Connecting…",
        "Подключён" => "Connected",
        "Подчёркнутый" => "Underline",
        "Поиск…" => "Searching…",
        "Показано положение на этикетке до калибровки принтера. Серые поля рулона 25 мм находятся за пределами печатающей головки." => {
            "Position before printer calibration. The grey margins on 25 mm stock are outside the printhead."
        }
        "Положение и размер блока в миллиметрах" => {
            "Block position and dimensions in millimetres"
        }
        "Порог чёрного, 1–255" => "Black threshold, 1–255",
        "После печати" => "After printing",
        "Правка" => "Edit",
        "Предпросмотр" => "Print preview",
        "Предыдущая" => "Previous",
        "Предыдущий образец" => "Previous sample",
        "При подстановке по строкам списки должны иметь одинаковую длину. Одно значение можно повторить для всех строк." => {
            "When matching by row, use lists of equal length. A single value can be reused for every row."
        }
        "При уменьшении этикетки блоки будут сдвинуты и уменьшены до её границ." => {
            "Reducing the label size moves or shrinks blocks to fit its boundaries."
        }
        "Приложение было закрыто. Проверьте уже напечатанные этикетки перед повторной отправкой" => {
            "The app was closed. Check which labels have already printed before resubmitting"
        }
        "Приложение закрывается" => "The application is closing",
        "Применить" => "Apply",
        "Принтер" => "Printer",
        "Принтер Bluetooth…" => "Bluetooth printer…",
        "Принтер вернул нулевой размер рулона" => {
            "The printer returned a zero roll size"
        }
        "Принтер не выбран" => "No printer selected",
        "Принтер не передал вместимость. Ожидание каталога по коду рулона." => {
            "The printer did not report capacity. Waiting for a catalog lookup by roll code."
        }
        "Приём документа" => "Receiving document",
        "Проект с таким набором будет слишком большим для сохранения; уменьшите количество этикеток или размер изображений" => {
            "The project would be too large to save. Reduce the label count or image size"
        }
        "Проект этикеток" => "Label project",
        "Прозрачные" => "Transparent",
        "Прозрачные этикетки" => "Transparent labels",
        "Протокол" => "Protocol",
        "Прошивка не сообщает размер" => {
            "Firmware did not report label size"
        }
        "Прошивка не сообщает размер этикетки. Выберите его в меню «Настройки → Размер этикетки»." => {
            "Firmware did not report label size. Choose it in Settings → Label size."
        }
        "Прямоугольная" => "Rectangular",
        "Прямоугольник" => "Rectangle",
        "Пустой контур этикетки" => "Empty label outline",
        "Радиус углов, мм" => "Corner radius, mm",
        "Развернуть печать на 180°" => "Rotate print by 180°",
        "Размер D101 · ширина рулона × длина" => {
            "D101 size · roll width × feed length"
        }
        "Размер рулона недоступен:" => "Roll size unavailable:",
        "Размер текста: 8–150 точек" => "Text size must be 8–150 dots",
        "Размер шрифта, пт" => "Font size, pt",
        "Размер этикетки" => "Label size",
        "Размер этикетки…" => "Label size…",
        "Редактирование · мм" => "Editing · mm",
        "Режим симуляции" => "Simulation mode",
        "С зазором" => "Gap labels",
        "С чёрной меткой" => "Black mark",
        "Свойства блока" => "Block properties",
        "Сетка" => "Grid",
        "Симуляция" => "Simulation",
        "Симуляция печати" => "Simulate printing",
        "Симуляция: PNG сохранены, принтер не использовался" => {
            "Simulation complete. PNGs saved."
        }
        "Системный принтер Windows…" => "Windows printer…",
        "Скруглённая" => "Rounded",
        "Следующая" => "Next",
        "Следующий образец" => "Next sample",
        "Слишком большой контур этикетки" => {
            "Label outline is too large"
        }
        "Слишком большой ответ каталога" => "Catalog response too large",
        "Слишком большой пакет Niimbot" => "Niimbot packet too large",
        "Слишком глубокая коллекция IPP" => {
            "IPP collection nested too deeply"
        }
        "Слишком длинный текст этикетки" => "Label text is too long",
        "Слишком много атрибутов IPP" => "Too many IPP attributes",
        "Слишком много страниц" => "Too many pages",
        "Слои" => "Layers",
        "Сначала выберите Bluetooth-принтер" => {
            "Select a Bluetooth printer first"
        }
        "Сначала выберите Bluetooth-принтер в приложении" => {
            "Select a Bluetooth printer in the app first"
        }
        "Содержимое" => "Content",
        "Создать набор" => "Create set",
        "Состояние принтера" => "Printer status",
        "Сохранение…" => "Saving…",
        "Сохранить" => "Save",
        "Сохранить…" => "Save…",
        "Списки генератора превышают 1 МБ" => {
            "Generator lists exceed 1 MB"
        }
        "Список превышает 256 КБ" => "List exceeds 256 KB",
        "Список строк" => "List of strings",
        "Справка" => "Help",
        "Страница превышает 25 × 200 мм. Выберите формат этикетки в окне печати Windows" => {
            "Page exceeds 25 × 200 mm. Select label media in the Windows print dialog"
        }
        "Строка списка длиннее 4096 символов" => {
            "A list entry exceeds 4,096 characters"
        }
        "Тайм-аут каталога NIIMBOT" => "NIIMBOT catalog timed out",
        "Тайм-аут отключения" => "Disconnect timed out",
        "Текст" => "Text",
        "Текст не помещается: сократите текст или увеличьте этикетку" => {
            "Text does not fit. Shorten it or enlarge the label"
        }
        "Текст создаваемого набора превышает 8 МБ" => {
            "Generated text exceeds 8 MB"
        }
        "Текст; копии" => "Text and copies",
        "Термоусадочная трубка" => "Heat-shrink tube",
        "Тип этикетки" => "Label type",
        "Толщина линии: 0,1–5 мм" => "Line thickness must be 0.1–5 mm",
        "Толщина, мм" => "Thickness, mm",
        "У принтера нет ожидаемого BLE-канала Niimbot" => {
            "Printer does not expose the expected Niimbot BLE channel"
        }
        "Увеличить" => "Zoom in",
        "Увеличить размер шрифта" => "Increase font size",
        "Удалить блок" => "Delete block",
        "Удалить этикетку" => "Delete label",
        "Уменьшить" => "Zoom out",
        "Уменьшить размер шрифта" => "Decrease font size",
        "Файл" => "File",
        "Файл изображения превышает 16 МБ" => "Image file exceeds 16 MB",
        "Файл проекта превышает 64 МБ" => "Project file exceeds 64 MB",
        "Фигура" => "Shape",
        "Форма этикетки" => "Label shape",
        "Холст этикетки" => "Label canvas",
        "Черновик восстановлен" => "Draft restored",
        "Черновик сохраняется автоматически на этом компьютере." => {
            "The draft is saved automatically on this computer."
        }
        "Черновик сохранён" => "Draft saved",
        "Чёрные" => "Black labels",
        "Шаблон не содержит прозрачного контура" => {
            "Template has no transparent outline"
        }
        "Шаблон рулона больше 2 МБ" => "Roll template exceeds 2 MB",
        "Шаг" => "Step",
        "Шаг 0,5 мм" => "Snap to 0.5 mm",
        "Шаг диапазона не может быть нулевым" => {
            "Range step cannot be zero"
        }
        "Шаг должен вести от начала диапазона к концу" => {
            "The step must lead from the start towards the end"
        }
        "Ширина печати должна быть 1–192 точки" => {
            "Print width must be 1–192 dots"
        }
        "Ширина рулона: до 25 мм" => "Roll width is limited to 25 mm",
        "Шрифт" => "Font",
        "Шрифт и стиль" => "Font and style",
        "Экземпляров" => "Copies",
        "Экспорт PNG…" => "Export PNG…",
        "Этикетка 1" => "Label 1",
        "Этикетка в принтере" => "Loaded label",
        "Этикетка не найдена" => "Label not found",
        "Этикетка-шаблон не найдена" => "Template label not found",
        "Этикетки" => "Labels",
        "Этот каталог данных уже используется другим экземпляром приложения" => {
            "Another application instance is using this data directory"
        }
        "Язык / Language" => "Language / Язык",
        "Язык изменён" => "Language changed",
        "Язык интерфейса" => "Interface language",
        "включено" => "on",
        "выбрано" => "selected",
        "выключено" => "off",
        "не выбрано" => "not selected",
        "неподходящая бумага" => "incompatible media",
        "нет этикеток" => "out of labels",
        "низкий заряд" => "low battery",
        "открыта крышка" => "cover open",
        "отменено на принтере" => "cancelled at the printer",
        "ошибка данных" => "data error",
        "ошибка принтера" => "printer error",
        "перегрев" => "overheating",
        "по верхнему краю" => "align top",
        "по левому краю" => "align left",
        "по нижнему краю" => "align bottom",
        "по правому краю" => "align right",
        "по центру вертикально" => "centre vertically",
        "по центру горизонтально" => "centre horizontally",
        "принтер занят" => "printer busy",
        "↑ На точку вверх" => "↑ Up one dot",
        "↓ На точку вниз" => "↓ Down one dot",
        _ => key,
    }
}

/// Keep interpolation compile-time checked in both languages.
#[macro_export]
macro_rules! local_format {
    ($ru:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::Language::current() == $crate::i18n::Language::En {
            format!($en $(, $arg)*)
        } else {
            format!($ru $(, $arg)*)
        }
    };
}
