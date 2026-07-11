package org.mhtoolkit.savesync

import org.junit.Assert.assertThrows
import org.junit.Test

class SafStabilityPolicyTest {
    @Test fun changedFingerprintFailsClosed() {
        assertThrows(IllegalArgumentException::class.java) {
            SafStabilityPolicy.requireMatching("before", "after")
        }
    }

    @Test fun matchingFingerprintIsAccepted() {
        SafStabilityPolicy.requireMatching("same", "same")
    }
}
