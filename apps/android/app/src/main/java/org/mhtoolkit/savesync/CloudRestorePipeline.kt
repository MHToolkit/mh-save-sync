package org.mhtoolkit.savesync

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.Settings
import java.io.File
import java.security.MessageDigest
import java.security.SecureRandom
import java.util.Locale
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject

data class CloudRestoreResult(
    val snapshotId: String,
    val fileCount: Long,
    val totalBytes: Long,
    val consistencyEstablished: Boolean = false,
)

class CloudDownloadPipeline(private val context: Context) {
    suspend fun execute(server: String): CloudRestoreResult = withContext(Dispatchers.IO) {
        val head = SyncServerProbe.fetchHeadForReplace(context, server) ?: error("cloud_version_missing")
        val cache = File(context.noBackupFilesDir, "cloud-cas").apply { mkdirs() }
        val destination = File(cache, "$head.mhsavebundle")
        val incoming = File(cache, ".${UUID.randomUUID()}.mhsavebundle")
        var secret: ByteArray? = null
        try {
            secret = AndroidSecretVault(context).load()
            val response = JSONObject(NativeSyncBridge.downloadCloudSnapshotToCache(
                SyncServerProbe.normalizeServer(server), secret,
                SyncServerProbe.MH3G_NEMESSIX_LOGICAL_SAVE_ID, head, androidDeviceId(context),
                incoming.absolutePath,
            ))
            check(!response.has("error") && incoming.isFile) { "cloud_cache_failed" }
            java.nio.file.Files.move(
                incoming.toPath(), destination.toPath(), java.nio.file.StandardCopyOption.REPLACE_EXISTING,
                java.nio.file.StandardCopyOption.ATOMIC_MOVE,
            )
            cache.listFiles().orEmpty().sortedByDescending { it.lastModified() }.drop(20).forEach { it.delete() }
            CloudRestoreResult(response.getString("snapshot_id"), response.getLong("file_count"), response.getLong("total_bytes"))
        } finally { secret?.fill(0); incoming.delete() }
    }
}

private fun androidDeviceId(context: Context): String {
    val raw = Settings.Secure.getString(context.contentResolver, Settings.Secure.ANDROID_ID).orEmpty().toByteArray()
    return try { "android-" + MessageDigest.getInstance("SHA-256").digest(raw).take(8).joinToString("") { "%02x".format(it) } }
    finally { raw.fill(0) }
}

data class RestoreStopEvidence(
    val confirmedAtMillis: Long,
    val sessionWasInactive: Boolean,
) {
    companion object {
        fun confirmed(sessionActive: Boolean) =
            RestoreStopEvidence(System.currentTimeMillis(), !sessionActive)
    }
}

internal object RestoreStopGate {
    // Nemessix may later emit this integration event; explicit confirmation and
    // the toolkit session boundary remain mandatory even when it is available.
    const val NEMESSIX_STOP_EVENT_ACTION = "io.github.vincentadamnemessisx.nemessix.SAVE_SESSION_STOPPED"
    fun requireFreshConfirmation(evidence: RestoreStopEvidence, now: Long = System.currentTimeMillis()) {
        check(evidence.sessionWasInactive) { "restore_stop_not_confirmed" }
        check(now >= evidence.confirmedAtMillis && now - evidence.confirmedAtMillis <= 120_000) { "restore_stop_evidence_expired" }
    }
}

internal object RestorePathPolicy {
    fun listFiles(root: File): List<Pair<String, File>> {
        val canonicalRoot = root.canonicalFile
        val files = root.walkTopDown().filter { it != root }.map { file ->
            check(!java.nio.file.Files.isSymbolicLink(file.toPath())) { "恢复暂存区包含符号链接" }
            val relative = file.relativeTo(root).invariantSeparatorsPath
            relative.split('/').forEach(SafCapturePolicy::validateName)
            check(file.canonicalPath.startsWith(canonicalRoot.path + File.separator)) { "恢复路径越界" }
            relative to file
        }.toList()
        validatePaths(files.map { it.first })
        return files.sortedWith(compareBy<Pair<String, File>>({ it.first.count { c -> c == '/' } }, { it.first }))
    }

