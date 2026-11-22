// Execute the actual JavaScript-only AudioWorklet bridge in Node's VM. Signal
// analysis and firmware remain Rust; these test the browser transfer contract.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
function worklet() {
  const sent = [];
  let Processor;
  class AudioWorkletProcessor {
    constructor() { this.port = { postMessage: message => sent.push(message) }; }
  }
  vm.runInNewContext(fs.readFileSync(new URL('./audio-worklet.js', import.meta.url), 'utf8'), {
    AudioWorkletProcessor, Float32Array, sampleRate: 48000,
    registerProcessor: (_, constructor) => { Processor = constructor; }
  });
  const processor = new Processor();
  const send = data => processor.port.onmessage({data});
  send({start:true, captureFrames:128, prebufferFrames:288, maxQueuedFrames:1920});
  function render(value = 0.1) {
    const input = new Float32Array(128).fill(value);
    const left = new Float32Array(128), right = new Float32Array(128);
    processor.process([[input]], [[left,right]]);
    assert.deepEqual(left,right,'both physical output channels must match');
    return left;
  }
  return { processor, send, sent, render };
}
test('captures one browser quantum without changing normal firmware PCM', () => {
  const {send,sent,render}=worklet();
  const pcm=Float32Array.from({length:512},(_,i)=>i/1024);
  send({audio:pcm});
  assert.deepEqual(render(),pcm.slice(0,128));
  assert.equal(sent.length,1);
  assert.equal(sent[0].mic.length,128);
  assert.ok(sent[0].mic.every(v=>Math.abs(v-0.1)<1e-6));
  assert.equal(sent[0].underruns,0);
  assert.equal(sent[0].resyncs,0);
});
test('recovers from an old playback backlog and reports discarded PCM', () => {
  const {processor,send,sent,render}=worklet();
  send({audio:new Float32Array(1600).fill(0.2)});
  send({audio:new Float32Array(1600).fill(0.8)});
  assert.equal(processor.queued,288);
  assert.ok(render().every(v=>Math.abs(v-0.8)<1e-6),'must play the fresh end, not old mic history');
  assert.equal(sent[0].resyncs,1);
});
test('records real input and firmware output while only the probe is monitored', () => {
  const {send,sent,render}=worklet();
  send({audio:new Float32Array(512).fill(0.25)});
  const probe=new Float32Array(512).fill(0.03);
  send({probe});
  for (let i=0;i<4;i++) assert.deepEqual(render(),probe.slice(0,128));
  const result=sent.find(m=>m.latencyResult);
  assert.equal(result.underruns,0);
  assert.ok(result.micRecording.every(v=>Math.abs(v-0.1)<1e-6));
  assert.ok(result.processedRecording.every(v=>v===0.25));
  send({audio:new Float32Array(512).fill(0.4)});
  assert.ok(render().every(v=>v===0),'monitoring stays muted until hook restoration');
  send({testFinished:true});
  assert.ok(render().every(v=>Math.abs(v-0.4)<1e-6));
});
test('a playback recovery during calibration invalidates its result', () => {
  const {send,sent,render}=worklet();
  send({probe:new Float32Array(128)});
  send({audio:new Float32Array(5000)});
  render();
  assert.equal(sent.find(m=>m.latencyResult).underruns,1);
});
