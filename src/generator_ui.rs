//! Isolated dialog: queue/status ticks do not rebuild variable editors or PNG previews.
use crate::preview::image_view;
use niimbot_d101::{
    document::Document,
    generator::{self, Combine, Plan, Settings, Source},
};
use niimbot_d101::{i18n::tr, local_format};
use std::{io::Cursor, sync::Arc};
use windows_reactor::*;

#[derive(Clone)]
pub struct Input {
    pub doc: Arc<Document>,
    pub page: usize,
    pub complete: Callback<Option<Box<Document>>>,
}
impl PartialEq for Input {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.doc, &other.doc)
            && self.page == other.page
            && self.complete == other.complete
    }
}

#[derive(Clone)]
pub enum Msg {
    Variable(Option<usize>),
    Source(Option<usize>),
    Range(u8, Option<f64>),
    List(String),
    Combine(Option<usize>),
    Copies(Option<f64>),
    Sample(Option<f64>),
    Closed(ContentDialogResult),
}
pub struct GeneratorDialog {
    input: Input,
    settings: Settings,
    variable: usize,
    sample: usize,
    plan: Option<Plan>,
    error: String,
    preview: Option<(EncodedImage, f64, f64, f64)>,
    sample_text: String,
}
impl GeneratorDialog {
    fn rebuild(&mut self) {
        self.error.clear();
        match Plan::new(&self.input.doc, self.input.page, &self.settings) {
            Ok(plan) => {
                self.sample = self.sample.min(plan.count - 1);
                self.plan = Some(plan);
            }
            Err(e) => {
                self.plan = None;
                self.error = format!("{e:#}");
            }
        }
        self.render_sample();
    }
    fn render_sample(&mut self) {
        self.preview = None;
        self.sample_text.clear();
        let Some(plan) = &self.plan else {
            return;
        };
        let result = (|| -> anyhow::Result<_> {
            let label = plan.label(self.sample)?;
            self.sample_text = label
                .blocks
                .iter()
                .filter_map(|b| match &b.content {
                    niimbot_d101::document::Content::Text { text, .. } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            let doc = Document {
                version: Document::VERSION,
                spec: self.input.doc.spec.clone(),
                generator: Settings::default(),
                labels: vec![label],
            };
            let raster = doc.preview(0, true)?;
            let scale = (440.0 / raster.width() as f64)
                .min(120.0 / raster.height() as f64)
                .min(2.0);
            let mut png = Cursor::new(Vec::new());
            raster.write_to(&mut png, image::ImageFormat::Png)?;
            Ok((
                EncodedImage::new(png.into_inner()),
                raster.width() as f64 * scale,
                raster.height() as f64 * scale,
                scale,
            ))
        })();
        match result {
            Ok(preview) => self.preview = Some(preview),
            Err(e) => self.error = local_format!("Предпросмотр: {e:#}", "Preview: {e:#}"),
        }
    }
}
impl Component for GeneratorDialog {
    type Input = Input;
    type Message = Msg;
    fn create(input: &Input, _: &ComponentContext<Self>) -> Self {
        let settings = input
            .doc
            .generator
            .for_template(&input.doc.labels[input.page])
            .unwrap_or_default();
        let mut state = Self {
            input: input.clone(),
            settings,
            variable: 0,
            sample: 0,
            plan: None,
            error: String::new(),
            preview: None,
            sample_text: String::new(),
        };
        state.rebuild();
        state
    }
    fn update(&mut self, msg: Msg, _: &ComponentContext<Self>) {
        match msg {
            Msg::Closed(result) => {
                if result == ContentDialogResult::Primary {
                    // Validate again at commit; no partially generated labels enter the project.
                    match generator::generate(&self.input.doc, self.input.page, &self.settings) {
                        Ok(doc) => {
                            let _ = self.input.complete.call(Some(Box::new(doc)));
                        }
                        Err(e) => {
                            self.error = format!("{e:#}");
                        }
                    }
                } else {
                    let _ = self.input.complete.call(None);
                }
                return;
            }
            Msg::Variable(Some(i)) if i < self.settings.variables.len() => {
                self.variable = i;
                return;
            }
            Msg::Source(Some(i)) => {
                if let Some(v) = self.settings.variables.get_mut(self.variable) {
                    if i == 0 && !matches!(v.source, Source::Range { .. }) {
                        v.source = Source::default();
                    }
                    if i == 1 && !matches!(v.source, Source::List { .. }) {
                        v.source = Source::List {
                            text: String::new(),
                        };
                    }
                }
            }
            Msg::Range(field, Some(n)) if n.is_finite() => {
                if let Some(v) = self.settings.variables.get_mut(self.variable)
                    && let Source::Range {
                        start,
                        end,
                        step,
                        digits,
                    } = &mut v.source
                {
                    let value = n.round().clamp(-1_000_000_000.0, 1_000_000_000.0) as i64;
                    match field {
                        0 => *start = value,
                        1 => *end = value,
                        2 => *step = value,
                        _ => *digits = value.clamp(0, 12) as u8,
                    }
                }
            }
            Msg::List(text) => {
                if let Some(v) = self.settings.variables.get_mut(self.variable) {
                    v.source = Source::List { text };
                }
            }
            Msg::Combine(Some(i)) => {
                self.settings.combine = if i == 1 {
                    Combine::Product
                } else {
                    Combine::Rows
                }
            }
            Msg::Copies(Some(n)) if n.is_finite() => {
                self.settings.copies = n.round().clamp(1.0, 1000.0) as usize
            }
            Msg::Sample(Some(n)) if n.is_finite() => {
                if let Some(plan) = &self.plan {
                    self.sample = (n.round().max(1.0) as usize - 1).min(plan.count - 1);
                }
                self.render_sample();
                return;
            }
            _ => return,
        }
        self.rebuild();
    }
    fn view(&self, _: &Input, ctx: &mut ViewContext<Self>) -> View {
        let mut children = vec![
            TextBlock::new()
                .text(local_format!(
                    "Шаблон: {}. Переменные задаются в текстовых блоках: {{номер}}, {{название}}.",
                    "Template: {}. Use variables in text blocks, such as {{number}} and {{name}}.",
                    self.input.doc.labels[self.input.page].name
                ))
                .text_wrapping(TextWrapping::Wrap)
                .into(),
        ];
        if let Some(variable) = self.settings.variables.get(self.variable) {
            children.push(
                Grid::new()
                    .columns([GridLength::STAR, GridLength::STAR])
                    .column_spacing(12.0)
                    .children((
                        field(
                            tr("Переменная"),
                            ComboBox::new()
                                .width(224.0)
                                .items_source(
                                    self.settings
                                        .variables
                                        .iter()
                                        .map(|v| format!("{{{}}}", v.name)),
                                )
                                .selected_index(self.variable)
                                .on_selection_changed(ctx.callback(Msg::Variable)),
                        ),
                        Border::new().grid_column(1).content(field(
                            tr("Источник значений"),
                            ComboBox::new()
                                .width(224.0)
                                .items_source([tr("Диапазон чисел"), tr("Список строк")])
                                .selected_index(
                                    if matches!(variable.source, Source::Range { .. }) {
                                        0
                                    } else {
                                        1
                                    },
                                )
                                .on_selection_changed(ctx.callback(Msg::Source)),
                        )),
                    )),
            );
            match &variable.source {
                Source::Range {
                    start,
                    end,
                    step,
                    digits,
                } => children.push(
                    Grid::new()
                        .columns([
                            GridLength::STAR,
                            GridLength::STAR,
                            GridLength::STAR,
                            GridLength::STAR,
                        ])
                        .column_spacing(8.0)
                        .keyed_children(
                            [
                                (tr("От"), *start),
                                (tr("До включительно"), *end),
                                (tr("Шаг"), *step),
                                (tr("Минимум цифр"), *digits as i64),
                            ]
                            .into_iter()
                            .enumerate()
                            .map(|(i, (name, value))| {
                                KeyedView::new(
                                    i.to_string(),
                                    Border::new().grid_column(i as i32).content(field(
                                        name,
                                        NumberBox::new()
                                            .automation_name(local_format!(
                                                "Диапазон: {name}",
                                                "Range: {name}"
                                            ))
                                            .minimum(if i == 3 { 0.0 } else { -1_000_000_000.0 })
                                            .maximum(if i == 3 { 12.0 } else { 1_000_000_000.0 })
                                            .value(value as f64)
                                            .on_value_changed(
                                                ctx.callback(move |v| Msg::Range(i as u8, v)),
                                            ),
                                    )),
                                )
                            }),
                        ),
                ),
                Source::List { text } => children.push(field(
                    tr("Одна строка — одно значение; пустые строки пропускаются"),
                    TextBox::new()
                        .automation_name(tr("Значения переменной"))
                        .text(text)
                        .accepts_return(true)
                        .text_wrapping(TextWrapping::Wrap)
                        .height(110.0)
                        .placeholder_text(tr("Питание\nLAN\nРезерв"))
                        .on_text_changed(ctx.callback(Msg::List)),
                )),
            }
            children.push(
                Grid::new()
                    .columns([GridLength::STAR, GridLength::STAR])
                    .column_spacing(12.0)
                    .children((
                        field(
                            tr("Несколько переменных"),
                            ComboBox::new()
                                .width(224.0)
                                .items_source([tr("По строкам"), tr("Все сочетания")])
                                .selected_index(if self.settings.combine == Combine::Rows {
                                    0
                                } else {
                                    1
                                })
                                .on_selection_changed(ctx.callback(Msg::Combine)),
                        ),
                        Border::new().grid_column(1).content(field(
                            tr("Копий каждой этикетки"),
                            NumberBox::new()
                                .automation_name(tr("Копий каждой этикетки"))
                                .minimum(1.0)
                                .maximum(1000.0)
                                .value(self.settings.copies as f64)
                                .on_value_changed(ctx.callback(Msg::Copies)),
                        )),
                    )),
            );
        }
        if !self.error.is_empty() {
            children.push(
                TextBlock::new()
                    .text(&self.error)
                    .foreground(ThemeBrush::SystemCritical)
                    .text_wrapping(TextWrapping::Wrap)
                    .into(),
            );
        }
        if let Some(plan) = &self.plan {
            children.push(
                TextBlock::new()
                    .text(local_format!(
                        "Будет создано: {} этикеток · {} экземпляров",
                        "Will create {} labels · {} copies",
                        plan.count,
                        plan.total_copies()
                    ))
                    .font_weight(FontWeight::SEMI_BOLD)
                    .into(),
            );
            children.push(field(
                tr("Образец №"),
                NumberBox::new()
                    .automation_name(tr("Номер образца"))
                    .minimum(1.0)
                    .maximum(plan.count as f64)
                    .value((self.sample + 1) as f64)
                    .on_value_changed(ctx.callback(Msg::Sample)),
            ));
            if let Some((image, w, h, scale)) = &self.preview {
                children.push(
                    Border::new()
                        .height(144.0)
                        .background(Color::rgb(127, 127, 127))
                        .corner_radius(6.0)
                        .content(
                            Border::new()
                                .horizontal_alignment(HorizontalAlignment::Center)
                                .vertical_alignment(VerticalAlignment::Center)
                                .content(image_view(image, *w, *h, *scale)),
                        ),
                );
            }
            children.push(
                TextBlock::new()
                    .text(&self.sample_text)
                    .max_height(64.0)
                    .text_wrapping(TextWrapping::Wrap)
                    .is_text_selection_enabled(true)
                    .into(),
            );
        }
        children.push(TextBlock::new().text(tr("Исходный шаблон сохранится в наборе и будет исключён из печати. Новые этикетки можно редактировать. Создание отменяется одним действием."))
            .font_size(12.0).opacity(0.7).text_wrapping(TextWrapping::Wrap).into());
        // A component needs a native root even when its only visible UI is a popup.
        Grid::new().children((ContentDialog::new()
            .title(tr("Генератор этикеток"))
            .is_open(true)
            .primary_button_text(tr("Создать набор"))
            .is_primary_button_enabled(self.plan.is_some() && self.error.is_empty())
            .close_button_text(tr("Отмена"))
            .on_closed(ctx.callback(Msg::Closed))
            .content(
                ScrollViewer::new().width(460.0).max_height(560.0).content(
                    StackPanel::new().spacing(12.0).keyed_children(
                        children
                            .into_iter()
                            .enumerate()
                            .map(|(i, v)| KeyedView::new(i.to_string(), v)),
                    ),
                ),
            ),))
    }
}
fn field(label: &str, view: impl Into<View>) -> View {
    StackPanel::new().spacing(5.0).children((
        TextBlock::new().text(label).font_size(12.0).opacity(0.75),
        view.into(),
    ))
}
