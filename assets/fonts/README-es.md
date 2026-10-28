# Fuentes incrustadas

- `Inter-Regular.ttf`, `Inter-SemiBold.ttf`: Inter 4.1 (<https://rsms.me/inter>, OFL, ver
  `Inter-LICENSE.txt`), recortadas a latín y símbolos **sin el área de uso privado**
  (U+E000–F8FF), donde Inter trae glifos propios que taparían los íconos de Phosphor:

  ```bash
  pyftsubset Inter-Regular.ttf --layout-features='*' --output-file=Inter-Regular.ttf \
    --unicodes="U+0000-007F,U+00A0-024F,U+0300-036F,U+2000-206F,U+2070-209F,U+20A0-20CF,\
  U+2100-214F,U+2190-21FF,U+2200-22FF,U+2300-23FF,U+25A0-25FF,U+2713,U+2715"
  ```

- `Phosphor.ttf`, `Phosphor-Fill.ttf`: íconos Phosphor (MIT, ver `Phosphor-LICENSE.txt`),
  tal cual vienen en el crate `egui-phosphor` 0.14. Los códigos usados están en
  `src/icons.rs`.
