//! Phone controls and browser I/O. Instrument behaviour belongs to the ARM image.
use js_sys::{Array, Float32Array, Function, Object, Reflect, Uint32Array, Uint8Array};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::*;
mod controls;
mod k2_bridge;
mod latency;
mod midi_file;
mod photo_geometry;
mod resample;
mod streaming;

#[wasm_bindgen(raw_module = "./engine.js")]
extern "C" {
    #[wasm_bindgen(js_name=default)]
    fn initialize_engine() -> js_sys::Promise;
    #[wasm_bindgen(js_name=Firmware)]
    type Cpu;
    #[wasm_bindgen(constructor,catch,js_class=Firmware)]
    fn new(image: &Uint8Array) -> Result<Cpu, JsValue>;
    #[wasm_bindgen(method, catch)]
    fn run(this: &Cpu, steps: u32) -> Result<Uint32Array, JsValue>;
    #[wasm_bindgen(method, catch)]
    fn run_for(this: &Cpu, steps: u32, nanoseconds: u64) -> Result<Uint32Array, JsValue>;
    #[wasm_bindgen(method)]
    fn input(this: &Cpu, samples: &Float32Array);
    #[wasm_bindgen(method, catch)]
    fn input_stereo(this: &Cpu, samples: &Float32Array) -> Result<(), JsValue>;
    #[wasm_bindgen(method)]
    fn sample_rate(this: &Cpu) -> f64;
    #[wasm_bindgen(method)]
    fn flash_image(this: &Cpu) -> Uint8Array;
    #[wasm_bindgen(method)]
    fn flash_revision(this: &Cpu) -> u64;
    #[wasm_bindgen(method, catch)]
    fn restore_flash(this: &Cpu, bytes: &Uint8Array) -> Result<(), JsValue>;
    #[wasm_bindgen(method)]
    fn pending_input(this: &Cpu) -> u32;
    #[wasm_bindgen(method)]
    fn virtual_time_ns(this: &Cpu) -> u64;
    #[wasm_bindgen(method)]
    fn take_audio(this: &Cpu) -> Float32Array;
    #[wasm_bindgen(method)]
    fn oled(this: &Cpu) -> Uint8Array;
    #[wasm_bindgen(method)]
    fn stats(this: &Cpu) -> String;
    #[wasm_bindgen(method, catch)]
    fn key(this: &Cpu, index: u8, pressed: bool) -> Result<(), JsValue>;
    #[wasm_bindgen(method)]
    fn encoder(this: &Cpu, steps: i32);
    #[wasm_bindgen(method, catch)]
    fn encoder_button(this: &Cpu, pressed: bool) -> Result<(), JsValue>;
    #[wasm_bindgen(method, catch)]
    fn handset(this: &Cpu, lifted: bool) -> Result<(), JsValue>;
    #[wasm_bindgen(method, catch)]
    fn mic_gain(this: &Cpu, value: f32) -> Result<(), JsValue>;
    #[wasm_bindgen(method, catch)]
    fn midi(this: &Cpu, bytes: &Uint8Array) -> Result<(), JsValue>;
}
fn get(o: &JsValue, k: &str) -> JsValue {
    Reflect::get(o, &k.into()).unwrap_or(JsValue::UNDEFINED)
}
fn set(o: &JsValue, k: &str, v: impl Into<JsValue>) {
    Reflect::set(o, &k.into(), &v.into()).unwrap();
}
fn object(kind: &str) -> Object {
    let o = Object::new();
    set(&o, "kind", kind);
    o
}
fn call(o: &JsValue, k: &str, args: &Array) -> Result<JsValue, JsValue> {
    get(o, k).dyn_into::<Function>()?.apply(o, args)
}
fn doc() -> Document {
    window().unwrap().document().unwrap()
}
fn el(id: &str) -> Element {
    doc().get_element_by_id(id).unwrap()
}
fn text(id: &str, s: &str) {
    el(id).set_text_content(Some(s));
}
fn report(e: JsValue) {
    web_sys::console::error_1(&e);
    text("status", "Stopped. Power off, then on.");
    text(
        "error-detail",
        &e.as_string().unwrap_or_else(|| format!("{e:?}")),
    );
}
fn bind(target: &EventTarget, name: &str, f: impl FnMut(Event) + 'static) {
    let c = Closure::<dyn FnMut(Event)>::new(f);
    target
        .add_event_listener_with_callback(name, c.as_ref().unchecked_ref())
        .unwrap();
    c.forget();
}
struct DialDrag {
    pointer: i32,
    origin: (i32, i32),
    last_y: i32,
    moved: bool,
    turn: Option<controls::EncoderTurn>,
}

thread_local! {
    static ENCODER_ANGLE: Cell<i64> = const { Cell::new(0) };
}

fn turn_encoder(steps: i32) {
    ENCODER_ANGLE.with(|angle| {
        let degrees =
            angle.get() + i64::from(steps) * i64::from(controls::ENCODER_DEGREES_PER_STEP);
        angle.set(degrees);
        set_wheel_angle("encoder-wheel", degrees);
    });
    command("encoder", "steps", steps);
}

fn set_wheel_angle(id: &str, degrees: i64) {
    let wheel: HtmlElement = el(id).unchecked_into();
    let _ = wheel
        .style()
        .set_property("--wheel-angle", &format!("{degrees}deg"));
}

fn gain_changed() {
    let input: HtmlInputElement = el("gain").unchecked_into();
    let value = input.value_as_number();
    text("gain-value", &format!("{:.0}%", value * 100.));
    let _ = input.set_attribute("aria-valuetext", &format!("{:.0} percent", value * 100.));
    set_wheel_angle("gain-wheel", ((value - 1.) * 150.).round() as i64);
    command("gain", "value", value);
}
fn bind_dial(id: &'static str, encoder: bool) {
    let state: Rc<RefCell<Option<DialDrag>>> = Rc::new(RefCell::new(None));
    let down = state.clone();
    bind(el(id).as_ref(), "pointerdown", move |e| {
        let p: PointerEvent = e.unchecked_into();
        if p.button() != 0 || down.borrow().is_some() {
            return;
        }
        p.prevent_default();
        let target: Element = p.current_target().unwrap().unchecked_into();
        let _ = target.set_pointer_capture(p.pointer_id());
        let origin = (p.client_x(), p.client_y());
        let wheel = if encoder {
            "encoder-wheel"
        } else {
            "gain-wheel"
        };
        let rect = el(wheel).get_bounding_client_rect();
        let turn = (rect.width() > 0.).then(|| {
            controls::EncoderTurn::new(
                (
                    rect.x() + rect.width() * 0.5,
                    rect.y() + rect.height() * 0.5,
                ),
                rect.width() * 0.17,
                (f64::from(origin.0), f64::from(origin.1)),
            )
        });
        *down.borrow_mut() = Some(DialDrag {
            pointer: p.pointer_id(),
            origin,
            last_y: origin.1,
            moved: false,
            turn,
        });
    });
    let movement = state.clone();
    bind(el(id).as_ref(), "pointermove", move |e| {
        let p: PointerEvent = e.unchecked_into();
        if let Some(drag) = movement
            .borrow_mut()
            .as_mut()
            .filter(|drag| drag.pointer == p.pointer_id())
        {
            let x = p.client_x();
            let y = p.client_y();
            drag.moved |= (y - drag.origin.1).abs() > 4
                || (drag.turn.is_some() && (x - drag.origin.0).abs() > 4);
            if !drag.moved {
                return;
            }
            let steps = if let Some(turn) = drag.turn.as_mut() {
                turn.move_to((f64::from(x), f64::from(y)))
            } else {
                let steps = (drag.last_y - y) / 4;
                drag.last_y -= steps * 4;
                steps
            };
            if steps != 0 {
                let _ = el(if encoder { "encoder" } else { "mic-dial" })
                    .class_list()
                    .add_1("dragging");
                if encoder {
                    turn_encoder(steps);
                } else {
                    let input: HtmlInputElement = el("gain").unchecked_into();
                    let increment = if drag.turn.is_some() { 0.10 } else { 0.04 };
                    input.set_value_as_number(
                        (input.value_as_number() + f64::from(steps) * increment).clamp(0., 2.),
                    );
                    gain_changed();
                }
            }
        }
    });
    for event in ["pointerup", "pointercancel", "lostpointercapture"] {
        let up = state.clone();
        bind(el(id).as_ref(), event, move |e| {
            let p: PointerEvent = e.unchecked_into();
            let mut state = up.borrow_mut();
            if state
                .as_ref()
                .is_some_and(|drag| drag.pointer == p.pointer_id())
            {
                let drag = state.take().unwrap();
                let _ = el(if encoder { "encoder" } else { "mic-dial" })
                    .class_list()
                    .remove_1("dragging");
                if encoder && !drag.moved && event == "pointerup" {
                    command("encoder_button", "value", true);
                    command("encoder_button", "value", false);
                }
            }
        });
    }
}
type MidiInputHandler = (JsValue, Closure<dyn FnMut(JsValue)>);
struct App {
    worker: Worker,
    context: AudioContext,
    node: AudioWorkletNode,
    stream: Option<MicCapture>,
    lifted: bool,
    tone: Option<(OscillatorNode, GainNode)>,
    midi: JsValue,
    outputs: Vec<JsValue>,
    output_index: Option<usize>,
    input_handlers: Vec<MidiInputHandler>,
    mic_request: Option<Rc<()>>,
    testing: bool,
}
thread_local! {static APP:RefCell<Option<App>>=const{RefCell::new(None)};}
fn send(message: &JsValue) {
    APP.with(|s| {
        if let Some(a) = s.borrow().as_ref() {
            if let Err(e) = a.worker.post_message(message) {
                report(e);
            }
        }
    });
}
fn command(kind: &str, key: &str, value: impl Into<JsValue>) {
    let value = value.into();
    let m = object(kind);
    set(&m, key, value);
    send(&m);
}
fn button(index: u8, pressed: bool) {
    if APP.with(|s| s.borrow().is_none()) {
        return;
    }
    let m = object("key");
    set(&m, "index", index);
    set(&m, "pressed", pressed);
    send(&m);
    if let Ok(Some(e)) = doc().query_selector(&format!("[data-key='{index}']")) {
        let _ = e.class_list().toggle_with_force("pressed", pressed);
    }
}

