//! Real WinUI 3 controls, hosted by Microsoft's Rust Windows Reactor.
use crate::{
    editor_geometry::{CANVAS_PAD_X, CANVAS_PAD_Y, HANDLES, Handle, fit_zoom},
    studio_style as ui,
};
use crate::{
    preview::image_view,
    preview_scene::{Scene, WORKSPACE_COLOR},
};
use anyhow::{Context, Result, ensure};
use niimbot_d101::i18n::Language;
use niimbot_d101::{
    document::{Alignment as LabelAlignment, Axis, Block, Bounds, Content, Document, Label},
    driver::ProtocolMode,
    engine::{Config, Engine},
    label_outline::LabelOutline,
    label_size::D101LabelSize,
    printer_status::{SizeProposal, SizeProposalTracker, StatusPhase},
    transport::{self, Device},
    win_text,
};
use niimbot_d101::{i18n::tr, local_format};
use std::{io::Cursor, path::PathBuf, sync::Arc, time::Duration};
use windows_reactor::*;
#[path = "studio_layout.rs"]
mod layout;

#[derive(Clone)]
pub struct Input {
    engine: Arc<Engine>,
    runtime: tokio::runtime::Handle,
}
impl PartialEq for Input {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.engine, &other.engine)
    }
}
pub fn run(engine: Arc<Engine>, runtime: tokio::runtime::Handle) -> Result<()> {
    if let Err(error) = crate::app_icon::prepare(&engine.directory) {
        eprintln!("Window icon: {error}");
    }
    App::run_component::<Studio>(Input { engine, runtime })
        .map_err(|e| anyhow::anyhow!("WinUI 3: {e}"))
}

#[derive(Clone)]
enum Msg {
    Command(String),
    Language(Language),
    HelpQuery(String),
    HelpTopic(Option<usize>),
    Generated(Option<Box<Document>>),
    Page(Option<usize>),
    Select(Option<usize>),
    Name(String),
    Copies(Option<f64>),
    Enabled(bool),
    Text(String),
    Font(Option<usize>),
    Points(Option<f64>),
    StepPoints(f32),
    Style(u8, bool),
    TextAlign(Axis, LabelAlignment),
    BlockAlign(Axis, LabelAlignment),
    Threshold(Option<f64>),
    Stroke(Option<f64>),
    Filled(bool),
    Press(PointerEventInfo),
    Move(PointerEventInfo),
    Release,
    Zoom(Option<f64>),
    WindowSize(WindowSize),
    Snap(bool),
    Density(Option<f64>),
    LabelSize(Option<usize>),
    Outline(Option<usize>),
    CornerRadius(Option<f64>),
    Address(String),
    Device(Option<usize>),
    Protocol(Option<usize>),
    Media(Option<usize>),
    Rotate(bool),
    OffsetStep(i8),
    DialogClosed(ContentDialogResult),
    Scan,
    Scanned(std::result::Result<Vec<Device>, String>),
    Tick,
    RefreshPrinterStatus,
    Cancel(i32),
}
struct Drag {
    id: u64,
    start: (f64, f64),
    bounds: Bounds,
    resize: Option<Handle>,
    before: Document,
}
struct Studio {
    input: Input,
    doc: Document,
    page: usize,
    selected: Option<u64>,
    fonts: Vec<String>,
    preview: Scene,
    zoom: f64,
    grid: bool,
    window_size: WindowSize,
    fit: bool,
    proof: Option<EncodedImage>,
    proof_page: usize,
    snap: bool,
    drag: Option<Drag>,
    undo: Vec<Document>,
    redo: Vec<Document>,
    dialog: String,
    draft: Config,
    devices: Vec<Device>,
    scanning: bool,
    status: String,
    help_query: String,
    help_topic: usize,
    path: Option<PathBuf>,
    dirty: bool,
    size_proposals: SizeProposalTracker,
    size_proposal: Option<SizeProposal>,
    generator_template: Option<Arc<Document>>,
}

const MAIN_MENUS: &[(&str, &[&str])] = &[
    (
        "Файл",
        &[
            "Новый проект",
            "Открыть…",
            "Сохранить…",
            "Импорт CSV…",
            "Экспорт PNG…",
            "Печатать текущую",
        ],
    ),
    (
        "Правка",
        &[
            "Отменить",
            "Повторить",
            "Дублировать блок",
            "Удалить блок",
            "На передний план",
            "На задний план",
            "Дублировать этикетку",
            "Удалить этикетку",
        ],
    ),
    (
        "Добавить",
        &[
            "Текст",
            "Изображение…",
            "Прямоугольник",
            "Линия",
            "Генератор этикеток…",
        ],
    ),
    ("Вид", &["Предпросмотр", "Сетка", "Вписать", "Масштаб 100%"]),
    (
        "Настройки",
        &[
            "Размер этикетки…",
            "Принтер Bluetooth…",
            "Калибровка печати…",
            "Системный принтер Windows…",
            "Язык / Language",
        ],
    ),
    (
        "Печать",
        &["Печатать набор", "Печатать текущую", "Очередь печати"],
    ),
];

// Reactor's native menu returns the displayed caption, not the MenuItem key.
fn menu_command(caption: &str, language: Language) -> Option<&'static str> {
    MAIN_MENUS
        .iter()
        .flat_map(|(_, keys)| keys.iter().copied())
        .find(|key| language.text(key) == caption)
}

// The editor refreshes for pointer movement and queue progress. Keep native MenuBarItem
// flyouts in a separate component: Reactor 0.100 rebuilds their Items collection on
// every parent publication, which can duplicate items in an open WinUI flyout.
struct MainMenu;
impl Component for MainMenu {
    type Input = (Language, Callback<String>);
    type Message = ();
    fn create(_: &Self::Input, _: &ComponentContext<Self>) -> Self {
        Self
    }
    fn update(&mut self, _: (), _: &ComponentContext<Self>) {}
    fn view(&self, (_, command): &Self::Input, _: &mut ViewContext<Self>) -> View {
        MenuBar::new().collection_slot(
            MenuBarSlot::Items,
            MAIN_MENUS.iter().copied().map(|(title, items)| {
                KeyedView::new(
                    tr(title),
                    MenuBarItem::new().title(tr(title)).menu(Menu::new(
                        items.iter().map(|&s| MenuItem::item(s, tr(s))),
                        command.clone(),
                    )),
                )
            }),
        )
    }
}

