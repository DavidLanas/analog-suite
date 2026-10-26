# analog-suite

Tres plugins CLAP para Linux (Bitwig Studio, Ubuntu Studio), en Rust con nih-plug,
que comparten núcleo de DSP e interfaz:

| Plugin | Qué es | Estado |
|---|---|---|
| FunkComp | compresor multimodo (FET 76 / VARI-MU 660 / OPTO 3A, "inspirado en") | FET 76, VARI-MU 660 y OPTO 3A listos; presets de fábrica y de usuario |
| PultecMaster | EQ pasivo de tubos "inspirado en" Pultec | Fase 3 |
| TapeMaster | grabadora de cinta de 2 pistas | Fase 4 |

## Estructura

```
crates/dsp-core    filtros, suavizado, envolventes, oversampling, saturación, medidores (sin deps)
crates/ui-common   perilla con pasos, selector, vúmetro, carátula, tema (egui)
crates/presets     presets JSON versionados (Preset, ChainPreset)
crates/funkcomp-dsp motor de FunkComp (detector, ganancia, saturación, modos FET 76, VARI-MU y OPTO 3A, presets de fábrica), sin nih-plug
plugins/*          un crate cdylib por plugin
xtask/             `cargo xtask bundle <plugin> --release`
scripts/           install.sh (→ ~/.clap) y ci.sh (fmt + clippy + test [+ clap-validator])
```

## Compilar e instalar

```bash
cargo xtask bundle funkcomp --release      # genera target/bundled/funkcomp.clap
scripts/install.sh funkcomp                # lo copia a ~/.clap
```

Bitwig: Settings → Locations → Plug-ins → "Rescan" (la carpeta `~/.clap` se escanea por defecto).

## Calidad

```bash
scripts/ci.sh              # fmt + clippy + tests
scripts/ci.sh --validate   # además clap-validator sobre cada .clap
cargo bench -p dsp-core    # criterion: biquad y oversampler 2x/4x/8x
```

## Decisiones

* **Sólo CLAP.** `nih_plug` se usa con `default-features = false`, lo que desactiva VST3. Los
  bindings VST3 de nih-plug son GPLv3: antes de activarlo hay que decidir la licencia de la
  suite (el workspace ya está declarado GPL-3.0-or-later, compatible).
* **`assert_process_allocs`** activo: en builds de debug, cualquier asignación en `process()`
  aborta, lo que hace verificable la regla "sin asignaciones en el hilo de audio".
* **Oversampling half-band FIR de fase lineal** (47 taps, Kaiser β=8, ≈80 dB): latencia
  determinista y reportable al host. 2x = 23 muestras, 4x ≈ 34.5, 8x ≈ 40.25 (a fs base).
* **Medidores vía `AtomicF32`**, sin locks entre audio y GUI.

## FunkComp — modo FET 76

* **Topología feed-back resuelta de forma implícita** (`funkcomp-dsp/src/gain.rs`): el lazo
  feed-back ingenuo con un retardo de una muestra es inestable con ataque de 20 µs y ratio
  20:1 (polo en −5). La actualización implícita `g' = g + w·(g_ss − g)` es una media convexa:
  nunca sobrepasa, y conserva la propiedad real de que a mayor ratio el ataque efectivo es
  más rápido.
* **Umbral fijo** (−10 dBFS de pico) y perilla Input que empuja contra él, como el hardware.
* **Attack 20 µs–800 µs y Release 50 ms–1.1 s**, perillas 1..7 invertidas, interpolación
  logarítmica. Ratios 4/8/12/20 y All (ratio 30, umbral −3 dB, tiempos más cortos, más drive).
* **Saturación FET asimétrica** normalizada (ganancia de pequeña señal = 1), cuyo drive crece
  con la reducción de ganancia. Sólo esta etapa corre sobremuestreada (Off/2x/4x).
* **Latencia reportada** = latencia del oversampler (0 / 23 / 35 muestras); la señal seca de
  Mix y Bypass se retarda lo mismo, así que no hay filtro de peine con Mix parcial.
* THD medida (1 kHz, 4x): 0.10 % a −30 dBFS, 0.41 % a −18 dBFS, 1.8 % a −3 dBFS. Son cifras
  del modelo, no una comparación con mediciones publicadas del hardware: afinar de oído.

## FunkComp — modo VARI-MU 660