    fun validatePaths(paths: List<String>) {
        val seen = mutableSetOf<String>()
        val folded = mutableSetOf<String>()
        paths.forEach { path ->
            check(seen.add(path)) { "恢复清单包含重复路径" }
            check(folded.add(path.lowercase(Locale.ROOT))) { "恢复清单包含大小写碰撞" }
        }
    }
}

internal enum class RestoreOperationState { PREPARED, MUTATING, ROLLBACK_REQUIRED, COMMITTED, ROLLED_BACK }

internal object RestoreTerminalPolicy {
    fun requireReceiptMatches(
        state: RestoreOperationState?,
        desiredSnapshotId: String?,
        backupSnapshotId: String?,
        challengeHex: String,
        outcome: String,
        finalManifestHash: String,
    ) {
        val matches = when (outcome) {
            "COMMITTED" -> state == RestoreOperationState.COMMITTED &&
                desiredSnapshotId == finalManifestHash && backupSnapshotId != null
            "ROLLED_BACK" -> state == RestoreOperationState.ROLLED_BACK &&
                backupSnapshotId == finalManifestHash
            "ABORTED" -> state in setOf(null, RestoreOperationState.PREPARED) &&
                finalManifestHash == challengeHex
            else -> false
        }
        check(matches) { "restore_terminal_receipt_mismatch" }
    }
}

internal data class DurableRestoreLease(
    val lease: NemessixQuiescenceLease,
    val treeFingerprint: String,
    val desiredSnapshotId: String? = null,
    val backupSnapshotId: String? = null,
)

internal object RestoreLeaseStore {
    private const val FILE_NAME = "lease.json"

    fun treeFingerprint(treeUri: Uri): String {
        val binding = "${treeUri.authority.orEmpty()}\n${DocumentsContract.getTreeDocumentId(treeUri)}"
        return MessageDigest.getInstance("SHA-256").digest(binding.toByteArray())
            .joinToString("") { "%02x".format(it) }
    }

    fun write(operation: File, value: DurableRestoreLease) {
        operation.mkdirs()
        val destination = File(operation, FILE_NAME)
        val next = File(operation, "$FILE_NAME.next")
        val json = JSONObject()
            .put("lease_id", value.lease.leaseId)
            .put("challenge_hex", value.lease.challengeHex)
            .put("operation_id", value.lease.operationId)
            .put("emulator_build", value.lease.emulatorBuild)
            .put("tree_fingerprint", value.treeFingerprint)
            .put("desired_snapshot_id", value.desiredSnapshotId ?: JSONObject.NULL)
            .put("backup_snapshot_id", value.backupSnapshotId ?: JSONObject.NULL)
            .toString()
        next.outputStream().use { output ->
            output.write(json.toByteArray())
            output.flush()
            output.fd.sync()
        }
        try {
            java.nio.file.Files.move(
                next.toPath(), destination.toPath(),
                java.nio.file.StandardCopyOption.ATOMIC_MOVE,
                java.nio.file.StandardCopyOption.REPLACE_EXISTING,
            )
        } catch (_: java.nio.file.AtomicMoveNotSupportedException) {
            java.nio.file.Files.move(next.toPath(), destination.toPath(), java.nio.file.StandardCopyOption.REPLACE_EXISTING)
        }
    }

    fun read(operation: File): DurableRestoreLease {
        val json = JSONObject(File(operation, FILE_NAME).readText())
        val lease = NemessixQuiescenceLease(
            json.getString("lease_id"),
            json.getString("challenge_hex"),
            json.getString("operation_id"),
            json.getString("emulator_build"),
        )
        check(lease.leaseId.isEmpty() || lease.leaseId.matches(Regex("[0-9a-f]{64}"))) { "restore_lease_invalid" }
        check(lease.challengeHex.matches(Regex("[0-9a-f]{64}"))) { "restore_challenge_invalid" }
        return DurableRestoreLease(
            lease,
            json.getString("tree_fingerprint"),
            json.optString("desired_snapshot_id").takeIf { it.matches(Regex("[0-9a-f]{64}")) },
            json.optString("backup_snapshot_id").takeIf { it.matches(Regex("[0-9a-f]{64}")) },
        )
    }

