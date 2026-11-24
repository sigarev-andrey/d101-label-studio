//! Layout of the native editor, following the supplied HTML design.
use super::*;
use crate::{
    editor_geometry::{CANVAS_PAD_X, CANVAS_PAD_Y, HANDLES},
    studio_style as ui,
};

fn action(ctx: &ViewContext<Studio>, title: &str, command: &str, glyph: View, bare: bool) -> View {
    ui::styled_button(false, false, bare)
        .automation_name(tr(title))
        .on_click(ctx.message(Msg::Command(command.into())))
        .content(ui::button_content(glyph, tr(title)))
}

fn square(
    ctx: &ViewContext<Studio>,
    title: &str,
    command: &str,
    glyph: View,
    enabled: bool,
) -> View {
    ui::styled_button(false, false, false)
        .square(ui::TOOL_SIDE)
        .is_enabled(enabled)
        .automation_name(tr(title))
        .on_click(ctx.message(Msg::Command(command.into())))
        .glyph(glyph, ui::TOOL_GLYPH, ui::TOOL_GLYPH)
        .tooltip(tr(title))
}

impl Studio {
    pub(super) fn editor_viewport(&self) -> (f64, f64) {
        (
            (self.window_size.width - 570.0).max(340.0),
            (self.window_size.height - 389.0).max(140.0),
        )
    }

