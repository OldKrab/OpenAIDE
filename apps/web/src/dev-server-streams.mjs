export function pipeProxyResponse(proxyRes, res) {
  return new Promise((resolve, reject) => {
    let settled = false;
    const finish = () => {
      if (settled) return;
      settled = true;
      cleanup();
      resolve();
    };
    const cancel = () => {
      if (settled) return;
      settled = true;
      cleanup();
      // A browser-aborted event stream must also release the App Server stream.
      // Otherwise the orphan can keep draining events that no browser can observe.
      proxyRes.destroy();
      resolve();
    };
    const fail = (error) => {
      if (isConnectionReset(error)) {
        cancel();
        return;
      }
      if (settled) return;
      settled = true;
      cleanup();
      proxyRes.destroy();
      reject(error);
    };
    const cleanup = () => {
      proxyRes.off("end", finish);
      proxyRes.off("error", fail);
      res.off("finish", finish);
      res.off("close", cancel);
      res.off("error", fail);
    };

    proxyRes.once("end", finish);
    proxyRes.once("error", fail);
    res.once("finish", finish);
    res.once("close", cancel);
    res.once("error", fail);
    proxyRes.pipe(res);
  });
}

export function watchPendingProxyResponse(proxyReq, res) {
  let pending = true;
  let resolveCancelled;
  const cancelled = new Promise((resolve) => {
    resolveCancelled = resolve;
  });
  const cleanup = () => {
    res.off("close", cancel);
    res.off("error", cancel);
  };
  const cancel = () => {
    if (!pending) return;
    pending = false;
    cleanup();
    // The downstream can disappear before the upstream sends headers. Abort
    // that pending request so it cannot later become an orphan event stream.
    proxyReq.destroy();
    resolveCancelled();
  };
  res.once("close", cancel);
  res.once("error", cancel);

  return {
    cancelled,
    handoff() {
      if (!pending) return false;
      pending = false;
      cleanup();
      return true;
    },
  };
}

export function isConnectionReset(error) {
  return error && typeof error === "object" && ["ECONNRESET", "EPIPE"].includes(error.code);
}