    fun exists(operation: File): Boolean = File(operation, FILE_NAME).isFile
}

internal class DurableRestoreState(private val directory: File) {
    private val file = File(directory, "state")
    fun read(): RestoreOperationState? = file.takeIf { it.isFile }?.readText()?.trim()?.let(RestoreOperationState::valueOf)
    fun write(state: RestoreOperationState) {
        directory.mkdirs()
        val next = File(directory, "state.next")
        next.outputStream().use { output -> output.write(state.name.toByteArray()); output.flush(); output.fd.sync() }
        try {
            java.nio.file.Files.move(
                next.toPath(), file.toPath(),
                java.nio.file.StandardCopyOption.ATOMIC_MOVE,
                java.nio.file.StandardCopyOption.REPLACE_EXISTING,
            )
        } catch (_: java.nio.file.AtomicMoveNotSupportedException) {
            java.nio.file.Files.move(next.toPath(), file.toPath(), java.nio.file.StandardCopyOption.REPLACE_EXISTING)
        }
    }
}

internal fun interface RestoreTreeBackend { fun replaceFrom(source: File) }

internal class RestoreTransaction(
    private val state: DurableRestoreState,
    private val backend: RestoreTreeBackend,
) {
    fun commit(desired: File, backup: File) {
        state.write(RestoreOperationState.PREPARED)
        state.write(RestoreOperationState.MUTATING)
        try {
            backend.replaceFrom(desired)
            state.write(RestoreOperationState.COMMITTED)
        } catch (failure: Throwable) {
            state.write(RestoreOperationState.ROLLBACK_REQUIRED)
            try {
                backend.replaceFrom(backup)
                state.write(RestoreOperationState.ROLLED_BACK)
            } catch (rollbackFailure: Throwable) {
                failure.addSuppressed(rollbackFailure)
                throw failure
            }
            throw IllegalStateException("restore_rolled_back", failure)
        }
    }

    fun recover(backup: File) {
        if (state.read() in setOf(RestoreOperationState.MUTATING, RestoreOperationState.ROLLBACK_REQUIRED)) {
            state.write(RestoreOperationState.ROLLBACK_REQUIRED)
            backend.replaceFrom(backup)
            state.write(RestoreOperationState.ROLLED_BACK)
        }
    }
}

class SafJournalRestorer(private val context: Context, private val treeUri: Uri, private val journal: File) : RestoreTreeBackend {
    override fun replaceFrom(source: File) {
        RestorePathPolicy.listFiles(source)
        journal.parentFile?.mkdirs()
        replaceTree(treeUri, source, journal)
    }

