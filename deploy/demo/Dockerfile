FROM ubuntu:24.04

WORKDIR /app

COPY deploy/demo/vendor/node-v25 /usr/local
COPY deploy/demo/vendor/lib/x86_64-linux-gnu/libatomic.so.1* /usr/lib/x86_64-linux-gnu/
COPY deploy/demo/vendor/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt

COPY package.json package-lock.json tsconfig.base.json ./
COPY packages/app-server-client/package.json packages/app-server-client/package.json
COPY packages/app-shell-contracts/package.json packages/app-shell-contracts/package.json
COPY packages/frontend/package.json packages/frontend/package.json
COPY apps/web/package.json apps/web/package.json
COPY apps/vscode-extension/package.json apps/vscode-extension/package.json

COPY node_modules node_modules

COPY packages packages
COPY apps apps
COPY scripts scripts
COPY deploy/demo/vendor/@agentclientprotocol/codex-acp /usr/local/lib/node_modules/@agentclientprotocol/codex-acp
COPY target/debug/openaide-app-server /usr/local/bin/openaide-app-server

RUN npm run build --workspace @openaide/app-server-client \
  && npm run build --workspace @openaide/app-shell-contracts \
  && npm run build --workspace openaide-frontend \
  && ln -sf /usr/local/lib/node_modules/@agentclientprotocol/codex-acp/dist/index.js /usr/local/bin/codex-acp

ENV NODE_ENV=production \
  HOME=/home/node \
  PATH=/usr/local/bin:$PATH \
  SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt \
  NODE_EXTRA_CA_CERTS=/etc/ssl/certs/ca-certificates.crt \
  OPENAIDE_APP_SERVER_PATH=/usr/local/bin/openaide-app-server \
  OPENAIDE_WEB_HOST=0.0.0.0 \
  OPENAIDE_WEB_PORT=8080 \
  OPENAIDE_WEB_VITE_PORT=5173 \
  OPENAIDE_WEB_STATE_ROOT=/var/lib/openaide/state \
  OPENAIDE_WEB_RUNTIME_ROOT=/var/lib/openaide/runtime \
  OPENAIDE_WEB_ALLOWED_HOSTS=localhost,127.0.0.1

RUN groupadd --gid 10001 node \
  && useradd --uid 10001 --gid 10001 --create-home --home-dir /home/node --shell /bin/false node \
  && mkdir -p /var/lib/openaide/state /var/lib/openaide/runtime /home/node/.codex \
  && chown -R node:node /var/lib/openaide /home/node

USER node

EXPOSE 8080

CMD ["npm", "run", "dev", "--workspace", "openaide-web"]
