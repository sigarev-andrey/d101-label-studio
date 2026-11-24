//! Native WinUI styling drawn from the supplied D101_Label_Studio.html reference.
use windows_reactor::*;

pub const WHITE: Color = Color::rgb(255, 255, 255);
pub const INK: Color = Color::rgb(32, 36, 43);
pub const MUTED: Color = Color::rgb(126, 133, 143);
pub const LINE: Color = Color::rgb(229, 232, 237);
pub const SOFT: Color = Color::rgb(245, 246, 248);
pub const RED: Color = Color::rgb(185, 50, 57);
pub const RED_WASH: Color = Color::rgb(250, 237, 239);
pub const GREEN: Color = Color::rgb(64, 168, 117);
pub const COMPACT_SIDE: f64 = 23.0;
pub const FORMAT_SIDE: f64 = 30.0;
pub const TOOL_SIDE: f64 = 34.0;
pub const STATUS_SIDE: f64 = 38.0;
pub const STATUS_GLYPH: f64 = 20.0;
pub const COMPACT_GLYPH: f64 = 13.0;
pub const TOOL_GLYPH: f64 = 16.0;

pub enum ContourIcon {
    Bold,
    Italic,
    Underline,
    Strike,
    Undo,
    Redo,
    Refresh,
    Help,
}

impl ContourIcon {
    pub fn view(self) -> View {
        let png: &'static [u8] = match self {
            Self::Bold => include_bytes!("../assets/ui/bold.png"),
            Self::Italic => include_bytes!("../assets/ui/italic.png"),
            Self::Underline => include_bytes!("../assets/ui/underline.png"),
            Self::Strike => include_bytes!("../assets/ui/strike.png"),
            Self::Undo => include_bytes!("../assets/ui/undo.png"),
            Self::Redo => include_bytes!("../assets/ui/redo.png"),
            Self::Refresh => include_bytes!("../assets/ui/refresh.png"),
            Self::Help => include_bytes!("../assets/ui/help.png"),
        };
        png_icon(png, 24.0)
    }
}

pub fn png_icon(png: &'static [u8], size: f64) -> View {
    Image::new()
        .source_data(EncodedImage::from_static(png))
        .width(size)
        .height(size)
        .stretch(Stretch::Uniform)
        .into()
}

pub fn battery_icon(percent: Option<u8>) -> View {
    const LEVELS: [&[u8]; 5] = [
        include_bytes!("../assets/ui/battery-0.png"),
        include_bytes!("../assets/ui/battery-1.png"),
        include_bytes!("../assets/ui/battery-2.png"),
        include_bytes!("../assets/ui/battery-3.png"),
        include_bytes!("../assets/ui/battery-4.png"),
    ];
    let level = percent.unwrap_or_default().min(100).div_ceil(25);
    png_icon(LEVELS[usize::from(level)], STATUS_GLYPH)
}

pub fn label(text: impl Into<String>, size: f64, color: Color) -> TextBlock {
    TextBlock::new()
        .text(text)
        .font_size(size)
        .foreground(color)
}

pub fn icon(symbol: Symbol) -> View {
    SymbolIcon::new()
        .symbol(symbol)
        .width(20.0)
        .height(20.0)
        .into()
}

pub fn path(data: &str) -> View {
    // Keep path coordinates inside a full 24-DIP design surface; scale the
    // complete surface, rather than clipping a 20/24-DIP path to a 17-DIP box.
    Viewbox::new()
        .width(20.0)
        .height(20.0)
        .stretch(Stretch::Uniform)
        .slot(
            ViewboxSlot::Child,
            PathIcon::new().data(data).width(24.0).height(24.0),
        )
}

pub fn text_icon() -> View {
    path("F1 M2,2H18V6H16V4H11V17H14V19H6V17H9V4H4V6H2Z")
}

pub fn shape_icon() -> View {
    png_icon(include_bytes!("../assets/ui/shape.png"), 16.0)
}

pub fn copy_icon() -> View {
    png_icon(include_bytes!("../assets/ui/copy.png"), 16.0)
}

pub fn grid_icon() -> View {
    path(
        "F0 M1,1H19V19H1Z M3,3V6H6V3Z M8,3V6H12V3Z M14,3V6H17V3Z M3,8V12H6V8Z M8,8V12H12V8Z M14,8V12H17V8Z M3,14V17H6V14Z M8,14V17H12V14Z M14,14V17H17V14Z",
    )
}

pub fn fit_icon() -> View {
    path("F1 M1,1H7V3H3V7H1Z M13,1H19V7H17V3H13Z M1,13H3V17H7V19H1Z M17,13H19V19H13V17H17Z")
}

pub fn eye_icon() -> View {
    path(
        "F0 M0,10Q10,-3 20,10Q10,23 0,10Z M2,10Q10,0 18,10Q10,20 2,10Z M6,10A4,4 0 1 1 14,10A4,4 0 1 1 6,10Z M8,10A2,2 0 1 0 12,10A2,2 0 1 0 8,10Z",
    )
}