#[derive(Clone, Copy)]
struct HandsetDrag {
    pointer: i32,
    origin: (f64, f64),
    moved: bool,
}

impl HandsetDrag {
    fn offset(self, event: &PointerEvent) -> (f64, f64) {
        (
            f64::from(event.client_x()) - self.origin.0,
            f64::from(event.client_y()) - self.origin.1,
        )
    }
}

fn move_receiver(x: f64, y: f64) {
    let receiver: HtmlElement = el("mobile-receiver").unchecked_into();
    let style = receiver.style();
    let _ = style.set_property("--receiver-x", &format!("{x}px"));
    let _ = style.set_property("--receiver-y", &format!("{y}px"));
}

fn bind_handset() {
    let handset = el("handset");
    let drag = Rc::new(Cell::new(None::<HandsetDrag>));
    let ignore_click = Rc::new(Cell::new(false));
    let gesture = drag.clone();
    let ignored = ignore_click.clone();
    bind(handset.as_ref(), "pointerdown", move |event| {
        let event: PointerEvent = event.unchecked_into();
        if event.button() != 0 || gesture.get().is_some() {
            return;
        }
        ignored.set(false);
        let can_drag = APP.with(|app| {
            app.borrow()
                .as_ref()
                .is_some_and(|app| !app.lifted && !app.testing)
        });
        if !can_drag || el("mobile-receiver").get_bounding_client_rect().width() == 0. {
            return;
        }
        event.prevent_default();
        let target: Element = event.current_target().unwrap().unchecked_into();
        let _ = target.set_pointer_capture(event.pointer_id());
        let _ = target.class_list().add_1("dragging");
        gesture.set(Some(HandsetDrag {
            pointer: event.pointer_id(),
            origin: (f64::from(event.client_x()), f64::from(event.client_y())),
            moved: false,
        }));
    });

    let gesture = drag.clone();
    bind(handset.as_ref(), "pointermove", move |event| {
        let event: PointerEvent = event.unchecked_into();
        if let Some(mut active) = gesture
            .get()
            .filter(|drag| drag.pointer == event.pointer_id())
        {
            let (x, y) = active.offset(&event);
            if !active.moved && x.hypot(y) > 4. {
                active.moved = true;
                gesture.set(Some(active));
            }
            move_receiver(x, y);
        }
    });

    for name in ["pointerup", "pointercancel", "lostpointercapture"] {
        let gesture = drag.clone();
        let ignored = ignore_click.clone();
        bind(handset.as_ref(), name, move |event| {
            let event: PointerEvent = event.unchecked_into();
            let Some(active) = gesture
                .get()
                .filter(|drag| drag.pointer == event.pointer_id())
            else {
                return;
            };
            gesture.set(None);
            let (x, y) = active.offset(&event);
            let distance = x.hypot(y);
            let _ = el("handset").class_list().remove_1("dragging");
            ignored.set(name != "pointerup" || active.moved || distance > 4.);
            if name == "pointerup"
                && distance >= 28.
                && !el("handset").class_list().contains("lifted")
            {
                lift();
            }
            if !el("handset").class_list().contains("lifted") {
                move_receiver(0., 0.);
            }
        });
    }

    bind(handset.as_ref(), "click", move |event| {
        let event: MouseEvent = event.unchecked_into();
        if !ignore_click.replace(false) || event.detail() == 0 {
            lift();
        }
    });
}