impl Studio {
    fn refresh(&mut self) {
        self.page = self.page.min(self.doc.labels.len() - 1);
        if !self.doc.labels[self.page]
            .blocks
            .iter()
            .any(|b| Some(b.id) == self.selected)
        {
            self.selected = None;
        }
        if let Err(e) = self.preview.refresh(&self.doc, self.page) {
            self.status = format!("{e:#}");
        }
    }
    fn remember(&mut self, before: Document) {
        self.undo.push(before);
        self.redo.clear();
        // Keep image-heavy histories bounded as well as limiting the number of edits.
        while self.undo.len() > 40
            || (self.undo.len() > 1
                && self.undo.iter().map(document_bytes).sum::<usize>() > 32 * 1024 * 1024)
        {
            self.undo.remove(0);
        }
        self.dirty = true;
    }
    fn change(&mut self, f: impl FnOnce(&mut Document, usize, Option<u64>)) {
        let before = self.doc.clone();
        f(&mut self.doc, self.page, self.selected);
        if self.doc == before {
            return;
        }
        if let Err(e) = self.doc.validate() {
            self.doc = before;
            self.status = format!("{e:#}");
            return;
        }
        self.remember(before);
        self.refresh();
    }
    fn change_block(&mut self, f: impl FnOnce(&mut Block)) {
        self.change(|d, p, id| {
            if let Some(b) = d.labels[p].blocks.iter_mut().find(|b| Some(b.id) == id) {
                f(b);
            }
        });
    }
    fn block(&self) -> Option<&Block> {
        self.doc.labels[self.page]
            .blocks
            .iter()
            .find(|b| Some(b.id) == self.selected)
    }
    fn save_session(&mut self) {
        if self.dirty {
            match save_document(
                &self.input.engine.directory.join("editor-session.json"),
                &self.doc,
            ) {
                Ok(()) => self.dirty = false,
                Err(e) => {
                    self.status = local_format!(
                        "Не удалось сохранить черновик: {e:#}",
                        "Could not save the draft: {e:#}"
                    )
                }
            }
        }
    }
    fn add(&mut self, content: Content) {
        let id = self.doc.next_id();
        self.change(|d, p, _| {
            let bounds = d.initial_bounds(20.0, 8.0);
            d.labels[p].blocks.push(Block {
                id,
                bounds,
                content,
            });
        });
        self.selected = Some(id);
    }
    fn command(&mut self, command: &str) -> Result<()> {
        match command {
            "Сетка" => self.grid = !self.grid,
            "Вписать" => {
                self.fit = true;
                self.zoom = fit_zoom(self.editor_viewport(), self.doc.spec.dimensions()?);
            }
            "Уменьшить" => {
                self.fit = false;
                self.zoom = (self.zoom - 10.0).max(10.0);
            }
            "Увеличить" => {
                self.fit = false;
                self.zoom = (self.zoom + 10.0).min(400.0);
            }
            "Масштаб 100%" => {
                self.fit = false;
                self.zoom = 100.0;
            }
            "Предпросмотр" => {
                self.proof_page = self
                    .doc
                    .labels
                    .iter()
                    .position(|l| l.enabled)
                    .context(tr("Включите хотя бы одну этикетку в набор"))?;
                self.prepare_proof()?;
                self.dialog = command.into();
            }
            "Предыдущий образец" | "Следующий образец" => {
                let index = if command == "Предыдущий образец" {
                    (0..self.proof_page)
                        .rev()
                        .find(|&i| self.doc.labels[i].enabled)
                } else {
                    (self.proof_page + 1..self.doc.labels.len())
                        .find(|&i| self.doc.labels[i].enabled)
                };
                if let Some(i) = index {
                    self.proof_page = i;
                    self.prepare_proof()?;
                }
            }
            "Фигура" | "Справка" => {
                self.help_query.clear();
                self.help_topic = 0;
                self.dialog = command.into();
            }
            "Язык / Language" => {
                self.draft = self.input.engine.config();
                self.dialog = command.into();
            }
            "Генератор этикеток…" => {
                self.generator_template = Some(Arc::new(self.doc.clone()));
                self.dialog = command.into();
            }
            "Новый проект" => {
                let before = self.doc.clone();
                self.doc = Document::new(self.doc.spec.clone());
                self.page = 0;
                self.path = None;
                self.remember(before);
            }
            "Открыть…" => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter(tr("Этикетки"), &["json"])
                    .pick_file()
                {
                    let doc = Document::load(&path)?;
                    let before = std::mem::replace(&mut self.doc, doc);
                    self.remember(before);
                    self.page = 0;
                    self.path = Some(path);
                }
            }
            "Сохранить…" => {
                let dialog = rfd::FileDialog::new()
                    .add_filter(tr("Проект этикеток"), &["json"])
                    .set_file_name("labels.json");
                if let Some(path) = dialog.save_file() {
                    save_document(&path, &self.doc)?;
                    self.status = local_format!("Сохранено: {}", "Saved: {}", path.display());
                    self.path = Some(path);
                }
            }
            "Импорт CSV…" => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter(tr("Текст; копии"), &["csv"])
                    .pick_file()
                {
                    ensure!(
                        std::fs::metadata(&path)?.len() <= 4 * 1024 * 1024,
                        tr("CSV превышает 4 МБ")
                    );
                    let mut reader = csv::ReaderBuilder::new().from_path(path)?;
                    let mut doc = Document {
                        version: Document::VERSION,
                        generator: Default::default(),
                        spec: self.doc.spec.clone(),
                        labels: vec![],
                    };
                    for record in reader.records() {
                        let record = record?;
                        let i = doc.labels.len();
                        ensure!(i < 1000, tr("Не более 1000 этикеток"));
                        doc.labels.push(Label {
                            name: local_format!("Этикетка {}", "Label {}", i + 1),
                            copies: record
                                .get(1)
                                .unwrap_or("1")
                                .parse()
                                .context(tr("Количество копий в CSV"))?,
                            enabled: true,
                            blocks: vec![],
                        });
                        doc.add_text(i, record.get(0).unwrap_or("").into());
                    }
                    doc.validate()?;
                    let before = std::mem::replace(&mut self.doc, doc);
                    self.remember(before);
                    self.page = 0;
                    self.path = None;
                }
            }
            "Экспорт PNG…" => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("PNG", &["png"])
                    .set_file_name("label.png")
                    .save_file()
                {
                    self.doc.preview(self.page, false)?.save(path)?;
                    self.status = tr("PNG сохранён в разрешении печати 203 dpi").into();
                }
            }
            "Текст" => {
                let id = self.doc.next_id();
                self.change(|d, p, _| {
                    d.add_text(p, tr("Новый текст").into());
                });
                self.selected = Some(id);
            }
            "Изображение…" => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter(tr("Изображения"), &["png", "jpg", "jpeg"])
                    .pick_file()
                {
                    ensure!(
                        std::fs::metadata(&path)?.len() <= 16 * 1024 * 1024,
                        tr("Файл изображения превышает 16 МБ")
                    );
                    let img = niimbot_d101::document::load_image(&std::fs::read(&path)?)?;
                    let mut png = Cursor::new(Vec::new());
                    img.write_to(&mut png, image::ImageFormat::Png)?;
                    self.add(Content::Image {
                        name: path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into(),
                        png: png.into_inner(),
                        threshold: 128,
                    });
                }
            }
            "Прямоугольник" => {
                self.dialog.clear();
                self.add(Content::Rectangle {
                    stroke_mm: 0.25,
                    filled: false,
                });
            }
            "Линия" => {
                self.dialog.clear();
                self.add(Content::Line { stroke_mm: 0.25 });
            }
            "Удалить блок" => {
                self.change(|d, p, id| d.labels[p].blocks.retain(|b| Some(b.id) != id))
            }
            "Дублировать блок" => self.change(|d, p, id| {
                if let Some(mut block) = d.labels[p]
                    .blocks
                    .iter()
                    .find(|b| Some(b.id) == id)
                    .cloned()
                {
                    block.id = d.next_id();
                    block.bounds =
                        block
                            .bounds
                            .moved(1.0, 1.0, d.spec.length_mm, d.spec.width_mm, false);
                    d.labels[p].blocks.push(block);
                }
            }),
            "На передний план" | "На задний план" => {
                self.change(|d, p, id| {
                    if let Some(i) = d.labels[p].blocks.iter().position(|b| Some(b.id) == id) {
                        let b = d.labels[p].blocks.remove(i);
                        if command == "На передний план" {
                            d.labels[p].blocks.push(b);
                        } else {
                            d.labels[p].blocks.insert(0, b);
                        }
                    }
                })
            }
            "Отменить" => {
                if let Some(doc) = self.undo.pop() {
                    self.redo.push(std::mem::replace(&mut self.doc, doc));
                    self.dirty = true;
                }
            }
            "Повторить" => {
                if let Some(doc) = self.redo.pop() {
                    self.undo.push(std::mem::replace(&mut self.doc, doc));
                    self.dirty = true;
                }
            }
            "Добавить этикетку" => {
                self.change(|d, _, _| {
                    d.labels.push(Label {
                        name: local_format!("Этикетка {}", "Label {}", d.labels.len() + 1),
                        copies: 1,
                        enabled: true,
                        blocks: vec![],
                    })
                });
                self.page = self.doc.labels.len() - 1;
                self.selected = None;
            }
            "Дублировать этикетку" => {
                self.change(|d, p, _| {
                    let mut label = d.labels[p].clone();
                    for (id, b) in (d.next_id()..).zip(label.blocks.iter_mut()) {
                        b.id = id;
                    }
                    label.name.push_str(tr(" — копия"));
                    d.labels.push(label);
                });
                self.page = self.doc.labels.len() - 1;
                self.selected = None;
            }
            "Удалить этикетку" => {
                ensure!(
                    self.doc.labels.len() > 1,
                    tr("Оставьте хотя бы одну этикетку")
                );
                self.change(|d, p, _| {
                    d.labels.remove(p);
                });
            }
            "Калибровка печати…" => {
                self.draft = self.input.engine.config();
                ensure!(
                    self.input.engine.simulated || !self.draft.address.is_empty(),
                    tr("Сначала выберите Bluetooth-принтер")
                );
                self.dialog = command.into();
            }
            "Размер этикетки…"
            | "Принтер Bluetooth…"
            | "Системный принтер Windows…"
            | "Очередь печати" => {
                self.draft = self.input.engine.config();
                self.draft.label = self.doc.spec.clone();
                self.dialog = command.into();
            }
            "Печатать набор" | "Печатать текущую" => {
                let mut doc = self.doc.clone();
                if command == "Печатать текущую" {
                    doc.labels = vec![doc.labels[self.page].clone()];
                    doc.labels[0].enabled = true;
                }
                let pages = doc.print_pages()?;
                let total = pages.len();
                let mut config = self.input.engine.config();
                config.label = doc.spec;
                self.input.engine.set_config(config)?;
                let id = self.input.engine.create_job(
                    tr("Набор из редактора").into(),
                    1,
                    Some(pages),
                )?;
                self.status = local_format!(
                    "Задание №{id}: {total} этикеток в очереди",
                    "Job #{id}: {total} labels queued"
                );
                self.dialog = "Очередь печати".into();
            }
            _ => {}
        }
        self.refresh();
        if self.fit {
            self.zoom = fit_zoom(self.editor_viewport(), self.doc.spec.dimensions()?);
        }
        Ok(())
    }
    fn prepare_proof(&mut self) -> Result<()> {
        let mut image = self.doc.preview(self.proof_page, true)?;
        if self.doc.spec.rotate_180 {
            image = image::imageops::rotate180(&image);
        }
        let mut png = Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png)?;
        self.proof = Some(EncodedImage::new(png.into_inner()));
        Ok(())
    }
    fn schedule_tick(ctx: &ComponentContext<Self>) {
        ctx.spawn_background(|_| {
            std::thread::sleep(Duration::from_millis(750));
            Msg::Tick
        });
    }
}

