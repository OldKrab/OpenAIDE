/**
 * This app's OpenAIDE version, baked in at build time by `appBuildVersion.ts`: a release
 * version, or the last release plus the commit for any other build. Undefined only where a
 * build config does not define it, such as a test runner.
 */
export const APP_VERSION: string | undefined = import.meta.env.OPENAIDE_APP_VERSION;
