# Cambios

## [0.2.0] - 2026-10-01

### Nuevo

- **Interfaz en inglés.** El idioma se detecta del sistema al iniciar (español o inglés)
  y se puede cambiar desde el selector al pie del panel lateral.
- **Escalar (S):**
  - con un manipulador o desde el panel;
  - por porcentaje o por medida objetivo en cada eje;
  - con *Escala uniforme* para escalar los tres ejes por igual;
  - la base queda a la misma altura.

  En las primitivas, la escala pasa a sus dimensiones cuando la forma sigue siendo del
  mismo tipo.
- **Clonar en matriz:** copias en columnas (X) × filas (Y), con hueco independiente en
  X e Y, vista previa de las posiciones y un solo paso de deshacer.
- **Medir (L):** distancia entre dos puntos a lo largo de un eje (el dominante, o el
  fijado con X/Y/Z).
- **Regla (G):** reglas sobre un eje con marcas cada N mm. Al mover un objeto, sus
  bordes, su centro, sus esquinas y sus centros de agujeros se enganchan a las marcas.
  El plano de corte también se engancha.
- **Snap del cursor** al medir y poner reglas: centros de agujeros y arcos, esquinas,
  aristas, superficie y suelo.
- **Primitivas nuevas:**
  - **Tubo**: cilindro con agujero, útil para postes de tornillos;
  - **Caja redondeada**: aristas verticales redondeadas; con el radio máximo es una
    ranura extruida.
- **Ícono nuevo** (cubo y esfera). Aparece también en el `.exe` de Windows y en la
  ventana.

### Cambios

- **Reparar malla** fusiona en un solo sólido las piezas que se tocan o se superponen,
  como los STL exportados como grupo de cuerpos. Antes esos archivos no se podían cortar
  ni usar en booleanas.
- **Mallas con aristas de 4 caras:** ya no se marcan como defectuosas si las caras están
  bien emparejadas (dos sólidos que se tocan por una arista). Manifold las acepta.
- **Barra de herramientas:** está siempre visible. Las herramientas de objeto se
  desactivan sin selección; Medir y Regla funcionan siempre.
- **Clones:** ahora se llaman `nombre (1)`, `nombre (2)`…, igual en todos los idiomas.
  Antes era `nombre (copia1)`.
- **Panel lateral:** al pie indica que todas las medidas están en mm, y la grilla
  muestra su espaciado en mm.

### Correcciones

- Un clic en un menú o una lista desplegable que queda sobre el visor ya no deselecciona
  el objeto. Por eso, por ejemplo, *Clonar en matriz…* se cerraba apenas se abría.
- Con un nombre de objeto largo, el botón **…** quedaba tapado. Ahora el nombre se trunca
  y se ve completo al pasar el mouse.

## [0.1.0] - 2026-09-27

Primera versión:

- visor 3D con cubo de vista y proyección ortogonal;
- objetos STL: abrir, exportar, clonar, renombrar, color y visibilidad;
- Mover, Rotar, Corte, Booleana y Apoyar en cara;
- primitivas paramétricas: cubo, esfera, cilindro y cono;
- deshacer y rehacer;
- reparación de mallas.

[0.2.0]: https://github.com/jcontrerasf/simpleSTL/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/jcontrerasf/simpleSTL/releases/tag/v0.1.0