impl Drop for Studio {
    fn drop(&mut self) {
        self.save_session();
    }
}
impl Component for Studio {
    type Input = Input;
    type Message = Msg;
    fn create(input: &Input, ctx: &ComponentContext<Self>) -> Self {
        let draft = input.engine.config();
        let session = input.engine.directory.join("editor-session.json");
        let (doc, status) = if session.exists() {
            match Document::load(&session) {
                Ok(doc) => (doc, tr("Черновик восстановлен").into()),
                Err(e) => (
                    Document::new(draft.label.clone()),
                    local_format!(
                        "Не удалось восстановить черновик: {e:#}",
                        "Could not restore the draft: {e:#}"
                    ),
                ),
            }
        } else {
            (
                Document::new(draft.label.clone()),
                tr("Добавьте блок и перетащите его на этикетке").into(),
            )
        };
        let mut state = Self {
            input: input.clone(),
            doc,
            page: 0,
            selected: Some(1),
            fonts: win_text::font_families(),
            preview: Scene::default(),
            zoom: 160.0,
            grid: false,
            window_size: WindowSize {
                width: 1440.0,
                height: 920.0,
            },
            fit: false,
            proof: None,
            proof_page: 0,
            snap: true,
            drag: None,
            undo: vec![],
            redo: vec![],
            dialog: String::new(),
            draft,
            devices: vec![],
            scanning: false,
            status,
            help_query: String::new(),
            help_topic: 0,
            path: None,
            dirty: false,
            size_proposals: SizeProposalTracker::default(),
            size_proposal: None,
            generator_template: None,
        };
        state.refresh();
        Self::schedule_tick(ctx);
        state
    }
    fn update(&mut self, msg: Msg, ctx: &ComponentContext<Self>) {
        match msg {
            Msg::Generated(result) => {
                if let Some(doc) = result {
                    let first = self.doc.labels.len();
                    let count = doc.labels.len() - first;
                    let before = std::mem::replace(&mut self.doc, *doc);
                    self.remember(before);
                    self.page = first;
                    self.selected = None;
                    self.refresh();
                    self.status = local_format!(
                        "Создано {count} этикеток. Шаблон исключён из печати.",
                        "Created {count} labels. The template was excluded from printing."
                    );
                }
                self.dialog.clear();
                self.generator_template = None;
            }
            Msg::Command(c) => {
                if let Err(e) = self.command(&c) {
                    self.status = format!("{e:#}");
                }
            }
            Msg::Page(Some(p)) if p < self.doc.labels.len() && p != self.page => {
                self.page = p;
                self.selected = self.doc.labels[p].blocks.first().map(|b| b.id);
                self.refresh();
            }
            Msg::Select(Some(i)) => {
                // Replacing a ComboBox ItemsSource raises a transient empty selection.
                // Only the canvas explicitly clears the selected block.
                if let Some(block) = self.doc.labels[self.page].blocks.get(i) {
                    self.selected = Some(block.id);
                }
            }
            Msg::Name(name) => {
                self.change(|d, p, _| d.labels[p].name = name.chars().take(100).collect())
            }
            Msg::Copies(Some(n)) if n.is_finite() => {
                self.change(|d, p, _| d.labels[p].copies = n.round().clamp(1.0, 1000.0) as usize)
            }
            Msg::Enabled(v) => self.change(|d, p, _| d.labels[p].enabled = v),
            Msg::Text(value) => self.change_block(|b| {
                if let Content::Text { text, .. } = &mut b.content {
                    *text = value;
                }
            }),
            Msg::Font(Some(i)) if i < self.fonts.len() => {
                let family = self.fonts[i].clone();
                self.change_block(|b| {
                    if let Content::Text { style, .. } = &mut b.content {
                        style.family = family;
                    }
                });
            }
            Msg::Points(Some(n)) if n.is_finite() => self.change_block(|b| {
                if let Content::Text { style, .. } = &mut b.content {
                    style.points = n.clamp(1.0, 144.0) as f32;
                }
            }),
            Msg::StepPoints(step) => self.change_block(|b| {
                if let Content::Text { style, .. } = &mut b.content {
                    style.points = (style.points + step).clamp(1.0, 144.0);
                }
            }),
            Msg::Style(i, v) => self.change_block(|b| {
                if let Content::Text { style, .. } = &mut b.content {
                    match i {
                        0 => style.bold = v,
                        1 => style.italic = v,
                        2 => style.underline = v,
                        _ => style.strikeout = v,
                    }
                }
            }),
            Msg::TextAlign(axis, alignment) => self.change_block(|b| {
                if let Content::Text { style, .. } = &mut b.content {
                    match axis {
                        Axis::Horizontal => style.alignment = alignment as u8,
                        Axis::Vertical => style.vertical_alignment = alignment as u8,
                    }
                }
            }),
            Msg::BlockAlign(axis, alignment) => {
                let (w, h) = (self.doc.spec.length_mm, self.doc.spec.width_mm);
                self.change_block(|b| b.bounds.align_to(axis, alignment, w, h));
            }
            Msg::Threshold(Some(n)) if n.is_finite() => self.change_block(|b| {
                if let Content::Image { threshold, .. } = &mut b.content {
                    *threshold = n.clamp(1.0, 255.0) as u8;
                }
            }),
            Msg::Stroke(Some(n)) if n.is_finite() => self.change_block(|b| match &mut b.content {
                Content::Rectangle { stroke_mm, .. } | Content::Line { stroke_mm } => {
                    *stroke_mm = n.clamp(0.1, 5.0) as f32
                }
                _ => {}
            }),
            Msg::Filled(v) => self.change_block(|b| {
                if let Content::Rectangle { filled, .. } = &mut b.content {
                    *filled = v;
                }
            }),
            Msg::Press(e) if e.is_left_button_pressed && e.capture_succeeded => {
                let s = self.scale();
                let local = (e.x - CANVAS_PAD_X, e.y - CANVAS_PAD_Y);
                let (x, y) = ((local.0 / s) as f32, (local.1 / s) as f32);
                let resize = self.block().and_then(|b| {
                    HANDLES.into_iter().find(|handle| {
                        let (hx, hy) = handle.point(b.bounds);
                        (hx as f64 * s - local.0).abs() < 7.0
                            && (hy as f64 * s - local.1).abs() < 7.0
                    })
                });
                if resize.is_none() {
                    self.selected = self.doc.hit_test(self.page, x, y);
                }
                if let Some(b) = self.block() {
                    self.drag = Some(Drag {
                        id: b.id,
                        start: (e.window_x, e.window_y),
                        bounds: b.bounds,
                        resize,
                        before: self.doc.clone(),
                    });
                }
            }
            Msg::Move(e) => {
                if !e.is_left_button_pressed {
                    self.finish_drag();
                    return;
                }
                if let Some(drag) = &self.drag {
                    let s = self.scale();
                    let (dx, dy) = (
                        ((e.window_x - drag.start.0) / s) as f32,
                        ((e.window_y - drag.start.1) / s) as f32,
                    );
                    let mut bounds = drag.bounds;
                    if let Some(handle) = drag.resize {
                        bounds = handle.resize(
                            bounds,
                            dx,
                            dy,
                            self.doc.spec.length_mm,
                            self.doc.spec.width_mm,
                            self.snap,
                        );
                    } else {
                        bounds = bounds.moved(
                            dx,
                            dy,
                            self.doc.spec.length_mm,
                            self.doc.spec.width_mm,
                            self.snap,
                        );
                    }
                    if let Some(b) = self.doc.labels[self.page]
                        .blocks
                        .iter_mut()
                        .find(|b| b.id == drag.id)
                        && b.bounds != bounds
                    {
                        b.bounds = bounds;
                        if drag.resize.is_some() {
                            self.refresh();
                        }
                    }
                }
            }
            Msg::Release => self.finish_drag(),
            Msg::Zoom(Some(n)) if n.is_finite() => {
                self.zoom = n.clamp(10.0, 400.0);
                self.fit = false;
            }
            Msg::WindowSize(size) => {
                self.window_size = size;
                if self.fit {
                    self.zoom = fit_zoom(
                        self.editor_viewport(),
                        self.doc.spec.dimensions().unwrap_or((1, 1)),
                    );
                }
            }
            Msg::Snap(v) => self.snap = v,
            Msg::Density(Some(n)) if n.is_finite() => {
                self.draft.print.density = n.clamp(1.0, 3.0) as u8
            }
            Msg::LabelSize(Some(i)) => {
                if let Some(size) = D101LabelSize::ALL.get(i) {
                    size.apply(&mut self.draft.label);
                }
            }
            Msg::Outline(Some(i)) => {
                self.draft.label.outline = match i {
                    0 => LabelOutline::Auto,
                    1 => LabelOutline::Rectangle,
                    2 => LabelOutline::Rounded { radius_tenths: 20 },
                    3 => LabelOutline::Ellipse,
                    4 if D101LabelSize::from_spec(&self.draft.label)
                        == Some(D101LabelSize::W12_5L109) =>
                    {
                        LabelOutline::CableFlag
                    }
                    _ => self.draft.label.outline.clone(),
                };
            }
            Msg::CornerRadius(Some(r)) if r.is_finite() => {
                self.draft.label.outline = LabelOutline::Rounded {
                    radius_tenths: (r.clamp(0.0, 12.5) * 10.0).round() as u16,
                };
            }
            Msg::Address(value) => self.draft.address = value.trim().into(),
            Msg::Device(Some(i)) if i < self.devices.len() => {
                self.draft.address = self.devices[i].address.clone();
                self.draft.device_name = self.devices[i].name.clone();
            }
            Msg::Protocol(Some(i)) => {
                self.draft.print.protocol = match i {
                    1 => ProtocolMode::B1,
                    2 => ProtocolMode::V4,
                    _ => ProtocolMode::Auto,
                }
            }
            Msg::Media(Some(i)) => self.draft.print.label_type = if i == 0 { 1 } else { 5 },
            Msg::Rotate(v) => self.draft.label.rotate_180 = v,
            Msg::OffsetStep(step) => self
                .draft
                .set_vertical_offset(self.draft.vertical_offset().saturating_add(step)),
            Msg::Language(language) if self.dialog == "Язык / Language" => {
                match self.input.engine.set_language(language) {
                    Ok(()) => {
                        self.status = tr("Язык изменён").into();
                        self.dialog.clear();
                    }
                    Err(e) => self.status = format!("{e:#}"),
                }
            }
            Msg::HelpQuery(query) if self.help_query != query => {
                self.help_query = query;
                self.help_topic = 0;
            }
            Msg::HelpTopic(Some(index)) => self.help_topic = index,
            Msg::DialogClosed(result) => {
                if result == ContentDialogResult::Primary && self.dialog == "Калибровка печати…"
                {
                    let mut config = self.input.engine.config();
                    config.set_vertical_offset(self.draft.vertical_offset());
                    self.status = match self.input.engine.set_config(config) {
                        Ok(()) => tr("Калибровка печати сохранена").into(),
                        Err(e) => local_format!(
                            "Поправка не сохранена: {e:#}",
                            "Could not save the offset: {e:#}"
                        ),
                    };
                }
                if result == ContentDialogResult::Primary
                    && self.dialog == "Этикетка в принтере"
                    && let Some(proposal) = self.size_proposal.take()
                {
                    if let Some(proposal) = proposal.refreshed(&self.input.engine.printer_status())
                    {
                        let mut config = self.input.engine.config();
                        config.label = self.doc.spec.clone();
                        proposal.size.apply(&mut config.label);
                        config.label.outline = proposal.outline;
                        match self.input.engine.set_config(config.clone()) {
                            Ok(()) => {
                                self.change(|d, _, _| {
                                    d.resize_label(config.label.length_mm, config.label.width_mm);
                                    d.spec.outline = config.label.outline.clone();
                                });
                                self.status = local_format!(
                                    "Применён размер рулона: {}",
                                    "Applied roll size: {}",
                                    proposal.size.title()
                                );
                            }
                            Err(e) => {
                                self.status = local_format!(
                                    "Размер не изменён: {e:#}",
                                    "Could not change the size: {e:#}"
                                )
                            }
                        }
                    } else {
                        self.status = tr("Данные рулона изменились или связь потеряна. Обновите статус принтера.").into();
                    }
                }
                if result == ContentDialogResult::Primary
                    && (self.dialog == "Размер этикетки…" || self.dialog == "Принтер Bluetooth…")
                {
                    match self.input.engine.set_config(self.draft.clone()) {
                        Ok(()) => {
                            let spec = self.draft.label.clone();
                            self.change(|d, _, _| {
                                d.resize_label(spec.length_mm, spec.width_mm);
                                d.spec.rotate_180 = spec.rotate_180;
                                d.spec.outline = spec.outline;
                            });
                            self.status = tr("Настройки сохранены").into();
                        }
                        Err(e) => {
                            self.status = local_format!(
                                "Настройки не сохранены: {e:#}",
                                "Could not save settings: {e:#}"
                            )
                        }
                    }
                }
                self.dialog.clear();
                self.size_proposal = None;
            }
            Msg::Scan if !self.scanning => {
                self.scanning = true;
                let runtime = self.input.runtime.clone();
                ctx.spawn_background(move |_| {
                    Msg::Scanned(
                        runtime
                            .block_on(transport::scan())
                            .map_err(|e| format!("{e:#}")),
                    )
                });
            }
            Msg::Scanned(result) => {
                self.scanning = false;
                match result {
                    Ok(devices) => {
                        self.status = local_format!(
                            "Найдено устройств: {}",
                            "Devices found: {}",
                            devices.len()
                        );
                        self.devices = devices;
                    }
                    Err(e) => self.status = e,
                }
            }
            Msg::Tick => {
                if self.drag.is_none() {
                    self.save_session();
                    if self.dialog.is_empty() {
                        self.size_proposal = self
                            .size_proposals
                            .offer(&self.input.engine.printer_status(), &self.doc.spec);
                        if self.size_proposal.is_some() {
                            self.dialog = "Этикетка в принтере".into();
                        }
                    }
                }
                Self::schedule_tick(ctx);
            }
            Msg::RefreshPrinterStatus => self.input.engine.request_status_refresh(),
            Msg::Cancel(id) => {
                if let Err(e) = self.input.engine.cancel(id) {
                    self.status = format!("{e:#}");
                }
            }
            _ => {}
        }
    }
    fn view(&self, _: &Input, ctx: &mut ViewContext<Self>) -> View {
        self.layout(ctx)
    }
}

