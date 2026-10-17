package io.openaide.android;

import android.Manifest;
import android.app.Activity;
import android.content.ClipData;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.content.pm.ResolveInfo;
import android.net.Uri;
import android.provider.MediaStore;
import androidx.core.content.FileProvider;
import java.io.File;
import java.io.IOException;

/**
 * Offers a new photo beside the document picker when the Web Shell asks for images.
 * A WebView, unlike Chrome, never adds the camera to a file input; the app must.
 * The photo is written to this app's cache through its own FileProvider, so the
 * WebView receives it as a content document like any picked file. Android refuses
 * the camera intent from an app that declares CAMERA (pairing scans with it) until
 * the permission is granted, so the caller asks first or offers documents alone.
 * Owns at most one pending capture, from opening the picker to its result.
 */
final class CameraCapture {
    static final String PERMISSION = Manifest.permission.CAMERA;
    private static final String DIRECTORY = "camera";
    private static final int GRANTS = Intent.FLAG_GRANT_WRITE_URI_PERMISSION | Intent.FLAG_GRANT_READ_URI_PERMISSION;
    /** A delivered photo stays while its upload may still read it; older ones are removed. */
    private static final long STALE_MS = 24L * 60 * 60 * 1000;
    private final Activity activity;
    private File pending;
    private Uri pendingUri;

    CameraCapture(Activity activity) { this.activity = activity; }

    /** Whether a file input's accepted types include images. */
    static boolean acceptsImages(String[] types) {
        if (types == null) return false;
        for (String type : types) if (type != null && type.trim().toLowerCase(java.util.Locale.ROOT).startsWith("image/")) return true;
        return false;
    }

    /** Whether this phone has a camera app to take the photo. */
    boolean available() {
        return activity.getPackageManager().hasSystemFeature(PackageManager.FEATURE_CAMERA_ANY)
            && new Intent(MediaStore.ACTION_IMAGE_CAPTURE).resolveActivity(activity.getPackageManager()) != null;
    }

    boolean permitted() { return activity.checkSelfPermission(PERMISSION) == PackageManager.PERMISSION_GRANTED; }

    /** Wraps the document picker in a chooser that also offers a new photo. */
    Intent chooser(Intent documents) throws IOException {
        discard();
        File directory = new File(activity.getCacheDir(), DIRECTORY);
        if (!directory.isDirectory() && !directory.mkdirs()) throw new IOException("Camera directory unavailable");
        removeStale(directory);
        pending = File.createTempFile("photo-", ".jpg", directory);
        pendingUri = FileProvider.getUriForFile(activity, activity.getPackageName() + ".captures", pending);
        Intent camera = new Intent(MediaStore.ACTION_IMAGE_CAPTURE)
            .putExtra(MediaStore.EXTRA_OUTPUT, pendingUri)
            .addFlags(GRANTS);
        camera.setClipData(ClipData.newRawUri("", pendingUri));
        // A camera started from a chooser's extra targets does not always receive the intent's own grant.
        for (ResolveInfo app : activity.getPackageManager().queryIntentActivities(camera, PackageManager.MATCH_DEFAULT_ONLY))
            activity.grantUriPermission(app.activityInfo.packageName, pendingUri, GRANTS);
        return Intent.createChooser(documents, "Attach images")
            .putExtra(Intent.EXTRA_INITIAL_INTENTS, new Intent[]{camera});
    }

    /** The photo a picker result carries, or null when the user chose documents or cancelled. */
    Uri photo(int result, Intent data) {
        if (pending == null) return null;
        // A camera returns no data, or returns the output it was given; the document picker returns its choice.
        boolean documents = data != null && data.getClipData() != null
            || data != null && data.getData() != null && !data.getData().equals(pendingUri);
        if (result != Activity.RESULT_OK || documents || pending.length() == 0) { discard(); return null; }
        Uri photo = pendingUri;
        // The WebView reads it inside this app; the camera needs no access once it has written.
        activity.revokeUriPermission(photo, GRANTS);
        pending = null;
        pendingUri = null;
        return photo;
    }

    /** Drops a capture that will not be delivered. */
    void discard() {
        if (pendingUri != null) activity.revokeUriPermission(pendingUri, GRANTS);
        if (pending != null && !pending.delete() && pending.exists()) pending.deleteOnExit();
        pending = null;
        pendingUri = null;
    }

    private static void removeStale(File directory) {
        File[] photos = directory.listFiles();
        if (photos == null) return;
        long cutoff = System.currentTimeMillis() - STALE_MS;
        for (File photo : photos) if (photo.lastModified() < cutoff) photo.delete();
    }
}
