export function createMobileStatus(listTasks, now = Date.now) {
  let cached;
  let expires = 0;
  let pending;
  return async () => {
    if (cached && now() < expires) return cached;
    if (pending) return pending;
    pending = (async () => {
      let active = 0;
      let waiting = 0;
      let cursor;
      const cursors = new Set();
      do {
        const response = await listTasks({ lifecycle: 'open', ...(cursor ? { cursor } : {}) });
        const page = response?.result ?? response;
        if (!Array.isArray(page?.tasks)) throw new Error('Invalid task status');
        for (const task of page.tasks) {
          if (['preparing', 'starting', 'running', 'stopping'].includes(task.status)) active++;
          else if (task.status === 'waiting') waiting++;
          else if (!['idle', 'interrupted', 'failed', 'completed'].includes(task.status)) throw new Error('Unknown task status');
        }
        cursor = page.nextCursor;
        if (cursor && cursors.has(JSON.stringify(cursor))) throw new Error('Invalid task cursor');
        cursors.add(JSON.stringify(cursor));
      } while (cursor);
      cached = { product: 'OpenAIDE', mobileProtocol: 1, active, waiting };
      expires = now() + 2_000;
      return cached;
    })().finally(() => { pending = undefined; });
    return pending;
  };
}
