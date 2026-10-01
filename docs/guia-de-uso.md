# Guía de uso

## La ventana

- **Panel izquierdo**:
  - arriba, los botones *Abrir STL…*, *Primitivas* y *Encuadrar*;
  - luego, la lista de objetos y la información del seleccionado;
  - debajo, las opciones de la herramienta activa;
  - al pie, el último mensaje de estado, el espaciado de la grilla y el selector de
    idioma.
- **Visor 3D**:
  - arriba a la izquierda, la barra de herramientas;
  - arriba a la derecha, el cubo de vista;
  - la base translúcida en z = 0 marca el "suelo".

### Idioma

La interfaz está en español o en inglés. Al iniciar se elige según el idioma del
sistema (las variables `LANGUAGE`, `LC_ALL`, `LC_MESSAGES` y `LANG` en Linux): español
si es alguna variante de español, inglés en cualquier otro caso. El selector al pie del
panel lo cambia durante la sesión; la elección no se guarda.

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

Al clonar, la copia aparece al lado del original y se llama `nombre (1)`,
`nombre (2)`… Clonar una copia continúa la numeración en vez de acumular
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
5. orienta cada pieza hacia afuera;
6. si la malla quedó cerrada, fusiona sus piezas sueltas en un solo sólido.

El paso 6 es para los STL exportados como un grupo de cuerpos que se tocan o se
superponen, por ejemplo postes apoyados sobre una base o paredes que se tocan por una
arista. Esos archivos se imprimen bien, porque el slicer une los contornos de cada
capa, pero los cortes y las booleanas necesitan un sólido único.

El mensaje de estado resume lo que hizo. Si después de reparar quedan bordes o aristas
defectuosas, el mensaje lo indica.
Para probarlo está `samples/cubo_roto.stl`.

## Primitivas

**Primitivas** → *Cubo*, *Esfera*, *Cilindro*, *Cono*, *Tubo* o *Caja redondeada*. La
pieza aparece apoyada en la base, a la derecha de lo que ya hay en la escena.

Al seleccionar una primitiva aparece la sección **Dimensiones**:

| Primitiva | Parámetros |
|---|---|
| Cubo | Ancho (X), Fondo (Y), Alto (Z) |
| Esfera | Radio, Segmentos |
| Cilindro | Radio, Alto, Segmentos |
| Cono | Radio inferior, Radio superior (0 = punta), Alto, Segmentos |
| Tubo | Radio exterior, Radio del agujero, Alto, Segmentos |
| Caja redondeada | Ancho (X), Fondo (Y), Alto (Z), Radio de esquinas, Segmentos |

El **tubo** es un cilindro con un agujero pasante, útil para postes de tornillos. El
agujero siempre queda más angosto que el tubo.

La **caja redondeada** tiene las aristas verticales redondeadas. Con radio 0 es una caja
común, y con el radio máximo (la mitad del lado menor) es una ranura extruida: dos
semicírculos unidos por rectas. Si el radio no cabe, se ajusta solo.

Al cambiar un valor, la base de la pieza se queda a la misma altura. Después de un
corte o una booleana, el resultado ya no es una primitiva y sus dimensiones no se
pueden editar.

## Herramientas

La barra superior está siempre visible. Mover, Rotar, Corte, Booleana y Apoyar en
cara actúan sobre el objeto seleccionado y se desactivan sin selección; Medir y Regla
están siempre disponibles. Solo una herramienta está activa a la vez. Se activa con un
clic o con su tecla; la misma tecla o **Esc** la cierra. Sus opciones aparecen en el
panel lateral.

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

Si hay reglas, al desplazarlo con la flecha se engancha a la marca más cercana al punto
donde cruza cada regla, también si está inclinado. Las reglas casi paralelas al plano
no cuentan.

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

### Snap del cursor

Con Medir y Regla, el cursor se engancha al rasgo más cercano. La forma del marcador
amarillo indica a qué:

| Marcador | Se engancha a |
|---|---|
| Círculo con cruz | Centro de un agujero, un poste o un arco (p. ej. los extremos de una ranura) |
| Cuadrado | Esquina |
| Rombo | Punto de una arista |
| Punto | La superficie bajo el cursor o, fuera de los objetos, el suelo z = 0 |

Las aristas que cuentan son las "vivas", donde las caras forman más de 30°. El
facetado de cilindros y esferas no cuenta. Los rasgos tapados por otra superficie se
ignoran.

### Medir (L)

- Mide la distancia entre dos puntos **a lo largo de un eje**, nunca en diagonal.
- Primer clic: punto A. Segundo clic: punto B. Un tercer clic empieza otra medición.
- Mientras falta B, la cota sigue al cursor.
- El eje es aquel en que más se separan A y B. **X**, **Y** o **Z** lo fijan, y la
  misma tecla lo libera; también se elige en el panel (*Auto*, X, Y, Z).
- La cota se dibuja con el color del eje. Una línea punteada une su extremo con B.
- El panel muestra el valor y, como referencia, ΔX, ΔY y ΔZ.
- Esc descarta el punto A pendiente. La medición desaparece al cerrar la herramienta.

### Regla (G)

- Primer clic: donde empieza la regla (su cero). Segundo clic: hacia dónde se
  extiende. El eje se elige como en Medir.
- Las marcas van cada *Marcas cada* unidades (10 por defecto), con una marca más larga
  cada cinco. Los números se muestran donde caben.
- Puede haber varias reglas. Quedan visibles con cualquier herramienta hasta
  quitarlas en el panel de Regla, donde también se cambian su largo y su espaciado.
- **Mover con reglas**: al arrastrar un objeto con Mover, se engancha a la marca más
  cercana cuando está a menos de 10 píxeles. Se enganchan los bordes y el centro de su
  caja envolvente, sus esquinas y sus centros de agujeros. Solo en los ejes que se
  están moviendo, y la marca enganchada se resalta.
- **Cortar con reglas**: el plano de corte también se engancha a las marcas al
  desplazarlo (ver *Corte*).
- Las reglas no entran en el historial de deshacer.

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
| L / G | Medir / Regla |
| X / Y / Z | Con Medir o Regla: fijar o liberar el eje |
| Esc | Descartar el punto pendiente de Medir o Regla; si no lo hay, cerrar la herramienta |
| Ctrl+Z / Ctrl+Y | Deshacer / rehacer (también Ctrl+Shift+Z) |
| O | Alternar perspectiva / ortogonal |
| Ctrl+D | Clonar el objeto seleccionado |
| Supr | Eliminar el objeto seleccionado |
| Ctrl (al arrastrar) | Pasos de 1 unidad o 15° |

Los atajos no se activan mientras se escribe en un campo de texto.
