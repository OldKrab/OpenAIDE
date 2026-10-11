import { copyFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

const output = resolve('apps/android/build/runtime');
// The runtime is the App Server alone: the app ships the Frontend and starts the server itself.
rmSync(output, { recursive: true, force: true });
mkdirSync(output, { recursive: true });
// Setup compares this with the app's version and offers the matching runtime when they differ.
writeFileSync(resolve(output, 'VERSION'), `${JSON.parse(readFileSync('package.json', 'utf8')).version}\n`);
mkdirSync(resolve(output, 'bin'), { recursive: true });
copyFileSync('target/aarch64-linux-android/release/openaide-app-server', resolve(output, 'bin/openaide-app-server'));
copyFileSync('LICENSE', resolve(output, 'LICENSE'));
