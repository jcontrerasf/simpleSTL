// En Windows (release) no abrir una consola junto a la ventana.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod camera;
mod csg;
mod ground;
mod history;
mod i18n;
mod manipulator;
mod mesh;
mod place_on_face;
mod primitives;
mod repair;
mod toolbar;
mod viewcube;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use three_d::egui;
use three_d::*;

use camera::CameraController;
use csg::BooleanOp;
use ground::Ground;
use history::History;
use i18n::{Lang, tr};
use manipulator::{GizmoSetup, Manipulator, Pose};
use mesh::{MeshData, Topology};
use place_on_face::{Facet, Facets};
use primitives::Primitive;
use toolbar::Tool;

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
    /// Compartida (`Arc`) con las instantáneas del historial y con los clones.
    mesh: Arc<MeshData>,
    pose: Pose,
    /// Caja envolvente en el mundo, recalculada al cambiar la pose.
    world_bbox: (Vec3, Vec3),
    topology: Topology,
    model: Gm<Mesh, PhysicalMaterial>,
    color: [u8; 3],
    visible: bool,
    /// Parámetros si es una primitiva; permiten cambiarle el tamaño.
    primitive: Option<Primitive>,
}

impl SceneObject {
    /// `mesh` viene en coordenadas del mundo; se centra y el desplazamiento pasa a la pose.
    fn new(context: &Context, name: String, mesh: MeshData, color: [u8; 3]) -> Self {
        let (min, max) = mesh.bounding_box();
        let center = (min + max) * 0.5;
        Self::from_local(context, name, Arc::new(mesh.transformed(|p| p - center)), Pose::at(center), color)
    }

    /// `mesh` en coordenadas locales (centrada en el origen), ubicada en el mundo por `pose`.
    fn from_local(context: &Context, name: String, mesh: Arc<MeshData>, pose: Pose, color: [u8; 3]) -> Self {
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
            pose,
            world_bbox: (vec3(0.0, 0.0, 0.0), vec3(0.0, 0.0, 0.0)),
            model,
            color,
            visible: true,
            primitive: None,
        };
        obj.set_pose(pose);
        obj
    }

    /// Reemplaza la malla local (p. ej. al cambiar el tamaño de una primitiva) sin mover la pose.
    fn set_local_mesh(&mut self, context: &Context, mesh: MeshData) {
        self.model.geometry = Mesh::new(context, &mesh.to_cpu_mesh());
        self.topology = mesh.topology();
        self.mesh = Arc::new(mesh);
        self.set_pose(self.pose);
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
    /// Herramienta activa de la barra superior (solo con un objeto seleccionado).
    tool: Option<Tool>,
    /// Plano de corte: `translation` es un punto del plano y `rotation * Z` su normal.
    cut_pose: Pose,
    /// Caras para "Apoyar en cara" del objeto seleccionado.
    facets: Option<FacetCache>,
    history: History<SceneState>,
    /// Último estado confirmado; el historial guarda los anteriores.
    committed: SceneState,
    /// "Apoyar en cara" también alinea el objeto con X/Y (ver `placement_rotation`).
    align_on_place: bool,
    /// Para numerar las primitivas agregadas ("Cubo 1", "Esfera 2", …).
    primitive_count: usize,
    grid_spacing: f32,
    /// Objeto que se está renombrando en la lista, con el texto en edición.
    renaming: Option<(usize, String)>,
    /// El campo de renombrar debe tomar el foco en el próximo cuadro (solo al abrirse:
    /// pedirlo siempre impediría que Enter lo suelte y confirme).
    rename_needs_focus: bool,
}

/// Lo que el historial guarda de cada objeto. La malla se comparte (`Arc`), así que una
/// instantánea no copia geometría.
#[derive(Clone)]
struct ObjectState {
    name: String,
    mesh: Arc<MeshData>,
    pose: Pose,
    color: [u8; 3],
    visible: bool,
    primitive: Option<Primitive>,
}

impl ObjectState {
    fn same_as(&self, other: &ObjectState) -> bool {
        Arc::ptr_eq(&self.mesh, &other.mesh)
            && self.name == other.name
            && self.pose == other.pose
            && self.color == other.color
            && self.visible == other.visible
            && self.primitive == other.primitive
    }
}

#[derive(Clone)]
struct SceneState {
    objects: Vec<ObjectState>,
    /// Se restaura al deshacer, pero seleccionar por sí solo no crea un paso de historial.
    selected: Option<usize>,
}

impl SceneState {
    fn same_scene(&self, other: &SceneState) -> bool {
        self.objects.len() == other.objects.len() && self.objects.iter().zip(&other.objects).all(|(a, b)| a.same_as(b))
    }
}

/// Caras de "Apoyar en cara" calculadas para un objeto en una pose dada, con su dibujo.
struct FacetCache {
    object: usize,
    pose: Pose,
    facets: Facets,
    overlays: Vec<Gm<Mesh, ColorMaterial>>,
    hovered: Option<usize>,
}

const FACET_COLOR: Srgba = Srgba::new(255, 150, 40, 90);
const FACET_HOVER_COLOR: Srgba = Srgba::new(255, 220, 60, 200);

