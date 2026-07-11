package org.mhtoolkit.savesync

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.Settings
import java.io.File
import java.security.MessageDigest
import java.util.Locale
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject

data class CloudRestoreResult(val snapshotId: String, val fileCount: Long, val totalBytes: Long)

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
    val verifiedIpcStoppedLease: Boolean,
) {
    companion object {
        fun confirmed(sessionActive: Boolean, verifiedIpcStoppedLease: Boolean = false) =
            RestoreStopEvidence(System.currentTimeMillis(), !sessionActive, verifiedIpcStoppedLease)
    }
}

internal object RestoreStopGate {
    // Nemessix may later emit this integration event; explicit confirmation and
    // the toolkit session boundary remain mandatory even when it is available.
    const val NEMESSIX_STOP_EVENT_ACTION = "io.github.vincentadamnemessisx.nemessix.SAVE_SESSION_STOPPED"
    fun requireFreshConfirmation(evidence: RestoreStopEvidence, now: Long = System.currentTimeMillis()) {
        check(evidence.sessionWasInactive) { "restore_stop_not_confirmed" }
        check(now >= evidence.confirmedAtMillis && now - evidence.confirmedAtMillis <= 120_000) { "restore_stop_evidence_expired" }
        check(evidence.verifiedIpcStoppedLease) { "restore_ipc_stopped_lease_unavailable" }
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
        } catch (failure: Exception) {
            state.write(RestoreOperationState.ROLLBACK_REQUIRED)
            backend.replaceFrom(backup)
            state.write(RestoreOperationState.ROLLED_BACK)
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
            when (DurableRestoreState(operation).read()) {
                null, RestoreOperationState.PREPARED, RestoreOperationState.ROLLED_BACK ->
                    operation.deleteRecursively()
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

    fun pending(root: File): List<File> = root.listFiles().orEmpty().filter { operation ->
        DurableRestoreState(operation).read() in setOf(RestoreOperationState.MUTATING, RestoreOperationState.ROLLBACK_REQUIRED)
    }

    fun recoverPending(root: File, backend: (File) -> RestoreTreeBackend) {
        for (operation in pending(root)) {
            val backup = File(operation, "before")
            check(backup.isDirectory) { "restore_backup_missing" }
            RestoreTransaction(DurableRestoreState(operation), backend(operation)).recover(backup)
            operation.deleteRecursively()
        }
    }

    fun retainEncryptedBackup(context: Context, operation: File, encryptedBackup: File, snapshotId: String) {
        val cas = File(context.noBackupFilesDir, "restore-cas").apply { mkdirs() }
        val target = File(cas, "$snapshotId.mhsavebundle")
        check(encryptedBackup.renameTo(target)) { "restore_backup_cas_commit_failed" }
        operation.deleteRecursively()
        cleanupRetention(context)
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
    ): CloudRestoreResult =
        withContext(Dispatchers.IO) {
            LocalReplacePolicy.requireSessionStopped(sessionActive)
            RestoreStopGate.requireFreshConfirmation(stopEvidence)
            // ActivityManager is only supplementary: Android 15 can hide other
            // UID processes, so absence here never grants restore capability.
            NemessixProcessGate(context).requireStopped()
            val restoreRoot = File(context.noBackupFilesDir, "restore").apply { mkdirs() }
            RestoreRecovery.recoverPending(restoreRoot) { op ->
                SafJournalRestorer(context, treeUri, File(op, "actions.log"))
            }
            val head = SyncServerProbe.fetchHeadForReplace(context, server) ?: error("云端没有可恢复版本")
            val backupCapture = SafStableStager(context).capture(treeUri)
            val operation = File(restoreRoot, UUID.randomUUID().toString()).apply { mkdirs() }
            val backup = File(operation, "before").also {
                if (!backupCapture.root.renameTo(it)) {
                    check(backupCapture.root.copyRecursively(it, overwrite = false)) { "无法持久保存恢复前备份" }
                    backupCapture.root.deleteRecursively()
                }
            }
            val desired = File(operation, "incoming")
            val encryptedBackup = File(operation, "before.mhsavebundle")
            var secret: ByteArray? = null
            try {
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
                val confirmedHead = SyncServerProbe.fetchHeadForReplace(context, server)
                check(confirmedHead == head) { "restore_cloud_version_changed" }
                RestoreStopGate.requireFreshConfirmation(stopEvidence)
                NemessixProcessGate(context).requireStopped()
                val transaction = RestoreTransaction(
                    DurableRestoreState(operation),
                    SafJournalRestorer(context, treeUri, File(operation, "actions.log")),
                )
                transaction.commit(desired, backup)
                val backupSnapshotId = backupResult.getString("snapshot_id")
                RestoreRecovery.retainEncryptedBackup(context, operation, encryptedBackup, backupSnapshotId)
                CloudRestoreResult(response.getString("snapshot_id"), response.getLong("file_count"), response.getLong("total_bytes"))
            } finally {
                secret?.fill(0)
                desired.deleteRecursively()
                when (DurableRestoreState(operation).read()) {
                    null, RestoreOperationState.PREPARED, RestoreOperationState.ROLLED_BACK -> operation.deleteRecursively()
                    else -> Unit // crash recovery needs the plaintext backup until rollback completes
                }
            }
        }

}
