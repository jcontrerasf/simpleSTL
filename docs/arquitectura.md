# Arquitectura

simpleSTL es un único binario. three-d pone la ventana, OpenGL y la integración con
egui; Manifold hace la geometría sólida (booleanas, cortes, envolventes,
primitivas).

## Módulos

| Archivo | Responsabilidad |
|---|---|
| `src/main.rs` | Estado de la aplicación (`App`, `SceneObject`), ciclo de dibujo, panel lateral |
| `src/mesh.rs` | `MeshData`: malla indexada independiente del renderer. Lectura y escritura de STL, topología, volumen, centroide |
| `src/csg.rs` | Puente con Manifold: booleanas, `split` por plano, envolvente convexa, primitivas |
| `src/primitives.rs` | `Primitive`: parámetros de cubo, esfera, cilindro y cono, y su malla |
| `src/place_on_face.rs` | Caras candidatas para apoyar y la rotación que las deja en el suelo |
| `src/manipulator.rs` | `Pose` y el manipulador 3D (transform-gizmo) alimentado con eventos de three-d |
| `src/camera.rs` | Cámara tornamesa (azimut y elevación), desplazamiento, zoom, encuadre y transiciones animadas |
| `src/viewcube.rs` | Cubo de navegación dibujado con egui |
| `src/toolbar.rs` | `Tool` y la barra superior |
| `src/ground.rs` | Base translúcida y grilla en z = 0 |

## Modelo de datos

Cada `SceneObject` guarda:

- **`mesh`**: la malla en coordenadas **locales**, centrada en el origen al crearse.
- **`pose`**: traslación y rotación (cuaternión). Como la malla está centrada, la
  traslación es también el centro de rotación, y por eso el manipulador rota el
  objeto sobre sí mismo.
- **`world_bbox`**: la caja envolvente en el mundo, recalculada en `set_pose`.
- **`model`**: la copia en GPU (`Gm<Mesh, PhysicalMaterial>`). La transformación de
  la pose se aplica como matriz de modelo; los vértices no se tocan.
- **`primitive`**: los parámetros, si es una primitiva, para poder regenerarla.

Las operaciones que necesitan coordenadas del mundo (booleanas, cortes, exportar,
apoyar en cara) usan `world_mesh()`, que devuelve la malla ya transformada. El
resultado de una booleana o un corte es una malla nueva con pose sin rotación.

El render usa sombreado plano: `MeshData::to_cpu_mesh` duplica los vértices por
triángulo para que cada uno tenga la normal de su cara. `MeshData` conserva los
vértices compartidos, que Manifold necesita para reconocer una malla cerrada.

## Un cuadro

`window.render_loop` en `main.rs`, en orden:

1. **Viewport**: el visor ocupa lo que deja libre el panel lateral, de ancho fijo
   300 px.
2. **`Manipulator::track_input`**: lee el ratón **antes** de que egui marque
   eventos como consumidos.
3. **`gui.update`**: dibuja el panel lateral, el cubo de vista, la barra de
   herramientas y el manipulador. El cubo y la barra se registran como zonas
   bloqueadas.
4. **Aplicar resultados**:
   - la nueva pose del objeto o del plano de corte, según la herramienta;
   - los atajos de teclado;
   - las acciones del cubo de vista.
5. **Clic en la escena**: `consume_events` distingue un clic de un arrastre.
   - Con *Apoyar en cara*, primero se prueba `pick` contra las caras resaltadas.
   - Si no, `pick` contra los objetos visibles decide la selección.
6. **Acciones del panel** (`UiActions`): abrir, exportar, eliminar, clonar,
   primitivas, booleana, corte. Se ejecutan fuera del closure de egui para evitar
   conflictos de préstamos.
7. **Cambios de herramienta o selección**: `on_tool_or_selection_changed` reinicia
   el plano de corte, fija A en la booleana e invalida las caras.
8. **Cámara**: órbita, desplazamiento y zoom con los eventos que nadie consumió, y
   avance de las transiciones animadas.
