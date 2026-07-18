package org.mhtoolkit.savesync

object NativeSyncBridge {
    init { System.loadLibrary("save_client") }

    external fun bridgeHealth(): String

    external fun uploadStableStage(
        stagingRoot: String,
        serverEndpoint: String,
        recoverySecret: ByteArray,
        logicalSaveId: String,
        baseHead: String?,
        deviceId: String,
    ): String

    external fun queueStableStage(
        stagingRoot: String,
        queueRoot: String,
        serverEndpoint: String,
        recoverySecret: ByteArray,
        logicalSaveId: String,
        baseHead: String?,
        deviceId: String,
    ): String

    external fun drainUploadQueue(
        queueRoot: String,
        recoverySecret: ByteArray,
    ): String

    external fun fetchCloudHead(
        serverEndpoint: String,
        recoverySecret: ByteArray,
        logicalSaveId: String,
        deviceId: String,
    ): String

    external fun fetchUnresolvedConflicts(
        serverEndpoint: String,
        recoverySecret: ByteArray,
        logicalSaveId: String,
        deviceId: String,
    ): String

    external fun resolveConflicts(
        serverEndpoint: String,
        recoverySecret: ByteArray,
        logicalSaveId: String,
        deviceId: String,
        conflictIdsJson: String,
        chosenSnapshotId: String,
        replaceWithLocal: Boolean,
    ): String

    external fun downloadCloudSnapshotToStage(
        serverEndpoint: String,
        recoverySecret: ByteArray,
        logicalSaveId: String,
        snapshotId: String,
        deviceId: String,
        privateStageTarget: String,
    ): String

    external fun encryptStageBackup(
        privateStageRoot: String,
        recoverySecret: ByteArray,
        destinationBundle: String,
    ): String

    external fun verifyEncryptedBundle(
        bundlePath: String,
        recoverySecret: ByteArray,
        expectedSnapshotId: String,
    ): String

    external fun restoreEncryptedBundleToStage(
        bundlePath: String,
        recoverySecret: ByteArray,
        privateStageTarget: String,
    ): String

    external fun downloadCloudSnapshotToCache(
        serverEndpoint: String,
        recoverySecret: ByteArray,
        logicalSaveId: String,
        snapshotId: String,
        deviceId: String,
        destinationBundle: String,
    ): String
}
