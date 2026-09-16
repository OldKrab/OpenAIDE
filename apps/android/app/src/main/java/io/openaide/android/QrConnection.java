package io.openaide.android;

import android.content.Context;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.net.Uri;
import com.google.zxing.BinaryBitmap;
import com.google.zxing.MultiFormatReader;
import com.google.zxing.RGBLuminanceSource;
import com.google.zxing.common.HybridBinarizer;

final class QrConnection {
    static String read(Context context, Uri document) throws Exception {
        BitmapFactory.Options options = new BitmapFactory.Options();
        options.inJustDecodeBounds = true;
        try (var input = context.getContentResolver().openInputStream(document)) { BitmapFactory.decodeStream(input, null, options); }
        if (options.outWidth <= 0 || options.outHeight <= 0) throw new IllegalArgumentException();
        options.inSampleSize = 1;
        while (options.outWidth / options.inSampleSize > 2048 || options.outHeight / options.inSampleSize > 2048) options.inSampleSize *= 2;
        options.inJustDecodeBounds = false;
        Bitmap bitmap;
        try (var input = context.getContentResolver().openInputStream(document)) { bitmap = BitmapFactory.decodeStream(input, null, options); }
        if (bitmap == null) throw new IllegalArgumentException();
        try {
            int width = bitmap.getWidth();
            int height = bitmap.getHeight();
            int[] pixels = new int[width * height];
            bitmap.getPixels(pixels, 0, width, 0, 0, width, height);
            String address = new MultiFormatReader().decode(new BinaryBitmap(new HybridBinarizer(new RGBLuminanceSource(width, height, pixels)))).getText();
            return new ConnectionProfile(address, "validate", "validate", false).endpoint;
        } finally { bitmap.recycle(); }
    }
}
