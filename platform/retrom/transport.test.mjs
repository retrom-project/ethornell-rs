import {test} from 'node:test';
import assert from 'node:assert/strict';
import {create} from './bridge.mjs';

function environment(t, readFile) {
  const saved = new Map();
  const set = (key, value) => {saved.set(key, Object.getOwnPropertyDescriptor(globalThis, key));Object.defineProperty(globalThis,key,{value,writable:true,configurable:true});};
  t.after(() => {for (const [key, descriptor] of saved) {if(descriptor)Object.defineProperty(globalThis,key,descriptor);else delete globalThis[key];}});
  class Port {postMessage(){} close(){}}
  class AudioNode {connect(){} disconnect(){} port=new Port()}
  class AudioContext {sampleRate=48000;destination={};audioWorklet={addModule:async()=>{}};createGain(){return Object.assign(new AudioNode(),{gain:{value:1}})}async close(){}async resume(){}async suspend(){}}
  class Worker {
    static instance;
    constructor(){Worker.instance=this;this.terminated=false}
    postMessage(message){if(message.type==='start'){this.shared=message.shared;queueMicrotask(()=>this.onmessage({data:{type:'started',frames:1,progress:false,revision:'0',changed:false}}))}}
    terminate(){this.terminated=true}
    read(values){Atomics.store(new Int32Array(this.shared),0,1);return this.onmessage({data:{type:'read',file:0,offset:0,length:4,...values}})}
  }
  set('crossOriginIsolated',true);set('window',new EventTarget());set('Worker',Worker);set('AudioContext',AudioContext);set('AudioWorkletNode',AudioNode);
  set('MessageChannel',class {port1=new Port();port2=new Port()});set('fetch',async()=>new Response(new Uint8Array([1])));
  const canvas=Object.assign(new EventTarget(),{style:{},focus(){},transferControlToOffscreen(){return {}}});
  const failures=[];
  return {options:{canvas,files:[{path:'system.arc',sizeBytes:8}],readFile,identity:'scope',restore:null,
    fontURL:'blob:font',workerURL:'blob:worker',wasmURL:'blob:wasm',audioWorkletURL:'blob:audio',onFailure:error=>failures.push(error)},
    worker:()=>Worker.instance,failures};
}

test('exit wakes a blocked synchronous reader without waiting for its network callback',async t=>{
  let finish;
  const e=environment(t,()=>new Promise(resolve=>{finish=resolve}));
  const game=await create(e.options), worker=e.worker(), reading=worker.read({});
  await game.close();
  assert.equal(worker.terminated,true);assert.equal(Atomics.load(new Int32Array(worker.shared),0),-2);
  finish(new Uint8Array([1,2,3,4]));await reading;
  assert.equal(Atomics.load(new Int32Array(worker.shared),0),-2);assert.equal(e.failures.length,0);
});
test('rejects out-of-file and oversized requests before invoking Content I/O',async t=>{
  let reads=0;const e=environment(t,async()=>{reads++;return new Uint8Array(4)});
  const game=await create(e.options);await e.worker().read({offset:7,length:4});
  assert.equal(reads,0);assert.match(String(e.failures[0]),/BGI_READ_INVALID/);assert.equal(e.worker().terminated,true);await game.close();
});
