//! Íconos de Phosphor (<https://phosphoricons.com>) usados en la interfaz.
//!
//! La fuente va incrustada (ver `theme::install`) como respaldo de Inter, así que un ícono
//! se puede mezclar con texto: `format!("{OPEN} Abrir…")`. Regular y Fill comparten
//! códigos; el relleno se pide con la familia [`crate::theme::FILL`]. Códigos tomados de
//! `egui-phosphor` 0.14.

// Archivo y edición.
pub const OPEN: &str = "\u{E256}"; // folder-open
pub const EXPORT: &str = "\u{EAF0}"; // export
pub const UNDO: &str = "\u{E08A}"; // arrow-u-up-left
pub const REDO: &str = "\u{E08C}"; // arrow-u-up-right
pub const FIT: &str = "\u{E1D0}"; // corners-out
pub const MENU: &str = "\u{E2F0}"; // list
pub const MORE: &str = "\u{E1FE}"; // dots-three
pub const LANGUAGE: &str = "\u{E4A2}"; // translate
pub const OK: &str = "\u{E182}"; // check
pub const CANCEL: &str = "\u{E4F6}"; // x
pub const RESET: &str = "\u{E038}"; // arrow-counter-clockwise

// Objetos.
pub const PRIMITIVES: &str = "\u{EC5E}"; // shapes
pub const VISIBLE: &str = "\u{E220}"; // eye
pub const HIDDEN: &str = "\u{E224}"; // eye-slash
pub const RENAME: &str = "\u{E3B4}"; // pencil-simple
pub const CLONE: &str = "\u{E1CA}"; // copy
pub const ARRAY: &str = "\u{E464}"; // squares-four
pub const DELETE: &str = "\u{E4A6}"; // trash
pub const REPAIR: &str = "\u{E0B2}"; // bandaids
pub const BOX: &str = "\u{E1DA}"; // cube
pub const SPHERE: &str = "\u{EE66}"; // sphere
pub const CYLINDER: &str = "\u{E8FC}"; // cylinder
pub const CONE: &str = "\u{E4B0}"; // triangle
pub const TUBE: &str = "\u{E564}"; // disc
pub const ROUNDED_BOX: &str = "\u{E700}"; // pill

// Herramientas.
pub const MOVE: &str = "\u{E0A4}"; // arrows-out-cardinal
pub const ROTATE: &str = "\u{E036}"; // arrow-clockwise
pub const SCALE: &str = "\u{ED6E}"; // resize
pub const CUT: &str = "\u{E636}"; // knife
pub const BOOLEAN: &str = "\u{E87E}"; // unite
pub const PLACE_ON_FACE: &str = "\u{E506}"; // align-bottom
pub const MEASURE: &str = "\u{EB06}"; // arrows-horizontal
pub const RULER: &str = "\u{E6B8}"; // ruler
pub const UNION: &str = "\u{E87E}"; // unite
pub const DIFFERENCE: &str = "\u{EBD6}"; // subtract
pub const INTERSECTION: &str = "\u{E2D2}"; // intersect
pub const SECTION: &str = "\u{ED08}"; // file-svg

// Vista.
pub const PERSPECTIVE: &str = "\u{EBE6}"; // perspective
pub const ORTHOGRAPHIC: &str = "\u{E1DA}"; // cube
