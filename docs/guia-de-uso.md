# Guía de uso

## La ventana

- **Panel izquierdo**:
  - arriba, los botones *Abrir STL…*, *Primitivas* y *Encuadrar*;
  - luego, la lista de objetos y la información del seleccionado;
  - debajo, las opciones de la herramienta activa;
  - al pie, el último mensaje de estado y el espaciado de la grilla.
- **Visor 3D**:
  - arriba a la izquierda, la barra de herramientas, que aparece al seleccionar un
    objeto;
  - arriba a la derecha, el cubo de vista;
  - la base translúcida en z = 0 marca el "suelo".

## Navegar

| Acción | Cómo |
|---|---|
| Orbitar | Arrastrar con el botón izquierdo en una zona vacía |
| Desplazar | Arrastrar con el botón derecho o central |
| Zoom | Rueda |
| Encuadrar todo lo visible | Botón **Encuadrar** |

La órbita es tipo tornamesa: gira alrededor del eje Z y la elevación se detiene en
la vista cenital, sin dar la vuelta.

### Perspectiva u ortogonal

El botón bajo el cubo de vista, o la tecla **O**, alterna entre *Perspectiva* y
*Ortogonal*. En ortogonal las líneas paralelas se mantienen paralelas, útil para
comparar medidas en vistas de frente, lado o planta. El encuadre, el zoom y el
desplazamiento funcionan igual en ambos modos.

### Cubo de vista

Cada cara del cubo está dividida en 3 × 3 zonas:

- el **centro** da la vista de esa cara (Frente, Atrás, Izquierda, Derecha,
  Superior, Inferior);
- los **bordes** dan la vista de la arista, a 45° entre dos caras;
- las **esquinas** dan la vista isométrica de ese vértice.

Al pasar el ratón, la zona se resalta en azul. Arrastrar sobre el cubo también
orbita la cámara. Se sigue la convención de FreeCAD: "Frente" mira hacia +Y.

## Objetos

Cada fila de la lista tiene:

- **Ojo**: muestra u oculta el objeto. Tachado significa oculto.
- **Color**: clic para cambiarlo.
- **Nombre**: un clic lo selecciona y un doble clic lo renombra. Enter o hacer clic
  afuera confirma y Esc cancela.
- **…** (o clic derecho sobre el nombre): *Renombrar*, *Clonar*, *Exportar STL…* y
  *Eliminar* (también con la tecla **Supr** sobre el objeto seleccionado).

Al clonar, la copia aparece al lado del original y se llama `nombre (copia1)`,
`nombre (copia2)`… Clonar una copia continúa la numeración en vez de acumular
sufijos.

La información del objeto seleccionado incluye:

- triángulos y vértices;
- tamaño (caja envolvente en sus ejes locales);
- si la malla es **cerrada**, y si no lo es, cuántos bordes abiertos y aristas
  defectuosas tiene;
- el volumen, cuando es cerrada.

Los cortes y las booleanas requieren mallas cerradas.

### Reparar malla

Si la malla no está cerrada, o está cerrada pero con **normales invertidas** (caras
apuntando hacia adentro), aparece el botón **Reparar malla**. Aplica, en orden:

1. suelda vértices casi coincidentes (STL con vértices duplicados);
2. quita triángulos degenerados y duplicados;
3. orienta las caras de forma consistente;
4. cierra agujeros;
5. orienta cada pieza hacia afuera.

El mensaje de estado resume lo que hizo. Las aristas compartidas por más de dos caras
(geometría no-manifold real) no se pueden resolver así, y el mensaje lo indica.
Para probarlo está `samples/cubo_roto.stl`.

## Primitivas

**Primitivas** → *Cubo*, *Esfera*, *Cilindro* o *Cono*. La pieza aparece apoyada en
la base, a la derecha de lo que ya hay en la escena.

Al seleccionar una primitiva aparece la sección **Dimensiones**:

| Primitiva | Parámetros |
|---|---|
| Cubo | Ancho (X), Fondo (Y), Alto (Z) |
| Esfera | Radio, Segmentos |
| Cilindro | Radio, Alto, Segmentos |
| Cono | Radio inferior, Radio superior (0 = punta), Alto, Segmentos |

Al cambiar un valor, la base de la pieza se queda a la misma altura. Después de un
corte o una booleana, el resultado ya no es una primitiva y sus dimensiones no se
pueden editar.

## Herramientas

Con un objeto seleccionado aparece la barra superior. Solo una herramienta está
activa a la vez. Se activa con un clic o con su tecla; la misma tecla o **Esc** la
cierra. Sus opciones aparecen en el panel lateral.

### Mover (M)

Flechas roja, verde y azul para trasladar en X, Y y Z. En el panel, la posición se
puede escribir directamente. Con **Ctrl** el movimiento va en pasos de 1 unidad.

### Rotar (R)

Anillos para rotar alrededor de X, Y y Z, siempre sobre el centro del objeto. Con
**Ctrl** la rotación va en pasos de 15°. *Restablecer rotación* vuelve a la
orientación original.

### Corte (C)

Aparece un plano amarillo por el centro del objeto, con su propio manipulador:

- la **flecha** lo desplaza a lo largo de su normal;
- los **anillos** lo inclinan en cualquier ángulo.

En el panel:

- *Orientar X / Y / Z* pone la normal según ese eje;
- *Desplazamiento* ubica el plano con precisión;
- *Cortar* crea dos objetos, `nombre (+)` y `nombre (−)`, ambos cerrados, y oculta
  el original.

### Booleana (B)

- **A** parte como el objeto seleccionado y **B** como otro objeto; ambos se pueden cambiar en las listas.
- Las operaciones son *Unión*, *Resta A − B* e *Intersección*.
- *Aplicar* crea el resultado y oculta A y B. El botón se desactiva si A y B son el
  mismo objeto o alguna malla no es cerrada; el motivo aparece al pasar el ratón.

### Apoyar en cara (F)

- Se resaltan en naranja las caras planas sobre las que la pieza puede apoyarse sin
  volcarse. La que está bajo el cursor se ve en amarillo.
- Al hacer clic en una, la pieza gira para que esa cara quede hacia abajo y baja
  hasta tocar z = 0.
- Con **Alinear con los ejes X/Y** (activado por defecto), además gira sobre Z para
  que sus caras verticales miren a ±X o ±Y.
- Las superficies curvas, como una esfera, no tienen caras planas y el panel lo
  indica.

## Deshacer y rehacer

**Ctrl+Z** deshace y **Ctrl+Y** (o Ctrl+Shift+Z) rehace. Cubren todo cambio en la
escena: abrir, primitivas, clonar, eliminar, booleanas, cortes, apoyar en cara,
mover, rotar, dimensiones, reparar, renombrar, color y visibilidad. Un arrastre
completo, del manipulador o de un valor, cuenta como un solo paso. Se guardan los
últimos 100. No afectan a la cámara ni a la herramienta activa.

## Atajos

| Tecla | Acción |
|---|---|
| M / R / C / B / F | Mover / Rotar / Corte / Booleana / Apoyar en cara |
| Esc | Cerrar la herramienta activa |
| Ctrl+Z / Ctrl+Y | Deshacer / rehacer (también Ctrl+Shift+Z) |
| O | Alternar perspectiva / ortogonal |
| Ctrl+D | Clonar el objeto seleccionado |
| Supr | Eliminar el objeto seleccionado |
| Ctrl (al arrastrar) | Pasos de 1 unidad o 15° |

Los atajos no se activan mientras se escribe en un campo de texto.
