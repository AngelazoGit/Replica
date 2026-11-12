// Prueba manual: simula pulsaciones para grabar y reproducir una ruta.
use rdev::{Button, EventType::*, Key, simulate};
use std::{thread::sleep, time::Duration};
fn s(e: rdev::EventType) {
    simulate(&e).unwrap();
    sleep(Duration::from_millis(60));
}
fn main() {
    let arg = std::env::args().nth(1).unwrap_or_default();
    match arg.as_str() {
        "record" => {
            s(KeyPress(Key::F9));
            s(KeyRelease(Key::F9));
            s(MouseMove {
                x: 1100.0,
                y: 300.0,
            });
            s(ButtonPress(Button::Left));
            s(ButtonRelease(Button::Left));
            s(KeyPress(Key::KeyW));
            sleep(Duration::from_millis(300));
            s(KeyRelease(Key::KeyW));
            s(MouseMove {
                x: 1150.0,
                y: 350.0,
            });
            s(KeyPress(Key::F9));
            s(KeyRelease(Key::F9));
        }
        // F9 mantenido: el SO repite KeyPress; sólo debe contar una vez.
        "hold" => {
            for _ in 0..4 {
                s(KeyPress(Key::F9));
            }
            s(KeyRelease(Key::F9));
        }
        // Clic sin mover el ratón antes: debe grabarse en la posición real del cursor.
        "still" => {
            s(KeyPress(Key::F9));
            s(KeyRelease(Key::F9));
            s(ButtonPress(Button::Left));
            s(ButtonRelease(Button::Left));
            s(KeyPress(Key::F9));
            s(KeyRelease(Key::F9));
        }
        // Muchos movimientos seguidos: la reproducción no debe acumular retraso.
        "moves" => {
            s(KeyPress(Key::F9));
            s(KeyRelease(Key::F9));
            for i in 0..150 {
                simulate(&MouseMove {
                    x: 200.0 + i as f64 * 4.0,
                    y: 400.0,
                })
                .unwrap();
                sleep(Duration::from_millis(20));
            }
            s(KeyPress(Key::F9));
            s(KeyRelease(Key::F9));
        }
        "play" => {
            s(KeyPress(Key::F10));
            s(KeyRelease(Key::F10));
        }
        _ => {}
    }
}
