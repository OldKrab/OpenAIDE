package io.openaide.android;

import android.annotation.SuppressLint;
import android.app.Activity;
import android.content.ClipData;
import android.content.Intent;
import android.net.Uri;
import android.os.Build;
import android.os.ext.SdkExtensions;
import android.provider.MediaStore;
import android.webkit.WebChromeClient;
import java.util.ArrayList;
import java.util.List;

/**
 * Chooses the Android picker for a Web Shell file input and reads its result.
 * An input accepting any image opens the system photo picker; every other input,
 * and a phone without that picker, opens the document picker. A narrower image
 * type stays a document input, because the photo picker filters only by whole
 * media kind. Both answer with content documents, so the caller validates them alike.
 */
final class DocumentPicker {
    private static final String ANY_IMAGE = "image/*";

    private DocumentPicker() {}

    /** Whether a file input accepts any image and nothing else. */
    static boolean wantsPhotos(String[] types) {
        return types != null && types.length == 1 && types[0] != null
            && ANY_IMAGE.equals(types[0].trim().toLowerCase(java.util.Locale.ROOT));
    }

    /** Whether an intent from {@link #intent} opens the photo picker. */
    static boolean opensPhotos(Intent picker) { return MediaStore.ACTION_PICK_IMAGES.equals(picker.getAction()); }

    /** The picker for a file input. */
    static Intent intent(String[] types, boolean multiple) {
        return wantsPhotos(types) && photosAvailable() ? photos(multiple) : documents(types, multiple);
    }

    /**
     * The URIs a picker result carries, or null when the user cancelled or chose nothing.
     * A multiple selection arrives as clip data, which parseResult is not documented to read.
     */
    static Uri[] selection(int result, Intent data) {
        ClipData clip = result == Activity.RESULT_OK && data != null ? data.getClipData() : null;
        if (clip == null || clip.getItemCount() == 0) return WebChromeClient.FileChooserParams.parseResult(result, data);
        List<Uri> uris = new ArrayList<>(clip.getItemCount());
        for (int index = 0; index < clip.getItemCount(); index++) {
            Uri uri = clip.getItemAt(index).getUri();
            if (uri != null) uris.add(uri);
        }
        return uris.isEmpty() ? null : uris.toArray(new Uri[0]);
    }

    /** Android 13, or Android 11 and 12 once their media module has been updated. */
    private static boolean photosAvailable() {
        return Build.VERSION.SDK_INT >= 33
            || Build.VERSION.SDK_INT >= 30 && SdkExtensions.getExtensionVersion(Build.VERSION_CODES.R) >= 2;
    }

    // Lint cannot see that intent() reaches this only through photosAvailable().
    @SuppressLint({"NewApi", "InlinedApi"})
    private static Intent photos(boolean multiple) {
        Intent picker = new Intent(MediaStore.ACTION_PICK_IMAGES).setType(ANY_IMAGE);
        // Without the extra the picker selects one photo; a value past the limit makes it cancel.
        if (multiple) picker.putExtra(MediaStore.EXTRA_PICK_IMAGES_MAX, MediaStore.getPickImagesMaxLimit());
        return picker;
    }

    private static Intent documents(String[] types, boolean multiple) {
        Intent picker = new Intent(Intent.ACTION_OPEN_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE);
        picker.setType(types.length == 1 && types[0].contains("/") ? types[0] : "*/*");
        if (types.length > 1) picker.putExtra(Intent.EXTRA_MIME_TYPES, types);
        picker.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, multiple);
        picker.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
        return picker;
    }
}
