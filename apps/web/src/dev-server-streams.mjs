export function pipeProxyResponse(proxyRes, res) {
  return new Promise((resolve, reject) => {
    let settled = false;
    const finish = () => {
      if (settled) return;
      settled = true;
      cleanup();
      resolve();
    };
    const fail = (error) => {
      if (isConnectionReset(error)) {
        finish();
        return;
      }
      if (settled) return;
      settled = true;
      cleanup();
      reject(error);
    };
    const cleanup = () => {
      proxyRes.off("end", finish);
      proxyRes.off("error", fail);
      res.off("finish", finish);
      res.off("close", finish);
      res.off("error", fail);
    };

    proxyRes.once("end", finish);
    proxyRes.once("error", fail);
    res.once("finish", finish);
    res.once("close", finish);
    res.once("error", fail);
    proxyRes.pipe(res);
  });
}

export function isConnectionReset(error) {
  return error && typeof error === "object" && ["ECONNRESET", "EPIPE"].includes(error.code);
}