/// Superposición translúcida de una cara, levantada `lift` sobre la superficie para no
/// pelear en profundidad con la malla.
fn facet_overlay(context: &Context, facet: &Facet, lift: f32) -> Gm<Mesh, ColorMaterial> {
    let offset = facet.normal * lift;
    let positions = facet.triangles.iter().flat_map(|t| t.map(|p| p + offset)).collect();
    let cpu = CpuMesh { positions: Positions::F32(positions), ..Default::default() };
    Gm::new(
        Mesh::new(context, &cpu),
        ColorMaterial::new_transparent(context, &CpuMaterial { albedo: FACET_COLOR, ..Default::default() }),
    )
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
    fn next_color(&self) -> [u8; 3] {
        PALETTE[self.objects.len() % PALETTE.len()]
    }

    fn add_object(&mut self, name: String, mesh: MeshData) {
        let color = self.next_color();
        self.objects.push(SceneObject::new(&self.context, name, mesh, color));
        self.selected = Some(self.objects.len() - 1);
    }

    /// Agrega una primitiva apoyada en z = 0, a la derecha de lo que ya hay en la escena.
    fn add_primitive(&mut self, primitive: Primitive) {
        let Some(mesh) = primitive.mesh() else {
            self.status = match i18n::current() {
                Lang::Es => format!("No se pudo generar {}", primitive.label()),
                Lang::En => format!("Could not generate {}", primitive.label()),
            };
            return;
        };
        let (local_min, local_max) = mesh.bounding_box();
        let width = local_max.x - local_min.x;
        let x = match self.bounding_box() {
            Some((_, scene_max)) => scene_max.x + width * 0.7,
            None => 0.0,
        };
        let pose = Pose::at(vec3(x, 0.0, -local_min.z));
        self.primitive_count += 1;
        let name = format!("{} {}", primitive.label(), self.primitive_count);
        let mut obj = SceneObject::from_local(&self.context, name, Arc::new(mesh), pose, self.next_color());
        obj.primitive = Some(primitive);
        self.status = match i18n::current() {
            Lang::Es => format!("Agregado {}", obj.name),
            Lang::En => format!("Added {}", obj.name),
        };
        self.objects.push(obj);
        self.selected = Some(self.objects.len() - 1);
    }

    /// Cambia los parámetros de una primitiva manteniendo su base a la misma altura.
    fn resize_primitive(&mut self, object: usize, primitive: Primitive) {
        let Some(mesh) = primitive.mesh() else { return };
        let obj = &mut self.objects[object];
        let bottom = obj.world_bbox.0.z;
        obj.set_local_mesh(&self.context, mesh);
        obj.primitive = Some(primitive);
        let mut pose = obj.pose;
        pose.translation.z += bottom - obj.world_bbox.0.z;
        obj.set_pose(pose);
        // La malla cambió aunque la pose no: las caras calculadas ya no valen.
        self.facets = None;
    }

    /// Copia del objeto, desplazada en X para que quede al lado del original.
    fn clone_object(&mut self, object: usize) {
        let original = &self.objects[object];
        let (min, max) = original.world_bbox;
        let mut pose = original.pose;
        pose.translation.x += (max.x - min.x) * 1.1;
        let name = copy_name(&original.name, self.objects.iter().map(|o| o.name.as_str()));
        let (mesh, primitive) = (original.mesh.clone(), original.primitive);
        let mut copy = SceneObject::from_local(&self.context, name, mesh, pose, self.next_color());
        copy.primitive = primitive;
        self.status = match i18n::current() {
            Lang::Es => format!("Clonado como {}", copy.name),
            Lang::En => format!("Cloned as {}", copy.name),
        };
        self.objects.push(copy);
        self.selected = Some(self.objects.len() - 1);
    }

    fn load(&mut self, path: &Path) {
        match MeshData::load_stl(path) {
            Ok(mesh) => {
                let name = path.file_name().map_or(tr("sin nombre", "unnamed").into(), |n| n.to_string_lossy().into_owned());
                let n = mesh.triangles.len();
                self.status = match i18n::current() {
                    Lang::Es => format!("Cargado {name} ({n} triángulos)"),
                    Lang::En => format!("Loaded {name} ({n} triangles)"),
                };
                self.add_object(name, mesh);
            }
            Err(e) => self.status = format!("{} {}: {e}", tr("Error en", "Error in"), path.display()),
        }
    }

    /// Plano de corte activo (solo con la herramienta Corte y un objeto seleccionado).
    fn cut_plane(&self) -> Option<CutPlane> {
        if self.tool != Some(Tool::Cut) {
            return None;
        }
        let object = self.selected?;
        let (min, max) = self.objects.get(object)?.world_bbox;
        let n = self.cut_pose.rotation * vec3(0.0, 0.0, 1.0);
        // El cuadrado base (2×2, normal +Z) se escala para cubrir el objeto.
        let size = (max - min).magnitude() * 0.6;
        Some(CutPlane {
            object,
            normal: [n.x as f64, n.y as f64, n.z as f64],
            offset: n.dot(self.cut_pose.translation) as f64,
            transform: self.cut_pose.matrix() * Mat4::from_scale(size),
        })
    }

    /// Ajustes al cambiar de herramienta o de objeto seleccionado.
    fn on_tool_or_selection_changed(&mut self) {
        let Some(selected) = self.selected else {
            self.tool = None;
            self.facets = None;
            return;
        };
        match self.tool {
            Some(Tool::Cut) => {
                // Plano horizontal por el centro del objeto.
                let (min, max) = self.objects[selected].world_bbox;
                self.cut_pose = Pose::at((min + max) * 0.5);
            }
            Some(Tool::Boolean) => {
                self.bool_a = selected;
                self.bool_b = (0..self.objects.len()).find(|&i| i != selected).unwrap_or(selected);
            }
            _ => {}
        }
        self.facets = None;
    }

    /// Recalcula las caras de "Apoyar en cara" si la herramienta está activa y el objeto
    /// seleccionado (o su pose) cambió desde el último cálculo.
    fn update_facets(&mut self) {
        let (Some(Tool::PlaceOnFace), Some(i)) = (self.tool, self.selected) else {
            self.facets = None;
            return;
        };
        let obj = &self.objects[i];
        if self.facets.as_ref().is_some_and(|c| c.object == i && c.pose == obj.pose) {
            return;
        }
        let facets = place_on_face::find_facets(&obj.world_mesh());
        let (min, max) = obj.world_bbox;
        let lift = (max - min).magnitude() * 2e-3;
        let overlays = facets.candidates.iter().map(|f| facet_overlay(&self.context, f, lift)).collect();
        self.facets = Some(FacetCache { object: i, pose: obj.pose, facets, overlays, hovered: None });
    }

    /// Rota el objeto (sobre su centro) para que la cara candidata `facet` mire hacia abajo
    /// (y, si está activado, alinea sus caras verticales con X/Y); luego lo baja o sube hasta
    /// que quede apoyado en z = 0.
    fn place_on_face(&mut self, facet: usize) {
        let Some(cache) = &self.facets else { return };
        let (object, facets) = (cache.object, &cache.facets);
        let rotation = place_on_face::placement_rotation(facets.candidates[facet].normal, &facets.planes, self.align_on_place);
        let obj = &mut self.objects[object];
        let mut pose = obj.pose;
        pose.rotation = (rotation * pose.rotation).normalize();
        obj.set_pose(pose);
        pose.translation.z -= obj.world_bbox.0.z;
        obj.set_pose(pose);
        self.status = match i18n::current() {
            Lang::Es => format!("{} apoyado en una cara", obj.name),
            Lang::En => format!("{} placed on a face", obj.name),
        };
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
                let (time, n) = (start.elapsed(), mesh.triangles.len());
                self.status = match i18n::current() {
                    Lang::Es => format!("{} en {time:.0?} ({n} triángulos)", op.label()),
                    Lang::En => format!("{} in {time:.0?} ({n} triangles)", op.label()),
                };
                self.objects[self.bool_a].visible = false;
                self.objects[self.bool_b].visible = false;
                self.add_object(name, mesh);
            }
            Ok(None) => self.status = format!("{}: {}", op.label(), tr("el resultado es vacío", "the result is empty")),
            Err(e) => self.status = e,
        }
    }

    fn apply_cut(&mut self, plane: CutPlane) {
        let obj = &self.objects[plane.object];
        let base = obj.name.clone();
        let start = Instant::now();
        match csg::split(&obj.world_mesh(), plane.normal, plane.offset) {
            Ok((positive, negative)) => {
                if positive.is_none() || negative.is_none() {
                    self.status = tr("El plano no atraviesa el objeto", "The plane does not cross the object").into();
                    return;
                }
                self.objects[plane.object].visible = false;
                for (half, sign) in [(positive, "+"), (negative, "−")] {
                    self.add_object(format!("{base} ({sign})"), half.unwrap());
                }
                self.status = format!("{} {:.0?}", tr("Corte en", "Cut in"), start.elapsed());
            }
            Err(e) => self.status = e,
        }
    }

    fn snapshot(&self) -> SceneState {
        let objects = self
            .objects
            .iter()
            .map(|o| ObjectState {
                name: o.name.clone(),
                mesh: o.mesh.clone(),
                pose: o.pose,
                color: o.color,
                visible: o.visible,
                primitive: o.primitive,
            })
            .collect();
        SceneState { objects, selected: self.selected }
    }

    /// Vuelve a un estado del historial. Reutiliza la malla en GPU de los objetos cuya
    /// geometría no cambió; solo sube de nuevo las que ya no existen (p. ej. al deshacer
    /// un borrado).
    fn restore(&mut self, state: SceneState) {
        let mut old: Vec<Option<SceneObject>> = std::mem::take(&mut self.objects).into_iter().map(Some).collect();
        let mut objects = Vec::with_capacity(state.objects.len());
        for s in &state.objects {
            let reused = old.iter_mut().find(|o| o.as_ref().is_some_and(|o| Arc::ptr_eq(&o.mesh, &s.mesh))).and_then(Option::take);
            let mut obj = reused
                .unwrap_or_else(|| SceneObject::from_local(&self.context, s.name.clone(), s.mesh.clone(), s.pose, s.color));
            obj.name = s.name.clone();
            obj.set_color(s.color);
            obj.visible = s.visible;
            obj.primitive = s.primitive;
            obj.set_pose(s.pose);
            objects.push(obj);
        }
        self.objects = objects;
        self.selected = state.selected.filter(|&i| i < self.objects.len());
        self.facets = None;
        self.renaming = None;
        self.committed = state;
    }

    fn undo(&mut self) {
        match self.history.undo(self.snapshot()) {
            Some(previous) => {
                self.restore(previous);
                self.status = tr("Deshecho", "Undone").into();
            }
            None => self.status = tr("Nada que deshacer", "Nothing to undo").into(),
        }
    }

    fn redo(&mut self) {
        match self.history.redo(self.snapshot()) {
            Some(next) => {
                self.restore(next);
                self.status = tr("Rehecho", "Redone").into();
            }
            None => self.status = tr("Nada que rehacer", "Nothing to redo").into(),
        }
    }

    /// Si la escena cambió desde el último estado confirmado, guarda ese estado en el
    /// historial. Se llama al final de cada cuadro sin botones del ratón presionados, así
    /// que un arrastre completo queda como un solo paso.
    fn commit_changes(&mut self) {
        if self.renaming.is_some() {
            return;
        }
        let current = self.snapshot();
        if current.same_scene(&self.committed) {
            self.committed.selected = current.selected;
        } else {
            let previous = std::mem::replace(&mut self.committed, current);
            self.history.record(previous);
        }
    }

    /// Repara la malla del objeto (ver `repair.rs`) conservando su pose.
    fn repair_object(&mut self, object: usize) {
        let obj = &mut self.objects[object];
        let (fixed, report) = repair::repair(&obj.mesh);
        if !report.changed_anything() {
            self.status = format!("{}: {}", obj.name, tr("no se encontró nada que reparar", "nothing to repair"));
            return;
        }
        obj.set_local_mesh(&self.context, fixed);
        obj.primitive = None;
        let mut parts = Vec::new();
        for (count, what) in [
            (report.welded_vertices, tr("vértices soldados", "vertices welded")),
            (report.removed_triangles, tr("triángulos eliminados", "triangles removed")),
            (report.flipped_triangles, tr("caras invertidas", "faces flipped")),
            (report.holes_filled, tr("agujeros cerrados", "holes filled")),
            (report.merged_parts, tr("piezas fusionadas", "parts merged")),
        ] {
            if count > 0 {
                parts.push(format!("{what}: {count}"));
            }
        }
        let (name, parts) = (&obj.name, parts.join(", "));
        let (boundary, bad) = (report.topology.boundary_edges, report.topology.bad_edges);
        self.status = match (report.topology.is_closed(), i18n::current()) {
            (true, Lang::Es) => format!("{name} reparado: {parts}"),
            (true, Lang::En) => format!("{name} repaired: {parts}"),
            (false, Lang::Es) => {
                format!("{name} reparado en parte ({parts}); quedan {boundary} bordes y {bad} aristas con más de dos caras")
            }
            (false, Lang::En) => {
                format!("{name} partially repaired ({parts}); {boundary} boundary edges and {bad} edges with more than two faces remain")
            }
        };
        self.facets = None;
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
    add_primitive: Option<Primitive>,
    clone: Option<usize>,
    resize: Option<(usize, Primitive)>,
    repair: Option<usize>,
}

