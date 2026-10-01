//! Barra superior de herramientas (estilo PrusaSlicer): aparece con un objeto seleccionado
//! y deja activa una sola herramienta a la vez. Sus opciones van en el panel lateral.

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
}

impl Tool {
    pub const ALL: [Tool; 5] = [Tool::Move, Tool::Rotate, Tool::Cut, Tool::Boolean, Tool::PlaceOnFace];

    pub fn label(self) -> &'static str {
        match self {
            Tool::Move => tr("Mover", "Move"),
            Tool::Rotate => tr("Rotar", "Rotate"),
            Tool::Cut => tr("Corte", "Cut"),
            Tool::Boolean => tr("Booleana", "Boolean"),
            Tool::PlaceOnFace => tr("Apoyar en cara", "Place on face"),
        }
    }

    pub fn key(self) -> Key {
        match self {
            Tool::Move => Key::M,
            Tool::Rotate => Key::R,
            Tool::Cut => Key::C,
            Tool::Boolean => Key::B,
            Tool::PlaceOnFace => Key::F,
        }
    }

    fn shortcut(self) -> &'static str {
        match self {
            Tool::Move => "M",
            Tool::Rotate => "R",
            Tool::Cut => "C",
            Tool::Boolean => "B",
            Tool::PlaceOnFace => "F",
        }
    }
}

/// Dibuja la barra en la esquina superior izquierda del visor, sin pasar de `max_width`
/// (si no cabe, los botones pasan a una segunda fila). Pulsar la herramienta activa la
/// desactiva. Devuelve el área ocupada, para que no cuente como clic en la escena.
pub fn show(ctx: &egui::Context, view: Rect, max_width: f32, active: &mut Option<Tool>) -> Rect {
    egui::Area::new(egui::Id::new("toolbar"))
        .fixed_pos(view.min + egui::vec2(8.0, 8.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(max_width);
                ui.horizontal_wrapped(|ui| {
                    for tool in Tool::ALL {
                        let selected = *active == Some(tool);
                        let response = ui
                            .selectable_label(selected, tool.label())
                            .on_hover_text(format!("{} ({})", tool.label(), tool.shortcut()));
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
