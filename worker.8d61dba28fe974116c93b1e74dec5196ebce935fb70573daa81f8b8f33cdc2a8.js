// Browser module-worker entry. All emulation and message handling live in Rust.
import init, { start_worker } from './phone.8d61dba28fe974116c93b1e74dec5196ebce935fb70573daa81f8b8f33cdc2a8.js';
await init();
await start_worker(new URL('./synthphone.8d61dba28fe974116c93b1e74dec5196ebce935fb70573daa81f8b8f33cdc2a8.bin', import.meta.url).href);
