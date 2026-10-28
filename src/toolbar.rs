//! Herramientas de la barra superior (estilo PrusaSlicer): deja activa una sola a la vez y
//! sus opciones van en el panel lateral. Las que actúan sobre un objeto se
//! desactivan sin selección; Medir y Regla siempre están disponibles.

use three_d::Key;
use three_d::egui;

use crate::i18n::tr;
use crate::{icons, theme};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Move,
    Rotate,
    Scale,
    Cut,
    Boolean,
    PlaceOnFace,
    Measure,
    Ruler,
}

impl Tool {
    pub const ALL: [Tool; 8] =
        [Tool::Move, Tool::Rotate, Tool::Scale, Tool::Cut, Tool::Boolean, Tool::PlaceOnFace, Tool::Measure, Tool::Ruler];

    /// Si la herramienta actúa sobre el objeto seleccionado.
    pub fn needs_object(self) -> bool {
        !matches!(self, Tool::Measure | Tool::Ruler)
    }

    pub fn label(self) -> &'static str {
        match self {
            Tool::Move => tr("Mover", "Move"),
            Tool::Rotate => tr("Rotar", "Rotate"),
            Tool::Scale => tr("Escalar", "Scale"),
            Tool::Cut => tr("Corte", "Cut"),
            Tool::Boolean => tr("Booleana", "Boolean"),
            Tool::PlaceOnFace => tr("Apoyar en cara", "Place on face"),
            Tool::Measure => tr("Medir", "Measure"),
            Tool::Ruler => tr("Regla", "Ruler"),
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Tool::Move => icons::MOVE,
            Tool::Rotate => icons::ROTATE,
            Tool::Scale => icons::SCALE,
            Tool::Cut => icons::CUT,
            Tool::Boolean => icons::BOOLEAN,
            Tool::PlaceOnFace => icons::PLACE_ON_FACE,
            Tool::Measure => icons::MEASURE,
            Tool::Ruler => icons::RULER,
        }
    }

    pub fn key(self) -> Key {
        match self {
            Tool::Move => Key::M,
            Tool::Rotate => Key::R,
            Tool::Scale => Key::S,
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
            Tool::Scale => "S",
            Tool::Cut => "C",
            Tool::Boolean => "B",
            Tool::PlaceOnFace => "F",
            Tool::Measure => "L",
            Tool::Ruler => "G",
        }
    }
}

/// Botones de las herramientas, uno por ícono; el nombre y el atajo van en el tooltip.
/// Pulsar la herramienta activa la desactiva.
pub fn tools(ui: &mut egui::Ui, has_selection: bool, active: &mut Option<Tool>) {
    for tool in Tool::ALL {
        if tool == Tool::Measure {
            ui.separator();
        }
        let selected = *active == Some(tool);
        let enabled = has_selection || !tool.needs_object();
        let response = theme::toggle(ui, enabled, selected, tool.icon(), &format!("{} ({})", tool.label(), tool.shortcut()))
            .on_disabled_hover_text(format!("{} · {}", tool.label(), tr("Selecciona un objeto", "Select an object")));
        if response.clicked() {
            *active = if selected { None } else { Some(tool) };
        }
    }
}
