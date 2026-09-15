set -eu
: "${ANDROID_JAR:?Android platform jar required}"
: "${D8_JAR:?D8 jar required}"
: "${APKSIGNER_JAR:?APK signer jar required}"
: "${QR_JAR:?ZXing jar required}"
output=apps/android/build/device-tests
platform=$(dirname "$ANDROID_JAR")
mkdir -p "$output/classes" "$output/dex"
classpath="$ANDROID_JAR:$platform/optional/android.test.runner.jar:$platform/optional/android.test.base.jar"
find apps/android/app/src/androidTest/java -name '*.java' > "$output/sources"
javac -source 17 -target 17 -cp "$classpath:apps/android/build/local-apk/classes:$QR_JAR" -d "$output/classes" @"$output/sources"
jar cf "$output/classes.jar" -C "$output/classes" .
java -cp "$D8_JAR" com.android.tools.r8.D8 --min-api 26 --lib "$ANDROID_JAR" \
    --lib "$platform/optional/android.test.runner.jar" --lib "$platform/optional/android.test.base.jar" \
    --classpath apps/android/build/local-apk/classes.jar --classpath "$QR_JAR" --output "$output/dex" "$output/classes.jar"
node --input-type=module <<'JS'
import fs from 'node:fs';
fs.writeFileSync('apps/android/build/device-tests/AndroidManifest.xml',
  fs.readFileSync('apps/android/app/src/androidTest/AndroidManifest.xml', 'utf8').replace('<manifest ', '<manifest package="io.openaide.android.test" '));
JS
aapt2 link -I "$ANDROID_JAR" --manifest "$output/AndroidManifest.xml" -o "$output/tests.apk"
(cd "$output/dex"; jar uf ../tests.apk classes*.dex)
java -jar "$APKSIGNER_JAR" sign --ks "$OPENAIDE_ANDROID_KEYSTORE" --ks-type PKCS12 --ks-key-alias openaide-debug \
    --ks-pass "file:$OPENAIDE_ANDROID_PASSWORD_FILE" "$output/tests.apk"
printf 'Device tests built: %s/tests.apk\n' "$output"
