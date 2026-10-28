//! Aspecto de la aplicación: paleta oscura neutra con un acento azul, Inter para el texto y
//! la fuente de íconos Phosphor. Igual que en paint1, pero solo con el tema oscuro, que
//! acompaña al fondo del visor 3D.

use three_d::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Response, RichText, Shadow, Stroke,
    TextStyle, Ui, Vec2,
};

/// Familia con los íconos rellenos (para lo que está activo).
pub const FILL: &str = "fill";
/// Familia con Inter SemiBold (títulos).
pub const STRONG: &str = "strong";

/// Tamaño de los íconos de las barras.
const ICON_SIZE: f32 = 18.0;

/// Colores de la interfaz.
pub struct Palette {
    /// Fondo de los paneles y barras.
    pub panel: Color32,
    /// Ventanas, campos de texto y fondos "hundidos".
    pub window: Color32,
    pub button: Color32,
    pub button_hover: Color32,
    pub button_active: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_soft: Color32,
    /// Acento: herramienta activa, selección, foco, acción principal.
    pub accent: Color32,
    pub accent_strong: Color32,
    pub accent_soft: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub error: Color32,
}

pub const DARK: Palette = Palette {
    panel: Color32::from_rgb(0x1f, 0x20, 0x24),
    window: Color32::from_rgb(0x28, 0x29, 0x2e),
    button: Color32::from_rgb(0x30, 0x31, 0x37),
    button_hover: Color32::from_rgb(0x3b, 0x3c, 0x44),
    button_active: Color32::from_rgb(0x4a, 0x4b, 0x55),
    border: Color32::from_rgb(0x3a, 0x3b, 0x43),
    text: Color32::from_rgb(0xe6, 0xe7, 0xeb),
    text_soft: Color32::from_rgb(0x9b, 0x9d, 0xa8),
    accent: Color32::from_rgb(0x4f, 0x86, 0xff),
    accent_strong: Color32::from_rgb(0x7c, 0xa5, 0xff),
    accent_soft: Color32::from_rgb(0x26, 0x36, 0x5e),
    success: Color32::from_rgb(0x5c, 0xc8, 0x7a),
    warning: Color32::from_rgb(0xe0, 0xa4, 0x2c),
    error: Color32::from_rgb(0xff, 0x6b, 0x6b),
};

/// Paleta en uso.
pub fn pal() -> &'static Palette {
    &DARK
}

pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, visuals(pal()));
    ctx.style_mut_of(egui::Theme::Dark, style);
}

fn style(s: &mut egui::Style) {
    s.text_styles = [
        (TextStyle::Small, FontId::proportional(11.0)),
        (TextStyle::Body, FontId::proportional(13.5)),
        (TextStyle::Button, FontId::proportional(13.5)),
        (TextStyle::Monospace, FontId::monospace(12.5)),
        (TextStyle::Heading, FontId::new(17.0, FontFamily::Name(STRONG.into()))),
    ]
    .into();
    let sp = &mut s.spacing;
    sp.item_spacing = Vec2::new(8.0, 6.0);
    sp.button_padding = Vec2::new(8.0, 4.0);
    sp.interact_size.y = 24.0;
    sp.window_margin = Margin::same(12);
    sp.menu_margin = Margin::same(6);
    sp.slider_width = 120.0;
    sp.indent = 16.0;
    s.interaction.tooltip_delay = 0.35;
}