fn main() {
    i18n::set(i18n::detect());
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
        tool: None,
        cut_pose: Pose::at(vec3(0.0, 0.0, 0.0)),
        facets: None,
        primitive_count: 0,
        align_on_place: true,
        history: History::new(100),
        committed: SceneState { objects: Vec::new(), selected: None },
        grid_spacing: ground.spacing,
        renaming: None,
        rename_needs_focus: false,
    };
    for arg in std::env::args_os().skip(1) {
        app.load(&PathBuf::from(arg));
    }
    // Los archivos abiertos al iniciar son el punto de partida, no un paso para deshacer.
    app.committed = app.snapshot();
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

        manipulator.track_input(&frame_input.events, dpr, window_viewport.height);
        let mut dragged_pose = None;
        let mut cube_action = None;
        // Zonas del visor tapadas por controles: ahí los clics no seleccionan en la escena.
        let mut blocked: Vec<egui::Rect> = Vec::new();
        let mut orthographic = control.is_orthographic();
        let (tool_before, selected_before) = (app.tool, app.selected);

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
                let cube_rect = viewcube::rect(view_rect);
                cube_action = viewcube::show(ui.ctx(), cube_rect, &camera);
                blocked.push(cube_rect);
                blocked.push(viewcube::projection_button(ui.ctx(), cube_rect, &mut orthographic));
                if app.selected.is_some() {
                    // La barra usa el ancho libre a la izquierda del cubo de vista.
                    let max_width = (cube_rect.min.x - view_rect.min.x - 32.0).max(120.0);
                    blocked.push(toolbar::show(ui.ctx(), view_rect, max_width, &mut app.tool));
                }
                dragged_pose = manipulator.update(ui.ctx(), &camera, view_rect, &blocked, gizmo_target(&app));
            },
        );

        // Animaciones de egui (menús, tooltips, cursor de texto): seguir dibujando mientras las pida.
        // (2 y no 1: al final del cuadro se descuenta uno.)
        if gui.context().requested_repaint_last_pass() || gui.context().has_requested_repaint() {
            extra_frames = extra_frames.max(2);
        }
        if let (Some(pose), Some(i)) = (dragged_pose, app.selected) {
            if app.tool == Some(Tool::Cut) {
                app.cut_pose = pose;
            } else {
                app.objects[i].set_pose(pose);
            }
        }
        if orthographic != control.is_orthographic() {
            control.set_orthographic(&mut camera, orthographic);
        }
        // Atajos de teclado (egui ya marcó como manejadas las teclas si hay un campo de texto activo).
        let has_selection = app.selected.is_some();
        for event in frame_input.events.iter_mut() {
            let Event::KeyPress { kind, modifiers, handled } = event else { continue };
            if *handled || modifiers.alt {
                continue;
            }
            *handled = true;
            match (*kind, modifiers.ctrl) {
                (Key::Z, true) if modifiers.shift => app.redo(),
                (Key::Z, true) => app.undo(),
                (Key::Y, true) => app.redo(),
                (Key::D, true) if has_selection => actions.clone = app.selected,
                (Key::O, false) => control.set_orthographic(&mut camera, !control.is_orthographic()),
                (Key::Escape, false) if has_selection => app.tool = None,
                (Key::Delete, false) if has_selection => actions.delete = app.selected,
                (key, false) if has_selection => match Tool::ALL.into_iter().find(|t| t.key() == key) {
                    Some(tool) => app.tool = if app.tool == Some(tool) { None } else { Some(tool) },
                    None => *handled = false,
                },
                _ => *handled = false,
            }
        }
        if let Some((i, pose)) = actions.pose {
            app.objects[i].set_pose(pose);
        }
        match cube_action {
            Some(viewcube::Action::LookFrom(direction)) => control.look_from(direction),
            Some(viewcube::Action::Orbit(delta)) => control.orbit_by_drag(&mut camera, delta),
            None => {}
        }
        // Posición física (three-d) en el visor libre de controles, si corresponde.
        let in_scene = |p: PhysicalPoint| {
            let point = egui::pos2(p.x / dpr, (window_viewport.height as f32 - p.y) / dpr);
            p.x >= viewport.x as f32 && !blocked.iter().any(|r| r.contains(point))
        };

        app.update_facets();
        // Resaltar la cara bajo el cursor.
        let last_motion = frame_input.events.iter().rev().find_map(|e| match e {
            Event::MouseMotion { position, .. } => Some(*position),
            _ => None,
        });
        if let (Some(cache), Some(p)) = (app.facets.as_mut(), last_motion) {
            let hovered = if in_scene(p) { pick_facet(&context, &camera, cache, p) } else { None };
            if hovered != cache.hovered {
                for (k, overlay) in cache.overlays.iter_mut().enumerate() {
                    overlay.material.color = if Some(k) == hovered { FACET_HOVER_COLOR } else { FACET_COLOR };
                }
                cache.hovered = hovered;
            }
        }

        if let Some(click) = manipulator.consume_events(&mut frame_input.events) {
            let p = click.position;
            if in_scene(p) {
                // Con "Apoyar en cara", un clic sobre una cara resaltada tiene prioridad.
                let facet = app.facets.as_ref().and_then(|c| pick_facet(&context, &camera, c, p));
                if let Some(facet) = facet {
                    app.place_on_face(facet);
                } else {
                    // Selección con el ratón: el índice de geometría es la posición en `visible`.
                    let visible: Vec<usize> = (0..app.objects.len()).filter(|&i| app.objects[i].visible).collect();
                    let geometries = visible.iter().map(|&i| &app.objects[i].model.geometry);
                    if let Ok(hit) = pick(&context, &camera, click.position, geometries, Cull::None) {
                        app.selected = hit.map(|h| visible[h.geometry_id as usize]);
                    }
                }
            }
        }

        if actions.open {
            if let Some(paths) = rfd::FileDialog::new()
                .set_title(tr("Abrir STL", "Open STL"))
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
            app.status = match i18n::current() {
                Lang::Es => format!("Eliminado {}", removed.name),
                Lang::En => format!("Deleted {}", removed.name),
            };
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
                .set_title(tr("Exportar STL", "Export STL"))
                .add_filter("STL", &["stl"])
                .set_file_name(default_name)
                .save_file()
            {
                app.status = match obj.world_mesh().save_stl(&path) {
                    Ok(()) => format!("{} {}", tr("Exportado", "Exported"), path.display()),
                    Err(e) => format!("{}: {e}", tr("Error al exportar", "Export failed")),
                };
            }
        }
        if let Some(primitive) = actions.add_primitive {
            app.add_primitive(primitive);
        }
        if let Some(i) = actions.clone {
            app.clone_object(i);
        }
        if let Some((i, primitive)) = actions.resize {
            app.resize_primitive(i, primitive);
        }
        if let Some(i) = actions.repair {
            app.repair_object(i);
        }
        if actions.boolean {
            app.apply_boolean();
        }
        if actions.cut {
            if let Some(plane) = app.cut_plane() {
                app.apply_cut(plane);
            }
        }
        if app.tool != tool_before || app.selected != selected_before {
            app.on_tool_or_selection_changed();
        }
        let cut_plane = app.cut_plane();

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

        // Confirmar cambios en el historial cuando no hay un arrastre en curso.
        if !gui.context().input(|i| i.pointer.any_down()) {
            app.commit_changes();
        }

        let mut scene: Vec<&dyn Object> = app.objects.iter().filter(|o| o.visible).map(|o| &o.model as &dyn Object).collect();
        scene.push(&axes);
        scene.extend(ground.objects());
        if let Some(cache) = &app.facets {
            scene.extend(cache.overlays.iter().map(|o| o as &dyn Object));
        }
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

