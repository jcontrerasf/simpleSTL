mod camera;
mod csg;
mod ground;
mod manipulator;
mod mesh;
mod viewcube;

use std::path::{Path, PathBuf};
use std::time::Instant;

use three_d::egui;
use three_d::*;

use camera::CameraController;
use csg::BooleanOp;
use ground::Ground;
use manipulator::{Manipulator, Mode, Pose};
use mesh::{MeshData, Topology};

const PALETTE: [[u8; 3]; 6] = [
    [120, 160, 220],
    [220, 140, 90],
    [120, 190, 120],
    [200, 120, 180],
    [210, 190, 100],
    [110, 190, 200],
];

struct SceneObject {
    name: String,
    /// Malla en coordenadas locales, centrada en el origen; `pose` la ubica en el mundo.
    mesh: MeshData,
    pose: Pose,
    /// Caja envolvente en el mundo, recalculada al cambiar la pose.
    world_bbox: (Vec3, Vec3),
    topology: Topology,
    model: Gm<Mesh, PhysicalMaterial>,
    color: [u8; 3],
    visible: bool,
}

impl SceneObject {
    /// `mesh` viene en coordenadas del mundo; se centra y el desplazamiento pasa a la pose.
    fn new(context: &Context, name: String, mesh: MeshData, color: [u8; 3]) -> Self {
        let (min, max) = mesh.bounding_box();
        let center = (min + max) * 0.5;
        let mesh = mesh.transformed(|p| p - center);
        let material = PhysicalMaterial::new_opaque(
            context,
            &CpuMaterial {
                albedo: Srgba::new_opaque(color[0], color[1], color[2]),
                roughness: 0.7,
                metallic: 0.0,
                ..Default::default()
            },
        );
        let model = Gm::new(Mesh::new(context, &mesh.to_cpu_mesh()), material);
        let mut obj = Self {
            name,
            topology: mesh.topology(),
            mesh,
            pose: Pose::at(center),
            world_bbox: (min, max),
            model,
            color,
            visible: true,
        };
        obj.set_pose(obj.pose);
        obj
    }

    fn set_pose(&mut self, pose: Pose) {
        self.pose = pose;
        self.model.set_transformation(pose.matrix());
        self.world_bbox = self.world_mesh().bounding_box();
    }

    /// Malla en coordenadas del mundo: lo que usan booleanas, cortes y exportación.
    fn world_mesh(&self) -> MeshData {
        self.mesh.transformed(|p| self.pose.apply(p))
    }

    fn set_color(&mut self, color: [u8; 3]) {
        self.color = color;
        self.model.material.albedo = Srgba::new_opaque(color[0], color[1], color[2]);
    }
}

const AXES: [&str; 3] = ["X", "Y", "Z"];

struct App {
    context: Context,
    objects: Vec<SceneObject>,
    selected: Option<usize>,
    status: String,
    bool_a: usize,
    bool_b: usize,
    bool_op: BooleanOp,
    cut_axis: usize,
    /// Posición del plano de corte como fracción (0..1) de la caja del objeto seleccionado.
    cut_fraction: f32,
    show_cut_plane: bool,
    manipulator_mode: Mode,
    grid_spacing: f32,
    /// Objeto que se está renombrando en la lista, con el texto en edición.
    renaming: Option<(usize, String)>,
    /// El campo de renombrar debe tomar el foco en el próximo cuadro (solo al abrirse:
    /// pedirlo siempre impediría que Enter lo suelte y confirme).
    rename_needs_focus: bool,
}

/// Plano de corte ya resuelto en coordenadas del mundo.
struct CutPlane {
    object: usize,
    normal: [f64; 3],
    offset: f64,
    /// Transformación del cuadrado unitario de `CpuMesh::square()` para la vista previa.
    transform: Mat4,
}

impl App {
    fn add_object(&mut self, name: String, mesh: MeshData) {
        let color = PALETTE[self.objects.len() % PALETTE.len()];
        self.objects.push(SceneObject::new(&self.context, name, mesh, color));
        self.selected = Some(self.objects.len() - 1);
    }

    fn load(&mut self, path: &Path) {
        match MeshData::load_stl(path) {
            Ok(mesh) => {
                let name = path.file_name().map_or("sin nombre".into(), |n| n.to_string_lossy().into_owned());
                self.status = format!("Cargado {name} ({} triángulos)", mesh.triangles.len());
                self.add_object(name, mesh);
            }
            Err(e) => self.status = format!("Error en {}: {e}", path.display()),
        }
    }