impl Studio {
    fn scale(&self) -> f64 {
        self.zoom / 100.0 * niimbot_d101::bitmap::DPI as f64 / 25.4
    }
    fn finish_drag(&mut self) {
        if let Some(d) = self.drag.take()
            && d.before != self.doc
        {
            self.remember(d.before);
            // Commit one undo step for the entire gesture; unchanged textures stay cached.
            self.refresh();
        }
    }
    fn inspector(&self, ctx: &ViewContext<Self>) -> View {
        let Some(b) = self.block() else {
            return StackPanel::new().spacing(12.0).children((
                heading(tr("Свойства блока")),
                TextBlock::new()
                    .text(tr(
                        "Выберите блок на холсте или добавьте новый через верхнее меню.",
                    ))
                    .text_wrapping(TextWrapping::Wrap),
            ));
        };
        let kind = match b.content {
            Content::Text { .. } => "Текст",
            Content::Image { .. } => tr("Изображение"),
            _ => "Фигура",
        };
        let mut items = vec![
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto])
                .margin(Thickness::new(0.0, 0.0, 0.0, 8.0))
                .children((
                    heading(tr("Свойства блока")),
                    Border::new()
                        .grid_column(1)
                        .background(ui::SOFT)
                        .corner_radius(5.0)
                        .padding(Thickness::xy(7.0, 3.0))
                        .content(ui::label(tr(kind), 10.5, ui::MUTED)),
                )),
        ];
        match &b.content {
            Content::Text { text, style } => {
                items.push(ui::section(tr("Содержимое"), ui::text_icon()));
                items.push(field(
                    "Текст",
                    TextBox::new()
                        .text(text)
                        .accepts_return(true)
                        .text_wrapping(TextWrapping::Wrap)
                        .background(ui::WHITE)
                        .border_brush(ui::LINE)
                        .min_height(76.0)
                        .max_height(150.0)
                        .on_text_changed(ctx.callback(Msg::Text)),
                ));
                items.push(
                    button(ctx, "Генератор этикеток…", "Генератор этикеток…").tooltip(tr(
                        "Добавьте в текст {номер} или {название} и задайте значения в генераторе",
                    )),
                );
                items.push(ui::divider());
                items.push(ui::section(tr("Шрифт и стиль"), ui::text_icon()));
                items.push(field(
                    tr("Шрифт"),
                    ComboBox::new()
                        .horizontal_alignment(HorizontalAlignment::Stretch)
                        .items_source(self.fonts.clone())
                        .selected_index(self.fonts.iter().position(|f| f == &style.family))
                        .on_selection_changed(ctx.callback(Msg::Font)),
                ));
                items.push(field(
                    tr("Размер шрифта, пт"),
                    font_size_input(ctx, style.points),
                ));
                items.push(field(
                    tr("Начертание"),
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .spacing(4.0)
                        .children(
                            [
                                (0, tr("Жирный"), style.bold, ui::ContourIcon::Bold.view()),
                                (
                                    1,
                                    tr("Курсив"),
                                    style.italic,
                                    ui::ContourIcon::Italic.view(),
                                ),
                                (
                                    2,
                                    tr("Подчёркнутый"),
                                    style.underline,
                                    ui::ContourIcon::Underline.view(),
                                ),
                                (
                                    3,
                                    tr("Зачёркнутый"),
                                    style.strikeout,
                                    ui::ContourIcon::Strike.view(),
                                ),
                            ]
                            .map(
                                |(i, label, active, icon): (u8, &str, bool, View)| {
                                    icon_button(active)
                                        .automation_name(format!(
                                            "{label}: {}",
                                            if active {
                                                tr("включено")
                                            } else {
                                                tr("выключено")
                                            }
                                        ))
                                        .on_click(ctx.message(Msg::Style(i, !active)))
                                        .glyph(icon, ui::TOOL_GLYPH, ui::TOOL_GLYPH)
                                        .tooltip(label)
                                },
                            ),
                        ),
                ));
                items.push(ui::divider());
                items.push(ui::section(
                    tr("Выравнивание текста"),
                    ui::icon(Symbol::AlignCenter),
                ));
                items.push(alignment_toolbar(ctx, true, |axis, alignment| {
                    let value = match axis {
                        Axis::Horizontal => style.alignment,
                        Axis::Vertical => style.vertical_alignment,
                    };
                    value == alignment as u8
                }));
            }
            Content::Image {
                name, threshold, ..
            } => {
                items.push(
                    TextBlock::new()
                        .text(name)
                        .text_wrapping(TextWrapping::Wrap)
                        .into(),
                );
                items.push(field(
                    tr("Порог чёрного, 1–255"),
                    NumberBox::new()
                        .minimum(1.0)
                        .maximum(255.0)
                        .value(*threshold as f64)
                        .on_value_changed(ctx.callback(Msg::Threshold)),
                ));
            }
            Content::Rectangle { stroke_mm, filled } => {
                items.push(field(
                    tr("Толщина, мм"),
                    NumberBox::new()
                        .minimum(0.1)
                        .maximum(5.0)
                        .value(*stroke_mm as f64)
                        .on_value_changed(ctx.callback(Msg::Stroke)),
                ));
                items.push(
                    CheckBox::new()
                        .is_checked(*filled)
                        .on_is_checked_changed(ctx.callback(Msg::Filled))
                        .content(tr("Заливка")),
                );
            }
            Content::Line { stroke_mm } => items.push(field(
                tr("Толщина, мм"),
                NumberBox::new()
                    .minimum(0.1)
                    .maximum(5.0)
                    .value(*stroke_mm as f64)
                    .on_value_changed(ctx.callback(Msg::Stroke)),
            )),
        }
        items.push(ui::divider());
        items.push(ui::section(
            tr("Блок относительно этикетки"),
            ui::fit_icon(),
        ));
        items.push(alignment_toolbar(ctx, false, |axis, alignment| {
            let mut aligned = b.bounds;
            aligned.align_to(
                axis,
                alignment,
                self.doc.spec.length_mm,
                self.doc.spec.width_mm,
            );
            (aligned.x - b.bounds.x).abs() < 0.001 && (aligned.y - b.bounds.y).abs() < 0.001
        }));
        StackPanel::new().spacing(12.0).keyed_children(keyed(items))
    }
    fn help_view(&self, ctx: &ViewContext<Self>) -> View {
        let topics =
            niimbot_d101::help::search(niimbot_d101::i18n::Language::current(), &self.help_query);
        let selected = self.help_topic.min(topics.len().saturating_sub(1));
        let results = if let Some(topic) = topics.get(selected) {
            StackPanel::new().spacing(12.0).children((
                ComboBox::new()
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .automation_name(tr("Разделы справки"))
                    .items_source(topics.iter().map(|topic| topic.title))
                    .selected_index(selected)
                    .on_selection_changed(ctx.callback(Msg::HelpTopic)),
                ScrollViewer::new().max_height(330.0).content(
                    TextBlock::new()
                        .text(topic.body)
                        .text_wrapping(TextWrapping::Wrap)
                        .is_text_selection_enabled(true),
                ),
            ))
        } else {
            TextBlock::new()
                .text(tr(
                    "Ничего не найдено. Измените запрос или очистите поле поиска.",
                ))
                .text_wrapping(TextWrapping::Wrap)
                .into()
        };
        StackPanel::new().width(480.0).spacing(12.0).children((
            TextBox::new()
                .text(&self.help_query)
                .automation_name(tr("Поиск по справке"))
                .placeholder_text(tr("Поиск по справке"))
                .on_text_changed(ctx.callback(Msg::HelpQuery)),
            ui::label(
                local_format!("Разделов: {}", "Topics: {}", topics.len()),
                12.0,
                ui::MUTED,
            ),
            results,
        ))
    }
    fn dialog_view(&self, ctx: &ViewContext<Self>) -> View {
        if self.dialog == "Генератор этикеток…"
            && let Some(doc) = &self.generator_template
        {
            return View::component::<crate::generator_ui::GeneratorDialog>(
                crate::generator_ui::Input {
                    doc: doc.clone(),
                    page: self.page,
                    complete: ctx.callback(Msg::Generated),
                },
            );
        }
        if self.dialog.is_empty() {
            return View::empty();
        }
        let content=match self.dialog.as_str(){
            "Справка" => self.help_view(ctx),
            "Язык / Language" => field(tr("Язык интерфейса"), StackPanel::new()
                .orientation(Orientation::Horizontal).spacing(8.0)
                .children([(Language::Ru, "Русский"), (Language::En, "English")].map(|(language, name)| {
                    ui::styled_button(Language::current() == language, false, false)
                        .automation_name(name)
                        .on_click(ctx.message(Msg::Language(language))).content(name)
                }))),
            "Фигура"=>StackPanel::new().width(320.0).spacing(12.0).children((
                button(ctx,"Прямоугольник","Прямоугольник"),
                button(ctx,"Линия","Линия"),
            )),
            "Предпросмотр"=>{
                let page=&self.doc.labels[self.proof_page];
                let (w,h)=self.doc.spec.dimensions().unwrap_or((1,1));
                let scale=(480.0/w as f64).min(260.0/h as f64).min(3.0);
                StackPanel::new().width(520.0).spacing(18.0).children((
                    ui::label(local_format!("{} · {} экземпляров", "{} · Copies: {}",page.name,page.copies),14.0,ui::INK).font_weight(FontWeight::SEMI_BOLD),
                    Border::new().background(Color::rgb(WORKSPACE_COLOR[0],WORKSPACE_COLOR[1],WORKSPACE_COLOR[2])).padding(20.0)
                        .content(self.proof.as_ref().map(|p| image_view(p,w as f64*scale,h as f64*scale,scale)).unwrap_or_else(View::empty)),
                    ui::label(local_format!("{} × {} мм · 203 dpi · этикетка {} из {}", "{} × {} mm · 203 dpi · label {} of {}",self.doc.spec.length_mm,self.doc.spec.width_mm,
                        self.doc.labels[..=self.proof_page].iter().filter(|l|l.enabled).count(),self.doc.labels.iter().filter(|l|l.enabled).count()),12.0,ui::MUTED),
                    StackPanel::new().orientation(Orientation::Horizontal).spacing(10.0).children((
                        ui::styled_button(false,false,false).is_enabled(self.doc.labels[..self.proof_page].iter().any(|l|l.enabled))
                            .on_click(ctx.message(Msg::Command("Предыдущий образец".into()))).content(tr("Предыдущая")),
                        ui::styled_button(false,false,false).is_enabled(self.doc.labels[self.proof_page+1..].iter().any(|l|l.enabled))
                            .on_click(ctx.message(Msg::Command("Следующий образец".into()))).content(tr("Следующая")),
                    )),
                    ui::label(tr("Показано положение на этикетке до калибровки принтера. Серые поля рулона 25 мм находятся за пределами печатающей головки."),12.0,ui::MUTED).text_wrapping(TextWrapping::Wrap),
                ))
            },
            "Размер этикетки…"=>StackPanel::new().width(400.0).spacing(14.0).children((
                field(tr("Размер D101 · ширина рулона × длина"),ComboBox::new().width(400.0).placeholder_text(tr("Выберите поддерживаемый размер")).items_source(D101LabelSize::ALL.iter().map(|s|s.title())).selected_index(D101LabelSize::ALL.iter().position(|s|Some(*s)==D101LabelSize::from_spec(&self.draft.label))).on_selection_changed(ctx.callback(Msg::LabelSize))),
                field(tr("Форма этикетки"),ComboBox::new().width(400.0).items_source({
                    let mut forms=vec![tr("По формату"),tr("Прямоугольная"),tr("Скруглённая"),tr("Круглая / овальная")];
                    if D101LabelSize::from_spec(&self.draft.label)==Some(D101LabelSize::W12_5L109) {forms.push(tr("Кабельная 74 + 35 мм"));}
                    if matches!(self.draft.label.outline,LabelOutline::Catalog{..}) {forms.push(tr("Контур рулона NIIMBOT"));}
                    forms
                }).selected_index(match self.draft.label.outline {LabelOutline::Auto=>0,LabelOutline::Rectangle=>1,LabelOutline::Rounded{..}=>2,LabelOutline::Ellipse=>3,LabelOutline::CableFlag=>4,LabelOutline::Catalog{..}=>if D101LabelSize::from_spec(&self.draft.label)==Some(D101LabelSize::W12_5L109) {5} else {4}}).on_selection_changed(ctx.callback(Msg::Outline))),
                if let LabelOutline::Rounded{radius_tenths} = self.draft.label.outline {
                    field(tr("Радиус углов, мм"),NumberBox::new().minimum(0.0).maximum((self.draft.label.width_mm/2.0) as f64).value(radius_tenths as f64/10.0).on_value_changed(ctx.callback(Msg::CornerRadius)))
                } else {View::empty()},
                CheckBox::new().is_checked(self.draft.label.rotate_180).on_is_checked_changed(ctx.callback(Msg::Rotate)).content(tr("Развернуть печать на 180°")),
                TextBlock::new().text(tr("При уменьшении этикетки блоки будут сдвинуты и уменьшены до её границ.")).text_wrapping(TextWrapping::Wrap),
                TextBlock::new().text(tr("Области вне контура не печатаются. Серые края рулона 25 мм — вне печатающей головки. «По формату» использует типовой контур; точный контур загружается по коду рулона при подключении.")).text_wrapping(TextWrapping::Wrap),
            )),
            "Этикетка в принтере" => {
                let size = self.size_proposal.as_ref().map(|p|p.size.title()).unwrap_or_default();
                StackPanel::new().width(420.0).spacing(14.0).children((
                    TextBlock::new().text(local_format!("Размер установленного рулона: {size} (ширина × длина). Размер получен из принтера или каталога NIIMBOT по прочитанному коду рулона.", "Loaded roll: {size} (width × length). The printer or NIIMBOT catalog supplied this size using the roll code." )).text_wrapping(TextWrapping::Wrap),
                    TextBlock::new().text(local_format!("Текущий холст: {} × {} мм. Применить размер и форму рулона ко всем этикеткам проекта?", "Current canvas: {} × {} mm. Apply the roll size and shape to every label in this project?",self.doc.spec.width_mm,self.doc.spec.length_mm)).text_wrapping(TextWrapping::Wrap),
                    TextBlock::new().text(tr("Блоки за границами нового холста будут сдвинуты или уменьшены. Изменение можно отменить.")).text_wrapping(TextWrapping::Wrap),
                ))
            },
            "Принтер Bluetooth…"=>StackPanel::new().width(420.0).spacing(12.0).children((
                ui::styled_button(false,false,false).is_enabled(!self.scanning).on_click(ctx.message(Msg::Scan)).content(if self.scanning{tr("Поиск…")}else{tr("Найти Niimbot D101")}),
                ComboBox::new().width(420.0).placeholder_text(tr("Найденные устройства")).items_source(self.devices.iter().map(|d|format!("{} · {}",d.name,d.address))).selected_index(self.devices.iter().position(|d|d.address==self.draft.address)).on_selection_changed(ctx.callback(Msg::Device)),
                field(tr("Bluetooth-адрес"),TextBox::new().text(&self.draft.address).on_text_changed(ctx.callback(Msg::Address))),
                field(tr("Плотность"),NumberBox::new().minimum(1.0).maximum(3.0).value(self.draft.print.density as f64).on_value_changed(ctx.callback(Msg::Density))),
                field(tr("Лента"),ComboBox::new().items_source([tr("Обычные этикетки"),tr("Прозрачные этикетки")]).selected_index(if self.draft.print.label_type==1{0}else{1}).on_selection_changed(ctx.callback(Msg::Media))),
                field(tr("Протокол"),ComboBox::new().items_source([tr("Автоматически"),"B1","V4"]).selected_index(match self.draft.print.protocol{ProtocolMode::Auto=>0,ProtocolMode::B1=>1,ProtocolMode::V4=>2}).on_selection_changed(ctx.callback(Msg::Protocol))),
                TextBlock::new().text(&self.status).text_wrapping(TextWrapping::Wrap),
            )),
            "Калибровка печати…" => {
                let dots = self.draft.vertical_offset();
                let limit = niimbot_d101::engine::MAX_OFFSET_DOTS;
                let mm = format!("{:+.2}", f32::from(dots) * 25.4 / niimbot_d101::bitmap::DPI).replace('.', ",");
                StackPanel::new().width(390.0).spacing(14.0).children((
                    heading(tr("Вертикальная поправка в точках")),
                    ui::label(local_format!("{dots:+}  ({mm} мм)", "{dots:+}  ({mm} mm)"), 20.0, ui::INK),
                    StackPanel::new().orientation(Orientation::Horizontal).spacing(8.0).children((
                        ui::styled_button(false,false,false).is_enabled(dots > -limit)
                            .on_click(ctx.message(Msg::OffsetStep(-1))).content(tr("↑ На точку вверх")),
                        ui::styled_button(false,false,false).is_enabled(dots < limit)
                            .on_click(ctx.message(Msg::OffsetStep(1))).content(tr("↓ На точку вниз")),
                    )),
                    ui::label(tr("Если отпечаток выше макета, сдвиньте его вниз. Направление указано для этикетки с выключенным поворотом на 180°. Значение 0 отключает поправку."), 13.0, ui::MUTED).text_wrapping(TextWrapping::Wrap),
                ))
            },
            "Системный принтер Windows…"=>StackPanel::new().width(460.0).spacing(12.0).children((
                heading("Microsoft IPP Class Driver"),TextBlock::new().text(local_format!("Адрес принтера: http://127.0.0.1:{}/ipp/print", "Printer address: http://127.0.0.1:{}/ipp/print",self.input.engine.port)).is_text_selection_enabled(true).text_wrapping(TextWrapping::Wrap),
                TextBlock::new().text(tr("Запустите scripts/Install-Printer.ps1 от имени администратора. После установки D101 появится в списке принтеров Windows. Приложение должно оставаться запущенным во время печати.")).text_wrapping(TextWrapping::Wrap),
                TextBlock::new().text(tr("Перед печатью задайте размер этикетки и выберите Bluetooth-принтер в меню «Настройки».")).text_wrapping(TextWrapping::Wrap),TextBlock::new().text(self.input.engine.server_status()),
            )),
            "Очередь печати"=>{
                let jobs=self.input.engine.jobs();let rows=jobs.iter().take(20).map(|j|StackPanel::new().spacing(5.0).children((
                    TextBlock::new().text(format!("№{}  ·  {}  ·  {} / {}",j.id,j.state_text(),j.completed,j.total)).font_weight(FontWeight::SEMI_BOLD),TextBlock::new().text(&j.message).text_wrapping(TextWrapping::Wrap),
                    ui::styled_button(false,false,false).is_enabled(j.state<7).on_click(ctx.message(Msg::Cancel(j.id))).content(tr("Отменить задание")),
                ))).collect::<Vec<View>>();
                ScrollViewer::new().width(480.0).max_height(480.0).content(StackPanel::new().spacing(18.0).keyed_children(keyed(if rows.is_empty(){vec![TextBlock::new().text(tr("Очередь пуста")).into()]}else{rows})))
            },
            _=>View::empty(),
        };
        ContentDialog::new()
            .title(tr(&self.dialog).trim_end_matches('…'))
            .is_open(!self.dialog.is_empty())
            .primary_button_text(
                if matches!(
                    self.dialog.as_str(),
                    "Размер этикетки…" | "Принтер Bluetooth…" | "Калибровка печати…"
                ) {
                    tr("Сохранить")
                } else if self.dialog == "Этикетка в принтере" {
                    tr("Применить")
                } else {
                    ""
                },
            )
            .close_button_text(if self.dialog == "Этикетка в принтере" {
                tr("Оставить текущий")
            } else {
                tr("Закрыть")
            })
            .on_closed(ctx.callback(Msg::DialogClosed))
            .content(content)
    }
}
fn icon_button(active: bool) -> ui::StudioButton {
    ui::styled_button(active, false, false).square(ui::FORMAT_SIDE)
}