/// Separa el sufijo de copia " (N)" de un nombre: ("pieza", Some(2)) para "pieza (2)".
/// Es igual en todos los idiomas, para que la numeración no dependa del idioma activo.
fn split_copy_suffix(name: &str) -> (&str, Option<u32>) {
    if let Some((base, rest)) = name.rsplit_once(" (") {
        if let Some(Ok(n)) = rest.strip_suffix(')').map(str::parse::<u32>) {
            return (base, Some(n));
        }
    }
    (name, None)
}

/// Nombre para una copia de `name`: "base (N)", con N uno más que la mayor copia existente
/// de la misma base. Copiar una copia no acumula sufijos.
fn copy_name<'a>(name: &str, existing: impl Iterator<Item = &'a str>) -> String {
    let (base, _) = split_copy_suffix(name);
    let last = existing
        .filter_map(|other| match split_copy_suffix(other) {
            (b, Some(n)) if b == base => Some(n),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    format!("{base} ({})", last + 1)
}

/// Qué manipula el gizmo según la herramienta activa: el objeto, el plano de corte o nada.
fn gizmo_target(app: &App) -> Option<(Pose, GizmoSetup)> {
    let obj = app.selected.and_then(|i| app.objects.get(i)).filter(|o| o.visible)?;
    match app.tool? {
        Tool::Move => Some((obj.pose, GizmoSetup::translate())),
        Tool::Rotate => Some((obj.pose, GizmoSetup::rotate())),
        Tool::Cut => Some((app.cut_pose, GizmoSetup::cut_plane())),
        Tool::Boolean | Tool::PlaceOnFace => None,
    }
}

/// Índice de la cara de "Apoyar en cara" bajo el píxel dado, si hay alguna.
fn pick_facet(context: &Context, camera: &Camera, cache: &FacetCache, pixel: PhysicalPoint) -> Option<usize> {
    let geometries = cache.overlays.iter().map(|o| &o.geometry);
    pick(context, camera, pixel, geometries, Cull::None).ok().flatten().map(|h| h.geometry_id as usize)
}

fn boolean_panel(ui: &mut egui::Ui, app: &mut App, actions: &mut UiActions) {
    if app.objects.len() < 2 {
        ui.weak(tr("Se necesitan dos objetos", "Two objects are needed"));
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
            let text = if op == BooleanOp::Difference { tr("Resta A − B", "Difference A − B") } else { op.label() };
            ui.radio_value(&mut app.bool_op, op, text);
        }
    });

    let (a, b) = (&app.objects[app.bool_a], &app.objects[app.bool_b]);
    let problem = if app.bool_a == app.bool_b {
        Some(tr("A y B deben ser objetos distintos", "A and B must be different objects"))
    } else if !a.topology.is_closed() || !b.topology.is_closed() {
        Some(tr("Ambas mallas deben ser cerradas", "Both meshes must be closed"))
    } else {
        None
    };
    let button = ui.add_enabled(problem.is_none(), egui::Button::new(tr("Aplicar", "Apply")));
    actions.boolean = button.on_disabled_hover_text(problem.unwrap_or_default()).clicked();
}

