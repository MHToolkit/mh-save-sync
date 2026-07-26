// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "MH3GSaveConverterMac",
    platforms: [.macOS(.v15)],
    products: [
        .library(name: "ConverterPresentation", targets: ["ConverterPresentation"]),
        .executable(name: "MH3GSaveConverterMac", targets: ["MH3GSaveConverterMac"]),
    ],
    targets: [
        .target(name: "ConverterPresentation"),
        .executableTarget(
            name: "MH3GSaveConverterMac",
            dependencies: ["ConverterPresentation"],
            resources: [.process("Resources")]
        ),
        .testTarget(
            name: "ConverterPresentationTests",
            dependencies: ["ConverterPresentation"]
        ),
    ]
)