fn font_size_input(ctx: &ViewContext<Studio>, points: f32) -> View {
    let arrow = |path: &str, name: &str, step, enabled| {
        ui::styled_button(false, false, false)
            .stepper()
            .is_enabled(enabled)
            .automation_name(name)
            .on_click(ctx.message(Msg::StepPoints(step)))
            .glyph(
                PathIcon::new().data(path).width(8.0).height(4.0).into(),
                8.0,
                4.0,
            )
            .tooltip(name)
    };
    Grid::new().children((
        NumberBox::new()
            .minimum(1.0)
            .maximum(144.0)
            .value(points as f64)
            .automation_name(tr("Размер шрифта, пт"))
            .on_value_changed(ctx.callback(Msg::Points)),
        StackPanel::new()
            .horizontal_alignment(HorizontalAlignment::Right)
            .vertical_alignment(VerticalAlignment::Center)
            .margin(Thickness::new(0.0, 0.0, 3.0, 0.0))
            .children((
                arrow(
                    "M0,4 L4,0 L8,4 Z",
                    tr("Увеличить размер шрифта"),
                    1.0,
                    points < 144.0,
                ),
                arrow(
                    "M0,0 L8,0 L4,4 Z",
                    tr("Уменьшить размер шрифта"),
                    -1.0,
                    points > 1.0,
                ),
            )),
    ))
}

