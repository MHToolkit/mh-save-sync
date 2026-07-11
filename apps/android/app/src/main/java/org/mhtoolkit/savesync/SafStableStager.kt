package org.mhtoolkit.savesync

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import java.io.File
import java.security.MessageDigest

data class StableStage(val root: File, val fingerprint: String, val fileCount: Int, val totalBytes: Long)

internal object SafStabilityPolicy {
    fun requireMatching(first: String, second: String) {
        require(first == second) { "存档仍在变化，未创建或上传快照" }
    }
}

internal object SafCapturePolicy {
    const val MAX_DEPTH = 32
    const val MAX_FILES = 10_000
    const val MAX_TOTAL_BYTES = 128L * 1024 * 1024
    const val MAX_FILE_BYTES = 128L * 1024 * 1024

    fun validateName(name: String) {
        require(name.isNotEmpty() && name != "." && name != "..") { "存档路径名称无效" }
        require(!name.contains('/') && !name.contains('\\') && name.none { it.isISOControl() }) {
            "存档路径包含非法字符"
        }
    }

    fun validateDepth(depth: Int) = require(depth <= MAX_DEPTH) { "存档目录递归层级超过安全上限" }
    fun validateFileBytes(bytes: Long) = require(bytes <= MAX_FILE_BYTES) { "单文件超过安全上限" }
    fun validateTotals(files: Int, bytes: Long) =
        require(files <= MAX_FILES && bytes <= MAX_TOTAL_BYTES) { "存档文件数或总大小超过安全上限" }
}

class SafStableStager(private val context: Context) {
    suspend fun capture(treeUri: Uri, debounceMillis: Long = 2000, observationGapMillis: Long = 500): StableStage {
        kotlinx.coroutines.delay(debounceMillis)
        val first = copyOnce(treeUri, "first")
        kotlinx.coroutines.delay(observationGapMillis)
        val second = copyOnce(treeUri, "second")
        try {
            SafStabilityPolicy.requireMatching(first.fingerprint, second.fingerprint)
        } catch (error: IllegalArgumentException) {
            first.root.deleteRecursively(); second.root.deleteRecursively()
            throw IllegalStateException(error.message, error)
        }
        first.root.deleteRecursively()
        return second
    }

    private fun copyOnce(treeUri: Uri, suffix: String): StableStage {
        val stage = File(context.cacheDir, "save-stage-${System.nanoTime()}-$suffix").apply { mkdirs() }
        try {
            val digest = MessageDigest.getInstance("SHA-256")
            var count = 0
            var total = 0L
            val seenPaths = mutableSetOf<String>()
            val seenCaseFoldedPaths = mutableSetOf<String>()
            fun walk(documentId: String, relative: String, depth: Int) {
                SafCapturePolicy.validateDepth(depth)
            val children = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, documentId)
            context.contentResolver.query(children, arrayOf(
                DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                DocumentsContract.Document.COLUMN_MIME_TYPE,
            ), null, null, null)?.use { cursor ->
                val childrenToVisit = mutableListOf<Triple<String, String, String>>()
                while (cursor.moveToNext()) {
                    childrenToVisit += Triple(cursor.getString(0), cursor.getString(1), cursor.getString(2))
                }
                childrenToVisit.sortBy { it.second }
                for ((id, name, mime) in childrenToVisit) {
                    SafCapturePolicy.validateName(name)
                    val rel = if (relative.isEmpty()) name else "$relative/$name"
                    require(seenPaths.add(rel)) { "重复存档路径，拒绝同步" }
                    require(seenCaseFoldedPaths.add(rel.lowercase())) { "存档路径存在大小写碰撞，拒绝同步" }
                    if (mime == DocumentsContract.Document.MIME_TYPE_DIR) {
                        walk(id, rel, depth + 1)
                    } else {
                        val target = File(stage, rel)
                        require(target.canonicalPath.startsWith(stage.canonicalPath + File.separator))
                        target.parentFile?.mkdirs()
                        val uri = DocumentsContract.buildDocumentUriUsingTree(treeUri, id)
                        val fileDigest = MessageDigest.getInstance("SHA-256")
                        var fileBytes = 0L
                        context.contentResolver.openInputStream(uri).use { input ->
                            requireNotNull(input) { "无法只读打开 SAF 文件" }
                            target.outputStream().use { output ->
                                val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
                                while (true) {
                                    val read = input.read(buffer)
                                    if (read < 0) break
                                    fileBytes += read
                                    SafCapturePolicy.validateFileBytes(fileBytes)
                                    SafCapturePolicy.validateTotals(count + 1, total + fileBytes)
                                    fileDigest.update(buffer, 0, read)
                                    output.write(buffer, 0, read)
                                }
                                output.flush()
                            }
                        }
                        digest.update(rel.toByteArray()); digest.update(0)
                        digest.update(fileBytes.toString().toByteArray()); digest.update(0)
                        digest.update(fileDigest.digest())
                        count++; total += fileBytes
                        SafCapturePolicy.validateTotals(count, total)
                    }
                }
            } ?: error("无法枚举 SAF 存档目录")
            }
            walk(DocumentsContract.getTreeDocumentId(treeUri), "", 0)
            require(count > 0) { "存档目录为空，拒绝上传" }
            return StableStage(stage, digest.digest().joinToString("") { "%02x".format(it) }, count, total)
        } catch (error: Throwable) {
            stage.deleteRecursively()
            throw error
        }
    }
}