fn lift() {
    APP.with(|s| {
        if let Some(a) = s.borrow_mut().as_mut() {
            if a.testing {
                return;
            }
            a.lifted = !a.lifted;
            if !a.lifted {
                move_receiver(0., 0.);
            }
            let m = object("handset");
            set(&m, "value", a.lifted);
            let _ = a.worker.post_message(&m);
            let _ = el("handset")
                .class_list()
                .toggle_with_force("lifted", a.lifted);
            let _ = el("phone")
                .class_list()
                .toggle_with_force("offhook", a.lifted);
            let _ = el("handset")
                .set_attribute("aria-pressed", if a.lifted { "true" } else { "false" });
            let _ = el("handset").set_attribute(
                "aria-label",
                if a.lifted {
                    "Replace handset"
                } else {
                    "Lift handset"
                },
            );
            text(
                "hook-status",
                if a.lifted {
                    "RECEIVER LIFTED"
                } else {
                    "RECEIVER ON HOOK"
                },
            );
        }
    });
}
fn screen(bytes: &Uint8Array) {
    let canvas: HtmlCanvasElement = el("oled").unchecked_into();
    let c: CanvasRenderingContext2d = canvas.get_context("2d").unwrap().unwrap().unchecked_into();
    c.set_fill_style_str("#081417");
    c.fill_rect(0., 0., 512., 128.);
    c.set_fill_style_str("#a7d6ef");
    for (i, b) in bytes.to_vec().iter().enumerate() {
        for bit in 0..8 {
            if b & (1 << bit) != 0 {
                c.fill_rect(
                    (i % 128 * 4) as f64,
                    ((i / 128 * 8 + bit) * 4) as f64,
                    4.,
                    4.,
                );
            }
        }
    }
}
fn enable() {
    let _ = el("power").remove_attribute("disabled");
    for id in [
        "handset",
        "encoder",
        "enc-down",
        "enc-up",
        "gain",
        "microphone",
        "midi",
        "tone",
        "midi-file",
        "latency-test",
        "software-latency-test",
    ] {
        let _ = el(id).remove_attribute("disabled");
    }
    for n in 0..12 {
        let _ = doc()
            .query_selector(&format!("[data-key='{n}']"))
            .unwrap()
            .unwrap()
            .remove_attribute("disabled");
    }
    let _ = el("power-led").class_list().add_1("on");
    let _ = el("phone").class_list().add_1("powered");
    let _ = el("power").set_attribute("data-on", "true");
    let _ = el("power").set_attribute("aria-label", "Power off");
}
fn configuration() {
    let m = object("configure");
    let gain: HtmlInputElement = el("gain").unchecked_into();
    set(&m, "gain", gain.value_as_number());
    APP.with(|s| {
        if let Some(a) = s.borrow().as_ref() {
            set(&m, "rate", a.context.sample_rate());
        }
    });
    if let Ok(Some(storage)) = window().unwrap().local_storage() {
        if let Ok(Some(saved)) = storage.get_item("synthphone-nor-v1") {
            if saved.len() % 2 == 0 && saved.len() <= 20_000_000 {
                let data: Option<Vec<u8>> = saved
                    .as_bytes()
                    .chunks_exact(2)
                    .map(|pair| {
                        std::str::from_utf8(pair)
                            .ok()
                            .and_then(|s| u8::from_str_radix(s, 16).ok())
                    })
                    .collect();
                if let Some(data) = data {
                    set(&m, "flash", Uint8Array::from(data.as_slice()));
                }
            }
        }
    }
    send(&m);
}
fn save_flash(bytes: &Uint8Array) {
    let result = (|| -> Result<(), JsValue> {
        let storage = window()
            .unwrap()
            .local_storage()?
            .ok_or_else(|| JsValue::from_str("Storage is unavailable"))?;
        let mut value = String::with_capacity(bytes.length() as usize * 2);
        use std::fmt::Write;
        for byte in bytes.to_vec() {
            write!(value, "{byte:02x}").unwrap();
        }
        storage.set_item("synthphone-nor-v1", &value)
    })();
    text(
        "persistence",
        if result.is_ok() {
            "Settings saved on this device."
        } else {
            "This browser cannot save the phone’s settings. Playing still works."
        },
    );
}
fn power_off() {
    APP.with(|s| {
        if let Some(a) = s.borrow_mut().take() {
            if let Some(port) = a.output_index.and_then(|i| a.outputs.get(i)) {
                silence_output(port);
            }
            a.worker.terminate();
            if !a.midi.is_undefined() {
                set(&a.midi, "onstatechange", JsValue::NULL);
            }
            for (input, _) in &a.input_handlers {
                set(input, "onmidimessage", JsValue::NULL);
            }
            drop(a.stream);
            let _ = a.context.close();
        }
    });
    for id in [
        "handset",
        "encoder",
        "enc-down",
        "enc-up",
        "gain",
        "microphone",
        "midi",
        "tone",
        "midi-file",
        "file-play",
        "file-stop",
        "latency-test",
        "software-latency-test",
    ] {
        let _ = el(id).set_attribute("disabled", "");
    }
    for n in 0..12 {
        let key = doc()
            .query_selector(&format!("[data-key='{n}']"))
            .unwrap()
            .unwrap();
        let _ = key.set_attribute("disabled", "");
        let _ = key.class_list().remove_1("pressed");
    }
    let _ = el("power-led").class_list().remove_1("on");
    let _ = el("phone").class_list().remove_2("powered", "offhook");
    let _ = el("power").set_attribute("data-on", "false");
    let _ = el("power").set_attribute("aria-label", "Power on");
    let _ = el("handset").class_list().remove_2("lifted", "dragging");
    move_receiver(0., 0.);
    let _ = el("handset").set_attribute("aria-pressed", "false");
    let _ = el("handset").set_attribute("aria-label", "Lift handset");
    text("hook-status", "RECEIVER ON HOOK");
    let tone: HtmlInputElement = el("tone").unchecked_into();
    tone.set_checked(false);
    mic_status("MIC", "");
    text("status", "Off.");
    text("performance", "Phone off.");
    text(
        "file-status",
        "Load a MIDI file after switching on the phone.",
    );
    text("midi-status", "");
    el("midi-in").set_inner_html("<option value=''>None</option>");
    el("midi-out").set_inner_html("<option value=''>None</option>");
    screen(&Uint8Array::new_with_length(512));
    let _ = el("power").remove_attribute("disabled");
}
async fn power() -> Result<(), JsValue> {
    text("error-detail", "");
    let options = AudioContextOptions::new();
    options.set_latency_hint(&"interactive".into());
    let context = AudioContext::new_with_context_options(&options)?;
    JsFuture::from(context.resume()?).await?;
    let worklet_url = el("runtime")
        .get_attribute("data-worklet")
        .ok_or("Missing audio runtime")?;
    JsFuture::from(context.audio_worklet()?.add_module(&worklet_url)?).await?;
    let node = AudioWorkletNode::new(&context, "firmware-audio")?;
    node.connect_with_audio_node(&context.destination())?;
    let options = WorkerOptions::new();
    options.set_type(WorkerType::Module);
    let worker_url = el("runtime")
        .get_attribute("data-worker")
        .ok_or("Missing worker runtime")?;
    let worker = Worker::new_with_options(&worker_url, &options)?;
    bind(worker.as_ref(), "message", move |e| {
        let e: MessageEvent = e.unchecked_into();
        let d = e.data();
        match get(&d, "kind").as_string().as_deref() {
            Some("connect") => {
                let connected = APP.with(|s| -> Result<(), JsValue> {
                    let s = s.borrow();
                    let a = s.as_ref().ok_or("Phone was powered off")?;
                    let port = a.node.port()?;
                    let message = object("audio_port");
                    set(&message, "port", JsValue::from(port.clone()));
                    a.worker
                        .post_message_with_transfer(&message, &Array::of1(port.as_ref()))
                });
                if let Err(error) = connected {
                    power_off();
                    report(error);
                    return;
                }
                configuration();
                text("status", "Starting the firmware…");
            }
            Some("ready") => {
                enable();
                text("status", "Ready.");
            }
            Some("screen") => screen(&get(&d, "bytes").unchecked_into()),
            Some("performance") => {
                let state = APP.with(|s| {
                    s.borrow()
                        .as_ref()
                        .map(|a| {
                            get(a.context.as_ref(), "state")
                                .as_string()
                                .unwrap_or_default()
                        })
                        .unwrap_or_default()
                });
                text(
                    "performance",
                    &format!(
                        "Audio: {state}. {}",
                        get(&d, "text").as_string().unwrap_or_default()
                    ),
                );
                if get(&d, "slow").as_bool() == Some(true) {
                    text("status", "Audio is falling behind. Details in Engine.");
                } else if el("status")
                    .text_content()
                    .is_some_and(|s| s.starts_with("Audio is falling behind"))
                {
                    text("status", "Ready.");
                }
            }
            Some("error") => report(get(&d, "error")),
            Some("flash") => save_flash(&Uint8Array::new(&get(&d, "bytes"))),
            Some("persistence") => text(
                "persistence",
                &get(&d, "text").as_string().unwrap_or_default(),
            ),
            Some("file_status") => {
                text(
                    "file-status",
                    &get(&d, "text").as_string().unwrap_or_default(),
                );
                if get(&d, "loaded").is_truthy() {
                    let _ = el("file-play").remove_attribute("disabled");
                    let _ = el("file-stop").remove_attribute("disabled");
                }
            }
            Some("midi_out") => midi_output(&Uint8Array::new(&get(&d, "bytes"))),
            Some("latency_status") => {
                APP.with(|s| {
                    if let Some(a) = s.borrow_mut().as_mut() {
                        a.testing = false;
                    }
                });
                text(
                    "latency-status",
                    &get(&d, "text").as_string().unwrap_or_default(),
                );
                for id in LATENCY_CONTROLS {
                    let _ = el(id).remove_attribute("disabled");
                }
            }
            Some("shutdown") => {
                save_flash(&Uint8Array::new(&get(&d, "bytes")));
                power_off();
            }
            _ => {}
        }
    });
    bind(worker.as_ref(), "error", |_| {
        report("Could not start the audio worker. Reload to try again.".into())
    });
    let sample_rate = context.sample_rate();
    APP.with(|s| {
        *s.borrow_mut() = Some(App {
            worker,
            context,
            node,
            stream: None,
            lifted: false,
            tone: None,
            midi: JsValue::UNDEFINED,
            outputs: vec![],
            output_index: None,
            input_handlers: vec![],
            mic_request: None,
            testing: false,
        })
    });
    let _ = sample_rate;
    Ok(())
}
struct MicCapture {
    stream: MediaStream,
    source: Option<MediaStreamAudioSourceNode>,
}

