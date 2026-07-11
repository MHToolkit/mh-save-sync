package org.mhtoolkit.savesync

import android.content.Context
import android.content.ComponentName
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Bundle
import java.security.MessageDigest
import java.security.SecureRandom
import java.util.UUID

data class NemessixQuiescenceLease(
    val leaseId: String,
    val challengeHex: String,
    val operationId: String,
    val emulatorBuild: String,
)

sealed interface NemessixQuiescenceStatus {
    data class Active(val lease: NemessixQuiescenceLease) : NemessixQuiescenceStatus
    data class Released(
        val lease: NemessixQuiescenceLease,
        val outcome: String,
        val finalManifestHash: String,
    ) : NemessixQuiescenceStatus
    data object Unknown : NemessixQuiescenceStatus
}

class NemessixQuiescenceClient(private val context: Context) {
    fun acquire(
        operation: String = UUID.randomUUID().toString(),
        challenge: String = ByteArray(32).also(SecureRandom()::nextBytes).toHex(),
    ): NemessixQuiescenceLease {
        val response = call("acquire", Bundle().apply {
            putString("operation_id", operation)
            putString("challenge_hex", challenge)
        })
        check(response.getInt("protocol") == 1 && response.getString("state") == "QUIESCENT") {
            "nemessix_quiescence_denied"
        }
        check(response.getString("challenge_hex") == challenge) { "nemessix_quiescence_challenge_mismatch" }
        check(response.getString("operation_id") == operation) { "nemessix_quiescence_operation_mismatch" }
        val leaseId = response.getString("lease_id").orEmpty()
        check(leaseId.matches(HEX_32_BYTES)) { "nemessix_quiescence_invalid_lease" }
        return NemessixQuiescenceLease(
            leaseId,
            challenge,
            operation,
            response.getString("emulator_build").orEmpty(),
        )
    }

    fun status(operationId: String, challengeHex: String): NemessixQuiescenceStatus {
        val response = call("status", Bundle().apply {
            putString("operation_id", operationId)
            putString("challenge_hex", challengeHex)
        })
        if (response.getString("state") == "DENIED" && response.getString("reason") == "UNKNOWN_OPERATION") {
            return NemessixQuiescenceStatus.Unknown
        }
        check(response.getInt("protocol") == 1) { "nemessix_quiescence_protocol_mismatch" }
        val lease = NemessixQuiescenceLease(
            response.getString("lease_id").orEmpty(), challengeHex, operationId,
            response.getString("emulator_build").orEmpty(),
        )
        check(lease.leaseId.matches(HEX_32_BYTES)) { "nemessix_quiescence_invalid_lease" }
        check(response.getString("challenge_hex") == challengeHex) { "nemessix_quiescence_challenge_mismatch" }
        check(response.getString("operation_id") == operationId) { "nemessix_quiescence_operation_mismatch" }
        return when (response.getString("state")) {
            "QUIESCENT" -> NemessixQuiescenceStatus.Active(lease)
            "RELEASED" -> {
                val outcome = response.getString("outcome").orEmpty()
                val hash = response.getString("final_manifest_hash").orEmpty()
                check(outcome in setOf("COMMITTED", "ROLLED_BACK", "ABORTED"))
                check(hash.matches(HEX_32_BYTES))
                NemessixQuiescenceStatus.Released(lease, outcome, hash)
            }
            else -> error("nemessix_quiescence_status_invalid")
        }
    }

    fun validate(lease: NemessixQuiescenceLease) {
        val response = call("validate", Bundle().apply {
            putString("lease_id", lease.leaseId)
            putString("challenge_hex", lease.challengeHex)
        })
        check(response.getInt("protocol") == 1 && response.getString("state") == "QUIESCENT") {
            "nemessix_quiescence_lease_lost"
        }
        check(response.getString("lease_id") == lease.leaseId) { "nemessix_quiescence_lease_mismatch" }
        check(response.getString("challenge_hex") == lease.challengeHex) {
            "nemessix_quiescence_challenge_mismatch"
        }
        check(response.getString("operation_id") == lease.operationId) {
            "nemessix_quiescence_operation_mismatch"
        }
    }

    fun release(lease: NemessixQuiescenceLease, outcome: String, finalManifestHash: String) {
        check(outcome in setOf("COMMITTED", "ROLLED_BACK", "ABORTED"))
        check(finalManifestHash.matches(HEX_32_BYTES))
        val response = call("release", Bundle().apply {
            putString("lease_id", lease.leaseId)
            putString("outcome", outcome)
            putString("final_manifest_hash", finalManifestHash)
        })
        check(response.getInt("protocol") == 1 && response.getString("state") == "RELEASED") {
            "nemessix_quiescence_release_failed"
        }
        val receipt = status(lease.operationId, lease.challengeHex)
        check(
            receipt is NemessixQuiescenceStatus.Released &&
                receipt.lease.leaseId == lease.leaseId &&
                receipt.outcome == outcome && receipt.finalManifestHash == finalManifestHash,
        ) { "nemessix_quiescence_release_receipt_mismatch" }
    }

    private fun call(method: String, extras: Bundle): Bundle {
        verifyInstalledNemessixCertificate()
        try {
            return requireNotNull(context.contentResolver.call(AUTHORITY_URI, method, null, extras)) {
                "nemessix_quiescence_unavailable"
            }
        } catch (_: IllegalArgumentException) {
            context.startActivity(Intent().apply {
                component = ComponentName(
                    SyncScheduler.NEMESSIX_PACKAGE,
                    "org.citra.citra_emu.savesync.SaveQuiescenceWakeActivity",
                )
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_ANIMATION)
            })
            repeat(10) {
                Thread.sleep(100)
                runCatching {
                    context.contentResolver.call(AUTHORITY_URI, method, null, extras)
                }.getOrNull()?.let { return it }
            }
            error("nemessix_quiescence_unavailable")
        }
    }

    @Suppress("DEPRECATION")
    private fun verifyInstalledNemessixCertificate() {
        val provider = context.packageManager.resolveContentProvider(AUTHORITY_URI.authority.orEmpty(), 0)
        check(provider?.packageName == SyncScheduler.NEMESSIX_PACKAGE) {
            "nemessix_quiescence_untrusted_provider"
        }
        val info = context.packageManager.getPackageInfo(
            SyncScheduler.NEMESSIX_PACKAGE,
            PackageManager.GET_SIGNING_CERTIFICATES,
        )
        val matches = info.signingInfo?.apkContentsSigners.orEmpty().any { certificate ->
            MessageDigest.getInstance("SHA-256").digest(certificate.toByteArray()).toHex() ==
                NEMESSIX_RELEASE_CERT_SHA256
        }
        check(matches) { "nemessix_quiescence_untrusted_emulator" }
    }

    private fun ByteArray.toHex(): String = joinToString("") { "%02x".format(it) }

    companion object {
        private val AUTHORITY_URI = Uri.parse(
            "content://io.github.vincentadamnemessisx.nemessix.save_quiescence_v1",
        )
        private const val NEMESSIX_RELEASE_CERT_SHA256 =
            "69fcde2693e7175f5978e89a0674e0216037ba4fb6b14706edb890cd3fddcbfe"
        private val HEX_32_BYTES = Regex("[0-9a-f]{64}")
    }
}
