import {test} from 'node:test';
import assert from 'node:assert/strict';
import * as core from './bridge.mjs';
test('registers the same ABI for the verified iframe module loader', () => {
  assert.equal(globalThis.retromEthornell?.abi, 'ethornell-host-v1');
  assert.equal(globalThis.retromEthornell?.create, core.create);
});