// The border owns the visible selection and corners. The transparent native
// button retains focus, keyboard activation, pointer feedback and automation.
pub struct StudioButton {
    surface: Border,
    button: Button,
}

pub fn styled_button(active: bool, primary: bool, bare: bool) -> StudioButton {
    let clear = Color::argb(0, 0, 0, 0);
    let (fill, stroke) = if primary {
        (RED, RED)
    } else if active {
        (RED_WASH, RED)
    } else if bare {
        (clear, clear)
    } else {
        (WHITE, Color::rgb(213, 218, 225))
    };
    let fg = if primary { WHITE } else { INK };
    let resources = ResourceOverrides::new()
        .set("ButtonBackground", clear)
        .set("ButtonBackgroundDisabled", clear)
        .set("ButtonBackgroundPointerOver", Color::argb(14, 0, 0, 0))
        .set("ButtonBackgroundPressed", Color::argb(28, 0, 0, 0))
        .set("ButtonForeground", fg)
        .set("ButtonForegroundPointerOver", fg)
        .set("ButtonForegroundPressed", fg)
        .set("ButtonBorderBrush", clear)
        .set("ButtonBorderBrushDisabled", clear)
        .set("ButtonBorderBrushPointerOver", clear)
        .set("ButtonBorderBrushPressed", clear);
    StudioButton {
        surface: Border::new()
            .height(34.0)
            .horizontal_alignment(HorizontalAlignment::Left)
            .vertical_alignment(VerticalAlignment::Center)
            .corner_radius(7.0)
            .border_thickness(1.0)
            .border_brush(stroke)
            .background(fill),
        button: Button::new()
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .vertical_alignment(VerticalAlignment::Stretch)
            .horizontal_content_alignment(HorizontalAlignment::Center)
            .vertical_content_alignment(VerticalAlignment::Center)
            .resource_overrides(resources),
    }
}

impl StudioButton {
    pub fn square(mut self, side: f64) -> Self {
        self.surface = self.surface.corner_radius(4.0).width(side).height(side);
        self
    }
    pub fn stepper(mut self) -> Self {
        self.surface = self
            .surface
            .width(18.0)
            .height(12.0)
            .background(SOFT)
            .corner_radius(1.0);
        self
    }
    /// Center the glyph independently of the native button's content padding.
    /// The transparent button on top handles the whole pointer and keyboard target.
    pub fn glyph(self, glyph: View, width: f64, height: f64) -> View {
        let image = Viewbox::new()
            .width(width)
            .height(height)
            .horizontal_alignment(HorizontalAlignment::Center)
            .vertical_alignment(VerticalAlignment::Center)
            .stretch(Stretch::Uniform)
            .slot(ViewboxSlot::Child, glyph);
        self.surface
            .content(Grid::new().children((image, self.button.min_width(0.0).min_height(0.0))))
    }
    pub fn height(mut self, height: f64) -> Self {
        self.surface = self.surface.height(height);
        self
    }
    pub fn grid_column(mut self, column: i32) -> Self {
        self.surface = self.surface.grid_column(column);
        self
    }
    pub fn horizontal_alignment(mut self, alignment: HorizontalAlignment) -> Self {
        self.surface = self.surface.horizontal_alignment(alignment);
        self
    }
    pub fn horizontal_content_alignment(mut self, alignment: HorizontalAlignment) -> Self {
        self.button = self.button.horizontal_content_alignment(alignment);
        self
    }
    #[allow(clippy::wrong_self_convention)] // Mirror the WinUI Button builder.
    pub fn is_enabled(mut self, enabled: bool) -> Self {
        self.button = self.button.is_enabled(enabled);
        self.surface = self.surface.opacity(if enabled { 1.0 } else { 0.65 });
        self
    }
    pub fn automation_name(mut self, name: impl Into<String>) -> Self {
        self.button = self.button.automation_name(name);
        self
    }
    pub fn on_click(mut self, callback: impl IntoUnitCallback) -> Self {
        self.button = self.button.on_click(callback);
        self
    }
    pub fn content(self, content: impl Into<View>) -> View {
        self.surface.content(self.button.content(content))
    }
}

pub fn button_content(glyph: View, text: &str) -> View {
    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(8.0)
        .vertical_alignment(VerticalAlignment::Center)
        .children((glyph, TextBlock::new().text(text).font_size(12.0)))
}

pub fn divider() -> View {
    Border::new()
        .height(1.0)
        .background(LINE)
        .margin(Thickness::xy(0.0, 10.0))
        .into()
}

pub fn vertical_divider() -> View {
    Border::new()
        .width(1.0)
        .height(23.0)
        .background(LINE)
        .margin(Thickness::xy(7.0, 0.0))
        .vertical_alignment(VerticalAlignment::Center)
        .into()
}

pub fn section(title: &str, glyph: View) -> View {
    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(8.0)
        .children((
            glyph,
            label(title, 12.0, INK).font_weight(FontWeight::SEMI_BOLD),
        ))
}
