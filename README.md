# Virtual Synthphone E

The virtual instrument runs the physical Synthphone E firmware with photographed
controls, browser audio and MIDI routing. [Virtual Daisy](https://github.com/UnitedCode/virtual-daisy)
provides the reusable board models and ARM CPU core as pinned Git dependencies.
This application previously lived in that repository's `web` directory; its Git
history and public mirror were retained when it moved here.

Power on, enable the microphone, then pick up the receiver. Play the photographed
keys and drag the real side dials. MIDI files and MIDI input/output are in the
small toolbar; keyboard shortcuts are under **?**.

To play the phone from K2's keyboard or MIDI player, open the
[K2 + Synthphone paired view](https://nathansbradshaw.github.io/k2-midi-viewer/?duet=1)
and power on the phone. Channel 2 plays the synth, channel 1 controls vocals,
and channel 10 plays drums. The phone's controls and microphone remain available.
The connection needs no virtual MIDI driver or Web MIDI permission.

**Mic delay:** in **I/O**, enable the live mic, keep the receiver down and choose
**Measure mic delay**. Use speakers in a quiet room. Five quiet probes measure
the physical speaker/mic loop and the current firmware effect separately.
Start with Dry. Effects that change the probe too much return an inconclusive
result. Recordings stay in memory; they are never uploaded or saved.

**Without a mic:** choose **I/O → Measure software delay**. A silent probe
passes through the browser audio path and the real ARM firmware. The result
excludes the mic, speakers and their system buffers.

The Rust WebAssembly backend boots the real ARM firmware. Its UI, OLED, synth,
vocal effects and saved settings run on that CPU. The photo layers preserve the
actual instrument; see [asset provenance](phone/assets/README.md).

**Experimental:** live vocal audio can fall behind on slower browsers. The
firmware lab renders complete clips and offers WAV downloads. Processing stays
on the visitor's computer; the host serves static files. Timing is functional,
not hardware calibrated.

<details><summary>Build and run locally</summary>

Install the `wasm32-unknown-unknown` Rust target and `wasm-bindgen-cli` version
`0.2.127`. From this repository:

```sh
CARGO_TARGET_DIR=target cargo run --manifest-path site-builder/Cargo.toml -- build
CARGO_TARGET_DIR=target cargo run --manifest-path site-builder/Cargo.toml -- serve
```

The preview binds `127.0.0.1:3030`. Check its ownership before launching it;
stop your preview when finished. Deploy `dist` to any static HTTPS
host. No Renode installation, login, hardware, backend, or paid API is required
for visitors.

For agent work, reuse an existing compatible `CARGO_TARGET_DIR`.

GitHub Pages rebuilds on app changes and checks firmware `main` hourly. The
workflow builds the hardware repository's normal release target, boots those
exact ARM bytes in the emulator, and tests the audio path before deploying.
See the live [firmware revision](https://unitedcode.github.io/virtual-synthphone-e/firmware-revision.txt)
and [image hash](https://unitedcode.github.io/virtual-synthphone-e/firmware-sha256.txt).

To request a production rebuild now, sign in with `gh` and run:

```sh
CARGO_TARGET_DIR=target cargo run --manifest-path site-builder/Cargo.toml -- pages
```

To build the same bundle from a clean firmware checkout locally, install the
`thumbv7em-none-eabihf` target and `llvm-tools-preview` component, then run:

```sh
CARGO_TARGET_DIR=target cargo run --manifest-path site-builder/Cargo.toml -- build-firmware /path/to/synth-phone-e-v2-rust
```

Plain `build` uses the checked-in regression image. `build-firmware` compiles
the original `app` binary with the firmware's target configuration and lockfile.
If the ARM bytes change, the builder profiles the new image before compiling
the browser engine. It never applies hot instruction addresses from an older image.

</details>

<details><summary>Verification and implementation</summary>

```sh
cargo test --release --manifest-path engine/Cargo.toml
cargo test --manifest-path phone/Cargo.toml
```

The engine connects a Rust Cortex-M interpreter to Virtual Daisy's RCC, memory,
GPIO, QSPI, DMA, SAI and codec models. Browser-specific Rust adapters provide
TIM2, USART1, the blocking I2C buses, and the SSD1306 display. ADC coverage
currently includes the firmware's initialization sequence, not general ADC
conversions. Oscillator readiness is immediate; analog transients and bus timing
are not calibrated. Unmapped accesses stop with an error.

Short sequences are decoded at build time into Rust instruction calls and
guarded register-local expressions. These still execute CPU instruction
semantics and require the matching firmware hash. In native tests, every
accepted lifted block is shadowed by interpretation and compared bit for bit:
FP and general registers, flags, changed memory, PC, instruction count and cycles.
Differential tests also compare complete audio with interpretation.
Busy idle and decrement loops have bounded CPU-state-preserving coalescing.
Board service follows audio, timer, UART, encoder and flash deadlines. MMIO
accesses synchronize first; interrupt boundaries stop translated execution.
Browser slices also stop after two virtual milliseconds, keeping captured mic
input and queued controls responsive. A finer polling schedule is checked
against the same firmware's PCM output. A transferred message port connects the
firmware worker directly to the audio thread; UI rendering does not relay PCM.
The ARM firmware finishes its startup before live capture opens, so old mic
audio cannot collect during boot. Capture and output packets are 128 frames;
playback starts with 6 ms queued. CPU slices retain a small codec input guard.
The Engine drawer reports the input and playback queues separately from
throughput. These queue lengths are not a substitute for acoustic measurement.
Playback overflow above 40 ms discards stale queued PCM and reports a recovery;
the delay test rejects any capture with dropouts or discarded audio. Runtime
assets use content-derived filenames so browser caches cannot mix releases.

Generate repeatable test input and measure the real ARM firmware in V8:

```sh
rustc -O engine/benchmark-fixture.rs -o /tmp/synthphone-fixture
/tmp/synthphone-fixture /tmp/synthphone-voice.f32 voice
node engine/benchmark.mjs dist /tmp/synthphone-voice.f32 6 3 full
/tmp/synthphone-fixture /tmp/synthphone-stress.mid midi
```

Profiles are 0 Pitch Control, 1 Vocode, 2 Dry, 3 Tuned, 4 Harmony and 5 Percussion.
`full` adds eight MIDI voices, drums and octave/formant controls. The generated
60-second MIDI file exercises the same controls through the phone's file player.

On an Apple M5 Pro with Node 26.5.1 (2026-11-22), six-second `full` runs rendered
at 1.09×, 1.75×, 1.29×, 1.09×, 1.04× and 1.26× respectively. These measure
firmware throughput, not microphone-to-speaker latency; slower devices can
still fall behind. Capture/playback packet buffering is separate from the
firmware's own FFT latency.

The interpreter's source, Apache license, upstream revision and CPU tests live in
[Virtual Daisy's CPU crate](https://github.com/UnitedCode/virtual-daisy/tree/main/cpu). Regression tests cover interrupt IT state, NVIC banks, conditional float
moves, instruction flags, and 12,402 independently executed QEMU ARM float cases
(also tested with sticky IXC). Firmware tests check actual microphone processing,
silent input, MIDI channel routing, keypad scanning, encoder menus, hook channel
selection, sustained streaming, and firmware QSPI save/restore across power
cycles. Nonvolatile flash is stored in localStorage; microphone samples are not
persisted. These are simulated checks, not physical
hardware or listening acceptance. See the [backend parity audit](docs/rust-backend-parity.md)
for the tested interrupt fixes and remaining emulator differences.

MIDI channel 1 controls vocal effects silently. Channel 2 plays the synthesizer;
the lab's audible A4 test uses channel 2. This follows the firmware's routing.

</details>