    fn cut_plane(&self) -> Option<CutPlane> {
        let object = self.selected?;
        let (min, max) = self.objects.get(object)?.world_bbox;
        let axis = self.cut_axis;
        let offset = min[axis] + self.cut_fraction * (max[axis] - min[axis]);

        let mut normal = [0.0; 3];
        normal[axis] = 1.0;
        let mut center = (min + max) * 0.5;
        center[axis] = offset;
        // El cuadrado base está en el plano XY (normal +Z); se rota para que su normal sea el eje elegido.
        let rotation = match axis {
            0 => Mat4::from_angle_y(degrees(90.0)),
            1 => Mat4::from_angle_x(degrees(-90.0)),
            _ => Mat4::identity(),
        };
        let size = (max - min).magnitude() * 0.6;
        let transform = Mat4::from_translation(center) * rotation * Mat4::from_scale(size);
        Some(CutPlane { object, normal, offset: offset as f64, transform })
    }

    fn apply_boolean(&mut self) {
        let (Some(a), Some(b)) = (self.objects.get(self.bool_a), self.objects.get(self.bool_b)) else {
            return;
        };
        let op = self.bool_op;
        let name = format!("{} {} {}", a.name, op.symbol(), b.name);
        let start = Instant::now();
        match csg::boolean(&a.world_mesh(), &b.world_mesh(), op) {
            Ok(Some(mesh)) => {
                self.status = format!("{} en {:.0?} ({} triángulos)", op.label(), start.elapsed(), mesh.triangles.len());
                self.objects[self.bool_a].visible = false;
                self.objects[self.bool_b].visible = false;
                self.add_object(name, mesh);
            }
            Ok(None) => self.status = format!("{}: el resultado es vacío", op.label()),
            Err(e) => self.status = e,
        }
    }

    fn apply_cut(&mut self, plane: CutPlane) {
        let obj = &self.objects[plane.object];
        let axis = AXES[self.cut_axis];
        let base = obj.name.clone();
        let start = Instant::now();
        match csg::split(&obj.world_mesh(), plane.normal, plane.offset) {
            Ok((positive, negative)) => {
                if positive.is_none() || negative.is_none() {
                    self.status = "El plano no atraviesa el objeto".into();
                    return;
                }
                self.objects[plane.object].visible = false;
                for (half, sign) in [(positive, "+"), (negative, "−")] {
                    self.add_object(format!("{base} ({sign}{axis})"), half.unwrap());
                }
                self.status = format!("Corte en {axis} = {:.2} en {:.0?}", plane.offset, start.elapsed());
            }
            Err(e) => self.status = e,
        }
    }

    /// Caja envolvente de los objetos visibles.
    fn bounding_box(&self) -> Option<(Vec3, Vec3)> {
        self.objects
            .iter()
            .filter(|o| o.visible)
            .map(|o| o.world_bbox)
            .reduce(|(amin, amax), (bmin, bmax)| {
                (
                    vec3(amin.x.min(bmin.x), amin.y.min(bmin.y), amin.z.min(bmin.z)),
                    vec3(amax.x.max(bmax.x), amax.y.max(bmax.y), amax.z.max(bmax.z)),
                )
            })
    }
}

/// Acciones pedidas desde la UI; se ejecutan fuera del closure de egui.
#[derive(Default)]
struct UiActions {
    open: bool,
    fit: bool,
    delete: Option<usize>,
    export: Option<usize>,
    pose: Option<(usize, Pose)>,
    boolean: bool,
    cut: bool,
}

