package org.mhtoolkit.savesync

import org.junit.Assert.assertThrows
import org.junit.Test

class SafCapturePolicyTest {
    @Test fun rejectsDepthPastLimit() {
        assertThrows(IllegalArgumentException::class.java) {
            SafCapturePolicy.validateDepth(SafCapturePolicy.MAX_DEPTH + 1)
        }
    }

    @Test fun rejectsControlCharactersAndEmptyNames() {
        for (name in listOf("", "..", "bad\u0000name", "a/b", "a\\b")) {
            assertThrows(name, IllegalArgumentException::class.java) { SafCapturePolicy.validateName(name) }
        }
    }

    @Test fun rejectsByteBeforeItCanExceedLimit() {
        assertThrows(IllegalArgumentException::class.java) {
            SafCapturePolicy.validateFileBytes(SafCapturePolicy.MAX_FILE_BYTES + 1)
        }
        assertThrows(IllegalArgumentException::class.java) {
            SafCapturePolicy.validateTotals(1, SafCapturePolicy.MAX_TOTAL_BYTES + 1)
        }
    }
}
