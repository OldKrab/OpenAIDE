package io.openaide.android

// TODO(ADR-0081): read codes through openaide-remote-core's native bindings, so this
// copy of the format goes away.

/**
 * Pairing Codes: the plain text a QR code carries between this phone and a computer.
 *
 * The format is owned by `openaide-remote-core` (`pairing_code.rs`); this is the
 * device half. An invite names the App Server and holds a single-use secret. An
 * OpenAIDE before ADR-0081 also showed join-request codes; those are only
 * recognized, so the user can be told where they came from.
 */
internal object PairingCode {
    private const val INVITE_PREFIX = "OAI1"
    private const val OLD_JOIN_PREFIX = "OAJ1"
    private const val KEY_BYTES = 32
    private const val SECRET_BYTES = 16
    const val MAX_LABEL_BYTES = 64
    private const val ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"

    enum class Problem { NOT_A_PAIRING_CODE, FROM_OLDER_OPENAIDE, MALFORMED }

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
            throw Invalid(if (code.startsWith(OLD_JOIN_PREFIX)) Problem.FROM_OLDER_OPENAIDE else Problem.NOT_A_PAIRING_CODE)
        }
        val bytes = decodeBase32(code.substring(INVITE_PREFIX.length)) ?: throw Invalid(Problem.MALFORMED)
        if (bytes.size != KEY_BYTES + SECRET_BYTES) throw Invalid(Problem.MALFORMED)
        return Invite(bytes.copyOfRange(0, KEY_BYTES), bytes.copyOfRange(KEY_BYTES, bytes.size))
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