fn main() {
    let window = Window::new(WindowSettings {
        title: "simpleSTL".to_string(),
        min_size: (640, 480),
        initial_size: Some((1280, 800)),
        ..Default::default()
    })
    .unwrap();
    let context = window.gl();

    let mut control = CameraController::new(vec3(0.0, 0.0, 0.0));
    let mut camera = control.new_camera(window.viewport());
    let mut gui = GUI::new(&context);

    let ambient = AmbientLight::new(&context, 0.35, Srgba::WHITE);
    let key = DirectionalLight::new(&context, 2.0, Srgba::WHITE, vec3(-1.0, 1.0, -2.0));
    let fill = DirectionalLight::new(&context, 0.8, Srgba::WHITE, vec3(1.0, -0.5, 1.0));
    let mut axes = Axes::new(&context, 0.01, 1.0);
    let mut ground = Ground::new(&context, vec3(0.0, 0.0, 0.0), 100.0);
    let mut cut_preview = Gm::new(
        Mesh::new(&context, &CpuMesh::square()),
        ColorMaterial::new_transparent(
            &context,
            &CpuMaterial { albedo: Srgba::new(255, 200, 60, 80), ..Default::default() },
        ),
    );

    let mut app = App {
        context: context.clone(),
        objects: Vec::new(),
        selected: None,
        status: String::new(),
        bool_a: 0,
        bool_b: 1,
        bool_op: BooleanOp::Difference,
        cut_axis: 2,
        cut_fraction: 0.5,
        show_cut_plane: true,
        manipulator_mode: Mode::Both,
        grid_spacing: ground.spacing,
        renaming: None,
        rename_needs_focus: false,
    };
    for arg in std::env::args_os().skip(1) {
        app.load(&PathBuf::from(arg));
    }
    let mut needs_fit = !app.objects.is_empty();
    // egui necesita un par de cuadros extra para asentar su layout tras cada evento.
    let mut extra_frames: u32 = 3;
    let mut manipulator = Manipulator::new();
    let mut panel_width = 0.0f32;

    window.render_loop(move |mut frame_input| {
        if !frame_input.events.is_empty() {
            extra_frames = 3;
        }
        let mut actions = UiActions::default();
        let window_viewport = frame_input.viewport;
        let dpr = frame_input.device_pixel_ratio;

        // El visor 3D ocupa lo que deja libre el panel lateral (ancho del cuadro anterior).
        let panel_px = (panel_width * dpr) as u32;
        let viewport = Viewport {
            x: panel_px as i32,
            y: 0,
            width: window_viewport.width.saturating_sub(panel_px).max(1),
            height: window_viewport.height,
        };
        camera.set_viewport(viewport);

        manipulator.mode = app.manipulator_mode;
        manipulator.track_input(&frame_input.events, dpr, window_viewport.height);
        let mut dragged_pose = None;
        let mut cube_action = None;
        let mut cube_rect = egui::Rect::NOTHING;

        gui.update(
            &mut frame_input.events,
            frame_input.accumulated_time,
            window_viewport,
            dpr,
            |ui| {
                panel_width = egui::Panel::left("panel")
                    // Ancho fijo: si se ajusta al contenido, el visor cambia de tamaño entre cuadros.
                    .exact_size(300.0)
                    .show_inside(ui, |ui| side_panel(ui, &mut app, &mut actions))
                    .response
                    .rect
                    .width();
                let view_rect = egui::Rect::from_min_max(
                    egui::pos2(panel_width, 0.0),
                    egui::pos2(window_viewport.width as f32 / dpr, window_viewport.height as f32 / dpr),
                );
                let pose = app.selected.and_then(|i| app.objects.get(i)).filter(|o| o.visible).map(|o| o.pose);
                cube_rect = viewcube::rect(view_rect);
                cube_action = viewcube::show(ui.ctx(), cube_rect, &camera);
                dragged_pose = manipulator.update(ui.ctx(), &camera, view_rect, cube_rect, pose);
            },
        );

        // Animaciones de egui (menús, tooltips, cursor de texto): seguir dibujando mientras las pida.
        // (2 y no 1: al final del cuadro se descuenta uno.)
        if gui.context().requested_repaint_last_pass() || gui.context().has_requested_repaint() {
            extra_frames = extra_frames.max(2);
        }
        if let (Some(pose), Some(i)) = (dragged_pose, app.selected) {
            app.objects[i].set_pose(pose);
        }
        if let Some((i, pose)) = actions.pose {
            app.objects[i].set_pose(pose);
        }
        match cube_action {
            Some(viewcube::Action::LookFrom(direction)) => control.look_from(direction),
            Some(viewcube::Action::Orbit(delta)) => control.orbit_by_drag(&mut camera, delta),
            None => {}
        }
        if let Some(click) = manipulator.consume_events(&mut frame_input.events) {
            let p = click.position;
            let in_cube = cube_rect.contains(egui::pos2(p.x / dpr, (window_viewport.height as f32 - p.y) / dpr));
            if p.x >= viewport.x as f32 && !in_cube {
                // Selección con el ratón: el índice de geometría es la posición en `visible`.
                let visible: Vec<usize> = (0..app.objects.len()).filter(|&i| app.objects[i].visible).collect();
                let geometries = visible.iter().map(|&i| &app.objects[i].model.geometry);
                if let Ok(hit) = pick(&context, &camera, click.position, geometries, Cull::None) {
                    app.selected = hit.map(|h| visible[h.geometry_id as usize]);
                }
            }
        }

        if actions.open {
            if let Some(paths) = rfd::FileDialog::new()
                .set_title("Abrir STL")
                .add_filter("STL", &["stl", "STL"])
                .pick_files()
            {
                let before = app.objects.len();
                for path in paths {
                    app.load(&path);
                }
                needs_fit |= before == 0 && !app.objects.is_empty();
            }
        }
        if let Some(i) = actions.delete {
            let removed = app.objects.remove(i);
            app.status = format!("Eliminado {}", removed.name);
            // Los índices posteriores se corren en uno.
            app.selected = match app.selected {
                Some(s) if s == i => None,
                Some(s) if s > i => Some(s - 1),
                s => s,
            };
            app.renaming = None;
        }
        if let Some(obj) = actions.export.and_then(|i| app.objects.get(i)) {
            let default_name = format!("{}.stl", obj.name.trim_end_matches(".stl"));
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Exportar STL")
                .add_filter("STL", &["stl"])
                .set_file_name(default_name)
                .save_file()
            {
                app.status = match obj.world_mesh().save_stl(&path) {
                    Ok(()) => format!("Exportado {}", path.display()),
                    Err(e) => format!("Error al exportar: {e}"),
                };
            }
        }
        if actions.boolean {
            app.apply_boolean();
        }
        let cut_plane = app.cut_plane();
        if actions.cut {
            if let Some(plane) = cut_plane {
                app.apply_cut(plane);
            }
        }
        let cut_plane = app.cut_plane().filter(|_| app.show_cut_plane);

        // El encuadre inicial espera a que el panel tenga su ancho real (no existe en el primer cuadro).
        if actions.fit || (needs_fit && viewport.x > 0) {
            if let Some((min, max)) = app.bounding_box() {
                control.fit(&mut camera, min, max);
                let size = (max - min).magnitude();
                axes = Axes::new(&context, size * 0.004, size * 0.3);
                ground = Ground::new(&context, (min + max) * 0.5, (max.x - min.x).max(max.y - min.y) * 1.6);
                app.grid_spacing = ground.spacing;
            }
            needs_fit = false;
        }
        control.handle_events(&mut camera, &mut frame_input.events);
        if control.animate(&mut camera, frame_input.accumulated_time) {
            extra_frames = extra_frames.max(2);
        }

        let mut scene: Vec<&dyn Object> = app.objects.iter().filter(|o| o.visible).map(|o| &o.model as &dyn Object).collect();
        scene.push(&axes);
        scene.extend(ground.objects());
        if let Some(plane) = &cut_plane {
            cut_preview.set_transformation(plane.transform);
            scene.push(&cut_preview);
        }

        frame_input
            .screen()
            .clear(ClearState::color_and_depth(0.16, 0.17, 0.19, 1.0, 1.0))
            .render(&camera, scene, &[&ambient, &key, &fill])
            .write(|| gui.render())
            .unwrap();

        extra_frames = extra_frames.saturating_sub(1);
        FrameOutput { wait_next_event: extra_frames == 0, ..Default::default() }
    });
}