impl Drop for MicCapture {
    fn drop(&mut self) {
        if let Some(source) = &self.source {
            let _ = source.disconnect();
        }
        for track in self.stream.get_tracks().iter() {
            track.unchecked_into::<MediaStreamTrack>().stop();
        }
    }
}

fn active_mic_request(app: &App, request: &Rc<()>) -> bool {
    app.mic_request
        .as_ref()
        .is_some_and(|id| Rc::ptr_eq(id, request))
}

fn mic_status(button: &str, status: &str) {
    text("microphone", button);
    text("mic-device", status);
    el("microphone").set_attribute("title", status).ok();
}

fn finish_mic_request(request: &Rc<()>, result: Result<JsValue, JsValue>) {
    // Own any returned tracks immediately. A late completion after a cancel,
    // timeout or power cycle must release the device without connecting it.
    let capture = result.and_then(|stream| {
        Ok(MicCapture {
            stream: stream.dyn_into()?,
            source: None,
        })
    });
    APP.with(|app| {
        let mut app = app.borrow_mut();
        let Some(app) = app.as_mut().filter(|app| active_mic_request(app, request)) else {
            return;
        };
        app.mic_request = None;
        let attached = capture.and_then(|mut capture| {
            let track: MediaStreamTrack = capture.stream.get_audio_tracks().get(0).dyn_into()?;
            let label = track.label();
            let source = app.context.create_media_stream_source(&capture.stream)?;
            source.connect_with_audio_node(&app.node)?;
            capture.source = Some(source);
            app.stream = Some(capture);
            Ok(label)
        });
        match attached {
            Ok(label) if label.is_empty() => mic_status("MIC ON", "Microphone connected."),
            Ok(label) => mic_status("MIC ON", &format!("Input: {label}")),
            Err(error) => {
                let message = match get(&error, "name").as_string().as_deref() {
                    Some("NotAllowedError" | "SecurityError") => "Microphone access denied. Check this site's microphone permission.",
                    Some("NotFoundError") => "No microphone found. Connect a microphone and try again.",
                    Some("NotReadableError" | "AbortError") => "The browser couldn't open the microphone. Check your input device and try again.",
                    _ => "Microphone unavailable. Check your browser and try again.",
                };
                mic_status("MIC", message);
                web_sys::console::warn_1(&error);
            }
        }
    });
}

