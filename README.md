# Replica

Bot en Rust que **graba lo que haces** (ratón + teclado), **mira tu pantalla** mientras lo haces
y luego **repite la ruta por ti**: rutas de farmeo, tareas repetitivas en el trabajo, comandos de juego, lanzar programas…

## Cómo funciona

1. **Grabar** (`F9` o el botón): se registra cada tecla, clic, rueda y movimiento con sus tiempos.
   En cada clic se guarda un recorte de 48×48 px de la pantalla alrededor del cursor (**ancla visual**).
2. **Reproducir** (`F10`): el bot repite los pasos. Antes de cada clic **busca el ancla en pantalla**:
   - si la encuentra desplazada (ventana movida, UI distinta), clica donde está ahora;
   - si aún no aparece (pantalla de carga, lag), **espera** hasta el timeout y luego clica igual o detiene la ruta (configurable).
3. **Parada de emergencia** (`F12`): detiene todo y suelta las teclas/botones pulsados.

Los atajos son globales: funcionan con el juego en primer plano.

## Funciones

- Repeticiones (0 = infinito), velocidad ×0.25–×4, pausa entre vueltas.
- Humanizar: variación aleatoria de ±15 % en tiempos y ±2 px en clics.
- Editor de pasos: cambiar retardos, borrar, reordenar, ver el ancla al pasar el ratón.
- Añadir pasos a mano: **escribir texto** (p. ej. `/home`, `/tp ...`), **Enter**, **ejecutar programa** (`notepad.exe`, `steam://rungameid/...`), **esperar**.
- **Simplificar movimientos**: elimina movimientos de ratón intermedios.
- Las rutas se guardan en `routes/<nombre>/route.json` (+ PNG de anclas), editables a mano.

## Compilar y ejecutar

```bash
cargo run --release
```

- **Windows / macOS**: no hace falta nada más. En macOS da permisos de *Accesibilidad* y *Grabación de pantalla* a la terminal/app.
- **Linux** (sólo X11; Wayland no permite capturar entrada global):

```bash
sudo apt install libx11-dev libxtst-dev libxi-dev libdbus-1-dev libxkbcommon-x11-0 libpipewire-0.3-dev clang
```

## Estructura

| Archivo | Qué hace |
|---|---|
| `src/model.rs` | Rutas, pasos y acciones (serializables a JSON) |
| `src/bot.rs` | Escucha global, grabación, reproductor, atajos, humanización |
| `src/vision.rs` | Captura de pantalla y búsqueda de plantillas (anclas) |
| `src/app.rs` | Interfaz gráfica (egui) |
| `src/theme.rs` | Estética: paleta "Sodio y Ultravioleta", efectos HUD |
| `examples/drive.rs` | Prueba automática: simula F9 → acciones → F9 → F10 |

## Limitaciones y avisos

- **Escribir texto** usa la distribución de teclado US: letras, números y espacios funcionan con cualquier teclado, pero algunos símbolos (`/`, `?`, `:`…) salen distintos con un teclado español. Para eso es mejor grabar las teclas directamente.
- Las coordenadas son absolutas: usa la misma resolución y posición de ventana que al grabar (las anclas compensan pequeños desplazamientos dentro del radio de búsqueda).
- Algunos juegos con anti-cheat a nivel de kernel ignoran la entrada simulada o la detectan.
- **Muchos juegos online prohíben los bots y macros en sus términos de servicio, y te pueden banear la cuenta.** Úsalo en juegos para un jugador, en servidores donde esté permitido o para automatizar tus propias tareas.