* **Ratio creciente con knee muy amplio** (`funkcomp-dsp/src/varimu.rs`): la ratio efectiva
  sube de 1:1 a 12:1 a lo largo de ±12 dB alrededor del umbral, `R(o) = 1 + 11·x²`. La
  reducción es la integral de la pendiente `1 − 1/R`, con forma cerrada (`x − atan(√a·x)/√a`),
  así que no hay tablas ni iteración por muestra. Threshold = centro del knee.
* **Time Constant de 6 posiciones.** 1–4: release 0.3 / 0.8 / 2 / 5 s. 5 y 6: release de dos
  etapas dependiente del programa (2 s → 10 s y 0.3 s → 25 s). La etapa lenta se carga hacia la
  rápida (τ = 0.4 s): un pulso aislado se suelta con la etapa rápida, una señal sostenida con
  la lenta. Valores de 1–5 según valores publicados de la unidad original; el de la posición 6
  es una aproximación (la fuente consultada no lo especifica). Afinar de oído.
* **Feed-forward**: el ataque (0.2 o 0.4 ms) no se acelera con la ratio.
* **Saturación de tubo**: polarización alta (0.2) → mucho 2º armónico (H2 1.4 % frente a
  0.6 % del FET a −14 dBFS), con drive que crece con la reducción.
* **Transformador**: low shelf +1.2 dB a 90 Hz y high shelf −1.5 dB a 12 kHz (medido:
  +1.00 dB a 60 Hz y −1.25 dB a 15 kHz respecto a 1 kHz). No modela la saturación de flujo
  del núcleo a baja frecuencia.
* **Cambio de modo sin saltos**: el modo nuevo arranca con la reducción de ganancia vigente
  (`warm_start`) y la coloración entra y sale con un cross-fade de 20 ms.

## FunkComp — modo OPTO 3A

Compresor óptico "inspirado en" un clásico de célula electroluminiscente; no es una réplica.

| Control | Efecto |
|---|---|
| Peak Reduction (0–100) | sensibilidad del side-chain: umbral = −0,4·PR dB. No hay Input |
| Gain | = Output: ganancia de salida (en el motor es el mismo parámetro) |
| Compress / Limit | Compress: 3:1, knee ancho (12 dB). Limit: 12:1, knee corto (6 dB) |

* **Ataque** de ~10 ms (la célula tarda en "encenderse").
* **Release en dos etapas con memoria**: tras una señal sostenida, el primer 50 % se suelta
  en ~60 ms y el resto en unos segundos; tras un pico aislado se suelta todo rápido. Está
  medido en los tests del motor (50 % a 61 ms; 10 % restante a 4,7 s tras señal sostenida
  frente a 200 ms tras un pulso de 30 ms).
* Detector de pico con caída de 8 ms, saturación de salida casi lineal.

## Presets

La barra superior del editor tiene: `<` `>` (anterior/siguiente), la lista por categorías
(Todos / Voz / Instrumentos / Mastering / Otros), **Guardar**, **Guardar como...** y **Borrar**.

* **Presets de fábrica (21)**: Voz 7, Instrumentos 9, Mastering 5. Son datos puros en
  `crates/funkcomp-dsp/src/factory.rs`; sus tests miden la reducción de ganancia y el cambio
  de nivel con un programa de referencia, así que cada preset hace lo que dice su descripción
  (pasa el cursor sobre un nombre para leerla). Los nombres de carácter son "inspirado en".
* **Guardar** sobrescribe el preset de usuario cargado; con un preset de fábrica (o sin
  preset) abre **Guardar como**, donde eliges nombre y categoría. Los de fábrica nunca se
  modifican: tus cambios se guardan como copia.
* Un `*` junto al nombre indica que hay cambios sin guardar.
* **Dónde se guardan**: `~/.config/analog-suite/funkcomp/presets/*.json` (o bajo
  `$XDG_CONFIG_HOME`). Son JSON legibles con valores normalizados por parámetro: se pueden
  copiar, respaldar o compartir. Un archivo ilegible se ignora, nunca bloquea el plugin.
* El nombre se reduce a un nombre de archivo seguro (sin `/`, `..` ni archivos ocultos).
* Los presets no incluyen Bypass. Todo el acceso a disco ocurre en el hilo de la interfaz,
  nunca en el de audio.
* El estado completo (todos los parámetros) también lo guarda el proyecto de Bitwig por su
  cuenta, independientemente de los presets.
