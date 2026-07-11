package org.mhtoolkit.savesync
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
class RecoverySecretFormatTest {
    @Test fun rejectsLowEntropyOrMalformedSecrets() { assertThrows(IllegalArgumentException::class.java) { RecoverySecretFormat.decodeHex("password") } }
    @Test fun acceptsExactly256Bits() { assertEquals(32, RecoverySecretFormat.decodeHex("ab".repeat(32)).size) }
}