fn alignment_toolbar(
    ctx: &ViewContext<Studio>,
    text: bool,
    is_selected: impl Fn(Axis, LabelAlignment) -> bool,
) -> View {
    let groups = [Axis::Horizontal, Axis::Vertical].map(|axis| {
        let buttons = [
            LabelAlignment::Start,
            LabelAlignment::Center,
            LabelAlignment::End,
        ]
        .map(|alignment| {
            let direction = match (axis, alignment) {
                (Axis::Horizontal, LabelAlignment::Start) => tr("по левому краю"),
                (Axis::Horizontal, LabelAlignment::Center) => tr("по центру горизонтально"),
                (Axis::Horizontal, LabelAlignment::End) => tr("по правому краю"),
                (Axis::Vertical, LabelAlignment::Start) => tr("по верхнему краю"),
                (Axis::Vertical, LabelAlignment::Center) => tr("по центру вертикально"),
                (Axis::Vertical, LabelAlignment::End) => tr("по нижнему краю"),
            };
            let label = format!(
                "{}: {direction}",
                if text {
                    tr("Текст")
                } else {
                    tr("Блок на холсте")
                }
            );
            let active = is_selected(axis, alignment);
            icon_button(active)
                .automation_name(format!(
                    "{label}: {}",
                    if active {
                        tr("выбрано")
                    } else {
                        tr("не выбрано")
                    }
                ))
                .on_click(ctx.message(if text {
                    Msg::TextAlign(axis, alignment)
                } else {
                    Msg::BlockAlign(axis, alignment)
                }))
                .glyph(
                    alignment_icon(text, axis, alignment),
                    ui::TOOL_GLYPH,
                    ui::TOOL_GLYPH,
                )
                .tooltip(label)
        });
        field(
            if axis == Axis::Horizontal {
                tr("По горизонтали")
            } else {
                tr("По вертикали")
            },
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(2.0)
                .children(buttons),
        )
    });
    StackPanel::new().spacing(12.0).children(groups)
}

