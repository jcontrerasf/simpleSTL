//! Barra superior de herramientas (estilo PrusaSlicer): deja activa una sola herramienta a
//! la vez y sus opciones van en el panel lateral. Las que actúan sobre un objeto se
//! desactivan sin selección; Medir y Regla siempre están disponibles.

use three_d::Key;
use three_d::egui::{self, Rect};

use crate::i18n::tr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Move,
    Rotate,
    Cut,
    Boolean,
    PlaceOnFace,
    Measure,
    Ruler,
}

impl Tool {
    pub const ALL: [Tool; 7] =
        [Tool::Move, Tool::Rotate, Tool::Cut, Tool::Boolean, Tool::PlaceOnFace, Tool::Measure, Tool::Ruler];

    /// Si la herramienta actúa sobre el objeto seleccionado.
    pub fn needs_object(self) -> bool {
        !matches!(self, Tool::Measure | Tool::Ruler)
    }

    pub fn label(self) -> &'static str {
        match self {
            Tool::Move => tr("Mover", "Move"),
            Tool::Rotate => tr("Rotar", "Rotate"),
            Tool::Cut => tr("Corte", "Cut"),
            Tool::Boolean => tr("Booleana", "Boolean"),
            Tool::PlaceOnFace => tr("Apoyar en cara", "Place on face"),
            Tool::Measure => tr("Medir", "Measure"),
            Tool::Ruler => tr("Regla", "Ruler"),
        }
    }

    pub fn key(self) -> Key {
        match self {
            Tool::Move => Key::M,
            Tool::Rotate => Key::R,
            Tool::Cut => Key::C,
            Tool::Boolean => Key::B,
            Tool::PlaceOnFace => Key::F,
            Tool::Measure => Key::L,
            Tool::Ruler => Key::G,
        }
    }

    fn shortcut(self) -> &'static str {
        match self {
            Tool::Move => "M",
            Tool::Rotate => "R",
            Tool::Cut => "C",
            Tool::Boolean => "B",
            Tool::PlaceOnFace => "F",
            Tool::Measure => "L",
            Tool::Ruler => "G",
        }
    }
}

/// Dibuja la barra en la esquina superior izquierda del visor, sin pasar de `max_width`
/// (si no cabe, los botones pasan a una segunda fila). Pulsar la herramienta activa la
/// desactiva. Devuelve el área ocupada, para que no cuente como clic en la escena.
pub fn show(ctx: &egui::Context, view: Rect, max_width: f32, has_selection: bool, active: &mut Option<Tool>) -> Rect {
    egui::Area::new(egui::Id::new("toolbar"))
        .fixed_pos(view.min + egui::vec2(8.0, 8.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(max_width);
                ui.horizontal_wrapped(|ui| {
                    for tool in Tool::ALL {
                        if tool == Tool::Measure {
                            ui.separator();
                        }
                        let selected = *active == Some(tool);
                        let enabled = has_selection || !tool.needs_object();
                        let response = ui
                            .add_enabled(enabled, egui::Button::selectable(selected, tool.label()))
                            .on_hover_text(format!("{} ({})", tool.label(), tool.shortcut()))
                            .on_disabled_hover_text(tr("Selecciona un objeto", "Select an object"));
                        if response.clicked() {
                            *active = if selected { None } else { Some(tool) };
                        }
                    }
                });
            });
        })
        .response
        .rect
}