fn side_panel(ui: &mut egui::Ui, app: &mut App, actions: &mut UiActions) {
    egui::Panel::bottom("status").show_inside(ui, |ui| {
        if !app.status.is_empty() {
            ui.label(&app.status);
        }
        ui.weak(format!("{} {}", tr("Base z = 0 · grilla cada", "Base z = 0 · grid every"), app.grid_spacing));
        ui.weak(tr(
            "Clic: seleccionar · Izq: orbitar · Der/Medio: desplazar · Rueda: zoom",
            "Click: select · Left: orbit · Right/Middle: pan · Wheel: zoom",
        ));
        language_selector(ui);
    });
    egui::ScrollArea::vertical().show(ui, |ui| panel_contents(ui, app, actions));
}

/// Selector de idioma; el cambio se ve en el siguiente cuadro.
fn language_selector(ui: &mut egui::Ui) {
    let mut lang = i18n::current();
    egui::ComboBox::from_id_salt("language").selected_text(lang.name()).show_ui(ui, |ui| {
        for option in Lang::ALL {
            ui.selectable_value(&mut lang, option, option.name());
        }
    });
    if lang != i18n::current() {
        i18n::set(lang);
    }
}

#[derive(Clone, Copy)]
enum ObjectAction {
    Rename,
    Clone,
    Export,
    Delete,
}

