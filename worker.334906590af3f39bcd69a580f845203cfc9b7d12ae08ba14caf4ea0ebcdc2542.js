// Browser module-worker entry. All emulation and message handling live in Rust.
import init, { start_worker } from './phone.334906590af3f39bcd69a580f845203cfc9b7d12ae08ba14caf4ea0ebcdc2542.js';
await init();
await start_worker(new URL('./synthphone.334906590af3f39bcd69a580f845203cfc9b7d12ae08ba14caf4ea0ebcdc2542.bin', import.meta.url).href);