fn alignment_icon(text: bool, axis: Axis, alignment: LabelAlignment) -> View {
    // Filled vector paths inherit the native button foreground in all themes.
    let path = match (text, axis, alignment) {
        (true, Axis::Horizontal, LabelAlignment::Start) => {
            "M1,2H19V4H1Z M1,7H13V9H1Z M1,12H19V14H1Z M1,17H13V19H1Z"
        }
        (true, Axis::Horizontal, LabelAlignment::Center) => {
            "M1,2H19V4H1Z M4,7H16V9H4Z M1,12H19V14H1Z M4,17H16V19H4Z"
        }
        (true, Axis::Horizontal, LabelAlignment::End) => {
            "M1,2H19V4H1Z M7,7H19V9H7Z M1,12H19V14H1Z M7,17H19V19H7Z"
        }
        (true, Axis::Vertical, LabelAlignment::Start) => {
            "M1,1H19V3H1Z M4,5H16V7H4Z M4,9H13V11H4Z M4,13H16V15H4Z M1,18H2V19H1Z M18,18H19V19H18Z"
        }
        (true, Axis::Vertical, LabelAlignment::Center) => {
            "M1,1H19V2H1Z M4,5H16V7H4Z M4,9H13V11H4Z M4,13H16V15H4Z M1,18H19V19H1Z"
        }
        (true, Axis::Vertical, LabelAlignment::End) => {
            "M1,1H2V2H1Z M18,1H19V2H18Z M4,5H16V7H4Z M4,9H13V11H4Z M4,13H16V15H4Z M1,17H19V19H1Z"
        }
        (false, Axis::Horizontal, LabelAlignment::Start) => {
            "M1,1H3V19H1Z M5,5H19V9H5Z M5,11H14V15H5Z"
        }
        (false, Axis::Horizontal, LabelAlignment::Center) => {
            "M9,1H11V19H9Z M1,5H19V9H1Z M4,11H16V15H4Z"
        }
        (false, Axis::Horizontal, LabelAlignment::End) => {
            "M17,1H19V19H17Z M1,5H15V9H1Z M6,11H15V15H6Z"
        }
        (false, Axis::Vertical, LabelAlignment::Start) => {
            "M1,1H19V3H1Z M5,5H9V19H5Z M11,5H15V14H11Z"
        }
        (false, Axis::Vertical, LabelAlignment::Center) => {
            "M1,9H19V11H1Z M5,1H9V19H5Z M11,4H15V16H11Z"
        }
        (false, Axis::Vertical, LabelAlignment::End) => {
            "M1,17H19V19H1Z M5,1H9V15H5Z M11,6H15V15H11Z"
        }
    };
    // Nonzero fill prevents holes where a centre guide intersects a block.
    ui::path(&format!("F1 {path}"))
}