    private fun replaceTree(treeUri: Uri, source: File, journal: File) {
        val resolver = context.contentResolver
        val rootId = DocumentsContract.getTreeDocumentId(treeUri)
        val rootUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, rootId)
        queryChildren(treeUri, rootId).forEach { (id, uri, mime) ->
            deleteRecursively(treeUri, id, uri, mime, journal)
        }
        val dirs = mutableMapOf("" to rootUri)
        for ((relative, file) in RestorePathPolicy.listFiles(source)) {
            val parentPath = relative.substringBeforeLast('/', "")
            if (file.isDirectory) {
                val parent = dirs[parentPath] ?: error("恢复目录顺序无效")
                dirs[relative] = DocumentsContract.createDocument(
                    resolver, parent, DocumentsContract.Document.MIME_TYPE_DIR, file.name,
                ) ?: error("无法创建恢复目录")
            } else {
                val parent = dirs[parentPath] ?: error("恢复父目录缺失")
                val target = DocumentsContract.createDocument(resolver, parent, "application/octet-stream", file.name)
                    ?: error("无法创建恢复文件")
                resolver.openOutputStream(target, "w").use { output ->
                    requireNotNull(output) { "无法写入恢复文件" }
                    file.inputStream().use { it.copyTo(output) }
                    output.flush()
                }
                journal.appendText("write:${MessageDigest.getInstance("SHA-256").digest(relative.toByteArray()).take(6).joinToString("") { "%02x".format(it) }}\n")
            }
        }
    }

    private fun deleteRecursively(tree: Uri, id: String, uri: Uri, mime: String, journal: File) {
        if (mime == DocumentsContract.Document.MIME_TYPE_DIR) {
            queryChildren(tree, id).forEach { (childId, childUri, childMime) ->
                deleteRecursively(tree, childId, childUri, childMime, journal)
            }
        }
        check(DocumentsContract.deleteDocument(context.contentResolver, uri)) { "无法清理原存档" }
        journal.appendText("delete:${MessageDigest.getInstance("SHA-256").digest(id.toByteArray()).take(6).joinToString("") { "%02x".format(it) }}\n")
    }

    private fun queryChildren(tree: Uri, parentId: String): List<Triple<String, Uri, String>> {
        val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, parentId)
        return context.contentResolver.query(children, arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
        ), null, null, null)?.use { cursor ->
            buildList {
                while (cursor.moveToNext()) {
                    val id = cursor.getString(0)
                    add(Triple(id, DocumentsContract.buildDocumentUriUsingTree(tree, id), cursor.getString(1)))
                }
            }
        } ?: error("无法枚举 SAF 存档目录")
    }
}

internal object RestoreRecovery {
    fun cleanupNonMutating(context: Context, root: File) {
        root.listFiles().orEmpty().forEach { operation ->
            if (RestoreLeaseStore.exists(operation)) return@forEach
            when (DurableRestoreState(operation).read()) {
                null, RestoreOperationState.PREPARED, RestoreOperationState.ROLLED_BACK ->
                    if (!RestoreLeaseStore.exists(operation)) operation.deleteRecursively()
                RestoreOperationState.COMMITTED -> {
                    val bundle = File(operation, "before.mhsavebundle")
                    if (bundle.isFile) {
                        val id = bundle.inputStream().use { input ->
                            val digest = MessageDigest.getInstance("SHA-256")
                            val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
                            while (true) {
                                val read = input.read(buffer)
                                if (read < 0) break
                                digest.update(buffer, 0, read)
                            }
                            digest.digest().joinToString("") { "%02x".format(it) }
                        }
                        retainEncryptedBackup(context, operation, bundle, id)
                    }
                }
                else -> Unit
            }
        }
        cleanupRetention(context)
    }

    fun pending(root: File): List<File> = root.listFiles().orEmpty().filter(RestoreLeaseStore::exists)

    fun pendingCount(root: File): Int = pending(root).size

    fun refreshPendingCount(context: Context, root: File) {
        context.getSharedPreferences(SyncScheduler.PREFERENCES, Context.MODE_PRIVATE).edit()
            .putInt(SyncScheduler.PENDING_RESTORE_RECOVERY_COUNT, pendingCount(root))
            .apply()
    }