fn boolean_panel(ui: &mut egui::Ui, app: &mut App, actions: &mut UiActions) {
    ui.separator();
    ui.label(egui::RichText::new("Booleanas").strong());
    if app.objects.len() < 2 {
        ui.weak("Se necesitan dos objetos");
        return;
    }
    app.bool_a = app.bool_a.min(app.objects.len() - 1);
    app.bool_b = app.bool_b.min(app.objects.len() - 1);

    for (label, index) in [("A", &mut app.bool_a), ("B", &mut app.bool_b)] {
        ui.horizontal(|ui| {
            ui.label(label);
            egui::ComboBox::from_id_salt(label)
                .selected_text(&app.objects[*index].name)
                .width(200.0)
                .show_ui(ui, |ui| {
                    for (i, obj) in app.objects.iter().enumerate() {
                        ui.selectable_value(index, i, &obj.name);
                    }
                });
        });
    }
    ui.horizontal_wrapped(|ui| {
        for op in BooleanOp::ALL {
            let text = if op == BooleanOp::Difference { "Resta A − B" } else { op.label() };
            ui.radio_value(&mut app.bool_op, op, text);
        }
    });

    let (a, b) = (&app.objects[app.bool_a], &app.objects[app.bool_b]);
    let problem = if app.bool_a == app.bool_b {
        Some("A y B deben ser objetos distintos")
    } else if !a.topology.is_closed() || !b.topology.is_closed() {
        Some("Ambas mallas deben ser cerradas")
    } else {
        None
    };
    let button = ui.add_enabled(problem.is_none(), egui::Button::new("Aplicar"));
    actions.boolean = button.on_disabled_hover_text(problem.unwrap_or_default()).clicked();
}

