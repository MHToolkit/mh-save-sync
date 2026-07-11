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

    external fun fetchCloudHead(
        serverEndpoint: String,
        recoverySecret: ByteArray,
        logicalSaveId: String,
        deviceId: String,
    ): String
}
