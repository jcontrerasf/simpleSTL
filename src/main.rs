// En Windows (release) no abrir una consola junto a la ventana.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod camera;
mod csg;
mod ground;
mod history;
mod i18n;
mod manipulator;
mod measure;
mod mesh;
mod place_on_face;
mod primitives;
mod repair;
mod snap;
mod toolbar;
mod viewcube;

use std::cell::OnceCell;
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
use measure::{Axis, Measurement, Ruler};
use mesh::{MeshData, Topology};
use place_on_face::{Facet, Facets};
use primitives::Primitive;
use snap::{Features, Snap};
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
    /// Rasgos para el snap (esquinas, aristas, centros), en coordenadas locales. Se
    /// calculan la primera vez que se necesitan.
    features: OnceCell<Features>,
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
            features: OnceCell::new(),
        };
        obj.set_pose(pose);
        obj
    }

    fn features(&self) -> &Features {
        self.features.get_or_init(|| snap::features(&self.mesh))
    }

    /// Reemplaza la malla local (p. ej. al cambiar el tamaño de una primitiva) sin mover la pose.
    fn set_local_mesh(&mut self, context: &Context, mesh: MeshData) {
        self.model.geometry = Mesh::new(context, &mesh.to_cpu_mesh());
        self.topology = mesh.topology();
        self.mesh = Arc::new(mesh);
        self.features = OnceCell::new();
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
    /// Medición de la herramienta Medir (una sola; se borra al salir de la herramienta).
    measurement: Measurement,
    /// Eje fijado para medir y poner reglas; `None` usa el dominante.
    axis_lock: Option<Axis>,
    /// Reglas en la escena (no entran en el historial).
    rulers: Vec<Ruler>,
    /// Primer punto de la regla que se está poniendo.
    ruler_start: Option<Vec3>,
    /// Espaciado de las marcas de las reglas nuevas.
    ruler_spacing: f32,
    /// Punto con snap bajo el cursor (con Medir o Regla activas).
    hover_snap: Option<Snap>,
    /// Pose del objeto al empezar el arrastre con Mover, y la pose "cruda" que entrega el
    /// gizmo (sin enganchar a las reglas). El gizmo aplica incrementos a la pose que recibe,
    /// así que debe seguir recibiendo la cruda o el objeto se desfasaría del cursor.
    drag_start: Option<Pose>,
    drag_raw: Option<Pose>,
    /// Marcas de regla a las que está enganchado el objeto que se arrastra.
    snapped_marks: Vec<Vec3>,
    /// Escalar bloquea la proporción: cualquier eje escala los tres por igual.
    scale_locked: bool,
    /// Clonar en matriz abierto en el panel (para el objeto seleccionado).
    array: Option<ArrayParams>,
}

/// Parámetros de "Clonar en matriz": copias en columnas (X) × filas (Y), separadas por un
/// hueco libre entre las cajas envolventes.
#[derive(Clone, Copy, Debug)]
struct ArrayParams {
    object: usize,
    columns: u32,
    rows: u32,
    gap_x: f32,
    gap_y: f32,
}

/// Desplazamientos de las copias de una matriz respecto del original, que ocupa la celda
/// (0, 0); la grilla crece hacia +X y +Y.
fn array_offsets(columns: u32, rows: u32, size: Vec3, gap_x: f32, gap_y: f32) -> Vec<Vec3> {
    let (step_x, step_y) = (size.x + gap_x, size.y + gap_y);
    (0..rows)
        .flat_map(|row| (0..columns).map(move |col| (col, row)))
        .filter(|&cell| cell != (0, 0))
        .map(|(col, row)| vec3(col as f32 * step_x, row as f32 * step_y, 0.0))
        .collect()
}

/// Escala mínima por eje (0.1 %).
const MIN_SCALE: f32 = 0.001;

