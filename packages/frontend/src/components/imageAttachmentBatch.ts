/** Preserve selection order, attempt every file, then surface the first failure. */
export async function attachEveryImage(
  files: File[],
  attach: (file: File) => Promise<void>,
) {
  let firstError: unknown;
  let failed = false;
  for (const file of files) {
    try {
      await attach(file);
    } catch (error) {
      if (!failed) firstError = error;
      failed = true;
    }
  }
  if (failed) throw firstError;
}