fn side_panel(ui: &mut egui::Ui, app: &mut App, actions: &mut UiActions) {
    egui::Panel::bottom("status").show_inside(ui, |ui| {
        if !app.status.is_empty() {
            ui.label(&app.status);
        }
        ui.weak(format!("Base z = 0 · grilla cada {}", app.grid_spacing));
        ui.weak("Clic: seleccionar · Izq: orbitar · Der/Medio: desplazar · Rueda: zoom");
    });
    egui::ScrollArea::vertical().show(ui, |ui| panel_contents(ui, app, actions));
}

#[derive(Clone, Copy)]
enum ObjectAction {
    Rename,
    Export,
    Delete,
}

/// Menú de un objeto de la lista (botón "…" o clic derecho sobre el nombre).
fn object_menu(ui: &mut egui::Ui) -> Option<ObjectAction> {
    let mut action = None;
    if ui.button("Renombrar").clicked() {
        action = Some(ObjectAction::Rename);
    }
    if ui.button("Exportar STL…").clicked() {
        action = Some(ObjectAction::Export);
    }
    if ui.button("Eliminar").clicked() {
        action = Some(ObjectAction::Delete);
    }
    if action.is_some() {
        ui.close();
    }
    action
}

/// Botón con forma de ojo para mostrar/ocultar; tachado cuando el objeto está oculto.
/// Se dibuja a mano porque la fuente por defecto de egui no garantiza el emoji.
fn eye_toggle(ui: &mut egui::Ui, visible: &mut bool) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(egui::vec2(20.0, 16.0), egui::Sense::click());
    if response.clicked() {
        *visible = !*visible;
        response.mark_changed();
    }
    let visuals = ui.visuals();
    let color = if response.hovered() {
        visuals.strong_text_color()
    } else if *visible {
        visuals.text_color()
    } else {
        visuals.weak_text_color()
    };
    let (c, half_w, half_h, n) = (rect.center(), 8.0, 4.5, 12);
    let curve = |k: usize, sign: f32| {
        let t = k as f32 / n as f32;
        egui::pos2(c.x - half_w + 2.0 * half_w * t, c.y + sign * half_h * (std::f32::consts::PI * t).sin())
    };
    let mut outline: Vec<egui::Pos2> = (0..=n).map(|k| curve(k, -1.0)).collect();
    outline.extend((1..n).rev().map(|k| curve(k, 1.0)));
    let painter = ui.painter();
    painter.add(egui::Shape::closed_line(outline, egui::Stroke::new(1.3_f32, color)));
    if *visible {
        painter.circle_filled(c, 2.3, color);
    } else {
        painter.line_segment([c + egui::vec2(-7.0, 6.0), c + egui::vec2(7.0, -6.0)], egui::Stroke::new(1.5_f32, color));
    }
    response.on_hover_text(if *visible { "Ocultar" } else { "Mostrar" })
}

