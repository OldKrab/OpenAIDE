package io.openaide.android

/**
 * Pairing Codes: the plain text a QR code carries between this phone and a computer.
 *
 * The format is owned by the App Server (`remote_devices/pairing_code.rs`); this is
 * the device half. An invite names the App Server and holds a single-use secret. A
 * join request names this device and its self-reported labels.
 */
internal object PairingCode {
    private const val INVITE_PREFIX = "OAI1"
    private const val JOIN_PREFIX = "OAJ1"
    private const val KEY_BYTES = 32
    private const val SECRET_BYTES = 16
    const val MAX_LABEL_BYTES = 64
    private const val ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"

    enum class Problem { NOT_A_PAIRING_CODE, WRONG_KIND, MALFORMED }

    class Invalid(val problem: Problem) : Exception(problem.name)

    class Invite(val server: ByteArray, val secret: ByteArray) {
        /** The secret as the App Server expects it in the pairing request. */
        val secretText: String get() = encodeBase32(secret)
        val serverId: String get() = server.joinToString("") { "%02x".format(it.toInt() and 0xff) }
    }

    fun parseInvite(text: String): Invite {
        // A pasted code may carry the spaces, dashes, or case a person or app added.
        val code = text.filterNot { it.isWhitespace() || it == '-' }.uppercase()
        if (!code.startsWith(INVITE_PREFIX)) {
            throw Invalid(if (code.startsWith(JOIN_PREFIX)) Problem.WRONG_KIND else Problem.NOT_A_PAIRING_CODE)
        }
        val bytes = decodeBase32(code.substring(INVITE_PREFIX.length)) ?: throw Invalid(Problem.MALFORMED)
        if (bytes.size != KEY_BYTES + SECRET_BYTES) throw Invalid(Problem.MALFORMED)
        return Invite(bytes.copyOfRange(0, KEY_BYTES), bytes.copyOfRange(KEY_BYTES, bytes.size))
    }

    fun join(device: ByteArray, name: String, model: String?): String {
        require(device.size == KEY_BYTES) { "device key must be 32 bytes" }
        val bytes = java.io.ByteArrayOutputStream()
        bytes.write(device)
        for (label in listOf(clampLabel(name), clampLabel(model ?: ""))) {
            val encoded = label.toByteArray(Charsets.UTF_8)
            bytes.write(encoded.size)
            bytes.write(encoded)
        }
        return JOIN_PREFIX + encodeBase32(bytes.toByteArray())
    }

    /** Trims a self-reported label to what one row can show, on a character boundary. */
    fun clampLabel(label: String): String {
        val trimmed = label.trim()
        val clamped = StringBuilder()
        var size = 0
        var index = 0
        while (index < trimmed.length) {
            val point = trimmed.codePointAt(index)
            val width = String(Character.toChars(point)).toByteArray(Charsets.UTF_8).size
            if (size + width > MAX_LABEL_BYTES) break
            size += width
            if (!Character.isISOControl(point)) clamped.appendCodePoint(point)
            index += Character.charCount(point)
        }
        return clamped.toString()
    }

    /** Groups a code for reading aloud or typing; the App Server ignores the spaces. */
    fun grouped(code: String): String = code.chunked(4).joinToString(" ")

    fun encodeBase32(bytes: ByteArray): String {
        val text = StringBuilder((bytes.size * 8 + 4) / 5)
        var buffer = 0
        var bits = 0
        for (byte in bytes) {
            buffer = (buffer shl 8) or (byte.toInt() and 0xff)
            bits += 8
            while (bits >= 5) {
                bits -= 5
                text.append(ALPHABET[(buffer shr bits) and 31])
            }
        }
        if (bits > 0) text.append(ALPHABET[(buffer shl (5 - bits)) and 31])
        return text.toString()
    }

    fun decodeBase32(text: String): ByteArray? {
        // Unpadded base32 never has these lengths; trailing bits must be zero.
        if (text.length % 8 in setOf(1, 3, 6)) return null
        val bytes = java.io.ByteArrayOutputStream(text.length * 5 / 8)
        var buffer = 0
        var bits = 0
        for (character in text) {
            val value = ALPHABET.indexOf(character)
            if (value < 0) return null
            buffer = ((buffer shl 5) or value) and 0xfff
            bits += 5
            if (bits >= 8) {
                bits -= 8
                bytes.write((buffer shr bits) and 0xff)
            }
        }
        if (bits > 0 && (buffer and ((1 shl bits) - 1)) != 0) return null
        return bytes.toByteArray()
    }
}