fn microphone() {
    let request = APP.with(|app| {
        let mut app = app.borrow_mut();
        let app = app.as_mut()?;
        if app.stream.take().is_some() || app.mic_request.take().is_some() {
            mic_status("MIC", "Microphone off.");
            return None;
        }
        let request = Rc::new(());
        app.mic_request = Some(Rc::clone(&request));
        Some(request)
    });
    let Some(request) = request else {
        return;
    };
    mic_status(
        "CANCEL MIC",
        "Allow microphone access in your browser. Click MIC again to cancel.",
    );
    let options = MediaStreamConstraints::new();
    let audio = Object::new();
    for key in ["echoCancellation", "noiseSuppression", "autoGainControl"] {
        set(&audio, key, false);
    }
    let hint = Object::new();
    set(&hint, "ideal", 0.);
    set(&audio, "latency", hint);
    let mono = Object::new();
    set(&mono, "ideal", 1.);
    set(&audio, "channelCount", mono);
    options.set_audio(&audio);
    options.set_video(&JsValue::FALSE);

    let window = window().unwrap();
    // Request directly in the click callback, before yielding to a future.
    let capture = window
        .navigator()
        .media_devices()
        .and_then(|devices| devices.get_user_media_with_constraints(&options));
    let capture = match capture {
        Ok(capture) => capture,
        Err(error) => return finish_mic_request(&request, Err(error)),
    };
    let deadline = Rc::clone(&request);
    let timeout = Closure::once_into_js(move || {
        APP.with(|app| {
        if let Some(app) = app.borrow_mut().as_mut().filter(|app| active_mic_request(app, &deadline)) {
            app.mic_request = None;
            mic_status("RETRY MIC", "The browser didn’t start the mic. Try again or open this page in your regular browser.");
            el("patchbay").set_attribute("open", "").ok();
        }
    })
    });
    let _ = window
        .set_timeout_with_callback_and_timeout_and_arguments_0(timeout.unchecked_ref(), 20_000);
    spawn_local(async move { finish_mic_request(&request, JsFuture::from(capture).await) });
}
const LATENCY_CONTROLS: [&str; 5] = [
    "latency-test",
    "software-latency-test",
    "microphone",
    "handset",
    "tone",
];
fn measure_latency(synthetic: bool) {
    let ready = APP.with(|s| {
        s.borrow().as_ref().is_some_and(|a| {
            (synthetic || a.stream.is_some())
                && a.mic_request.is_none()
                && !a.lifted
                && a.tone.is_none()
        })
    });
    if !ready {
        let message = if synthetic {
            "Replace the receiver, finish any mic request and turn Test voice off."
        } else {
            "Enable the live mic, replace the receiver and turn Test voice off. Use speakers for this test."
        };
        text("latency-status", message);
        return;
    }
    APP.with(|s| {
        if let Some(a) = s.borrow_mut().as_mut() {
            a.testing = true;
        }
    });
    for id in LATENCY_CONTROLS {
        let _ = el(id).set_attribute("disabled", "");
    }
    text(
        "latency-status",
        if synthetic {
            "Measuring software delay…"
        } else {
            "Measuring… Keep quiet for seven seconds."
        },
    );
    command("latency_start", "synthetic", synthetic);
}
fn midi_route(data: &JsValue) {
    let bytes = Uint8Array::new(&get(data, "data"));
    let m = object("midi");
    set(&m, "bytes", &bytes);
    send(&m);
    let thru: HtmlInputElement = el("thru").unchecked_into();
    if thru.checked() {
        let out: HtmlSelectElement = el("midi-out").unchecked_into();
        if let Ok(index) = out.value().parse::<usize>() {
            APP.with(|s| {
                if let Some(o) = s.borrow().as_ref().and_then(|a| a.outputs.get(index)) {
                    let args = Array::new();
                    args.push(&bytes);
                    let _ = call(o, "send", &args);
                }
            });
        }
    }
}
fn midi_output(bytes: &Uint8Array) {
    APP.with(|s| {
        if let Some(o) = s
            .borrow()
            .as_ref()
            .and_then(|a| a.output_index.and_then(|index| a.outputs.get(index)))
        {
            let args = Array::new();
            args.push(bytes);
            let _ = call(o, "send", &args);
        }
    });
}
fn silence_output(port: &JsValue) {
    for channel in 0..16 {
        let args = Array::new();
        args.push(&Uint8Array::from([0xb0 | channel, 123, 0].as_slice()));
        let _ = call(port, "send", &args);
    }
}
async fn load_file(file: File) -> Result<(), JsValue> {
    if file.size() > 32. * 1024. * 1024. {
        text("file-status", "Choose a MIDI file smaller than 32 MB.");
        return Ok(());
    }
    text("file-status", &format!("Reading {}…", file.name()));
    let bytes = Uint8Array::new(&JsFuture::from(file.array_buffer()).await?);
    let m = object("file");
    set(&m, "bytes", bytes);
    set(&m, "name", file.name());
    send(&m);
    Ok(())
}
fn refresh_midi() -> Result<(), JsValue> {
    APP.with(|s| -> Result<(), JsValue> {
        let mut s = s.borrow_mut();
        let Some(a) = s.as_mut() else {
            return Ok(());
        };
        if let Some(port) = a.output_index.and_then(|index| a.outputs.get(index)) {
            silence_output(port);
        }
        a.output_index = None;
        for (input, _) in &a.input_handlers {
            set(input, "onmidimessage", JsValue::NULL);
        }
        a.input_handlers.clear();
        a.outputs.clear();
        el("midi-in").set_inner_html("<option value=''>None</option>");
        el("midi-out").set_inner_html("<option value=''>None</option>");
        for (key, select) in [("inputs", "midi-in"), ("outputs", "midi-out")] {
            let iter = call(&get(&a.midi, key), "values", &Array::new())?;
            for (n, item) in js_sys::try_iter(&iter)?.unwrap().enumerate() {
                let item = item?;
                let option = doc().create_element("option")?;
                option.set_attribute("value", &n.to_string())?;
                option.set_text_content(Some(
                    &get(&item, "name")
                        .as_string()
                        .unwrap_or_else(|| format!("MIDI device {}", n + 1)),
                ));
                el(select).append_child(&option)?;
                if key == "outputs" {
                    a.outputs.push(item);
                } else {
                    let callback = Closure::<dyn FnMut(JsValue)>::new(move |event| {
                        let select: HtmlSelectElement = el("midi-in").unchecked_into();
                        if select.value() == n.to_string() {
                            midi_route(&event);
                        }
                    });
                    set(&item, "onmidimessage", callback.as_ref().clone());
                    a.input_handlers.push((item, callback));
                }
            }
        }
        text(
            "midi-status",
            "MIDI connected. Choose an input and an optional thru output.",
        );
        Ok(())
    })
}
async fn midi_connect() -> Result<(), JsValue> {
    let Some(context) = APP.with(|s| s.borrow().as_ref().map(|a| a.context.clone())) else {
        return Ok(());
    };
    let navigator: JsValue = window().unwrap().navigator().into();
    let request = get(&navigator, "requestMIDIAccess");
    if !request.is_function() {
        text(
            "midi-status",
            "This browser does not offer Web MIDI. Try a browser that supports it.",
        );
        return Ok(());
    }
    let access = JsFuture::from(
        call(&navigator, "requestMIDIAccess", &Array::new())?.dyn_into::<js_sys::Promise>()?,
    )
    .await?;
    if !APP.with(|s| {
        s.borrow()
            .as_ref()
            .is_some_and(|a| Object::is(a.context.as_ref(), context.as_ref()))
    }) {
        return Ok(());
    }
    let callback = Closure::<dyn FnMut(JsValue)>::new(|_| {
        if let Err(e) = refresh_midi() {
            report(e);
        }
    });
    set(&access, "onstatechange", callback.into_js_value());
    APP.with(|s| s.borrow_mut().as_mut().unwrap().midi = access);
    refresh_midi()
}
#[wasm_bindgen(start)]
pub fn start() {
    if window().is_none() {
        return;
    }
    photo_geometry::align_controls(&doc());
    k2_bridge::install();
    bind(el("zoom").as_ref(), "click", |_| {
        let zoom = el("zoom").get_attribute("aria-pressed").as_deref() != Some("true");
        let _ = el("workbench")
            .class_list()
            .toggle_with_force("zoomed", zoom);
        let _ = el("zoom").set_attribute("aria-pressed", if zoom { "true" } else { "false" });
        text("zoom", if zoom { "Fit" } else { "Zoom +" });
        if zoom {
            let area: HtmlElement = el("workbench").unchecked_into();
            area.set_scroll_left(
                (f64::from(el("phone").client_width()) * 0.48
                    - f64::from(area.client_width()) * 0.5)
                    .max(0.) as i32,
            );
            area.set_scroll_top(
                (f64::from(el("phone").client_height()) * 0.48
                    - f64::from(area.client_height()) * 0.45)
                    .max(0.) as i32,
            );
        }
    });
    bind(el("midi-out").as_ref(), "change", |_| {
        let select: HtmlSelectElement = el("midi-out").unchecked_into();
        APP.with(|s| {
            if let Some(a) = s.borrow_mut().as_mut() {
                if let Some(port) = a.output_index.and_then(|index| a.outputs.get(index)) {
                    silence_output(port);
                }
                a.output_index = select.value().parse().ok();
            }
        });
    });
    bind(el("midi-in").as_ref(), "change", |_| {
        let bytes: Vec<u8> = (0..16)
            .flat_map(|channel| [0xb0 | channel, 123, 0])
            .collect();
        let message = object("midi");
        set(&message, "bytes", Uint8Array::from(bytes.as_slice()));
        send(&message);
        let thru: HtmlInputElement = el("thru").unchecked_into();
        if thru.checked() {
            APP.with(|s| {
                if let Some(port) = s
                    .borrow()
                    .as_ref()
                    .and_then(|a| a.output_index.and_then(|i| a.outputs.get(i)))
                {
                    silence_output(port);
                }
            });
        }
    });
    bind(el("thru").as_ref(), "change", |_| {
        let thru: HtmlInputElement = el("thru").unchecked_into();
        if !thru.checked() {
            APP.with(|s| {
                if let Some(port) = s
                    .borrow()
                    .as_ref()
                    .and_then(|a| a.output_index.and_then(|i| a.outputs.get(i)))
                {
                    silence_output(port);
                }
            });
        }
    });
    bind(el("midi-file").as_ref(), "change", |_| {
        let input: HtmlInputElement = el("midi-file").unchecked_into();
        if let Some(file) = input.files().and_then(|f| f.get(0)) {
            spawn_local(async {
                if let Err(e) = load_file(file).await {
                    text("file-status", &format!("File could not be read: {e:?}"));
                }
            });
        }
    });
    bind(el("file-play").as_ref(), "click", |_| {
        command("file_play", "value", true)
    });
    bind(el("file-stop").as_ref(), "click", |_| {
        command("file_stop", "value", true)
    });
    bind(doc().as_ref(), "dragover", |e| {
        e.prevent_default();
    });
    bind(doc().as_ref(), "drop", |e| {
        e.prevent_default();
        let e: DragEvent = e.unchecked_into();
        if let Some(file) = e
            .data_transfer()
            .and_then(|d| d.files())
            .and_then(|files| files.get(0))
        {
            if APP.with(|s| s.borrow().is_some()) {
                spawn_local(async {
                    if let Err(e) = load_file(file).await {
                        text("file-status", &format!("File could not be read: {e:?}"));
                    }
                });
            } else {
                text("status", "Switch on the phone before dropping a MIDI file.");
            }
        }
    });
    bind(el("power").as_ref(), "click", |_| {
        if APP.with(|s| s.borrow().is_some()) {
            command("shutdown", "value", true);
            let _ = el("power").set_attribute("disabled", "");
            return;
        }
        let _ = el("power").set_attribute("disabled", "");
        text("status", "Starting…");
        spawn_local(async {
            if let Err(e) = power().await {
                report(e);
                let _ = el("power").remove_attribute("disabled");
            }
        });
    });
    bind_handset();
    for i in 0..12 {
        let key = doc()
            .query_selector(&format!("[data-key='{i}']"))
            .unwrap()
            .unwrap();
        bind(key.as_ref(), "pointerdown", move |e| {
            e.prevent_default();
            let p: PointerEvent = e.unchecked_into();
            let target: Element = p.current_target().unwrap().unchecked_into();
            let _ = target.set_pointer_capture(p.pointer_id());
            button(i, true);
        });
        for event in ["pointerup", "pointercancel", "lostpointercapture"] {
            bind(key.as_ref(), event, move |_| button(i, false));
        }
    }
    bind(el("enc-down").as_ref(), "click", |_| turn_encoder(-1));
    bind(el("enc-up").as_ref(), "click", |_| turn_encoder(1));
    bind_dial("encoder", true);
    bind_dial("gain", false);
    bind(el("encoder").as_ref(), "wheel", |e| {
        let e: WheelEvent = e.unchecked_into();
        e.prevent_default();
        turn_encoder(if e.delta_y() > 0. { 1 } else { -1 });
    });
    bind(el("gain").as_ref(), "input", |_| {
        gain_changed();
    });
    bind(el("microphone").as_ref(), "click", |_| microphone());
    bind(el("latency-test").as_ref(), "click", |_| {
        measure_latency(false)
    });
    bind(el("software-latency-test").as_ref(), "click", |_| {
        measure_latency(true)
    });
    bind(el("tone").as_ref(), "change", |_| {
        let input: HtmlInputElement = el("tone").unchecked_into();
        let checked = input.checked();
        let result = APP.with(|s| -> Result<(), JsValue> {
            if let Some(a) = s.borrow_mut().as_mut() {
                if checked {
                    let tone = a.context.create_oscillator()?;
                    tone.frequency().set_value(220.);
                    let gain = a.context.create_gain()?;
                    gain.gain().set_value(0.16);
                    tone.connect_with_audio_node(&gain)?;
                    gain.connect_with_audio_node(&a.node)?;
                    tone.start()?;
                    a.tone = Some((tone, gain));
                } else if let Some((tone, gain)) = a.tone.take() {
                    tone.stop()?;
                    gain.disconnect()?;
                }
            }
            Ok(())
        });
        if let Err(e) = result {
            report(e);
        }
    });
    bind(el("midi").as_ref(), "click", |_| {
        spawn_local(async {
            if let Err(e) = midi_connect().await {
                text("midi-status", &format!("MIDI unavailable: {e:?}"));
            }
        })
    });
    for name in ["keydown", "keyup"] {
        let down = name == "keydown";
        bind(doc().as_ref(), name, move |e| {
            let e: KeyboardEvent = e.unchecked_into();
            if e.repeat() || e.ctrl_key() || e.meta_key() || e.alt_key() {
                return;
            }
            if let Some(target) = e.target().and_then(|t| t.dyn_into::<Element>().ok()) {
                if matches!(target.tag_name().as_str(), "INPUT" | "SELECT" | "TEXTAREA") {
                    return;
                }
                if target.tag_name() == "BUTTON" && matches!(e.key().as_str(), " " | "Enter") {
                    if let Some(index) = target
                        .get_attribute("data-key")
                        .and_then(|n| n.parse::<u8>().ok())
                    {
                        e.prevent_default();
                        button(index, down);
                    } else if target.id() == "encoder" {
                        e.prevent_default();
                        command("encoder_button", "value", down);
                    }
                    return;
                }
            }
            let key = e.key();
            if let Some(index) = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "*", "0", "#"]
                .iter()
                .position(|v| *v == key)
            {
                e.prevent_default();
                button(index as u8, down);
            } else {
                match key.as_str() {
                    "ArrowLeft" | "ArrowDown" if down => {
                        e.prevent_default();
                        turn_encoder(-1);
                    }
                    "ArrowRight" | "ArrowUp" if down => {
                        e.prevent_default();
                        turn_encoder(1);
                    }
                    "Enter" => {
                        e.prevent_default();
                        command("encoder_button", "value", down);
                    }
                    " " if down => {
                        e.prevent_default();
                        lift();
                    }
                    _ => {}
                }
            }
        });
    }
    bind(window().unwrap().as_ref(), "blur", |_| {
        for n in 0..12 {
            button(n, false);
        }
        command("encoder_button", "value", false);
    });
    text("status", "Power on, then pick up.");
    let _ = el("power").remove_attribute("disabled");
}