fn panel_contents(ui: &mut egui::Ui, app: &mut App, actions: &mut UiActions) {
    ui.heading("simpleSTL");
    ui.horizontal(|ui| {
        actions.open = ui.button("Abrir STL…").clicked();
        actions.fit = ui.button("Encuadrar").clicked();
    });
    ui.separator();

    ui.label(egui::RichText::new("Objetos").strong());
    if app.objects.is_empty() {
        ui.weak("Ninguno cargado");
    }
    for (i, obj) in app.objects.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            eye_toggle(ui, &mut obj.visible);
            let mut color = obj.color;
            if ui.color_edit_button_srgb(&mut color).changed() {
                obj.set_color(color);
            }

            if let Some((_, text)) = app.renaming.as_mut().filter(|(r, _)| *r == i) {
                let edit = ui.add(egui::TextEdit::singleline(text).desired_width(ui.available_width() - 8.0));
                if std::mem::take(&mut app.rename_needs_focus) {
                    edit.request_focus();
                }
                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    app.renaming = None;
                } else if edit.lost_focus() {
                    // Enter o clic afuera confirman; un nombre vacío deja el anterior.
                    let name = text.trim();
                    if !name.is_empty() {
                        obj.name = name.to_string();
                    }
                    app.renaming = None;
                }
                return;
            }

            let mut menu_action = None;
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("…", |ui| menu_action = object_menu(ui));
                let label = ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.selectable_label(app.selected == Some(i), &obj.name)
                });
                let label = label.inner;
                if label.clicked() {
                    app.selected = Some(i);
                }
                if label.double_clicked() {
                    menu_action = Some(ObjectAction::Rename);
                }
                label.context_menu(|ui| menu_action = object_menu(ui).or(menu_action));
            });
            match menu_action {
                Some(ObjectAction::Rename) => {
                    app.renaming = Some((i, obj.name.clone()));
                    app.rename_needs_focus = true;
                }
                Some(ObjectAction::Export) => actions.export = Some(i),
                Some(ObjectAction::Delete) => actions.delete = Some(i),
                None => {}
            }
        });
    }

    if let Some(obj) = app.selected.and_then(|i| app.objects.get(i)) {
        ui.separator();
        ui.label(egui::RichText::new(&obj.name).strong());
        let (local_min, local_max) = obj.mesh.bounding_box();
        let size = local_max - local_min;
        egui::Grid::new("info").num_columns(2).show(ui, |ui| {
            ui.label("Triángulos");
            ui.label(obj.mesh.triangles.len().to_string());
            ui.end_row();
            ui.label("Vértices");
            ui.label(obj.mesh.vertices.len().to_string());
            ui.end_row();
            ui.label("Tamaño");
            ui.label(format!("{:.2} × {:.2} × {:.2}", size.x, size.y, size.z));
            ui.end_row();
            ui.label("Cerrada");
            if obj.topology.is_closed() {
                ui.colored_label(egui::Color32::LIGHT_GREEN, "sí");
            } else {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    format!("no ({} bordes, {} defectuosas)", obj.topology.boundary_edges, obj.topology.bad_edges),
                );
            }
            ui.end_row();
            if obj.topology.is_closed() {
                ui.label("Volumen");
                ui.label(format!("{:.2}", obj.mesh.volume().abs()));
                ui.end_row();
            }
        });

        let mut pose = obj.pose;
        let mut pose_changed = false;
        ui.horizontal(|ui| {
            ui.label("Posición");
            for c in [&mut pose.translation.x, &mut pose.translation.y, &mut pose.translation.z] {
                pose_changed |= ui.add(egui::DragValue::new(c).speed(0.1).max_decimals(2)).changed();
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Manipulador");
            for mode in Mode::ALL {
                ui.radio_value(&mut app.manipulator_mode, mode, mode.label());
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Restablecer rotación").clicked() {
                pose.rotation = Pose::at(pose.translation).rotation;
                pose_changed = true;
            }
        });
        if pose_changed {
            actions.pose = app.selected.map(|i| (i, pose));
        }
        ui.weak("Ctrl al arrastrar: pasos de 1 / 15°");

        let closed = obj.topology.is_closed();

        ui.separator();
        ui.label(egui::RichText::new("Corte por plano").strong());
        ui.horizontal(|ui| {
            ui.label("Normal");
            for (i, name) in AXES.iter().enumerate() {
                ui.radio_value(&mut app.cut_axis, i, *name);
            }
        });
        let (min, max) = obj.world_bbox;
        let axis = app.cut_axis;
        let position = min[axis] + app.cut_fraction * (max[axis] - min[axis]);
        ui.add(
            egui::Slider::new(&mut app.cut_fraction, 0.0..=1.0)
                .show_value(false)
                .text(format!("{} = {position:.2}", AXES[axis])),
        );
        ui.checkbox(&mut app.show_cut_plane, "Mostrar plano");
        let button = ui.add_enabled(closed, egui::Button::new("Cortar"));
        actions.cut = button.on_disabled_hover_text("La malla debe ser cerrada").clicked();
    }

    boolean_panel(ui, app, actions);
}