    pub(super) fn layout(&self, ctx: &mut ViewContext<Self>) -> View {
        ctx.window_title("D101 Label Studio");
        ctx.on_window_size(ctx.callback(Msg::WindowSize));
        let mut visuals = WindowVisuals::new()
            .client_size(1440.0, 920.0)
            .constraints(WindowConstraints {
                min_width: Some(1140.0),
                min_height: Some(720.0),
                ..Default::default()
            })
            .theme(WindowTheme::Light)
            .backdrop(WindowBackdrop::Mica);
        if let Some(path) = crate::app_icon::window_path() {
            visuals = visuals.icon(path);
        }
        ctx.window_visuals(visuals);
        let pending = self
            .input
            .engine
            .jobs()
            .iter()
            .filter(|j| j.state < 7)
            .count();
        let total = self
            .doc
            .labels
            .iter()
            .filter(|p| p.enabled)
            .map(|p| p.copies)
            .sum::<usize>();
        let header = Grid::new()
            .columns([GridLength::Pixel(230.0), GridLength::STAR, GridLength::Auto])
            .margin(Thickness::xy(22.0, 0.0))
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(10.0)
                    .children((
                        crate::app_icon::view(38.0),
                        StackPanel::new()
                            .vertical_alignment(VerticalAlignment::Center)
                            .children((
                                ui::label("D101", 16.0, ui::INK).font_weight(FontWeight::BOLD),
                                ui::label("L A B E L   S T U D I O", 9.0, ui::MUTED),
                            )),
                    )),
                Border::new()
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .content(View::component::<MainMenu>((
                        niimbot_d101::i18n::Language::current(),
                        ctx.callback(|caption: String| {
                            Msg::Command(
                                menu_command(&caption, Language::current())
                                    .unwrap_or_default()
                                    .into(),
                            )
                        }),
                    ))),
                StackPanel::new()
                    .grid_column(2)
                    .orientation(Orientation::Horizontal)
                    .spacing(10.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        Border::new()
                            .width(6.0)
                            .height(6.0)
                            .corner_radius(3.0)
                            .background(if self.dirty { ui::RED } else { ui::GREEN })
                            .vertical_alignment(VerticalAlignment::Center),
                        ui::label(
                            if self.dirty {
                                tr("Сохранение…")
                            } else {
                                tr("Черновик сохранён")
                            },
                            11.0,
                            ui::MUTED,
                        )
                        .vertical_alignment(VerticalAlignment::Center),
                        ui::vertical_divider(),
                        ui::styled_button(false, false, false)
                            .square(ui::COMPACT_SIDE)
                            .height(17.0)
                            .automation_name(tr("Справка"))
                            .on_click(ctx.message(Msg::Command("Справка".into())))
                            .glyph(
                                ui::ContourIcon::Help.view(),
                                ui::COMPACT_GLYPH,
                                ui::COMPACT_GLYPH,
                            )
                            .tooltip(tr("Справка")),
                    )),
            ));
        let toolbar = Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .margin(Thickness::xy(22.0, 0.0))
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(6.0)
                    .children((
                        action(ctx, "Текст", "Текст", ui::text_icon(), false),
                        action(
                            ctx,
                            tr("Изображение"),
                            "Изображение…",
                            ui::icon(Symbol::Pictures),
                            false,
                        ),
                        action(ctx, "Фигура", "Фигура", ui::shape_icon(), false),
                        ui::vertical_divider(),
                        square(
                            ctx,
                            "Отменить",
                            "Отменить",
                            ui::ContourIcon::Undo.view(),
                            !self.undo.is_empty(),
                        ),
                        square(
                            ctx,
                            "Повторить",
                            "Повторить",
                            ui::ContourIcon::Redo.view(),
                            !self.redo.is_empty(),
                        ),
                        ui::vertical_divider(),
                        square(
                            ctx,
                            "Дублировать блок",
                            "Дублировать блок",
                            ui::copy_icon(),
                            self.block().is_some(),
                        ),
                        square(
                            ctx,
                            "Удалить блок",
                            "Удалить блок",
                            ui::icon(Symbol::Delete),
                            self.block().is_some(),
                        ),
                    )),
                StackPanel::new()
                    .grid_column(1)
                    .orientation(Orientation::Horizontal)
                    .spacing(10.0)
                    .children((
                        action(ctx, "Предпросмотр", "Предпросмотр", ui::eye_icon(), true),
                        ui::styled_button(false, true, false)
                            .is_enabled(total > 0)
                            .automation_name(tr("Печатать набор"))
                            .on_click(ctx.message(Msg::Command("Печатать набор".into())))
                            .content(ui::button_content(
                                ui::icon(Symbol::Print),
                                if self.input.engine.simulated {
                                    tr("Симуляция печати")
                                } else {
                                    tr("Печатать набор")
                                },
                            )),
                    )),
            ));
        let body = Grid::new()
            .columns([
                GridLength::Pixel(252.0),
                GridLength::STAR,
                GridLength::Pixel(318.0),
            ])
            .children((
                Border::new()
                    .background(ui::WHITE)
                    .border_brush(ui::LINE)
                    .border_thickness(Thickness::new(0.0, 0.0, 1.0, 0.0))
                    .content(self.label_sidebar(ctx, pending)),
                Border::new().grid_column(1).content(self.editor(ctx)),
                Border::new()
                    .grid_column(2)
                    .background(ui::WHITE)
                    .border_brush(ui::LINE)
                    .border_thickness(Thickness::new(1.0, 0.0, 0.0, 0.0))
                    .content(
                        ScrollViewer::new()
                            .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
                            .content(
                                Border::new()
                                    .padding(Thickness::new(18.0, 20.0, 25.0, 20.0))
                                    .content(self.inspector(ctx)),
                            ),
                    ),
            ));
        let footer = Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .margin(Thickness::xy(20.0, 0.0))
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                ui::label(
                    local_format!(
                        "{}  ·  В наборе: {total} шт.  ·  В очереди: {pending}",
                        "{}  ·  Copies in set: {total}  ·  Queued: {pending}",
                        self.status
                    ),
                    11.0,
                    ui::MUTED,
                )
                .text_trimming(TextTrimming::CharacterEllipsis)
                .tooltip(&self.status),
                ui::label(
                    if self.input.engine.simulated {
                        tr("Режим симуляции").to_owned()
                    } else {
                        self.block()
                            .map(|b| local_format!("Блок: {}", "Block: {}", b.title()))
                            .unwrap_or_else(|| tr("Локальный редактор").into())
                    },
                    11.0,
                    ui::MUTED,
                )
                .grid_column(1)
                .max_width(270.0)
                .margin(Thickness::new(20.0, 0.0, 0.0, 0.0))
                .text_trimming(TextTrimming::CharacterEllipsis),
            ));
        let root = Grid::new()
            .rows([
                GridLength::Auto,
                GridLength::Pixel(59.0),
                GridLength::Pixel(77.0),
                GridLength::Pixel(56.0),
                GridLength::STAR,
                GridLength::Pixel(29.0),
            ])
            .children((
                Border::new()
                    .background(ui::RED)
                    .content(TitleBar::new().title("")),
                surface_row(1, header),
                surface_row(2, self.device_strip(ctx)),
                surface_row(3, toolbar),
                Border::new().grid_row(4).content(body),
                surface_row(5, footer),
            ));
        Grid::new().children((
            Border::new().background(ui::WHITE).content(root),
            self.dialog_view(ctx),
        ))
    }

    fn device_strip(&self, ctx: &ViewContext<Self>) -> View {
        let status = self.input.engine.printer_status();
        let can_refresh = !status.printing
            && matches!(
                status.phase,
                StatusPhase::Waiting | StatusPhase::Available | StatusPhase::Unavailable
            );
        let cell = |column: i32, glyph: View, caption: &str, value: String| {
            Grid::new()
                .grid_column(column)
                .columns([GridLength::Auto, GridLength::STAR])
                .column_spacing(10.0)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    Border::new()
                        .width(ui::STATUS_SIDE)
                        .height(ui::STATUS_SIDE)
                        .corner_radius(10.0)
                        .background(ui::RED_WASH)
                        .content(
                            Grid::new()
                                .horizontal_alignment(HorizontalAlignment::Center)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children([glyph]),
                        ),
                    StackPanel::new().grid_column(1).spacing(4.0).children((
                        ui::label(caption, 11.0, ui::MUTED),
                        ui::label(value, 12.5, ui::INK)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .text_trimming(TextTrimming::CharacterEllipsis),
                    )),
                ))
        };
        Grid::new()
            .margin(Thickness::xy(22.0, 10.0))
            .columns([GridLength::STAR; 5])
            .column_spacing(12.0)
            .automation_name(tr("Состояние принтера"))
            .children((
                cell(
                    0,
                    ui::png_icon(include_bytes!("../assets/ui/printer.png"), ui::STATUS_GLYPH),
                    tr("Принтер"),
                    status.model_text(),
                ),
                cell(
                    1,
                    ui::battery_icon(status.reading.as_ref().and_then(|r| r.battery_percent)),
                    tr("Батарея"),
                    status.battery_text(),
                ),
                cell(
                    2,
                    ui::png_icon(include_bytes!("../assets/ui/media.png"), ui::STATUS_GLYPH),
                    tr("Тип этикетки"),
                    status.media_text(),
                ),
                cell(
                    3,
                    ui::png_icon(include_bytes!("../assets/ui/roll.png"), ui::STATUS_GLYPH),
                    tr("Остаток рулона"),
                    status.remaining_text(),
                ),
                Grid::new()
                    .grid_column(4)
                    .columns([GridLength::Auto, GridLength::STAR])
                    .column_spacing(10.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        ui::styled_button(false, false, false)
                            .square(ui::STATUS_SIDE)
                            .is_enabled(can_refresh)
                            .automation_name(tr("Обновить состояние принтера"))
                            .on_click(ctx.message(Msg::RefreshPrinterStatus))
                            .glyph(
                                ui::ContourIcon::Refresh.view(),
                                ui::STATUS_GLYPH,
                                ui::STATUS_GLYPH,
                            )
                            .tooltip(tr("Обновить состояние принтера")),
                        ui::label(
                            status.state_text(),
                            11.5,
                            if status.phase == StatusPhase::Available {
                                ui::GREEN
                            } else {
                                ui::MUTED
                            },
                        )
                        .grid_column(1)
                        .text_wrapping(TextWrapping::Wrap)
                        .vertical_alignment(VerticalAlignment::Center),
                    )),
            ))
    }

    fn label_sidebar(&self, ctx: &ViewContext<Self>, pending: usize) -> View {
        let label = &self.doc.labels[self.page];
        let cards = self.doc.labels.iter().enumerate().map(|(i, p)| {
            let active = i == self.page;
            KeyedView::new(
                format!("label-{i}"),
                ui::styled_button(active, false, false)
                    .height(62.0)
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .horizontal_content_alignment(HorizontalAlignment::Stretch)
                    .automation_name(local_format!(
                        "Этикетка {}: {}",
                        "Label {}: {}",
                        i + 1,
                        p.name
                    ))
                    .on_click(ctx.message(Msg::Page(Some(i))))
                    .content(
                        Grid::new()
                            .columns([GridLength::Auto, GridLength::STAR])
                            .column_spacing(10.0)
                            .children((
                                Border::new()
                                    .width(24.0)
                                    .height(24.0)
                                    .corner_radius(6.0)
                                    .background(if active { ui::RED } else { ui::SOFT })
                                    .vertical_alignment(VerticalAlignment::Center)
                                    .content(
                                        ui::label(
                                            (i + 1).to_string(),
                                            11.0,
                                            if active { ui::WHITE } else { ui::MUTED },
                                        )
                                        .horizontal_alignment(HorizontalAlignment::Center)
                                        .vertical_alignment(VerticalAlignment::Center),
                                    ),
                                StackPanel::new().grid_column(1).spacing(4.0).children((
                                    ui::label(&p.name, 12.0, ui::INK)
                                        .font_weight(FontWeight::SEMI_BOLD)
                                        .text_trimming(TextTrimming::CharacterEllipsis),
                                    ui::label(
                                        local_format!(
                                            "{} × {} мм · {} шт.{}",
                                            "{} × {} mm · Copies: {}{}",
                                            self.doc.spec.length_mm,
                                            self.doc.spec.width_mm,
                                            p.copies,
                                            if p.enabled { "" } else { tr(" · выкл.") }
                                        ),
                                        10.5,
                                        ui::MUTED,
                                    ),
                                )),
                            )),
                    ),
            )
        });
        let layers = label.blocks.iter().enumerate().rev().map(|(i, b)| {
            KeyedView::new(
                format!("layer-{}", b.id),
                ui::styled_button(self.selected == Some(b.id), false, true)
                    .height(36.0)
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .horizontal_content_alignment(HorizontalAlignment::Stretch)
                    .automation_name(local_format!("Слой: {}", "Layer: {}", b.title()))
                    .on_click(ctx.message(Msg::Select(Some(i))))
                    .content(
                        Grid::new()
                            .columns([GridLength::Auto, GridLength::STAR])
                            .column_spacing(9.0)
                            .children((
                                content_icon(&b.content),
                                ui::label(
                                    b.title(),
                                    11.8,
                                    if self.selected == Some(b.id) {
                                        ui::RED
                                    } else {
                                        ui::MUTED
                                    },
                                )
                                .grid_column(1)
                                .text_trimming(TextTrimming::CharacterEllipsis),
                            )),
                    ),
            )
        });
        let title = Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .children((
                Border::new()
                    .vertical_alignment(VerticalAlignment::Center)
                    .content(heading(tr("Набор этикеток"))),
                ui::styled_button(true, false, true)
                    .grid_column(1)
                    .square(ui::TOOL_SIDE)
                    .automation_name(tr("Добавить этикетку"))
                    .on_click(ctx.message(Msg::Command("Добавить этикетку".into())))
                    .glyph(
                        ui::png_icon(include_bytes!("../assets/ui/add-label.png"), 20.0),
                        20.0,
                        20.0,
                    )
                    .tooltip(tr("Добавить этикетку")),
            ));
        let form = StackPanel::new().spacing(9.0).children((
            title,
            ScrollViewer::new()
                .max_height(200.0)
                .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
                .content(StackPanel::new().spacing(7.0).keyed_children(cards)),
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(7.0)
                .children((
                    action(
                        ctx,
                        tr("Копия"),
                        "Дублировать этикетку",
                        ui::copy_icon(),
                        true,
                    ),
                    square(
                        ctx,
                        "Удалить этикетку",
                        "Удалить этикетку",
                        ui::icon(Symbol::Delete),
                        self.doc.labels.len() > 1,
                    ),
                )),
            field(
                tr("Название"),
                TextBox::new()
                    .background(ui::WHITE)
                    .border_brush(ui::LINE)
                    .text(&label.name)
                    .on_text_changed(ctx.callback(Msg::Name)),
            ),
            field(
                tr("Размер этикетки"),
                ui::styled_button(false, false, false)
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .horizontal_content_alignment(HorizontalAlignment::Left)
                    .automation_name(tr("Размер этикетки"))
                    .on_click(ctx.message(Msg::Command("Размер этикетки…".into())))
                    .content(ui::button_content(
                        ui::icon(Symbol::Page),
                        &local_format!(
                            "{} × {} мм",
                            "{} × {} mm",
                            self.doc.spec.length_mm,
                            self.doc.spec.width_mm
                        ),
                    )),
            ),
            Grid::new()
                .columns([GridLength::Pixel(92.0), GridLength::STAR])
                .column_spacing(10.0)
                .children((
                    field(
                        tr("Экземпляров"),
                        NumberBox::new()
                            .minimum(1.0)
                            .maximum(1000.0)
                            .value(label.copies as f64)
                            .on_value_changed(ctx.callback(Msg::Copies)),
                    ),
                    CheckBox::new()
                        .grid_column(1)
                        .vertical_alignment(VerticalAlignment::Bottom)
                        .is_checked(label.enabled)
                        .on_is_checked_changed(ctx.callback(Msg::Enabled))
                        .content(ui::label(tr("В наборе"), 11.0, ui::INK)),
                )),
            ui::divider(),
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto])
                .children((
                    heading(tr("Слои")),
                    ui::label(
                        local_format!("{} объектов", "Objects: {}", label.blocks.len()),
                        11.0,
                        ui::MUTED,
                    )
                    .grid_column(1),
                )),
            StackPanel::new().spacing(5.0).keyed_children(layers),
        ));
        Grid::new()
            .rows([GridLength::STAR, GridLength::Auto])
            .children((
                ScrollViewer::new()
                    .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
                    .content(
                        Border::new()
                            .padding(Thickness::new(17.0, 20.0, 17.0, 16.0))
                            .content(form),
                    ),
                StackPanel::new()
                    .grid_row(1)
                    .margin(Thickness::new(17.0, 12.0, 17.0, 16.0))
                    .spacing(10.0)
                    .children((
                        ui::styled_button(false, false, false)
                            .horizontal_alignment(HorizontalAlignment::Stretch)
                            .automation_name(tr("Очередь печати"))
                            .on_click(ctx.message(Msg::Command("Очередь печати".into())))
                            .content(ui::button_content(
                                ui::icon(Symbol::List),
                                &local_format!(
                                    "Очередь печати   {pending}",
                                    "Print queue   {pending}"
                                ),
                            )),
                        ui::label(
                            tr("Черновик сохраняется автоматически на этом компьютере."),
                            11.0,
                            ui::MUTED,
                        )
                        .text_wrapping(TextWrapping::Wrap),
                    )),
            ))
    }

    fn editor(&self, ctx: &ViewContext<Self>) -> View {
        let page = &self.doc.labels[self.page];
        let viewport = self.editor_viewport();
        let pixels = self.doc.spec.dimensions().unwrap_or((1, 1));
        let extent = (
            (pixels.0 as f64 * self.zoom / 100.0 + 2.0 * CANVAS_PAD_X).max(viewport.0 - 2.0),
            (pixels.1 as f64 * self.zoom / 100.0 + 2.0 * CANVAS_PAD_Y).max(viewport.1 - 2.0),
        );
        let mut grid = Vec::new();
        if self.grid {
            for x in (0..extent.0 as u32).step_by(24) {
                grid.push(KeyedView::new(
                    format!("v{x}"),
                    Border::new()
                        .canvas_left(x as f64)
                        .width(1.0)
                        .height(extent.1)
                        .background(Color::rgb(216, 220, 226)),
                ));
            }
            for y in (0..extent.1 as u32).step_by(24) {
                grid.push(KeyedView::new(
                    format!("h{y}"),
                    Border::new()
                        .canvas_top(y as f64)
                        .height(1.0)
                        .width(extent.0)
                        .background(Color::rgb(216, 220, 226)),
                ));
            }
        }
        let stage = ScrollViewer::new()
            .horizontal_scroll_bar_visibility(ScrollBarVisibility::Auto)
            .vertical_scroll_bar_visibility(ScrollBarVisibility::Auto)
            .content(
                Grid::new().width(extent.0).height(extent.1).children((
                    Canvas::new()
                        .width(extent.0)
                        .height(extent.1)
                        .keyed_children(grid),
                    self.canvas_view(ctx),
                )),
            );
        Grid::new()
            .rows([
                GridLength::Pixel(82.0),
                GridLength::STAR,
                GridLength::Pixel(54.0),
            ])
            .children((
                Grid::new()
                    .columns([GridLength::STAR, GridLength::Auto])
                    .margin(Thickness::xy(24.0, 16.0))
                    .children((
                        StackPanel::new().spacing(5.0).children((
                            ui::label(&page.name, 20.0, ui::INK)
                                .font_weight(FontWeight::SEMI_BOLD)
                                .text_trimming(TextTrimming::CharacterEllipsis),
                            ui::label(
                                local_format!(
                                    "{} × {} мм  ·  203 dpi  ·  {} блоков",
                                    "{} × {} mm  ·  203 dpi  ·  Blocks: {}",
                                    self.doc.spec.length_mm,
                                    self.doc.spec.width_mm,
                                    page.blocks.len()
                                ),
                                11.5,
                                ui::MUTED,
                            ),
                        )),
                        StackPanel::new()
                            .grid_column(1)
                            .orientation(Orientation::Horizontal)
                            .spacing(3.0)
                            .vertical_alignment(VerticalAlignment::Center)
                            .children((
                                ui::styled_button(self.grid, false, true)
                                    .on_click(ctx.message(Msg::Command("Сетка".into())))
                                    .automation_name(tr("Сетка"))
                                    .content(ui::button_content(ui::grid_icon(), tr("Сетка"))),
                                action(ctx, "Вписать", "Вписать", ui::fit_icon(), true),
                            )),
                    )),
                Border::new()
                    .grid_row(1)
                    .background(Color::rgb(
                        WORKSPACE_COLOR[0],
                        WORKSPACE_COLOR[1],
                        WORKSPACE_COLOR[2],
                    ))
                    .content(
                        Grid::new().children((
                            stage,
                            Border::new()
                                .horizontal_alignment(HorizontalAlignment::Left)
                                .vertical_alignment(VerticalAlignment::Top)
                                .margin(Thickness::xy(18.0, 16.0))
                                .corner_radius(13.0)
                                .background(ui::WHITE)
                                .padding(Thickness::xy(10.0, 5.0))
                                .content(ui::label(tr("Редактирование · мм"), 10.0, ui::MUTED)),
                        )),
                    ),
                Border::new()
                    .grid_row(2)
                    .border_brush(ui::LINE)
                    .border_thickness(Thickness::new(0.0, 1.0, 0.0, 0.0))
                    .padding(Thickness::xy(12.0, 0.0))
                    .content(
                        Grid::new()
                            .columns([GridLength::STAR, GridLength::Auto])
                            .vertical_alignment(VerticalAlignment::Center)
                            .children((
                                CheckBox::new()
                                    .is_checked(self.snap)
                                    .on_is_checked_changed(ctx.callback(Msg::Snap))
                                    .content(ui::label(tr("Шаг 0,5 мм"), 11.0, ui::MUTED)),
                                StackPanel::new()
                                    .grid_column(1)
                                    .orientation(Orientation::Horizontal)
                                    .spacing(4.0)
                                    .vertical_alignment(VerticalAlignment::Center)
                                    .children((
                                        ui::styled_button(false, false, false)
                                            .square(ui::COMPACT_SIDE)
                                            .is_enabled(self.zoom > 10.0)
                                            .automation_name(tr("Уменьшить"))
                                            .on_click(ctx.message(Msg::Command("Уменьшить".into())))
                                            .glyph(
                                                ui::icon(Symbol::Remove),
                                                ui::COMPACT_GLYPH,
                                                ui::COMPACT_GLYPH,
                                            )
                                            .tooltip(tr("Уменьшить")),
                                        Slider::new()
                                            .width(90.0)
                                            .minimum(10.0)
                                            .maximum(400.0)
                                            .step_frequency(10.0)
                                            .value(self.zoom)
                                            .automation_name(tr("Масштаб"))
                                            .on_value_changed(ctx.callback(|v| Msg::Zoom(Some(v)))),
                                        ui::styled_button(false, false, false)
                                            .square(ui::COMPACT_SIDE)
                                            .is_enabled(self.zoom < 400.0)
                                            .automation_name(tr("Увеличить"))
                                            .on_click(ctx.message(Msg::Command("Увеличить".into())))
                                            .glyph(
                                                ui::icon(Symbol::Add),
                                                ui::COMPACT_GLYPH,
                                                ui::COMPACT_GLYPH,
                                            )
                                            .tooltip(tr("Увеличить")),
                                        ui::label(format!("{:.0}%", self.zoom), 11.5, ui::INK)
                                            .width(38.0)
                                            .vertical_alignment(VerticalAlignment::Center),
                                        ui::styled_button(false, false, false)
                                            .square(ui::COMPACT_SIDE)
                                            .on_click(
                                                ctx.message(Msg::Command("Масштаб 100%".into())),
                                            )
                                            .automation_name(tr("Масштаб 100%"))
                                            .glyph(
                                                ui::label("1:1", 11.0, ui::INK).into(),
                                                16.0,
                                                13.0,
                                            ),
                                    )),
                            )),
                    ),
            ))
    }

    pub(super) fn canvas_view(&self, ctx: &ViewContext<Self>) -> View {
        let scale = self.scale();
        let pixels = self.doc.spec.dimensions().unwrap_or((1, 1));
        let (w, h) = (
            pixels.0 as f64 * self.zoom / 100.0,
            pixels.1 as f64 * self.zoom / 100.0,
        );
        let (px, py) = (CANVAS_PAD_X, CANVAS_PAD_Y);
        let mut children = vec![KeyedView::new(
            "paper",
            Border::new()
                .canvas_left(px)
                .canvas_top(py)
                .width(w)
                .height(h)
                .background(ui::WHITE),
        )];
        if self
            .doc
            .spec
            .outline
            .is_cable(self.doc.spec.length_mm, self.doc.spec.width_mm)
        {
            for y in (2..h as u32).step_by(8) {
                children.push(KeyedView::new(
                    format!("fold-{y}"),
                    Border::new()
                        .canvas_left(
                            px + f64::from(niimbot_d101::label_outline::CABLE_FOLD_MM) * scale,
                        )
                        .canvas_top(py + y as f64)
                        .width(1.0)
                        .height(4.0_f64.min(h - y as f64))
                        .background(Color::rgb(170, 170, 170))
                        .tooltip(tr("Линия сгиба флажка")),
                ));
            }
        }
        for b in &self.doc.labels[self.page].blocks {
            if let Some(image) = self.preview.block_image(b.id) {
                let bounds = b.bounds;
                children.push(KeyedView::new(
                    format!("block-{}", b.id),
                    Border::new()
                        .canvas_left(px + bounds.x as f64 * scale)
                        .canvas_top(py + bounds.y as f64 * scale)
                        .width(bounds.width as f64 * scale)
                        .height(bounds.height as f64 * scale)
                        .content(image_view(
                            image,
                            bounds.width as f64 * scale,
                            bounds.height as f64 * scale,
                            self.zoom / 100.0,
                        )),
                ));
            }
        }
        if let Some(contour) = &self.preview.contour {
            children.push(KeyedView::new(
                "contour",
                Border::new()
                    .canvas_left(px)
                    .canvas_top(py)
                    .content(image_view(contour, w, h, self.zoom / 100.0)),
            ));
        }
        if let Some(block) = self.block() {
            let b = block.bounds;
            children.push(KeyedView::new(
                "selection",
                Border::new()
                    .canvas_left(px + b.x as f64 * scale)
                    .canvas_top(py + b.y as f64 * scale)
                    .width(b.width as f64 * scale)
                    .height(b.height as f64 * scale)
                    .border_thickness(1.5)
                    .border_brush(ui::RED),
            ));
            for (i, handle) in HANDLES.into_iter().enumerate() {
                let (x, y) = handle.point(b);
                children.push(KeyedView::new(
                    format!("handle-{i}"),
                    Border::new()
                        .canvas_left(px + x as f64 * scale - 4.0)
                        .canvas_top(py + y as f64 * scale - 4.0)
                        .width(8.0)
                        .height(8.0)
                        .border_brush(ui::RED)
                        .border_thickness(1.5)
                        .background(ui::WHITE),
                ));
            }
            let annotation = |text: String, x: f64, y: f64| {
                ui::label(text, 11.0, ui::RED)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .canvas_left(x)
                    .canvas_top(y)
                    .tooltip(tr("Положение и размер блока в миллиметрах"))
            };
            children.push(KeyedView::new(
                "coordinates",
                annotation(
                    format!("X: {:.1}   Y: {:.1}", b.x, b.y),
                    px + b.x as f64 * scale,
                    py + b.y as f64 * scale - 26.0,
                ),
            ));
            children.push(KeyedView::new(
                "width",
                annotation(
                    local_format!("{:.1} мм", "{:.1} mm", b.width),
                    px + (b.x + b.width / 2.0) as f64 * scale - 22.0,
                    py + (b.y + b.height) as f64 * scale + 10.0,
                ),
            ));
            children.push(KeyedView::new(
                "height",
                annotation(
                    format!("{:.1}", b.height),
                    px + (b.x + b.width) as f64 * scale + 10.0,
                    py + (b.y + b.height / 2.0) as f64 * scale - 8.0,
                ),
            ));
        }
        Border::new()
            .width(w + 2.0 * px)
            .height(h + 2.0 * py)
            .horizontal_alignment(HorizontalAlignment::Center)
            .vertical_alignment(VerticalAlignment::Center)
            .background(Color::argb(0, 0, 0, 0))
            .automation_name(tr("Холст этикетки"))
            .capture_pointer_on_press(true)
            .on_pointer_pressed(ctx.callback(Msg::Press))
            .on_pointer_moved(ctx.callback(Msg::Move))
            .on_pointer_released(ctx.callback(|_| Msg::Release))
            .on_pointer_capture_lost(ctx.message(Msg::Release))
            .on_pointer_canceled(ctx.message(Msg::Release))
            .content(
                Canvas::new()
                    .width(w + 2.0 * px)
                    .height(h + 2.0 * py)
                    .keyed_children(children),
            )
    }
}

fn surface_row(row: i32, content: View) -> View {
    Border::new()
        .grid_row(row)
        .background(ui::WHITE)
        .border_brush(ui::LINE)
        .border_thickness(Thickness::new(0.0, 0.0, 0.0, 1.0))
        .content(content)
}

fn content_icon(content: &Content) -> View {
    match content {
        Content::Text { .. } => ui::text_icon(),
        Content::Image { .. } => ui::icon(Symbol::Pictures),
        Content::Rectangle { .. } => ui::shape_icon(),
        Content::Line { .. } => ui::path("M1,9H19V11H1Z"),
    }
}
