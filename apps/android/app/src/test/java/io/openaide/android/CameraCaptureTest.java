package io.openaide.android;

import org.junit.Test;
import static org.junit.Assert.*;

public class CameraCaptureTest {
    @Test public void opensTheCameraOnlyForAPhotoInput() {
        assertTrue(CameraCapture.wantsPhoto(true, new String[]{"image/*"}));
        // Attach images has no capture: it stays a document picker and never asks for the camera.
        assertFalse(CameraCapture.wantsPhoto(false, new String[]{"image/*"}));
        assertFalse(CameraCapture.wantsPhoto(true, new String[]{""}));
    }

    @Test public void recognizesImageInputs() {
        assertTrue(CameraCapture.acceptsImages(new String[]{"image/*"}));
        assertTrue(CameraCapture.acceptsImages(new String[]{"application/pdf", " Image/png"}));
        assertFalse(CameraCapture.acceptsImages(new String[]{""}));
        assertFalse(CameraCapture.acceptsImages(new String[]{"application/pdf"}));
        assertFalse(CameraCapture.acceptsImages(new String[0]));
        assertFalse(CameraCapture.acceptsImages(null));
    }
}
