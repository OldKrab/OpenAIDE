import { cpSync, mkdirSync, copyFileSync } from 'node:fs';
import { resolve } from 'node:path';

const output = resolve('apps/android/build/runtime');
mkdirSync(output, { recursive: true });
for (const directory of ['apps/web/src', 'packages/frontend/dist']) {
  cpSync(directory, resolve(output, directory), { recursive: true });
}
for (const name of ['app-server-client', 'app-shell-contracts']) {
  const destination = resolve(output, 'node_modules/@openaide', name);
  mkdirSync(destination, { recursive: true });
  copyFileSync(`packages/${name}/package.json`, resolve(destination, 'package.json'));
  cpSync(`packages/${name}/dist`, resolve(destination, 'dist'), { recursive: true });
}
mkdirSync(resolve(output, 'bin'), { recursive: true });
copyFileSync('target/aarch64-linux-android/release/openaide-app-server', resolve(output, 'bin/openaide-app-server'));
copyFileSync('LICENSE', resolve(output, 'LICENSE'));
