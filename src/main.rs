mod camera;
mod mesh;

use std::path::{Path, PathBuf};

use three_d::egui;
use three_d::*;

use camera::CameraController;
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
    mesh: MeshData,
    topology: Topology,
    model: Gm<Mesh, PhysicalMaterial>,
    color: [u8; 3],
    visible: bool,
}

impl SceneObject {
    fn new(context: &Context, name: String, mesh: MeshData, color: [u8; 3]) -> Self {
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
        Self { name, topology: mesh.topology(), mesh, model, color, visible: true }
    }

    fn set_color(&mut self, color: [u8; 3]) {
        self.color = color;
        self.model.material.albedo = Srgba::new_opaque(color[0], color[1], color[2]);
    }
}

struct App {
    context: Context,
    objects: Vec<SceneObject>,
    selected: Option<usize>,
    status: String,
}

impl App {
    fn load(&mut self, path: &Path) {
        match MeshData::load_stl(path) {
            Ok(mesh) => {
                let name = path.file_name().map_or("sin nombre".into(), |n| n.to_string_lossy().into_owned());
                let color = PALETTE[self.objects.len() % PALETTE.len()];
                self.status = format!("Cargado {name} ({} triángulos)", mesh.triangles.len());
                self.objects.push(SceneObject::new(&self.context, name, mesh, color));
                self.selected = Some(self.objects.len() - 1);
            }
            Err(e) => self.status = format!("Error en {}: {e}", path.display()),
        }
    }

    /// Caja envolvente de los objetos visibles.
    fn bounding_box(&self) -> Option<(Vec3, Vec3)> {
        self.objects
            .iter()
            .filter(|o| o.visible)
            .map(|o| o.mesh.bounding_box())
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
}

fn main() {
    let window = Window::new(WindowSettings {
        title: "simpleSTL".to_string(),
        min_size: (640, 480),
        ..Default::default()
    })
    .unwrap();
    let context = window.gl();

    let mut camera = CameraController::new_camera(window.viewport());
    let mut control = CameraController::new(vec3(0.0, 0.0, 0.0));
    let mut gui = GUI::new(&context);

    let ambient = AmbientLight::new(&context, 0.35, Srgba::WHITE);
    let key = DirectionalLight::new(&context, 2.0, Srgba::WHITE, vec3(-1.0, 1.0, -2.0));
    let fill = DirectionalLight::new(&context, 0.8, Srgba::WHITE, vec3(1.0, -0.5, 1.0));
    let mut axes = Axes::new(&context, 0.01, 1.0);

    let mut app = App { context: context.clone(), objects: Vec::new(), selected: None, status: String::new() };
    for arg in std::env::args_os().skip(1) {
        app.load(&PathBuf::from(arg));
    }
    let mut needs_fit = !app.objects.is_empty();
    // egui necesita un par de cuadros extra para asentar su layout tras cada evento.
    let mut extra_frames: u32 = 3;

    window.render_loop(move |mut frame_input| {
        if !frame_input.events.is_empty() {
            extra_frames = 3;
        }
        let mut actions = UiActions::default();
        let mut panel_width = 0.0;

        gui.update(
            &mut frame_input.events,
            frame_input.accumulated_time,
            frame_input.viewport,
            frame_input.device_pixel_ratio,
            |ui| {
                panel_width = egui::Panel::left("panel")
                    .default_size(260.0)
                    .show_inside(ui, |ui| side_panel(ui, &mut app, &mut actions))
                    .response
                    .rect
                    .width();
            },
        );

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
            app.selected = None;
        }

        // El visor 3D ocupa lo que deja libre el panel lateral.
        let panel_px = (panel_width * frame_input.device_pixel_ratio) as u32;
        let viewport = Viewport {
            x: panel_px as i32,
            y: 0,
            width: frame_input.viewport.width.saturating_sub(panel_px).max(1),
            height: frame_input.viewport.height,
        };
        camera.set_viewport(viewport);
        if actions.fit || needs_fit {
            if let Some((min, max)) = app.bounding_box() {
                control.fit(&mut camera, min, max);
                let size = (max - min).magnitude();
                axes = Axes::new(&context, size * 0.004, size * 0.3);
            }
            needs_fit = false;
        }
        control.handle_events(&mut camera, &mut frame_input.events);

        let mut scene: Vec<&dyn Object> = app.objects.iter().filter(|o| o.visible).map(|o| &o.model as &dyn Object).collect();
        scene.push(&axes);

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

fn side_panel(ui: &mut egui::Ui, app: &mut App, actions: &mut UiActions) {
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
            ui.checkbox(&mut obj.visible, "");
            let mut color = obj.color;
            if ui.color_edit_button_srgb(&mut color).changed() {
                obj.set_color(color);
            }
            if ui.selectable_label(app.selected == Some(i), &obj.name).clicked() {
                app.selected = Some(i);
            }
        });
    }

    if let Some(obj) = app.selected.and_then(|i| app.objects.get(i)) {
        ui.separator();
        ui.label(egui::RichText::new(&obj.name).strong());
        let (min, max) = obj.mesh.bounding_box();
        let size = max - min;
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
        if ui.button("Eliminar").clicked() {
            actions.delete = app.selected;
        }
    }

    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        ui.weak("Izq: orbitar · Der/Medio: desplazar · Rueda: zoom");
        if !app.status.is_empty() {
            ui.label(&app.status);
        }
    });
}
