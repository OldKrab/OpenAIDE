package io.openaide.android;

import org.junit.Test;
import static org.junit.Assert.*;

public class DocumentPickerTest {
    @Test public void opensThePhotoPickerOnlyForAnyImage() {
        assertTrue(DocumentPicker.wantsPhotos(new String[]{"image/*"}));
        assertTrue(DocumentPicker.wantsPhotos(new String[]{" Image/* "}));
        // The photo picker cannot honour these, so they stay document inputs.
        assertFalse(DocumentPicker.wantsPhotos(new String[]{"image/png"}));
        assertFalse(DocumentPicker.wantsPhotos(new String[]{"image/*", "application/pdf"}));
        assertFalse(DocumentPicker.wantsPhotos(new String[]{""}));
        assertFalse(DocumentPicker.wantsPhotos(new String[]{null}));
        assertFalse(DocumentPicker.wantsPhotos(new String[0]));
        assertFalse(DocumentPicker.wantsPhotos(null));
    }
}