/// Menú de un objeto de la lista (botón "…" o clic derecho sobre el nombre).
fn object_menu(ui: &mut egui::Ui) -> Option<ObjectAction> {
    let mut action = None;
    if ui.button(tr("Renombrar", "Rename")).clicked() {
        action = Some(ObjectAction::Rename);
    }
    if ui.button(tr("Clonar (Ctrl+D)", "Clone (Ctrl+D)")).clicked() {
        action = Some(ObjectAction::Clone);
    }
    if ui.button(tr("Exportar STL…", "Export STL…")).clicked() {
        action = Some(ObjectAction::Export);
    }
    if ui.button(tr("Eliminar", "Delete")).clicked() {
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
    response.on_hover_text(if *visible { tr("Ocultar", "Hide") } else { tr("Mostrar", "Show") })
}

fn panel_contents(ui: &mut egui::Ui, app: &mut App, actions: &mut UiActions) {
    ui.heading("simpleSTL");
    ui.horizontal(|ui| {
        actions.open = ui.button(tr("Abrir STL…", "Open STL…")).clicked();
        ui.menu_button(tr("Primitivas", "Primitives"), |ui| {
            for primitive in Primitive::DEFAULTS {
                if ui.button(primitive.label()).clicked() {
                    actions.add_primitive = Some(primitive);
                    ui.close();
                }
            }
        });
        actions.fit = ui.button(tr("Encuadrar", "Fit view")).clicked();
    });
    ui.separator();

    ui.label(egui::RichText::new(tr("Objetos", "Objects")).strong());
    if app.objects.is_empty() {
        ui.weak(tr("Ninguno cargado", "None loaded"));
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
                Some(ObjectAction::Clone) => actions.clone = Some(i),
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
        let closed = obj.topology.is_closed();
        let volume = obj.mesh.volume();
        // Cerrada pero con volumen negativo: las caras apuntan hacia adentro.
        let inverted = closed && volume < 0.0;
        egui::Grid::new("info").num_columns(2).show(ui, |ui| {
            ui.label(tr("Triángulos", "Triangles"));
            ui.label(obj.mesh.triangles.len().to_string());
            ui.end_row();
            ui.label(tr("Vértices", "Vertices"));
            ui.label(obj.mesh.vertices.len().to_string());
            ui.end_row();
            ui.label(tr("Tamaño", "Size"));
            ui.label(format!("{:.2} × {:.2} × {:.2}", size.x, size.y, size.z));
            ui.end_row();
            ui.label(tr("Cerrada", "Closed"));
            if !closed {
                ui.colored_label(
                    egui::Color32::LIGHT_RED,
                    {
                        let (boundary, bad) = (obj.topology.boundary_edges, obj.topology.bad_edges);
                        match i18n::current() {
                            Lang::Es => format!("no ({boundary} bordes, {bad} defectuosas)"),
                            Lang::En => format!("no ({boundary} boundary, {bad} defective)"),
                        }
                    },
                );
            } else if inverted {
                ui.colored_label(egui::Color32::LIGHT_RED, tr("sí, con normales invertidas", "yes, with inverted normals"));
            } else {
                ui.colored_label(egui::Color32::LIGHT_GREEN, tr("sí", "yes"));
            }
            ui.end_row();
            if closed {
                ui.label(tr("Volumen", "Volume"));
                ui.label(format!("{:.2}", volume.abs()));
                ui.end_row();
            }
        });
        if !closed || inverted {
            let button = ui.button(tr("Reparar malla", "Repair mesh")).on_hover_text(tr(
                "Suelda vértices, quita triángulos duplicados, orienta las caras y cierra agujeros",
                "Welds vertices, removes duplicate triangles, orients faces and fills holes",
            ));
            if button.clicked() {
                actions.repair = app.selected;
            }
        }
        if let (Some(primitive), Some(i)) = (obj.primitive, app.selected) {
            if let Some(resized) = dimensions_section(ui, primitive) {
                actions.resize = Some((i, resized));
            }
        }
    }

    if app.selected.is_some() {
        ui.separator();
        tool_section(ui, app, actions);
    }
}

/// Parámetros editables de una primitiva. Devuelve los nuevos si el usuario cambió alguno.
fn dimensions_section(ui: &mut egui::Ui, mut primitive: Primitive) -> Option<Primitive> {
    let mut changed = false;
    let mut length = |ui: &mut egui::Ui, label: &str, value: &mut f32| {
        ui.label(label);
        let drag = egui::DragValue::new(value).range(primitives::MIN_SIZE..=f32::MAX).speed(0.1).max_decimals(2);
        changed |= ui.add(drag).changed();
        ui.end_row();
    };
    ui.label(egui::RichText::new(tr("Dimensiones", "Dimensions")).strong());
    let mut segments_changed = false;
    egui::Grid::new("dimensions").num_columns(2).show(ui, |ui| match &mut primitive {
        Primitive::Box { size } => {
            let names = [tr("Ancho (X)", "Width (X)"), tr("Fondo (Y)", "Depth (Y)"), tr("Alto (Z)", "Height (Z)")];
            for (name, value) in names.into_iter().zip(size.iter_mut()) {
                length(ui, name, value);
            }
        }
        Primitive::Sphere { radius, segments } => {
            length(ui, tr("Radio", "Radius"), radius);
            ui.label(tr("Segmentos", "Segments"));
            segments_changed = ui.add(egui::DragValue::new(segments).range(primitives::SEGMENTS)).changed();
        }
        Primitive::Cylinder { radius, height, segments } => {
            length(ui, tr("Radio", "Radius"), radius);
            length(ui, tr("Alto", "Height"), height);
            ui.label(tr("Segmentos", "Segments"));
            segments_changed = ui.add(egui::DragValue::new(segments).range(primitives::SEGMENTS)).changed();
        }
        Primitive::Cone { radius_bottom, radius_top, height, segments } => {
            length(ui, tr("Radio inferior", "Bottom radius"), radius_bottom);
            // El superior puede ser 0 (punta); el inferior no, o el sólido sería vacío.
            ui.label(tr("Radio superior", "Top radius"));
            let drag = egui::DragValue::new(radius_top).range(0.0..=f32::MAX).speed(0.1).max_decimals(2);
            segments_changed |= ui.add(drag).changed();
            ui.end_row();
            length(ui, tr("Alto", "Height"), height);
            ui.label(tr("Segmentos", "Segments"));
            segments_changed |= ui.add(egui::DragValue::new(segments).range(primitives::SEGMENTS)).changed();
        }
        Primitive::Tube { radius_outer, radius_inner, height, segments } => {
            length(ui, tr("Radio exterior", "Outer radius"), radius_outer);
            length(ui, tr("Radio del agujero", "Hole radius"), radius_inner);
            length(ui, tr("Alto", "Height"), height);
            ui.label(tr("Segmentos", "Segments"));
            segments_changed = ui.add(egui::DragValue::new(segments).range(primitives::SEGMENTS)).changed();
        }
        Primitive::RoundedBox { size, radius, segments } => {
            let names = [tr("Ancho (X)", "Width (X)"), tr("Fondo (Y)", "Depth (Y)"), tr("Alto (Z)", "Height (Z)")];
            for (name, value) in names.into_iter().zip(size.iter_mut()) {
                length(ui, name, value);
            }
            // 0 deja esquinas vivas; la mitad del lado menor, una ranura.
            ui.label(tr("Radio de esquinas", "Corner radius"));
            let drag = egui::DragValue::new(radius).range(0.0..=f32::MAX).speed(0.1).max_decimals(2);
            segments_changed |= ui.add(drag).changed();
            ui.end_row();
            ui.label(tr("Segmentos", "Segments"));
            segments_changed |= ui.add(egui::DragValue::new(segments).range(primitives::SEGMENTS)).changed();
        }
    });
    (changed || segments_changed).then(|| primitive.normalized())
}

/// Opciones de la herramienta activa (solo esa) para el objeto seleccionado.
fn tool_section(ui: &mut egui::Ui, app: &mut App, actions: &mut UiActions) {
    let Some(i) = app.selected else { return };
    let Some(tool) = app.tool else {
        ui.weak(tr(
            "Herramientas en la barra superior: Mover (M), Rotar (R), Corte (C), Booleana (B), Apoyar en cara (F). Esc cierra la activa.",
            "Tools in the top bar: Move (M), Rotate (R), Cut (C), Boolean (B), Place on face (F). Esc closes the active one.",
        ));
        return;
    };
    ui.label(egui::RichText::new(tool.label()).strong());
    let pose = app.objects[i].pose;
    match tool {
        Tool::Move => {
            let mut pose = pose;
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label(tr("Posición", "Position"));
                for c in [&mut pose.translation.x, &mut pose.translation.y, &mut pose.translation.z] {
                    changed |= ui.add(egui::DragValue::new(c).speed(0.1).max_decimals(2)).changed();
                }
            });
            if changed {
                actions.pose = Some((i, pose));
            }
            ui.weak(tr("Arrastra las flechas. Ctrl: pasos de 1.", "Drag the arrows. Ctrl: steps of 1."));
        }
        Tool::Rotate => {
            if ui.button(tr("Restablecer rotación", "Reset rotation")).clicked() {
                actions.pose = Some((i, Pose::at(pose.translation)));
            }
            ui.weak(tr("Arrastra los anillos. Ctrl: pasos de 15°.", "Drag the rings. Ctrl: steps of 15°."));
        }
        Tool::Cut => cut_section(ui, app, actions, i),
        Tool::Boolean => boolean_panel(ui, app, actions),
        Tool::PlaceOnFace => {
            ui.checkbox(&mut app.align_on_place, tr("Alinear con los ejes X/Y", "Align with the X/Y axes")).on_hover_text(tr(
                "Tras apoyar, gira sobre Z para que las caras verticales miren a ±X o ±Y",
                "After placing, rotates about Z so the vertical faces point to ±X or ±Y",
            ));
            match app.facets.as_ref().map(|c| c.facets.candidates.len()) {
                Some(0) => ui.weak(tr(
                    "No hay caras planas estables (p. ej. superficies curvas).",
                    "No stable flat faces (e.g. curved surfaces).",
                )),
                Some(n) => ui.label(match i18n::current() {
                    Lang::Es => format!("{n} caras disponibles: haz clic en una para apoyar el objeto sobre ella."),
                    Lang::En => format!("{n} faces available: click one to place the object on it."),
                }),
                None => ui.weak(tr("Calculando…", "Computing…")),
            };
        }
    }
}