fn fonts() -> FontDefinitions {
    let mut f = FontDefinitions::default();
    let data: [(&str, &'static [u8]); 4] = [
        ("inter", include_bytes!("../assets/fonts/Inter-Regular.ttf")),
        ("inter-semibold", include_bytes!("../assets/fonts/Inter-SemiBold.ttf")),
        ("phosphor", include_bytes!("../assets/fonts/Phosphor.ttf")),
        ("phosphor-fill", include_bytes!("../assets/fonts/Phosphor-Fill.ttf")),
    ];
    for (name, bytes) in data {
        f.font_data.insert(name.into(), FontData::from_static(bytes).into());
    }
    // Las fuentes propias de egui quedan de respaldo (símbolos que Inter no tiene).
    let fallback = f.families[&FontFamily::Proportional].clone();
    let family = |text: &str, icons: &str| {
        let mut v = vec![text.to_string(), icons.to_string()];
        v.extend(fallback.iter().cloned());
        v
    };
    f.families.insert(FontFamily::Proportional, family("inter", "phosphor"));
    f.families.insert(FontFamily::Name(FILL.into()), family("inter", "phosphor-fill"));
    f.families.insert(FontFamily::Name(STRONG.into()), family("inter-semibold", "phosphor"));
    if let Some(mono) = f.families.get_mut(&FontFamily::Monospace) {
        mono.insert(1.min(mono.len()), "phosphor".into());
    }
    f
}

fn visuals(p: &Palette) -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.override_text_color = None;
    v.panel_fill = p.panel;
    v.window_fill = p.window;
    v.window_stroke = Stroke::new(1.0_f32, p.border);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.window_shadow = Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(90) };
    v.popup_shadow = Shadow { offset: [0, 4], blur: 12, spread: 0, color: Color32::from_black_alpha(85) };
    v.faint_bg_color = p.button;
    v.extreme_bg_color = p.window;
    v.code_bg_color = p.button;
    v.hyperlink_color = p.accent_strong;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.error;
    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(1.0_f32, p.accent_strong);
    v.text_cursor.stroke = Stroke::new(2.0_f32, p.accent);
    v.slider_trailing_fill = true;
    v.interact_cursor = Some(egui::CursorIcon::PointingHand);

    let w = &mut v.widgets;
    let radius = CornerRadius::same(6);
    w.noninteractive.bg_fill = p.panel;
    w.noninteractive.weak_bg_fill = p.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.text);
    w.noninteractive.corner_radius = radius;
    for (state, fill) in
        [(&mut w.inactive, p.button), (&mut w.hovered, p.button_hover), (&mut w.active, p.button_active), (&mut w.open, p.button_hover)]
    {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.fg_stroke = Stroke::new(1.0_f32, p.text);
        state.corner_radius = radius;
        state.expansion = 0.0;
    }
    w.inactive.bg_stroke = Stroke::NONE;
    w.hovered.bg_stroke = Stroke::new(1.0_f32, p.border);
    w.active.bg_stroke = Stroke::new(1.0_f32, p.text_soft);
    w.open.bg_stroke = Stroke::new(1.0_f32, p.border);
    v
}

/// Texto de un ícono, relleno si `active`.
pub fn icon(icon: &str, active: bool) -> RichText {
    let family = if active { FontFamily::Name(FILL.into()) } else { FontFamily::Proportional };
    RichText::new(icon).family(family).size(ICON_SIZE)
}

/// Botón cuadrado solo con ícono; `help` va en el tooltip (nombre y atajo).
pub fn icon_button(ui: &mut Ui, ico: &str, help: &str) -> Response {
    icon_button_if(ui, true, ico, help)
}

/// Como [`icon_button`], desactivado si `!enabled`.
pub fn icon_button_if(ui: &mut Ui, enabled: bool, ico: &str, help: &str) -> Response {
    let b = egui::Button::new(icon(ico, false)).frame_when_inactive(false).min_size(Vec2::splat(30.0));
    ui.add_enabled(enabled, b).on_hover_text(help)
}

/// Botón de herramienta o interruptor: ícono relleno sobre el acento cuando está activo.
pub fn toggle(ui: &mut Ui, enabled: bool, active: bool, ico: &str, help: &str) -> Response {
    let p = pal();
    let text = icon(ico, active).color(if active { Color32::WHITE } else { p.text });
    let b = egui::Button::new(text).min_size(Vec2::splat(30.0));
    let b = if active { b.fill(p.accent) } else { b.frame_when_inactive(false) };
    ui.add_enabled(enabled, b).on_hover_text(help)
}

/// Botón de la acción principal (acento, texto blanco). Con `text` vacío queda solo el ícono.
pub fn primary_button(ui: &mut Ui, enabled: bool, ico: &str, text: &str) -> Response {
    let label = if text.is_empty() { ico.to_string() } else { format!("{ico}  {text}") };
    let b = egui::Button::new(RichText::new(label).color(Color32::WHITE).family(FontFamily::Name(STRONG.into())))
        .fill(pal().accent)
        .min_size(Vec2::new(0.0, 30.0));
    ui.add_enabled(enabled, b)
}

/// Botón secundario con ícono y texto.
pub fn button(ui: &mut Ui, enabled: bool, ico: &str, text: &str) -> Response {
    ui.add_enabled(enabled, egui::Button::new(format!("{ico}  {text}")))
}

/// Botón pequeño de ícono para las filas de una lista: marcado si `active`, tenue si no.
pub fn mini(ui: &mut Ui, active: bool, ico: &str, help: &str) -> Response {
    let p = pal();
    let color = if active { p.text } else { p.text_soft.gamma_multiply(0.7) };
    let text = RichText::new(ico).size(15.0).color(color);
    ui.add(egui::Button::new(text).frame_when_inactive(false).small()).on_hover_text(help)
}

/// Título de sección de un panel: pequeño, en mayúsculas y tenue.
pub fn section_title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text.to_uppercase()).size(11.0).family(FontFamily::Name(STRONG.into())).color(pal().text_soft));
}

/// Texto en la familia en negrita.
pub fn strong(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).family(FontFamily::Name(STRONG.into()))
}
