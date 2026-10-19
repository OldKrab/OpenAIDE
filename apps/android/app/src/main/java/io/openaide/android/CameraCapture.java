package io.openaide.android;

import android.Manifest;
import android.app.Activity;
import android.content.ClipData;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.provider.MediaStore;
import androidx.core.content.FileProvider;
import java.io.File;
import java.io.IOException;

/**
 * Takes a photo for a Web Shell file input that asks for one (`capture`, Take photo).
 * A WebView, unlike Chrome, never opens the camera for a file input; the app must.
 * The photo is written to this app's cache through its own FileProvider, so the
 * WebView receives it as a content document like any picked file. Android refuses
 * the camera intent from an app that declares CAMERA (pairing scans with it) until
 * the permission is granted, so the caller asks first. Inputs without `capture`
 * never reach the camera, so attaching images never asks for it.
 * Owns at most one pending capture, from opening the camera to its result.
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

    /** Whether a file input asks for a new photo rather than a document. */
    static boolean wantsPhoto(boolean capture, String[] types) { return capture && acceptsImages(types); }

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

    /** Whether the camera is writing a photo that its result will deliver. */
    boolean capturing() { return pending != null; }

    boolean permitted() { return activity.checkSelfPermission(PERMISSION) == PackageManager.PERMISSION_GRANTED; }

    /** The camera intent writing the pending photo. */
    Intent intent() throws IOException {
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
        return camera;
    }

    /** The photo a camera result carries, or null when the user cancelled or nothing was written. */
    Uri photo(int result) {
        if (pending == null) return null;
        // A camera writes to the output it was given; what it returns as data varies by app.
        if (result != Activity.RESULT_OK || pending.length() == 0) { discard(); return null; }
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