    fun recoverPending(
        context: Context,
        root: File,
        treeUri: Uri,
        quiescence: NemessixQuiescenceClient,
        backend: (File) -> RestoreTreeBackend,
    ) {
        for (operation in pending(root)) {
            var durableLease = RestoreLeaseStore.read(operation)
            val status = quiescence.status(durableLease.lease.operationId, durableLease.lease.challengeHex)
            if (status is NemessixQuiescenceStatus.Released) {
                RestoreTerminalPolicy.requireReceiptMatches(
                    DurableRestoreState(operation).read(),
                    durableLease.desiredSnapshotId,
                    durableLease.backupSnapshotId,
                    durableLease.lease.challengeHex,
                    status.outcome,
                    status.finalManifestHash,
                )
                if (status.outcome == "COMMITTED") {
                    val encryptedBackup = File(operation, "before.mhsavebundle")
                    retainEncryptedBackup(
                        context, operation, encryptedBackup,
                        requireNotNull(durableLease.backupSnapshotId),
                        deleteOperation = false,
                    )
                }
                operation.deleteRecursively()
                continue
            }
            val activeLease = when (status) {
                is NemessixQuiescenceStatus.Active -> status.lease
                NemessixQuiescenceStatus.Unknown -> quiescence.acquire(
                    durableLease.lease.operationId,
                    durableLease.lease.challengeHex,
                )
                is NemessixQuiescenceStatus.Released -> error("unreachable")
            }
            durableLease = durableLease.copy(lease = activeLease)
            RestoreLeaseStore.write(operation, durableLease)
            quiescence.validate(activeLease)
            val state = DurableRestoreState(operation).read()
            when (state) {
                RestoreOperationState.MUTATING, RestoreOperationState.ROLLBACK_REQUIRED -> {
                    check(durableLease.treeFingerprint == RestoreLeaseStore.treeFingerprint(treeUri)) {
                        "restore_tree_binding_mismatch"
                    }
                    val backup = File(operation, "before")
                    check(backup.isDirectory) { "restore_backup_missing" }
                    RestoreTransaction(DurableRestoreState(operation), backend(operation)).recover(backup)
                    check(DurableRestoreState(operation).read() == RestoreOperationState.ROLLED_BACK)
                    quiescence.release(
                        activeLease,
                        "ROLLED_BACK",
                        requireNotNull(durableLease.backupSnapshotId) { "restore_rollback_manifest_missing" },
                    )
                }
                RestoreOperationState.COMMITTED -> {
                    val encryptedBackup = File(operation, "before.mhsavebundle")
                    retainEncryptedBackup(
                        context, operation, encryptedBackup,
                        requireNotNull(durableLease.backupSnapshotId) { "restore_backup_manifest_missing" },
                        deleteOperation = false,
                    )
                    quiescence.release(
                        activeLease,
                        "COMMITTED",
                        requireNotNull(durableLease.desiredSnapshotId) { "restore_committed_manifest_missing" },
                    )
                }
                RestoreOperationState.ROLLED_BACK ->
                    quiescence.release(
                        activeLease, "ROLLED_BACK",
                        requireNotNull(durableLease.backupSnapshotId) { "restore_rollback_manifest_missing" },
                    )
                null, RestoreOperationState.PREPARED ->
                    quiescence.release(activeLease, "ABORTED", activeLease.challengeHex)
            }
            operation.deleteRecursively()
        }
    }

    fun retainEncryptedBackup(
        context: Context,
        operation: File,
        encryptedBackup: File,
        snapshotId: String,
        deleteOperation: Boolean = true,
    ) {
        val cas = File(context.noBackupFilesDir, "restore-cas").apply { mkdirs() }
        val target = File(cas, "$snapshotId.mhsavebundle")
        if (encryptedBackup.isFile) {
            if (target.isFile) {
                check(fileDigest(target).contentEquals(fileDigest(encryptedBackup))) {
                    "restore_backup_cas_collision"
                }
                encryptedBackup.delete()
            } else {
                check(encryptedBackup.renameTo(target)) { "restore_backup_cas_commit_failed" }
            }
        } else {
            check(target.isFile && target.length() > 0L) { "restore_backup_cas_missing" }
        }
        java.io.FileOutputStream(target, true).use { it.fd.sync() }
        var secret: ByteArray? = null
        try {
            secret = AndroidSecretVault(context).load()
            val verified = JSONObject(NativeSyncBridge.verifyEncryptedBundle(
                target.absolutePath,
                secret,
                snapshotId,
            ))
            check(!verified.has("error") && verified.optString("snapshot_id") == snapshotId) {
                "restore_backup_cas_verify_failed"
            }
        } finally {
            secret?.fill(0)
        }
        if (deleteOperation) operation.deleteRecursively()
        cleanupRetention(context)
    }

    private fun fileDigest(file: File): ByteArray = file.inputStream().use { input ->
        val digest = MessageDigest.getInstance("SHA-256")
        val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
        while (true) {
            val read = input.read(buffer)
            if (read < 0) break
            digest.update(buffer, 0, read)
        }
        digest.digest()
    }

