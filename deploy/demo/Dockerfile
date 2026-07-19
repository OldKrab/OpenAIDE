FROM rust:1-bookworm AS rust-builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY openaide-rs openaide-rs
RUN cargo build --locked --release -p openaide-app-server

FROM node:24-bookworm-slim AS web-builder

WORKDIR /app
COPY package.json package-lock.json tsconfig.base.json ./
COPY packages/app-server-client/package.json packages/app-server-client/package.json
COPY packages/app-shell-contracts/package.json packages/app-shell-contracts/package.json
COPY packages/frontend/package.json packages/frontend/package.json
COPY apps/web/package.json apps/web/package.json
COPY apps/vscode-extension/package.json apps/vscode-extension/package.json
RUN npm ci
COPY packages packages
COPY apps apps
COPY scripts scripts
RUN npm run build --workspace @openaide/app-server-client \
  && npm run build --workspace @openaide/app-shell-contracts \
  && npm run build --workspace openaide-frontend

FROM node:24-bookworm-slim

WORKDIR /app
RUN npm install --global @agentclientprotocol/codex-acp@1.1.4 \
  && groupadd --gid 10001 nodeapp \
  && useradd --uid 10001 --gid 10001 --create-home --home-dir /home/node --shell /bin/false nodeapp

COPY --from=web-builder /app /app
COPY --from=rust-builder /app/target/release/openaide-app-server /usr/local/bin/openaide-app-server

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

RUN mkdir -p /var/lib/openaide/state /var/lib/openaide/runtime /home/node/.codex \
  && chown -R nodeapp:nodeapp /var/lib/openaide /home/node

USER nodeapp

EXPOSE 8080

CMD ["node", "apps/web/src/dev-server.mjs"]