// The worker owns the only emulated instrument and all its mutable state.
struct Machine {
    cpu: Cpu,
    rate: f64,
    source_rate: f64,
    input: VecDeque<f32>,
    input_resampler: resample::Resampler,
    output_resampler: resample::Resampler,
    samples: u64,
    start: f64,
    last_ui: f64,
    underruns: f64,
    queued_frames: f64,
    resyncs: f64,
    capture_age_ms: f64,
    input_drops: u64,
    probe_input_drops: u64,
    failed: bool,
    switches: [controls::Switch; 13],
    peak: f32,
    configured: bool,
    booting: bool,
    flash_revision: u64,
    song: Option<midi_file::Song>,
    song_cursor: usize,
    song_start: u64,
    song_playing: bool,
    output: Vec<f32>,
    audio_port: Option<MessagePort>,
    tick_pending: bool,
    probe: Option<LatencyProbe>,
    lifted: bool,
}
struct LatencyProbe {
    reference: Vec<f32>,
    synthetic: bool,
}
fn queue_tick(machine: &mut Machine, port: &MessagePort) {
    if !machine.tick_pending && !machine.failed {
        machine.tick_pending = true;
        let _ = port.post_message(&JsValue::NULL);
    }
}
fn receive_mic(machine: &mut Machine, data: &JsValue) {
    machine.underruns = get(data, "underruns").as_f64().unwrap_or(0.);
    machine.queued_frames = get(data, "queuedFrames").as_f64().unwrap_or(0.);
    machine.resyncs = get(data, "resyncs").as_f64().unwrap_or(0.);
    machine.capture_age_ms = (now() - get(data, "capturedAt").as_f64().unwrap_or_else(now)).max(0.);
    let samples = Float32Array::new(&get(data, "mic")).to_vec();
    machine.input.extend(machine.input_resampler.process(
        &samples,
        machine.rate,
        machine.source_rate,
    ));
    while machine.input.len() > 8192 {
        machine.input.pop_front();
        machine.input_drops += 1;
    }
}
fn latency_status(scope: &DedicatedWorkerGlobalScope, message: &str) {
    let data = object("latency_status");
    set(&data, "text", message);
    post(scope, &data);
}
fn finish_measurement(machine: &mut Machine, data: &JsValue, scope: &DedicatedWorkerGlobalScope) {
    let Some(probe) = machine.probe.take() else {
        return;
    };
    // The receiver was down before the test. Restore the real hook input before
    // releasing the worklet's output mute; never monitor a speaker/mic loop.
    let _ = machine.cpu.handset(false);
    if let Some(port) = &machine.audio_port {
        let done = Object::new();
        set(&done, "testFinished", true);
        let _ = port.post_message(&done);
    }
    let mic = Float32Array::new(&get(data, "micRecording")).to_vec();
    let processed = Float32Array::new(&get(data, "processedRecording")).to_vec();
    let dropped = u32::from(machine.input_drops != machine.probe_input_drops);
    match latency::measure(&probe.reference, &mic, &processed, machine.rate,
        (get(data, "underruns").as_f64().unwrap_or(1.) as u32).saturating_add(dropped)) {
        Ok(result) if probe.synthetic => latency_status(scope, &format!(
            "Software delay ≈{:.1} ms through the current firmware mode. Five probes, {:.1} ms spread, {:.0}% match. Excludes mic and speaker hardware.",
            result.processing_ms, result.spread_ms, result.confidence*100.)),
        Ok(result) => latency_status(scope, &format!(
            "Measured ≈{:.1} ms mic → speaker, including the air path. Device loop {:.1} ms + app/firmware {:.1} ms. Five probes; spread {:.1} ms; match {:.0}%. This measures the current effect only.",
            result.total_ms(), result.device_ms, result.processing_ms, result.spread_ms, result.confidence*100.)),
        Err(error) => latency_status(scope, error),
    }
}
fn start_capture(port: &MessagePort, rate: f64) -> Result<(), JsValue> {
    let start = Object::new();
    set(&start, "start", true);
    set(
        &start,
        "maxQueuedFrames",
        (rate * streaming::MAX_PLAYBACK_SECONDS).ceil(),
    );
    set(&start, "captureFrames", streaming::PACKET_FRAMES as u32);
    set(
        &start,
        "prebufferFrames",
        (rate * streaming::PREBUFFER_SECONDS).ceil(),
    );
    port.post_message(&start)
}
fn now() -> f64 {
    js_sys::Date::now()
}
fn post(scope: &DedicatedWorkerGlobalScope, message: &JsValue) {
    let _ = scope.post_message(message);
}
fn fault(scope: &DedicatedWorkerGlobalScope, e: JsValue) {
    let m = object("error");
    set(
        &m,
        "error",
        e.as_string().unwrap_or_else(|| format!("{e:?}")),
    );
    post(scope, &m);
}
fn all_notes_off(
    cpu: &Cpu,
    scope: &DedicatedWorkerGlobalScope,
    controller: u8,
) -> Result<(), JsValue> {
    for channel in 0..16 {
        let bytes = Uint8Array::from([0xb0 | channel, controller, 0].as_slice());
        cpu.midi(&bytes)?;
        let message = object("midi_out");
        set(&message, "bytes", bytes);
        post(scope, &message);
    }
    Ok(())
}
fn tick(scope: DedicatedWorkerGlobalScope, state: Rc<RefCell<Machine>>, port: MessagePort) {
    let result = (|| -> Result<(), JsValue> {
        let mut m = state.borrow_mut();
        m.tick_pending = false;
        if m.failed {
            return Ok(());
        }
        if !m.configured {
            return Ok(());
        }
        if m.booting {
            // Boot the actual ARM image and restore settings before opening the
            // live codec stream. Capturing during boot leaves old mic frames in
            // the input/output FIFOs even after throughput catches up to 1×.
            for _ in 0..4 {
                m.cpu.run_for(100_000, 2_000_000)?;
            }
            m.cpu.take_audio();
            if m.cpu.virtual_time_ns() >= streaming::BOOT_NS {
                m.booting = false;
                m.start = now();
                m.source_rate = m.cpu.sample_rate();
                start_capture(
                    m.audio_port.as_ref().ok_or("Audio port unavailable")?,
                    m.rate,
                )?;
                post(&scope, &object("ready"));
            }
            return Ok(());
        }
        if m.song_playing {
            let elapsed = m.cpu.virtual_time_ns().saturating_sub(m.song_start);
            loop {
                let due = m
                    .song
                    .as_ref()
                    .and_then(|song| song.events.get(m.song_cursor))
                    .filter(|event| event.ns <= elapsed)
                    .map(|event| event.bytes.clone());
                let Some(bytes) = due else {
                    break;
                };
                m.cpu.midi(&Uint8Array::from(bytes.as_slice()))?;
                let message = object("midi_out");
                set(&message, "bytes", Uint8Array::from(bytes.as_slice()));
                post(&scope, &message);
                m.song_cursor += 1;
            }
            if m.song
                .as_ref()
                .is_some_and(|s| m.song_cursor == s.events.len() && elapsed >= s.duration)
            {
                m.song_playing = false;
                all_notes_off(&m.cpu, &scope, 123)?;
                let message = object("file_status");
                set(&message, "text", "MIDI file finished. Play to start again.");
                set(&message, "loaded", true);
                post(&scope, &message);
            }
        }
        if !m.input.is_empty() && m.cpu.pending_input() < 4096 {
            let n = m.input.len().min(4096 - m.cpu.pending_input() as usize);
            let samples: Vec<f32> = m.input.drain(..n).flat_map(|sample| [0., sample]).collect();
            m.cpu
                .input_stereo(&Float32Array::from(samples.as_slice()))?;
        }
        // Follow captured codec frames instead of generating future silence.
        // A bounded instruction batch cannot outrun the buffered input.
        if (m.cpu.pending_input() as usize) < streaming::INPUT_READY {
            return Ok(());
        }
        for _ in 0..4 {
            let time = m.cpu.virtual_time_ns();
            for index in 0..13 {
                while let Some(pressed) = m.switches[index].due(time) {
                    if index == 12 {
                        m.cpu.encoder_button(pressed)?;
                    } else {
                        m.cpu.key(index as u8, pressed)?;
                    }
                }
            }
            if (m.cpu.pending_input() as usize) < streaming::INPUT_READY {
                break;
            }
            let duration = streaming::slice_ns(m.cpu.pending_input() as usize, m.source_rate);
            m.cpu.run_for(100_000, duration)?;
        }
        let pcm = m.cpu.take_audio().to_vec();
        m.peak = pcm.iter().fold(0f32, |peak, sample| peak.max(sample.abs()));
        m.samples += pcm.len() as u64;
        if !pcm.is_empty() {
            let physical_rate = m.cpu.sample_rate();
            if physical_rate > 0. {
                m.source_rate = physical_rate;
            }
            let (source_rate, rate) = (m.source_rate, m.rate);
            let output = m.output_resampler.process(&pcm, source_rate, rate);
            m.output.extend(output);
            if m.output.len() >= streaming::PACKET_FRAMES {
                let output = std::mem::take(&mut m.output);
                let data = object("audio");
                let audio = Float32Array::from(output.as_slice());
                let transfer = Array::of1(audio.buffer().as_ref());
                set(&data, "audio", audio);
                m.audio_port
                    .as_ref()
                    .ok_or("Audio port is not connected")?
                    .post_message_with_transferable(&data, &transfer)?;
            }
        }
        let elapsed = now() - m.start;
        if now() - m.last_ui > 150. {
            m.last_ui = now();
            let data = object("screen");
            set(&data, "bytes", m.cpu.oled());
            post(&scope, &data);
            let revision = m.cpu.flash_revision();
            if revision != m.flash_revision {
                m.flash_revision = revision;
                let data = object("flash");
                set(&data, "bytes", m.cpu.flash_image());
                post(&scope, &data);
            }
            if elapsed > 1200. {
                let speed = m.samples as f64 / m.source_rate / (elapsed / 1000.);
                let data = object("performance");
                set(&data, "slow", speed < 0.95);
                set(
                    &data,
                    "text",
                    format!(
                        "{speed:.2}× throughput · capture delivery {:.0} ms · input queue {:.1} ms · playback queue {:.1} ms · {} output samples · peak {:.4} · {} underruns · {} overflow recoveries · {} input frames dropped. {}",
                        m.capture_age_ms,
                        (m.input.len() as f64 + f64::from(m.cpu.pending_input())) / m.source_rate * 1000.,
                        m.queued_frames / m.rate * 1000.,
                        m.samples,
                        m.peak,
                        m.underruns,
                        m.resyncs,
                        m.input_drops,
                        m.cpu.stats()
                    ),
                );
                post(&scope, &data);
            }
        }
        Ok(())
    })();
    if let Err(e) = result {
        state.borrow_mut().failed = true;
        fault(&scope, e);
        return;
    }
    let mut m = state.borrow_mut();
    if m.configured
        && (m.booting
            || m.cpu.pending_input() as usize >= streaming::INPUT_READY
            || !m.input.is_empty())
    {
        queue_tick(&mut m, &port);
    }
}
#[wasm_bindgen]
pub async fn start_worker(firmware_url: String) -> Result<(), JsValue> {
    JsFuture::from(initialize_engine()).await?;
    let scope: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    let response: Response = JsFuture::from(scope.fetch_with_str(&firmware_url))
        .await?
        .dyn_into()?;
    if !response.ok() {
        return Err("Firmware download failed".into());
    }
    let image = Uint8Array::new(&JsFuture::from(response.array_buffer()?).await?);
    let cpu = Cpu::new(&image)?;
    cpu.handset(false)?;
    let state = Rc::new(RefCell::new(Machine {
        cpu,
        rate: 48000.,
        source_rate: 48_187.487,
        input: VecDeque::new(),
        input_resampler: resample::Resampler::default(),
        output_resampler: resample::Resampler::default(),
        samples: 0,
        start: now(),
        last_ui: 0.,
        underruns: 0.,
        queued_frames: 0.,
        resyncs: 0.,
        capture_age_ms: 0.,
        input_drops: 0,
        probe_input_drops: 0,
        failed: false,
        switches: core::array::from_fn(|_| controls::Switch::default()),
        peak: 0.,
        configured: false,
        booting: true,
        flash_revision: 0,
        song: None,
        song_cursor: 0,
        song_start: 0,
        song_playing: false,
        output: Vec::new(),
        audio_port: None,
        tick_pending: false,
        probe: None,
        lifted: false,
    }));
    let channel = MessageChannel::new()?;
    let port = channel.port2();
    let s = state.clone();
    let scope_error = scope.clone();
    let wakeup = port.clone();
    bind(scope.as_ref(), "message", move |e| {
        let e: MessageEvent = e.unchecked_into();
        let d = e.data();
        let mut m = s.borrow_mut();
        let value = get(&d, "value");
        let result = match get(&d, "kind").as_string().as_deref() {
            Some("audio_port") => (|| -> Result<(), JsValue> {
                let audio_port: MessagePort = get(&d, "port").dyn_into()?;
                let state = s.clone();
                let wake = wakeup.clone();
                let measurement_scope = scope_error.clone();
                bind(audio_port.as_ref(), "message", move |event| {
                    let event: MessageEvent = event.unchecked_into();
                    let mut machine = state.borrow_mut();
                    if machine.failed {
                        return;
                    }
                    let data = event.data();
                    if get(&data, "latencyResult").is_truthy() {
                        finish_measurement(&mut machine, &data, &measurement_scope);
                    } else {
                        receive_mic(&mut machine, &data);
                    }
                    queue_tick(&mut machine, &wake);
                });
                audio_port.start();
                if m.configured && !m.booting {
                    start_capture(&audio_port, m.rate)?;
                }
                m.audio_port = Some(audio_port);
                Ok(())
            })(),
            Some("file") => {
                match midi_file::parse(&Uint8Array::new(&get(&d, "bytes")).to_vec()) {
                    Ok(song) => {
                        if m.song_playing {
                            let _ = all_notes_off(&m.cpu, &scope_error, 120);
                        }
                        let message = object("file_status");
                        set(
                            &message,
                            "text",
                            format!(
                                "{} · {:.1} s · {} MIDI events",
                                get(&d, "name").as_string().unwrap_or_default(),
                                song.duration as f64 / 1e9,
                                song.events.len()
                            ),
                        );
                        set(&message, "loaded", true);
                        post(&scope_error, &message);
                        m.song = Some(song);
                        m.song_cursor = 0;
                        m.song_playing = false;
                    }
                    Err(error) => {
                        let message = object("file_status");
                        set(&message, "text", error);
                        post(&scope_error, &message);
                    }
                }
                Ok(())
            }
            Some("file_play") => {
                if m.song.is_some() {
                    let _ = all_notes_off(&m.cpu, &scope_error, 120);
                    m.song_start = m.cpu.virtual_time_ns();
                    m.song_cursor = 0;
                    m.song_playing = true;
                    let message = object("file_status");
                    set(
                        &message,
                        "text",
                        "Playing through the firmware’s MIDI UART.",
                    );
                    set(&message, "loaded", true);
                    post(&scope_error, &message);
                }
                Ok(())
            }
            Some("file_stop") => {
                m.song_playing = false;
                m.song_cursor = 0;
                let message = object("file_status");
                set(&message, "text", "MIDI file stopped.");
                set(&message, "loaded", m.song.is_some());
                post(&scope_error, &message);
                all_notes_off(&m.cpu, &scope_error, 120)
            }
            Some("configure") => {
                m.rate = get(&d, "rate").as_f64().unwrap_or(48000.);
                let _ = m
                    .cpu
                    .mic_gain(get(&d, "gain").as_f64().unwrap_or(1.) as f32);
                if !m.configured {
                    let flash = get(&d, "flash");
                    if !flash.is_undefined()
                        && m.cpu.restore_flash(&Uint8Array::new(&flash)).is_err()
                    {
                        let data = object("persistence");
                        set(
                            &data,
                            "text",
                            "Saved settings could not be read. The phone is using defaults.",
                        );
                        post(&scope_error, &data);
                    }
                    m.configured = true;
                    m.start = now();
                    Ok(())
                } else {
                    Ok(())
                }
            }
            Some("shutdown") => {
                let data = object("shutdown");
                set(&data, "bytes", m.cpu.flash_image());
                post(&scope_error, &data);
                m.failed = true;
                Ok(())
            }
            Some("key") => {
                let index = get(&d, "index").as_f64().unwrap_or(0.) as usize;
                if index < 12 {
                    let pressed = get(&d, "pressed").is_truthy();
                    let now = m.cpu.virtual_time_ns();
                    m.switches[index].set(pressed, now);
                    Ok(())
                } else {
                    Err("Invalid keypad index".into())
                }
            }
            Some("encoder") => {
                m.cpu
                    .encoder(get(&d, "steps").as_f64().unwrap_or(0.) as i32);
                Ok(())
            }
            Some("encoder_button") => {
                let now = m.cpu.virtual_time_ns();
                m.switches[12].set(value.is_truthy(), now);
                Ok(())
            }
            Some("latency_start") => (|| -> Result<(), JsValue> {
                if m.probe.is_some() {
                    return Ok(());
                }
                if m.lifted || m.song_playing || m.cpu.virtual_time_ns() < 1_000_000_000 {
                    latency_status(&scope_error, "Let the phone finish starting, stop MIDI playback and replace the receiver before measuring.");
                    return Ok(());
                }
                let synthetic = get(&d, "synthetic").is_truthy();
                let probe = LatencyProbe {
                    reference: latency::probe(m.rate),
                    synthetic,
                };
                m.cpu.handset(true)?;
                let message = Object::new();
                let audio = Float32Array::from(probe.reference.as_slice());
                let transfer = Array::of1(audio.buffer().as_ref());
                set(&message, "probe", audio);
                set(&message, "synthetic", synthetic);
                m.audio_port
                    .as_ref()
                    .ok_or("Audio port unavailable")?
                    .post_message_with_transferable(&message, &transfer)?;
                m.probe = Some(probe);
                m.probe_input_drops = m.input_drops;
                Ok(())
            })(),
            Some("handset") => {
                if m.probe.is_some() {
                    Ok(())
                } else {
                    m.lifted = value.is_truthy();
                    m.cpu.handset(m.lifted)
                }
            }
            Some("gain") => m.cpu.mic_gain(value.as_f64().unwrap_or(1.) as f32),
            Some("midi") => m.cpu.midi(&Uint8Array::new(&get(&d, "bytes"))),
            _ => Ok(()),
        };
        if let Err(e) = result {
            m.failed = true;
            fault(&scope_error, e);
        } else {
            queue_tick(&mut m, &wakeup);
        }
    });
    let s = state.clone();
    let sc = scope.clone();
    let p = port.clone();
    bind(channel.port1().as_ref(), "message", move |_| {
        tick(sc.clone(), s.clone(), p.clone())
    });
    channel.port1().start();
    let ready = object("connect");
    post(&scope, &ready);
    tick(scope, state, port);
    Ok(())
}