    private fun cleanupRetention(context: Context) {
        File(context.noBackupFilesDir, "restore-cas").listFiles().orEmpty()
            .sortedByDescending { it.lastModified() }.drop(20).forEach { it.delete() }
    }
}

class CloudRestorePipeline(private val context: Context) {
    suspend fun execute(
        server: String,
        treeUri: Uri,
        sessionActive: Boolean,
        stopEvidence: RestoreStopEvidence,
    ): CloudRestoreResult {
        val restoreRoot = File(context.noBackupFilesDir, "restore").apply { mkdirs() }
        return try {
            executeWithRoot(server, treeUri, sessionActive, stopEvidence, restoreRoot)
        } finally {
            RestoreRecovery.refreshPendingCount(context, restoreRoot)
        }
    }

    private suspend fun executeWithRoot(
        server: String,
        treeUri: Uri,
        sessionActive: Boolean,
        stopEvidence: RestoreStopEvidence,
        restoreRoot: File,
    ): CloudRestoreResult =
        withContext(Dispatchers.IO) {
            LocalReplacePolicy.requireSessionStopped(sessionActive)
            RestoreStopGate.requireFreshConfirmation(stopEvidence)
            val quiescence = NemessixQuiescenceClient(context)
            RestoreRecovery.recoverPending(context, restoreRoot, treeUri, quiescence) { op ->
                SafJournalRestorer(context, treeUri, File(op, "actions.log"))
            }
            val operationId = UUID.randomUUID().toString()
            val challenge = ByteArray(32).also(SecureRandom()::nextBytes)
                .joinToString("") { "%02x".format(it) }
            val operation = File(restoreRoot, operationId).apply { mkdirs() }
            val treeFingerprint = RestoreLeaseStore.treeFingerprint(treeUri)
            RestoreLeaseStore.write(
                operation,
                DurableRestoreLease(
                    NemessixQuiescenceLease("", challenge, operationId, ""),
                    treeFingerprint,
                ),
            )
            val lease = quiescence.acquire(operationId, challenge)
            var durableLease = DurableRestoreLease(lease, treeFingerprint)
            RestoreLeaseStore.write(operation, durableLease)
            val desired = File(operation, "incoming")
            val encryptedBackup = File(operation, "before.mhsavebundle")
            var secret: ByteArray? = null
            var leaseReleased = false
            try {
                val head = SyncServerProbe.fetchHeadForReplace(context, server) ?: error("云端没有可恢复版本")
                val backupCapture = SafStableStager(context).capture(treeUri)
                val backup = File(operation, "before").also {
                    if (!backupCapture.root.renameTo(it)) {
                        check(backupCapture.root.copyRecursively(it, overwrite = false)) { "无法持久保存恢复前备份" }
                        backupCapture.root.deleteRecursively()
                    }
                }
                secret = AndroidSecretVault(context).load()
                val backupResult = JSONObject(NativeSyncBridge.encryptStageBackup(
                    backup.absolutePath, secret, encryptedBackup.absolutePath,
                ))
                check(!backupResult.has("error") && encryptedBackup.isFile) { "restore_backup_encrypt_failed" }
                val response = JSONObject(NativeSyncBridge.downloadCloudSnapshotToStage(
                    SyncServerProbe.normalizeServer(server), secret,
                    SyncServerProbe.MH3G_NEMESSIX_LOGICAL_SAVE_ID, head, androidDeviceId(context), desired.absolutePath,
                ))
                check(!response.has("error")) { "云端版本下载或校验失败" }
                val backupSnapshotId = backupResult.getString("snapshot_id")
                val desiredSnapshotId = response.getString("snapshot_id")
                check(backupSnapshotId.matches(Regex("[0-9a-f]{64}"))) { "restore_backup_snapshot_invalid" }
                check(desiredSnapshotId.matches(Regex("[0-9a-f]{64}"))) { "restore_cloud_snapshot_invalid" }
                durableLease = durableLease.copy(
                    desiredSnapshotId = desiredSnapshotId,
                    backupSnapshotId = backupSnapshotId,
                )
                RestoreLeaseStore.write(operation, durableLease)
                val confirmedHead = SyncServerProbe.fetchHeadForReplace(context, server)
                check(confirmedHead == head) { "restore_cloud_version_changed" }
                RestoreStopGate.requireFreshConfirmation(stopEvidence)
                // Starting the signed provider keeps the app process alive;
                // the quiescence lease is the authoritative restore gate.
                quiescence.validate(lease)
                val transaction = RestoreTransaction(
                    DurableRestoreState(operation),
                    SafJournalRestorer(context, treeUri, File(operation, "actions.log")),
                )
                transaction.commit(desired, backup)
                RestoreRecovery.retainEncryptedBackup(
                    context, operation, encryptedBackup, backupSnapshotId,
                    deleteOperation = false,
                )
                // Establish equality only from a fresh stable capture of the SAF tree after
                // journal commit while the signed quiescence lease still excludes emulator
                // writes. Never trust a post-release capture or the downloaded manifest.
                val consistencyEstablished = RestoreConsistencyCoordinator.complete(
                    captureAndEstablish = {
                        val committedStage = SafStableStager(context).capture(treeUri)
                        try {
                            SyncConsistencyLedgerStore(context).establish(
                                binding = SyncConsistencyBinding(
                                    serverEndpoint = SyncServerProbe.normalizeServer(server),
                                    logicalSaveId = SyncServerProbe.MH3G_NEMESSIX_LOGICAL_SAVE_ID,
                                    treeUri = treeUri.toString(),
                                    deviceId = androidDeviceId(context),
                                ),
                                remoteHead = head,
                                localFingerprint = committedStage.fingerprint,
                                mode = SyncEstablishmentMode.RESTORE,
                            )
                        } finally {
                            committedStage.root.deleteRecursively()
                        }
                    },
                    releaseLease = {
                        quiescence.release(lease, "COMMITTED", desiredSnapshotId)
                        leaseReleased = true
                    },
                )
                operation.deleteRecursively()
                CloudRestoreResult(
                    desiredSnapshotId,
                    response.getLong("file_count"),
                    response.getLong("total_bytes"),
                    consistencyEstablished,
                )
            } finally {
                secret?.fill(0)
                desired.deleteRecursively()
                val state = DurableRestoreState(operation).read()
                val persistedTerminal = runCatching { RestoreLeaseStore.read(operation) }.getOrNull()
                when {
                    leaseReleased -> Unit
                    state == RestoreOperationState.COMMITTED ->
                        quiescence.release(
                            lease, "COMMITTED",
                            requireNotNull(persistedTerminal?.desiredSnapshotId) {
                                "restore_committed_manifest_missing"
                            },
                        )
                    state == RestoreOperationState.ROLLED_BACK ->
                        quiescence.release(
                            lease, "ROLLED_BACK",
                            requireNotNull(persistedTerminal?.backupSnapshotId) {
                                "restore_rollback_manifest_missing"
                            },
                        )
                    state == null || state == RestoreOperationState.PREPARED -> {
                        quiescence.release(lease, "ABORTED", lease.challengeHex)
                        operation.deleteRecursively()
                    }
                    state == RestoreOperationState.MUTATING || state == RestoreOperationState.ROLLBACK_REQUIRED ->
                        Unit // fail closed: retain both the durable lease and plaintext rollback source
                    else -> Unit
                }
            }
        }

}

