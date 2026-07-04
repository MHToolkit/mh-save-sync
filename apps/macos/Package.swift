// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "MHSaveSyncMac",
    platforms: [.macOS(.v15)],
    products: [
        .executable(name: "MHSaveSyncMac", targets: ["MHSaveSyncMac"]),
    ],
    targets: [
        .executableTarget(name: "MHSaveSyncMac"),
    ]
)
