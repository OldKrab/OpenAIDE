set -eu
: "${ANDROID_JAR:?Set ANDROID_JAR to the Android 35 platform jar}"
: "${D8_JAR:?Set D8_JAR to the D8 compiler jar}"
: "${APKSIGNER_JAR:?Set APKSIGNER_JAR to the APK signer jar}"
: "${QR_JAR:?Set QR_JAR to ZXing core 3.5.3}"
: "${OPENAIDE_ANDROID_KEYSTORE:?Set the existing signing keystore path}"
: "${OPENAIDE_ANDROID_PASSWORD_FILE:?Set the private signing password file path}"
build_type="${OPENAIDE_ANDROID_BUILD_TYPE:-debug}"
case "$build_type" in debug|release) ;; *) printf 'Use debug or release build type\n' >&2; exit 1 ;; esac
export OPENAIDE_ANDROID_BUILD_TYPE="$build_type"
output=apps/android/build/local-apk
node --input-type=module -e "import fs from 'node:fs'; for (const directory of ['classes', 'dex']) fs.rmSync('apps/android/build/local-apk/' + directory, {recursive:true, force:true});"
mkdir -p "$output/generated" "$output/classes" "$output/dex"
node --input-type=module <<'JS'
import fs from 'node:fs';
import { validateProjectVersion } from './scripts/release-version.mjs';
const { version } = JSON.parse(fs.readFileSync('package.json', 'utf8'));
validateProjectVersion(version);
const [major, minor, patch] = version.split('-')[0].split('.').map(Number);
const versionCode = major * 1000000 + minor * 1000 + patch;
if (minor > 999 || patch > 999 || !Number.isSafeInteger(versionCode) || versionCode < 1 || versionCode > 2100000000) throw new Error('Android version is out of range');
fs.writeFileSync('apps/android/build/local-apk/version.env', `OPENAIDE_ANDROID_VERSION='${version}'\nOPENAIDE_ANDROID_VERSION_CODE=${versionCode}\n`);
fs.mkdirSync('apps/android/build/local-apk/generated/io/openaide/android', { recursive: true });
fs.writeFileSync('apps/android/build/local-apk/generated/io/openaide/android/BuildConfig.java',
  `package io.openaide.android; public final class BuildConfig { public static final String VERSION_NAME = ${JSON.stringify(version)}; public static final int VERSION_CODE = ${versionCode}; }\n`);
const manifest = fs.readFileSync('apps/android/app/src/main/AndroidManifest.xml', 'utf8')
  .replace('<manifest ', '<manifest package="io.openaide.android" ')
  .replace('<application', `<application android:debuggable="${process.env.OPENAIDE_ANDROID_BUILD_TYPE === 'debug'}"`);
fs.writeFileSync('apps/android/build/local-apk/AndroidManifest.xml', manifest);
JS
. "$output/version.env"
aapt2 compile --dir apps/android/app/src/main/res -o "$output/resources.zip"
aapt2 link -I "$ANDROID_JAR" --manifest "$output/AndroidManifest.xml" --min-sdk-version 26 --target-sdk-version 35 \
    --version-code "$OPENAIDE_ANDROID_VERSION_CODE" --version-name "$OPENAIDE_ANDROID_VERSION" --auto-add-overlay --java "$output/generated" \
    -A apps/android/app/src/main/assets -o "$output/unsigned.apk" "$output/resources.zip"
find apps/android/app/src/main/java "$output/generated" -name '*.java' > "$output/sources"
javac -source 17 -target 17 -cp "$ANDROID_JAR:$QR_JAR" -d "$output/classes" @"$output/sources"
jar cf "$output/classes.jar" -C "$output/classes" .
java -cp "$D8_JAR" com.android.tools.r8.D8 --min-api 26 --lib "$ANDROID_JAR" --output "$output/dex" "$output/classes.jar" "$QR_JAR"
cp "$output/unsigned.apk" "$output/app-$build_type.apk"
(cd "$output/dex"; jar uf "../app-$build_type.apk" classes*.dex)
java -jar "$APKSIGNER_JAR" sign --ks "$OPENAIDE_ANDROID_KEYSTORE" --ks-type PKCS12 --ks-key-alias openaide-debug \
    --ks-pass "file:$OPENAIDE_ANDROID_PASSWORD_FILE" "$output/app-$build_type.apk"
java -jar "$APKSIGNER_JAR" verify "$output/app-$build_type.apk"
printf 'APK built: %s/app-%s.apk\n' "$output" "$build_type"
