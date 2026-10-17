package io.openaide.android;

import org.junit.Test;
import static org.junit.Assert.*;

public class CameraCaptureTest {
    @Test public void offersTheCameraOnlyForImageInputs() {
        assertTrue(CameraCapture.acceptsImages(new String[]{"image/*"}));
        assertTrue(CameraCapture.acceptsImages(new String[]{"application/pdf", " Image/png"}));
        assertFalse(CameraCapture.acceptsImages(new String[]{""}));
        assertFalse(CameraCapture.acceptsImages(new String[]{"application/pdf"}));
        assertFalse(CameraCapture.acceptsImages(new String[0]));
        assertFalse(CameraCapture.acceptsImages(null));
    }
}