/// Factor uniforme de un arrastre de escala con la proporción bloqueada: el del eje que
/// más cambió respecto del inicio.
fn uniform_factor(start: Vec3, new: Vec3) -> f32 {
    [new.x / start.x, new.y / start.y, new.z / start.z]
        .into_iter()
        .max_by(|a, b| (a - 1.0).abs().total_cmp(&(b - 1.0).abs()))
        .unwrap_or(1.0)
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

    /// Agrega una copia del objeto desplazada `offset` y devuelve su índice.
    fn add_copy(&mut self, object: usize, offset: Vec3) -> usize {
        let original = &self.objects[object];
        let mut pose = original.pose;
        pose.translation += offset;
        let name = copy_name(&original.name, self.objects.iter().map(|o| o.name.as_str()));
        let (mesh, primitive) = (original.mesh.clone(), original.primitive);
        let mut copy = SceneObject::from_local(&self.context, name, mesh, pose, self.next_color());
        copy.primitive = primitive;
        self.objects.push(copy);
        self.objects.len() - 1
    }

    /// Copia del objeto, desplazada en X para que quede al lado del original.
    fn clone_object(&mut self, object: usize) {
        let (min, max) = self.objects[object].world_bbox;
        let copy = self.add_copy(object, vec3((max.x - min.x) * 1.1, 0.0, 0.0));
        self.status = match i18n::current() {
            Lang::Es => format!("Clonado como {}", self.objects[copy].name),
            Lang::En => format!("Cloned as {}", self.objects[copy].name),
        };
        self.selected = Some(copy);
    }

    /// Crea las copias de "Clonar en matriz" y cierra su sección.
    fn create_array(&mut self) {
        let Some(params) = self.array.take() else { return };
        let Some(obj) = self.objects.get(params.object) else { return };
        let (min, max) = obj.world_bbox;
        let offsets = array_offsets(params.columns, params.rows, max - min, params.gap_x, params.gap_y);
        for &offset in &offsets {
            self.add_copy(params.object, offset);
        }
        let n = offsets.len();
        self.status = match i18n::current() {
            Lang::Es => format!("Creadas {n} copias de {}", self.objects[params.object].name),
            Lang::En => format!("Created {n} copies of {}", self.objects[params.object].name),
        };
        self.selected = Some(params.object);
    }

    /// Cambia la escala del objeto manteniendo su base a la misma altura.
    fn set_scale(&mut self, object: usize, scale: Vec3) {
        let obj = &mut self.objects[object];
        let bottom = obj.world_bbox.0.z;
        let mut pose = obj.pose;
        pose.scale = vec3(scale.x.max(MIN_SCALE), scale.y.max(MIN_SCALE), scale.z.max(MIN_SCALE));
        obj.set_pose(pose);
        pose.translation.z += bottom - obj.world_bbox.0.z;
        obj.set_pose(pose);
    }

    /// Aplica la escala que entrega el gizmo; con la proporción bloqueada, cualquier asa
    /// escala los tres ejes por igual.
    fn drag_scale(&mut self, object: usize, raw: Pose) {
        let start = *self.drag_start.get_or_insert(self.objects[object].pose);
        self.drag_raw = Some(raw);
        let scale = if self.scale_locked { start.scale * uniform_factor(start.scale, raw.scale) } else { raw.scale };
        self.set_scale(object, scale);
    }

    /// Terminada una escala, la absorbe en los parámetros de la primitiva si la forma sigue
    /// siendo de ese tipo; si no, la pieza pasa a ser una malla común con su escala.
    fn settle_scale(&mut self) {
        let Some(obj) = self.selected.and_then(|i| self.objects.get_mut(i)) else { return };
        let Some(primitive) = obj.primitive else { return };
        let s = obj.pose.scale;
        if s == vec3(1.0, 1.0, 1.0) {
            return;
        }
        match primitive.scaled([s.x, s.y, s.z]).and_then(|p| p.mesh().map(|m| (p, m))) {
            Some((scaled, mesh)) => {
                // Las mallas de las primitivas están centradas: la forma en el mundo no cambia.
                obj.pose.scale = vec3(1.0, 1.0, 1.0);
                obj.set_local_mesh(&self.context, mesh);
                obj.primitive = Some(scaled);
            }
            None => {
                obj.primitive = None;
                self.status = match i18n::current() {
                    Lang::Es => format!("{} ya no es una primitiva (escala no uniforme)", obj.name),
                    Lang::En => format!("{} is no longer a primitive (non-uniform scale)", obj.name),
                };
            }
        }
        self.facets = None;
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
        // La medición y la regla a medio poner no sobreviven a un cambio de herramienta.
        if self.tool != Some(Tool::Measure) {
            self.measurement = Measurement::default();
        }
        if self.tool != Some(Tool::Ruler) {
            self.ruler_start = None;
        }
        if !matches!(self.tool, Some(Tool::Measure | Tool::Ruler)) {
            self.hover_snap = None;
        }
        if self.array.is_some_and(|a| Some(a.object) != self.selected) {
            self.array = None;
        }
        let Some(selected) = self.selected else {
            if self.tool.is_some_and(Tool::needs_object) {
                self.tool = None;
            }
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

    /// Clic con Medir o Regla en un punto (ya con snap).
    fn measure_click(&mut self, p: Vec3) {
        match self.tool {
            Some(Tool::Measure) => self.measurement.click(p),
            Some(Tool::Ruler) => match self.ruler_start.take() {
                None => self.ruler_start = Some(p),
                Some(start) => {
                    let axis = measure::axis_between(start, p, self.axis_lock);
                    match Ruler::between(start, p, axis, self.ruler_spacing) {
                        Some(ruler) => self.rulers.push(ruler),
                        // Segundo clic sin separarse en el eje: se vuelve a empezar desde ahí.
                        None => self.ruler_start = Some(p),
                    }
                }
            },
            _ => {}
        }
    }

    /// Esc: descarta el punto pendiente de Medir o Regla; si no lo hay, cierra la herramienta.
    fn escape(&mut self) {
        if self.ruler_start.take().is_some() {
            return;
        }
        if self.tool == Some(Tool::Measure) && self.measurement.a.is_some() && self.measurement.b.is_none() {
            self.measurement = Measurement::default();
            return;
        }
        self.tool = None;
    }

    /// Aplica la pose que entrega el gizmo al arrastrar el plano de corte. Si se está
    /// desplazando (no inclinando), se engancha a la marca de regla más cercana al punto
    /// donde el plano cruza cada regla.
    fn drag_cut_plane(&mut self, raw: Pose, tolerance: f32) {
        let start = *self.drag_start.get_or_insert(self.cut_pose);
        self.drag_raw = Some(raw);
        self.cut_pose = raw;
        self.snapped_marks.clear();
        if (raw.translation - start.translation).magnitude() < 1e-6 {
            return;
        }
        let normal = raw.rotation * vec3(0.0, 0.0, 1.0);
        if let Some((shift, mark)) = measure::plane_snap(&self.rulers, normal, raw.translation, tolerance) {
            self.cut_pose.translation += shift;
            self.snapped_marks.push(mark);
        }
    }

    /// Aplica la pose que entrega el gizmo al arrastrar con Mover, enganchando el objeto a
    /// las marcas de las reglas en los ejes en que se está moviendo. `tolerance` es la
    /// distancia de enganche en unidades del mundo.
    fn drag_to(&mut self, object: usize, raw: Pose, tolerance: f32) {
        let start = *self.drag_start.get_or_insert(self.objects[object].pose);
        self.drag_raw = Some(raw);
        self.objects[object].set_pose(raw);
        self.snapped_marks.clear();
        if self.rulers.is_empty() {
            return;
        }
        let moved = raw.translation - start.translation;
        let axes: Vec<Axis> = Axis::ALL.into_iter().filter(|a| moved.dot(a.unit()).abs() > 1e-6).collect();
        let obj = &self.objects[object];
        let (min, max) = obj.world_bbox;
        let features = obj.features();
        let mut points = vec![min, (min + max) * 0.5, max];
        points.extend(features.corners.iter().chain(&features.centers).map(|&p| raw.apply(p)));
        let snap = measure::ruler_snap(&self.rulers, &points, tolerance, &axes);
        if !snap.marks.is_empty() {
            let mut pose = raw;
            pose.translation += snap.offset;
            self.objects[object].set_pose(pose);
            self.snapped_marks = snap.marks;
        }
    }

    /// Punto con snap bajo el píxel dado: rasgos de los objetos visibles, la superficie bajo
    /// el cursor o el suelo.
    fn snap_at(&self, context: &Context, camera: &Camera, pixel: PhysicalPoint) -> Option<Snap> {
        let visible: Vec<&SceneObject> = self.objects.iter().filter(|o| o.visible).collect();
        let geometries = visible.iter().map(|o| &o.model.geometry);
        let surface = pick(context, camera, pixel, geometries, Cull::None).ok().flatten().map(|h| h.position);
        let targets: Vec<(&Features, Pose)> = visible.iter().map(|o| (o.features(), o.pose)).collect();
        snap::find_snap(camera, pixel, &targets, surface)
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
        self.array = None;
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
    scale: Option<(usize, Vec3)>,
    create_array: bool,
}

fn main() {
    i18n::set(i18n::detect());
    // La ventana se crea con winit directamente: `WindowSettings` de three-d no permite
    // ponerle ícono.
    let event_loop = winit::event_loop::EventLoop::new();
    let winit_window = winit::window::WindowBuilder::new()
        .with_title("simpleSTL")
        .with_min_inner_size(winit::dpi::LogicalSize::new(640.0, 480.0))
        .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0))
        .with_window_icon(window_icon())
        .build(&event_loop)
        .unwrap();
    winit_window.focus_window();
    let window = Window::from_winit_window(winit_window, event_loop, SurfaceSettings::default(), false).unwrap();
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
        measurement: Measurement::default(),
        axis_lock: None,
        rulers: Vec::new(),
        ruler_start: None,
        ruler_spacing: 10.0,
        hover_snap: None,
        drag_start: None,
        drag_raw: None,
        snapped_marks: Vec::new(),
        scale_locked: true,
        array: None,
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
        let mut pointer_over_ui = false;

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
                // La barra usa el ancho libre a la izquierda del cubo de vista.
                let max_width = (cube_rect.min.x - view_rect.min.x - 32.0).max(120.0);
                let has_selection = app.selected.is_some();
                blocked.push(toolbar::show(ui.ctx(), view_rect, max_width, has_selection, &mut app.tool));
                draw_overlays(ui.ctx(), &camera, view_rect, dpr, window_viewport.height, &app);
                dragged_pose = manipulator.update(ui.ctx(), &camera, view_rect, &blocked, gizmo_target(&app));
                // Menús y listas desplegables pueden quedar sobre el visor: un clic en ellos no
                // debe llegar a la escena (deseleccionaría el objeto).
                pointer_over_ui = ui.ctx().is_pointer_over_egui();
            },
        );

        // Animaciones de egui (menús, tooltips, cursor de texto): seguir dibujando mientras las pida.
        // (2 y no 1: al final del cuadro se descuenta uno.)
        if gui.context().requested_repaint_last_pass() || gui.context().has_requested_repaint() {
            extra_frames = extra_frames.max(2);
        }
        if let (Some(pose), Some(i)) = (dragged_pose, app.selected) {
            match app.tool {
                // Enganche a las reglas: 10 puntos de pantalla, en unidades del mundo.
                Some(Tool::Cut) => app.drag_cut_plane(pose, 10.0 * dpr * world_per_pixel(&camera, pose.translation)),
                Some(Tool::Move) => app.drag_to(i, pose, 10.0 * dpr * world_per_pixel(&camera, pose.translation)),
                Some(Tool::Scale) => app.drag_scale(i, pose),
                _ => app.objects[i].set_pose(pose),
            }
        }
        if orthographic != control.is_orthographic() {
            control.set_orthographic(&mut camera, orthographic);
        }
        // Atajos de teclado (egui ya marcó como manejadas las teclas si hay un campo de texto activo).
        let has_selection = app.selected.is_some();
        let measuring = matches!(app.tool, Some(Tool::Measure | Tool::Ruler));
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
                (Key::Escape, false) if app.tool.is_some() => app.escape(),
                (Key::Delete, false) if has_selection => actions.delete = app.selected,
                // Con Medir o Regla, X/Y/Z fijan el eje (o lo liberan si ya estaba fijado).
                (key @ (Key::X | Key::Y | Key::Z), false) if measuring => {
                    let axis = match key {
                        Key::X => Axis::X,
                        Key::Y => Axis::Y,
                        _ => Axis::Z,
                    };
                    app.axis_lock = if app.axis_lock == Some(axis) { None } else { Some(axis) };
                }
                (key, false) => match Tool::ALL.into_iter().find(|t| t.key() == key) {
                    Some(tool) if has_selection || !tool.needs_object() => {
                        app.tool = if app.tool == Some(tool) { None } else { Some(tool) };
                    }
                    _ => *handled = false,
                },
                _ => *handled = false,
            }
        }
        if let Some((i, pose)) = actions.pose {
            app.objects[i].set_pose(pose);
        }
        if let Some((i, scale)) = actions.scale {
            app.set_scale(i, scale);
        }
        match cube_action {
            Some(viewcube::Action::LookFrom(direction)) => control.look_from(direction),
            Some(viewcube::Action::Orbit(delta)) => control.orbit_by_drag(&mut camera, delta),
            None => {}
        }
        // Posición física (three-d) en el visor libre de controles, si corresponde.
        let in_scene = |p: PhysicalPoint| {
            let point = egui::pos2(p.x / dpr, (window_viewport.height as f32 - p.y) / dpr);
            !pointer_over_ui && p.x >= viewport.x as f32 && !blocked.iter().any(|r| r.contains(point))
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

        // Punto con snap bajo el cursor, para Medir y Regla.
        if measuring {
            if let Some(p) = last_motion {
                app.hover_snap = if in_scene(p) { app.snap_at(&context, &camera, p) } else { None };
            }
        }

        if let Some(click) = manipulator.consume_events(&mut frame_input.events) {
            let p = click.position;
            if in_scene(p) && measuring {
                if let Some(snap) = app.snap_at(&context, &camera, p) {
                    app.measure_click(snap.position);
                }
            } else if in_scene(p) {
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
            app.array = None;
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
        if actions.create_array {
            app.create_array();
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
            app.settle_scale();
            app.commit_changes();
            app.drag_start = None;
            app.drag_raw = None;
            app.snapped_marks.clear();
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

/// Ícono de la ventana (barra de título y de tareas). Wayland no lo usa: allí el escritorio
/// toma el del archivo .desktop.
fn window_icon() -> Option<winit::window::Icon> {
    let decoder = png::Decoder::new(&include_bytes!("../packaging/simplestl-64.png")[..]);
    let mut reader = decoder.read_info().ok()?;
    let mut rgba = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut rgba).ok()?;
    if frame.color_type != png::ColorType::Rgba || frame.bit_depth != png::BitDepth::Eight {
        return None; // scripts/build-icons.sh lo genera como RGBA de 8 bits
    }
    rgba.truncate(frame.buffer_size());
    winit::window::Icon::from_rgba(rgba, frame.width, frame.height).ok()
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
        Tool::Move => Some((app.drag_raw.unwrap_or(obj.pose), GizmoSetup::translate())),
        Tool::Rotate => Some((obj.pose, GizmoSetup::rotate())),
        Tool::Scale => Some((app.drag_raw.unwrap_or(obj.pose), GizmoSetup::scale())),
        Tool::Cut => Some((app.drag_raw.unwrap_or(app.cut_pose), GizmoSetup::cut_plane())),
        Tool::Boolean | Tool::PlaceOnFace | Tool::Measure | Tool::Ruler => None,
    }
}

/// Unidades del mundo por píxel físico cerca de `p`.
fn world_per_pixel(camera: &Camera, p: Vec3) -> f32 {
    let (a, b) = (camera.pixel_at_position(p), camera.pixel_at_position(p + camera.right_direction()));
    1.0 / vec2(b.x - a.x, b.y - a.y).magnitude().max(1e-6)
}

/// Reglas, la medición en curso y el marcador de snap, dibujados sobre el visor.
fn draw_overlays(ctx: &egui::Context, camera: &Camera, view: egui::Rect, dpr: f32, window_height: u32, app: &App) {
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("overlays"))).with_clip_rect(view);
    let project = |p: Vec3| {
        if (p - camera.position()).dot(camera.view_direction()) <= camera.z_near() {
            return None; // detrás de la cámara
        }
        let px = camera.pixel_at_position(p);
        Some(egui::pos2(px.x / dpr, (window_height as f32 - px.y) / dpr))
    };
    for ruler in &app.rulers {
        measure::draw_ruler(&painter, &project, ruler, &app.snapped_marks);
    }
    // Vista previa de "Clonar en matriz": la caja de cada copia.
    if let Some((params, obj)) = app.array.and_then(|a| app.objects.get(a.object).map(|o| (a, o))) {
        let (min, max) = obj.world_bbox;
        let stroke = egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(255, 200, 60));
        for offset in array_offsets(params.columns, params.rows, max - min, params.gap_x, params.gap_y) {
            let corner = |k: usize| {
                let pick = |bit: usize, lo: f32, hi: f32| if k & bit == 0 { lo } else { hi };
                vec3(pick(1, min.x, max.x), pick(2, min.y, max.y), pick(4, min.z, max.z)) + offset
            };
            // Aristas de la caja: pares de esquinas que difieren en un solo eje.
            for (a, b) in (0..8).flat_map(|a| [1, 2, 4].map(|bit| (a, a | bit))).filter(|(a, b)| a != b) {
                if let (Some(pa), Some(pb)) = (project(corner(a)), project(corner(b))) {
                    painter.line_segment([pa, pb], stroke);
                }
            }
        }
    }
    let hover = app.hover_snap.map(|s| s.position);
    match app.tool {
        Some(Tool::Measure) => {
            if let Some(a) = app.measurement.a {
                if let Some(b) = app.measurement.b.or(hover) {
                    measure::draw_measurement(&painter, &project, a, b, measure::axis_between(a, b, app.axis_lock));
                }
            }
        }
        Some(Tool::Ruler) => {
            if let (Some(start), Some(end)) = (app.ruler_start, hover) {
                let axis = measure::axis_between(start, end, app.axis_lock);
                if let Some(preview) = Ruler::between(start, end, axis, app.ruler_spacing) {
                    measure::draw_ruler(&painter, &project, &preview, &[]);
                }
            }
        }
        _ => return,
    }
    for point in [app.measurement.a, app.ruler_start].into_iter().flatten() {
        if let Some(p) = project(point) {
            painter.circle_filled(p, 4.0, egui::Color32::WHITE);
        }
    }
    if let Some(snap) = app.hover_snap {
        if let Some(p) = project(snap.position) {
            measure::draw_snap(&painter, p, snap.kind);
        }
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
        ui.weak(tr("Todas las medidas están en mm", "All measurements are in mm"));
        ui.weak(format!("{} {} mm", tr("Base z = 0 · grilla cada", "Base z = 0 · grid every"), app.grid_spacing));
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
    Array,
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
    if ui.button(tr("Clonar en matriz…", "Clone as array…")).clicked() {
        action = Some(ObjectAction::Array);
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
                    // Truncado al ancho libre: un nombre largo no debe tapar el botón "…".
                    let button = egui::Button::selectable(app.selected == Some(i), obj.name.as_str()).truncate();
                    ui.add(button).on_hover_text(&obj.name)
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
                Some(ObjectAction::Array) => {
                    app.selected = Some(i);
                    app.array = Some(ArrayParams { object: i, columns: 2, rows: 2, gap_x: 5.0, gap_y: 5.0 });
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
        let s = obj.pose.scale;
        let size = local_max - local_min;
        let size = vec3(size.x * s.x, size.y * s.y, size.z * s.z);
        let closed = obj.topology.is_closed();
        let volume = obj.mesh.volume() * s.x * s.y * s.z;
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
    if app.array.is_some_and(|a| Some(a.object) == app.selected) {
        ui.separator();
        array_section(ui, app, actions);
    }

    if app.selected.is_some() || app.tool.is_some() {
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
    let Some(tool) = app.tool else {
        ui.weak(tr(
            "Herramientas en la barra superior: Mover (M), Rotar (R), Escalar (S), Corte (C), Booleana (B), Apoyar en cara (F), Medir (L), Regla (G). Esc cierra la activa.",
            "Tools in the top bar: Move (M), Rotate (R), Scale (S), Cut (C), Boolean (B), Place on face (F), Measure (L), Ruler (G). Esc closes the active one.",
        ));
        return;
    };
    ui.label(egui::RichText::new(tool.label()).strong());
    match tool {
        Tool::Measure => return measure_section(ui, app),
        Tool::Ruler => return ruler_section(ui, app),
        _ => {}
    }
    let Some(i) = app.selected else { return };
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
            if !app.rulers.is_empty() {
                ui.weak(tr(
                    "Al arrastrar, los bordes, el centro, las esquinas y los centros de agujeros se enganchan a las marcas de las reglas.",
                    "While dragging, the edges, center, corners and hole centers snap to the ruler marks.",
                ));
            }
        }
        Tool::Rotate => {
            if ui.button(tr("Restablecer rotación", "Reset rotation")).clicked() {
                actions.pose = Some((i, Pose { rotation: Pose::at(pose.translation).rotation, ..pose }));
            }
            ui.weak(tr("Arrastra los anillos. Ctrl: pasos de 15°.", "Drag the rings. Ctrl: steps of 15°."));
        }
        Tool::Scale => scale_section(ui, app, actions, i),
        Tool::Cut => cut_section(ui, app, actions, i),
        Tool::Boolean => boolean_panel(ui, app, actions),
        Tool::Measure | Tool::Ruler => {}
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

/// Escala por eje (en los ejes del objeto), en porcentaje o como medida objetivo.
fn scale_section(ui: &mut egui::Ui, app: &mut App, actions: &mut UiActions, i: usize) {
    let obj = &app.objects[i];
    let (local_min, local_max) = obj.mesh.bounding_box();
    let base = local_max - local_min;
    let scale = obj.pose.scale;
    ui.checkbox(&mut app.scale_locked, tr("Escala uniforme", "Uniform scale"))
        .on_hover_text(tr("Cualquier eje escala los tres por igual", "Any axis scales all three equally"));
    let mut factor: Option<(usize, f32)> = None;
    egui::Grid::new("scale").num_columns(3).show(ui, |ui| {
        ui.label("");
        ui.label("%");
        ui.label(tr("Tamaño", "Size"));
        ui.end_row();
        for (k, axis) in Axis::ALL.into_iter().enumerate() {
            ui.label(egui::RichText::new(axis.name()).color(axis.color()));
            let mut percent = scale[k] * 100.0;
            let drag = egui::DragValue::new(&mut percent).range(MIN_SCALE * 100.0..=f32::MAX).speed(0.5).max_decimals(2).suffix(" %");
            if ui.add(drag).changed() {
                factor = Some((k, percent / 100.0 / scale[k]));
            }
            let mut size = base[k] * scale[k];
            let drag = egui::DragValue::new(&mut size).range(primitives::MIN_SIZE..=f32::MAX).speed(0.1).max_decimals(2);
            if ui.add_enabled(base[k] > 0.0, drag).changed() {
                factor = Some((k, size / (base[k] * scale[k])));
            }
            ui.end_row();
        }
    });
    if let Some((k, f)) = factor {
        let mut new = scale;
        if app.scale_locked {
            new *= f;
        } else {
            new[k] *= f;
        }
        actions.scale = Some((i, new));
    }
    if ui.button(tr("Restablecer escala", "Reset scale")).clicked() {
        actions.scale = Some((i, vec3(1.0, 1.0, 1.0)));
    }
    ui.weak(tr(
        "Arrastra los cubos del manipulador; el círculo escala en todos los ejes. Ctrl: pasos de 10 %. Los ejes son los del objeto.",
        "Drag the manipulator cubes; the circle scales all axes. Ctrl: steps of 10 %. Axes are the object's own.",
    ));
}

/// "Clonar en matriz": columnas × filas con un hueco entre piezas.
fn array_section(ui: &mut egui::Ui, app: &mut App, actions: &mut UiActions) {
    let Some(params) = app.array.as_mut() else { return };
    ui.label(egui::RichText::new(tr("Clonar en matriz", "Clone as array")).strong());
    egui::Grid::new("array").num_columns(2).show(ui, |ui| {
        ui.label(tr("Columnas (X)", "Columns (X)"));
        ui.add(egui::DragValue::new(&mut params.columns).range(1..=20));
        ui.end_row();
        ui.label(tr("Filas (Y)", "Rows (Y)"));
        ui.add(egui::DragValue::new(&mut params.rows).range(1..=20));
        ui.end_row();
        ui.label(tr("Hueco X", "Gap X"));
        ui.add(egui::DragValue::new(&mut params.gap_x).range(0.0..=f32::MAX).speed(0.1).max_decimals(2));
        ui.end_row();
        ui.label(tr("Hueco Y", "Gap Y"));
        ui.add(egui::DragValue::new(&mut params.gap_y).range(0.0..=f32::MAX).speed(0.1).max_decimals(2));
        ui.end_row();
    });
    let total = params.columns * params.rows;
    ui.weak(match i18n::current() {
        Lang::Es => format!("{total} piezas en total ({} copias nuevas). El hueco es la distancia libre entre piezas.", total - 1),
        Lang::En => format!("{total} pieces in total ({} new copies). The gap is the free distance between pieces.", total - 1),
    });
    ui.horizontal(|ui| {
        actions.create_array = ui.add_enabled(total > 1, egui::Button::new(tr("Crear", "Create"))).clicked();
        if ui.button(tr("Cancelar", "Cancel")).clicked() {
            app.array = None;
        }
    });
}

/// Eje de Medir y Regla: automático (el dominante) o fijado.
fn axis_selector(ui: &mut egui::Ui, lock: &mut Option<Axis>) {
    ui.horizontal(|ui| {
        ui.label(tr("Eje", "Axis"));
        ui.selectable_value(lock, None, tr("Auto", "Auto"));
        for axis in Axis::ALL {
            ui.selectable_value(lock, Some(axis), egui::RichText::new(axis.name()).color(axis.color()));
        }
    });
}

fn measure_section(ui: &mut egui::Ui, app: &mut App) {
    ui.weak(tr(
        "Haz clic en dos puntos. El cursor se engancha a centros de agujeros, esquinas y aristas. X/Y/Z fijan el eje.",
        "Click two points. The cursor snaps to hole centers, corners and edges. X/Y/Z lock the axis.",
    ));
    axis_selector(ui, &mut app.axis_lock);
    let hover = app.hover_snap.map(|s| s.position);
    match (app.measurement.a, app.measurement.b.or(hover)) {
        (Some(a), Some(b)) => {
            let axis = measure::axis_between(a, b, app.axis_lock);
            let value = (b - a).dot(axis.unit()).abs();
            ui.label(egui::RichText::new(format!("{value:.2}  ({})", axis.name())).size(18.0).color(axis.color()));
            let d = b - a;
            ui.weak(format!("ΔX {:.2} · ΔY {:.2} · ΔZ {:.2}", d.x, d.y, d.z));
        }
        (Some(_), None) => {
            ui.weak(tr("Haz clic en el segundo punto.", "Click the second point."));
        }
        _ => {
            ui.weak(tr("Haz clic en el primer punto.", "Click the first point."));
        }
    }
}

fn ruler_section(ui: &mut egui::Ui, app: &mut App) {
    ui.weak(tr(
        "Haz clic donde empieza y luego hacia dónde se extiende. Al mover un objeto, se engancha a las marcas.",
        "Click where it starts, then where it extends to. Moving objects snap to its marks.",
    ));
    axis_selector(ui, &mut app.axis_lock);
    ui.horizontal(|ui| {
        ui.label(tr("Marcas cada", "Marks every"));
        ui.add(egui::DragValue::new(&mut app.ruler_spacing).range(measure::MIN_SPACING..=f32::MAX).speed(0.1).max_decimals(2));
    });
    let mut remove = None;
    for (k, ruler) in app.rulers.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ruler.axis.name()).strong().color(ruler.axis.color()));
            ui.add(egui::DragValue::new(&mut ruler.length).range(measure::MIN_SPACING..=f32::MAX).speed(0.1).max_decimals(2))
                .on_hover_text(tr("Largo", "Length"));
            ui.label(tr("cada", "every"));
            ui.add(egui::DragValue::new(&mut ruler.spacing).range(measure::MIN_SPACING..=f32::MAX).speed(0.1).max_decimals(2));
            if ui.button(tr("Quitar", "Remove")).clicked() {
                remove = Some(k);
            }
        });
    }
    if let Some(k) = remove {
        app.rulers.remove(k);
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
    if !app.rulers.is_empty() {
        ui.weak(tr(
            "Al desplazarlo, se engancha a las marcas de las reglas que cruza.",
            "While moving it, it snaps to the marks of the rulers it crosses.",
        ));
    }
    let button = ui.add_enabled(closed, egui::Button::new(tr("Cortar", "Cut")));
    actions.cut = button.on_disabled_hover_text(tr("La malla debe ser cerrada", "The mesh must be closed")).clicked();
}

#[cfg(test)]
mod tests {
    use super::{array_offsets, copy_name, uniform_factor, window_icon};

    #[test]
    fn window_icon_decodes() {
        assert!(window_icon().is_some());
    }
    use three_d::vec3;

    #[test]
    fn array_offsets_use_size_plus_gap_and_skip_the_original() {
        let offsets = array_offsets(3, 2, vec3(20.0, 10.0, 5.0), 5.0, 8.0);
        assert_eq!(offsets.len(), 5);
        assert!(!offsets.contains(&vec3(0.0, 0.0, 0.0)));
        assert!(offsets.contains(&vec3(50.0, 0.0, 0.0)));
        assert!(offsets.contains(&vec3(25.0, 18.0, 0.0)));
        assert!(array_offsets(1, 1, vec3(1.0, 1.0, 1.0), 0.0, 0.0).is_empty());
    }

    #[test]
    fn locked_scale_follows_the_axis_that_changed_most() {
        let start = vec3(1.0, 2.0, 1.0);
        assert_eq!(uniform_factor(start, vec3(1.0, 3.0, 1.0)), 1.5);
        assert_eq!(uniform_factor(start, vec3(0.5, 2.0, 1.0)), 0.5);
    }

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