9. **Render**:
   - primero los opacos;
   - después los transparentes (base, plano de corte, caras), que three-d ordena
     por distancia;
   - al final egui.

La ventana solo redibuja cuando hace falta (`wait_next_event`). Un contador de
cuadros extra se renueva con cada evento, mientras dura una animación de cámara y
mientras egui pida repintar (`requested_repaint_last_pass`). Así los menús y
tooltips terminan su animación sin mantener la CPU ocupada en reposo.

## Detalles de implementación

### Manipulador y eventos

`transform-gizmo-egui` trae `GizmoExt::interact`, que registra un widget de egui
bajo el cursor en todo momento. Con eso, three-d vería cada arrastre como uso de la
interfaz y bloquearía la cámara. Por eso `Manipulator` llama directamente a
`Gizmo::update` con la posición y los botones leídos de los eventos de three-d, y
usa egui solo para dibujar. Si un arrastre empieza sobre el gizmo, sus eventos se
marcan como consumidos para que la cámara no los reciba.

`GizmoSetup` define qué asas aparecen:

| Herramienta | Asas |
|---|---|
| Mover | traslación en X/Y/Z globales |
| Rotar | rotación en X/Y/Z globales |
| Corte | traslación en Z **local** (a lo largo de la normal del plano) y rotación en X/Y locales (para inclinarlo) |

El plano de corte también es una `Pose`: su normal es `rotation * Z`.

### Cámara tornamesa

Con un vector "arriba" fijo en +Z, la vista cenital queda indefinida. La cámara
guarda azimut y elevación y calcula "arriba" como la derivada de la dirección
respecto de la elevación. Ese vector sigue definido en los polos, porque lo fija el
azimut. Las vistas del cubo interpolan azimut y elevación por el camino más corto.

### Apoyar en cara

1. **Envolvente convexa** (`csg::convex_hull`): un objeto apoyado toca el suelo con
   su envolvente, así que las caras cóncavas nunca sirven.
2. **Agrupar por plano**: los triángulos de la envolvente con normales a menos de 1°
   y la misma distancia al origen forman una cara. Su área es la suma de las áreas.
3. **Estabilidad**: el centro de masa se obtiene sumando los centroides de los
   tetraedros (origen, a, b, c) ponderados por su volumen con signo. Una cara es
   estable si ese punto, proyectado sobre su plano, cae dentro de ella.
4. **Filtro**: se descartan las caras de menos de 0,5% del área total (el facetado
   de superficies curvas) y se conservan hasta 64.
5. **Rotación** (`placement_rotation`):
   - la más corta que lleva la normal a −Z;
   - si *Alinear* está activo, un giro extra sobre Z hasta que la mayor cara que
     quedó vertical mire a ±X o ±Y;
   - después, una traslación en Z hasta que el punto más bajo quede en 0.

### Booleanas y cortes

`csg.rs` convierte `MeshData` a `Manifold` en f64. Manifold rechaza las mallas que
no son cerradas, y la interfaz lo anticipa con `MeshData::topology`: cada arista
dirigida debe aparecer una vez y su inversa también. `split` recibe cualquier
normal, así que los cortes inclinados no necesitan código especial.

## Compilación de Manifold

`manifold-csg-sys` compila Manifold ejecutando `cmake --build --parallel` sin
número, lo que equivale a `make -j` sin límite. En un equipo con 8 GB eso agotó la
memoria. `scripts/build-manifold.sh` compila la versión que espera el crate
(v3.5.3), estática, sin TBB y con `-j2`, en `third_party/manifold-lib`.
`.cargo/config.toml` apunta allí con `MANIFOLD_CSG_LIB_DIR` y
`MANIFOLD_CSG_LIB_KIND=static`.

Al actualizar `manifold3d` hay que revisar `MANIFOLD_VERSION` en el `build.rs` de
`manifold-csg-sys` y ajustar `MANIFOLD_REF` en el script.