class LocalBackupRestorePipeline(private val context: Context) {
    suspend fun execute(treeUri: Uri, snapshotId: String): CloudRestoreResult {
        val restoreRoot = File(context.noBackupFilesDir, "restore").apply { mkdirs() }
        return try {
            executeWithRoot(treeUri, snapshotId, restoreRoot)
        } finally {
            RestoreRecovery.refreshPendingCount(context, restoreRoot)
        }
    }

    private suspend fun executeWithRoot(
        treeUri: Uri,
        snapshotId: String,
        restoreRoot: File,
    ): CloudRestoreResult = withContext(Dispatchers.IO) {
        require(snapshotId.matches(Regex("[0-9a-f]{64}"))) { "local_backup_id_invalid" }
        val sourceBundle = File(context.noBackupFilesDir, "restore-cas/$snapshotId.mhsavebundle")
        require(sourceBundle.isFile) { "local_backup_missing" }
        val quiescence = NemessixQuiescenceClient(context)
        RestoreRecovery.recoverPending(context, restoreRoot, treeUri, quiescence) { op ->
            SafJournalRestorer(context, treeUri, File(op, "actions.log"))
        }
        val operationId = UUID.randomUUID().toString()
        val challenge = ByteArray(32).also(SecureRandom()::nextBytes)
            .joinToString("") { "%02x".format(it) }
        val operation = File(restoreRoot, operationId).apply { mkdirs() }
        val fingerprint = RestoreLeaseStore.treeFingerprint(treeUri)
        RestoreLeaseStore.write(
            operation,
            DurableRestoreLease(NemessixQuiescenceLease("", challenge, operationId, ""), fingerprint),
        )
        val lease = quiescence.acquire(operationId, challenge)
        var durable = DurableRestoreLease(lease, fingerprint)
        val current = File(operation, "before")
        val desired = File(operation, "incoming")
        val encryptedCurrent = File(operation, "before.mhsavebundle")
        var secret: ByteArray? = null
        var terminalReleased = false
        try {
            RestoreLeaseStore.write(operation, durable)
            val currentCapture = SafStableStager(context).capture(treeUri)
            check(currentCapture.root.renameTo(current) || currentCapture.root.copyRecursively(current, false)) {
                "local_history_current_backup_failed"
            }
            if (currentCapture.root.exists()) currentCapture.root.deleteRecursively()
            secret = AndroidSecretVault(context).load()
            val currentResult = JSONObject(NativeSyncBridge.encryptStageBackup(
                current.absolutePath, secret, encryptedCurrent.absolutePath,
            ))
            val desiredResult = JSONObject(NativeSyncBridge.restoreEncryptedBundleToStage(
                sourceBundle.absolutePath, secret, desired.absolutePath,
            ))
            check(!currentResult.has("error") && !desiredResult.has("error")) { "local_history_stage_failed" }
            val backupId = currentResult.getString("snapshot_id")
            val desiredId = desiredResult.getString("snapshot_id")
            check(backupId.matches(Regex("[0-9a-f]{64}"))) { "local_history_backup_id_invalid" }
            check(desiredId.matches(Regex("[0-9a-f]{64}"))) { "local_history_desired_id_invalid" }
            check(desiredId == snapshotId) { "local_history_bundle_id_mismatch" }
            durable = durable.copy(desiredSnapshotId = desiredId, backupSnapshotId = backupId)
            RestoreLeaseStore.write(operation, durable)
            quiescence.validate(lease)
            RestoreTransaction(
                DurableRestoreState(operation),
                SafJournalRestorer(context, treeUri, File(operation, "actions.log")),
            ).commit(desired, current)
            RestoreRecovery.retainEncryptedBackup(
                context, operation, encryptedCurrent, backupId, deleteOperation = false,
            )
            quiescence.release(lease, "COMMITTED", desiredId)
            terminalReleased = true
            operation.deleteRecursively()
            CloudRestoreResult(
                desiredId,
                desiredResult.getLong("file_count"),
                desiredResult.getLong("total_bytes"),
            )
        } finally {
            secret?.fill(0)
            desired.deleteRecursively()
            if (!terminalReleased) {
                val state = DurableRestoreState(operation).read()
                if (state == RestoreOperationState.ROLLED_BACK) {
                    val persisted = RestoreLeaseStore.read(operation)
                    quiescence.release(lease, "ROLLED_BACK", requireNotNull(persisted.backupSnapshotId))
                } else if (state == null || state == RestoreOperationState.PREPARED) {
                    quiescence.release(lease, "ABORTED", challenge)
                    operation.deleteRecursively()
                }
            }
        }
    }
}