fn cut_section(ui: &mut egui::Ui, app: &mut App, actions: &mut UiActions, i: usize) {
    let obj = &app.objects[i];
    let closed = obj.topology.is_closed();
    let (min, max) = obj.world_bbox;

    ui.horizontal(|ui| {
        ui.label(tr("Orientar", "Orient"));
        for (k, name) in AXES.iter().enumerate() {
            if ui.button(*name).on_hover_text(format!("{} {name}", tr("Normal según", "Normal along"))).clicked() {
                // Rotación que lleva la normal base (+Z) al eje elegido.
                app.cut_pose.rotation = match k {
                    0 => Quat::from_angle_y(degrees(90.0)),
                    1 => Quat::from_angle_x(degrees(-90.0)),
                    _ => Pose::at(vec3(0.0, 0.0, 0.0)).rotation,
                };
            }
        }
    });

    // Rango del desplazamiento: proyección de las esquinas de la caja sobre la normal.
    let n = app.cut_pose.rotation * vec3(0.0, 0.0, 1.0);
    let (lo, hi) = (0..8)
        .map(|k| vec3(if k & 1 == 0 { min.x } else { max.x }, if k & 2 == 0 { min.y } else { max.y }, if k & 4 == 0 { min.z } else { max.z }))
        .fold((f32::MAX, f32::MIN), |(lo, hi), c| (lo.min(n.dot(c)), hi.max(n.dot(c))));
    let current = n.dot(app.cut_pose.translation);
    let mut offset = current;
    ui.horizontal(|ui| {
        ui.label(tr("Desplazamiento", "Offset"));
        let drag = egui::DragValue::new(&mut offset).range(lo..=hi).speed((hi - lo) * 0.005).max_decimals(2);
        if ui.add(drag).changed() {
            app.cut_pose.translation += n * (offset - current);
        }
    });
    ui.weak(tr(
        "Arrastra la flecha para desplazar el plano y los anillos para inclinarlo.",
        "Drag the arrow to move the plane and the rings to tilt it.",
    ));
    let button = ui.add_enabled(closed, egui::Button::new(tr("Cortar", "Cut")));
    actions.cut = button.on_disabled_hover_text(tr("La malla debe ser cerrada", "The mesh must be closed")).clicked();
}

#[cfg(test)]
mod tests {
    use super::copy_name;

    #[test]
    fn copies_are_numbered_without_nesting() {
        assert_eq!(copy_name("pieza", ["pieza"].into_iter()), "pieza (1)");
        let names = ["pieza", "pieza (1)", "pieza (2)"];
        // Copiar el original o cualquier copia da el siguiente número.
        assert_eq!(copy_name("pieza", names.into_iter()), "pieza (3)");
        assert_eq!(copy_name("pieza (1)", names.into_iter()), "pieza (3)");
        // Paréntesis que no son de copia se conservan (también las mitades de un corte).
        assert_eq!(copy_name("tapa (v2)", ["tapa (v2)"].into_iter()), "tapa (v2) (1)");
        assert_eq!(copy_name("pieza (+)", ["pieza (+)"].into_iter()), "pieza (+) (1)");
    }
}
