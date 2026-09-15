import assert from 'node:assert/strict';
import test from 'node:test';
import { createMobileStatus } from './mobile-status.mjs';

test('mobile status counts all pages without disclosing task data', async () => {
  const requests = [];
  const status = createMobileStatus(async (params) => {
    requests.push(params);
    return { result: params.cursor
      ? { tasks: [{ status: 'waiting', title: 'private' }, { status: 'completed' }] }
      : { tasks: [{ status: 'running', taskId: 'private' }], nextCursor: 'next' } };
  });
  assert.deepEqual(await status(), { product: 'OpenAIDE', mobileProtocol: 1, active: 1, waiting: 1 });
  await status();
  assert.equal(requests.length, 2);
  assert.equal(requests[1].cursor, 'next');
});

test('failed or malformed status must never be interpreted as idle', async () => {
  const failed = createMobileStatus(async () => { throw new Error('offline'); });
  await assert.rejects(failed());
  const malformed = createMobileStatus(async () => ({}));
  await assert.rejects(malformed());
});