fn heading(text: &str) -> View {
    TextBlock::new()
        .text(text)
        .font_size(14.0)
        .foreground(ui::INK)
        .font_weight(FontWeight::SEMI_BOLD)
        .into()
}
fn keyed(views: Vec<View>) -> impl Iterator<Item = KeyedView> {
    views
        .into_iter()
        .enumerate()
        .map(|(i, v)| KeyedView::new(i.to_string(), v))
}
fn field(label: &str, view: impl Into<View>) -> View {
    StackPanel::new().spacing(5.0).children((
        TextBlock::new()
            .text(tr(label))
            .font_size(11.5)
            .foreground(ui::MUTED),
        view.into(),
    ))
}
fn button(ctx: &ViewContext<Studio>, label: &str, command: &str) -> View {
    ui::styled_button(false, false, false)
        .on_click(ctx.message(Msg::Command(command.into())))
        .content(tr(label))
}
fn document_bytes(d: &Document) -> usize {
    let sources = d
        .generator
        .variables
        .iter()
        .map(|v| match &v.source {
            niimbot_d101::generator::Source::List { text } => text.len(),
            _ => 128,
        })
        .sum::<usize>();
    sources
        + d.labels
            .iter()
            .flat_map(|p| &p.blocks)
            .map(|b| match &b.content {
                Content::Image { png, .. } => png.len(),
                Content::Text { text, .. } => text.len() + 128,
                _ => 128,
            })
            .sum::<usize>()
            .max(1024)
}
fn save_document(path: &std::path::Path, doc: &Document) -> Result<()> {
    doc.validate()?;
    let tmp = path.with_extension("json.tmp");
    let mut file = std::fs::File::create(&tmp)?;
    serde_json::to_writer_pretty(&mut file, doc)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod menu_tests {
    use super::*;
    #[test]
    fn every_caption_invokes_the_same_command_in_both_languages() {
        for lang in [Language::Ru, Language::En] {
            for key in MAIN_MENUS.iter().flat_map(|(_, keys)| keys.iter().copied()) {
                assert_eq!(menu_command(lang.text(key), lang), Some(key));
            }
        }
    }
}
