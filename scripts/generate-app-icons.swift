#!/usr/bin/env swift

import AppKit
import Foundation

enum RenderMode {
    case app
    case menuBar
    case legacyRound
}

enum IconError: Error, CustomStringConvertible {
    case invalidArguments
    case bitmapAllocation(Int)
    case pngEncoding(URL)

    var description: String {
        switch self {
        case .invalidArguments:
            return "usage: generate-app-icons.swift --out-root <repository-root>"
        case let .bitmapAllocation(size):
            return "could not allocate \(size)x\(size) bitmap"
        case let .pngEncoding(url):
            return "could not encode PNG at \(url.path)"
        }
    }
}

private let designSize: CGFloat = 1024
private let supersample = 4

private extension NSColor {
    convenience init(hex: UInt32) {
        self.init(
            calibratedRed: CGFloat((hex >> 16) & 0xff) / 255,
            green: CGFloat((hex >> 8) & 0xff) / 255,
            blue: CGFloat(hex & 0xff) / 255,
            alpha: 1
        )
    }
}

private func shieldPath() -> NSBezierPath {
    let path = NSBezierPath()
    path.move(to: NSPoint(x: 512, y: 224))
    path.line(to: NSPoint(x: 746, y: 314))
    path.line(to: NSPoint(x: 746, y: 482))
    path.curve(to: NSPoint(x: 512, y: 806), controlPoint1: NSPoint(x: 746, y: 638), controlPoint2: NSPoint(x: 656, y: 740))
    path.curve(to: NSPoint(x: 278, y: 482), controlPoint1: NSPoint(x: 368, y: 740), controlPoint2: NSPoint(x: 278, y: 638))
    path.line(to: NSPoint(x: 278, y: 314))
    path.close()
    path.lineJoinStyle = .round
    return path
}

private func saveSlotDetailsPath() -> NSBezierPath {
    let path = NSBezierPath()
    path.move(to: NSPoint(x: 428, y: 392))
    path.line(to: NSPoint(x: 428, y: 482))
    path.line(to: NSPoint(x: 596, y: 482))
    path.line(to: NSPoint(x: 596, y: 392))
    path.move(to: NSPoint(x: 440, y: 566))
    path.line(to: NSPoint(x: 584, y: 566))
    path.lineCapStyle = .round
    path.lineJoinStyle = .round
    return path
}

private func checkPath() -> NSBezierPath {
    let path = NSBezierPath()
    path.move(to: NSPoint(x: 440, y: 686))
    path.line(to: NSPoint(x: 494, y: 734))
    path.line(to: NSPoint(x: 596, y: 620))
    path.lineCapStyle = .round
    path.lineJoinStyle = .round
    return path
}

private func drawAppGlyph() {
    let shield = shieldPath()
    NSColor.white.setFill()
    shield.fill()

    let slot = NSBezierPath(roundedRect: NSRect(x: 380, y: 392, width: 264, height: 228), xRadius: 48, yRadius: 48)
    NSColor(hex: 0x4936B7).setFill()
    slot.fill()

    NSColor.white.setStroke()
    let details = saveSlotDetailsPath()
    details.lineWidth = 42
    details.stroke()

    NSColor(hex: 0x4936B7).setStroke()
    let check = checkPath()
    check.lineWidth = 42
    check.stroke()
}

private func drawMenuBarGlyph() {
    NSColor.black.setStroke()
    let shield = NSBezierPath()
    shield.move(to: NSPoint(x: 512, y: 96))
    shield.line(to: NSPoint(x: 864, y: 256))
    shield.line(to: NSPoint(x: 864, y: 512))
    shield.curve(to: NSPoint(x: 512, y: 992), controlPoint1: NSPoint(x: 864, y: 736), controlPoint2: NSPoint(x: 736, y: 896))
    shield.curve(to: NSPoint(x: 160, y: 512), controlPoint1: NSPoint(x: 288, y: 896), controlPoint2: NSPoint(x: 160, y: 736))
    shield.line(to: NSPoint(x: 160, y: 256))
    shield.close()
    shield.lineJoinStyle = .round
    shield.lineWidth = 80
    shield.stroke()

    let details = NSBezierPath()
    details.move(to: NSPoint(x: 320, y: 544))
    details.line(to: NSPoint(x: 704, y: 544))
    details.move(to: NSPoint(x: 384, y: 384))
    details.line(to: NSPoint(x: 640, y: 384))
    details.line(to: NSPoint(x: 640, y: 608))
    details.line(to: NSPoint(x: 384, y: 608))
    details.close()
    details.move(to: NSPoint(x: 384, y: 736))
    details.line(to: NSPoint(x: 448, y: 800))
    details.line(to: NSPoint(x: 608, y: 608))
    details.lineCapStyle = .round
    details.lineJoinStyle = .round
    details.lineWidth = 74
    details.stroke()
}

private func drawArtwork(mode: RenderMode) {
    switch mode {
    case .menuBar:
        drawMenuBarGlyph()
    case .app, .legacyRound:
        let boundary: NSBezierPath
        if mode == .legacyRound {
            boundary = NSBezierPath(ovalIn: NSRect(x: 40, y: 40, width: 944, height: 944))
        } else {
            boundary = NSBezierPath(roundedRect: NSRect(x: 40, y: 40, width: 944, height: 944), xRadius: 224, yRadius: 224)
        }

        NSGraphicsContext.saveGraphicsState()
        boundary.addClip()
        // AppKit's gradient direction is evaluated after the flipped design-space
        // transform. Reverse the stops so the raster matches the SVG's dark
        // upper-left to light lower-right B3 direction.
        let gradient = NSGradient(colors: [NSColor(hex: 0x9B72F2), NSColor(hex: 0x4936B7)])!
        gradient.draw(in: NSRect(x: 40, y: 40, width: 944, height: 944), angle: 45)
        NSGraphicsContext.restoreGraphicsState()
        drawAppGlyph()
    }
}

private func bitmap(size: Int) throws -> NSBitmapImageRep {
    guard let image = NSBitmapImageRep(
        bitmapDataPlanes: nil,
        pixelsWide: size,
        pixelsHigh: size,
        bitsPerSample: 8,
        samplesPerPixel: 4,
        hasAlpha: true,
        isPlanar: false,
        colorSpaceName: .deviceRGB,
        bitmapFormat: [],
        bytesPerRow: 0,
        bitsPerPixel: 0
    ) else {
        throw IconError.bitmapAllocation(size)
    }
    image.size = NSSize(width: size, height: size)
    return image
}

func render(size: Int, mode: RenderMode, output: URL) throws {
    let highSize = size * supersample
    let high = try bitmap(size: highSize)
    guard let highContext = NSGraphicsContext(bitmapImageRep: high) else {
        throw IconError.bitmapAllocation(highSize)
    }

    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = highContext
    highContext.cgContext.clear(CGRect(x: 0, y: 0, width: highSize, height: highSize))
    highContext.cgContext.translateBy(x: 0, y: CGFloat(highSize))
    highContext.cgContext.scaleBy(x: CGFloat(highSize) / designSize, y: -CGFloat(highSize) / designSize)
    drawArtwork(mode: mode)
    highContext.flushGraphics()
    NSGraphicsContext.restoreGraphicsState()

    let target = try bitmap(size: size)
    guard let targetContext = NSGraphicsContext(bitmapImageRep: target) else {
        throw IconError.bitmapAllocation(size)
    }
    let image = NSImage(size: NSSize(width: highSize, height: highSize))
    image.addRepresentation(high)

    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = targetContext
    targetContext.cgContext.clear(CGRect(x: 0, y: 0, width: size, height: size))
    targetContext.imageInterpolation = .high
    image.draw(
        in: NSRect(x: 0, y: 0, width: size, height: size),
        from: NSRect(x: 0, y: 0, width: highSize, height: highSize),
        operation: .copy,
        fraction: 1
    )
    targetContext.flushGraphics()
    NSGraphicsContext.restoreGraphicsState()

    try FileManager.default.createDirectory(at: output.deletingLastPathComponent(), withIntermediateDirectories: true)
    guard let data = target.representation(using: .png, properties: [:]) else {
        throw IconError.pngEncoding(output)
    }
    try data.write(to: output, options: .atomic)
}

private func outputRoot(arguments: [String]) throws -> URL {
    guard let flag = arguments.firstIndex(of: "--out-root"), arguments.indices.contains(flag + 1) else {
        throw IconError.invalidArguments
    }
    return URL(fileURLWithPath: arguments[flag + 1], isDirectory: true).standardizedFileURL
}

private func generate(outRoot: URL) throws {
    let iconset = outRoot.appendingPathComponent("apps/macos/Resources/AppIcon/MHSaveSync.iconset", isDirectory: true)
    let iconsetOutputs: [(String, Int)] = [
        ("icon_16x16.png", 16),
        ("icon_16x16@2x.png", 32),
        ("icon_32x32.png", 32),
        ("icon_32x32@2x.png", 64),
        ("icon_128x128.png", 128),
        ("icon_128x128@2x.png", 256),
        ("icon_256x256.png", 256),
        ("icon_256x256@2x.png", 512),
        ("icon_512x512.png", 512),
        ("icon_512x512@2x.png", 1024),
    ]
    for (name, size) in iconsetOutputs {
        try render(size: size, mode: .app, output: iconset.appendingPathComponent(name))
    }

    try render(
        size: 36,
        mode: .menuBar,
        output: outRoot.appendingPathComponent("apps/macos/Resources/AppIcon/mh-save-sync-menubar-template.png")
    )

    let androidOutputs: [(String, Int)] = [
        ("mdpi", 48),
        ("hdpi", 72),
        ("xhdpi", 96),
        ("xxhdpi", 144),
        ("xxxhdpi", 192),
    ]
    for (density, size) in androidOutputs {
        let directory = outRoot.appendingPathComponent("apps/android/app/src/main/res/mipmap-\(density)", isDirectory: true)
        try render(size: size, mode: .app, output: directory.appendingPathComponent("ic_launcher.png"))
        try render(size: size, mode: .legacyRound, output: directory.appendingPathComponent("ic_launcher_round.png"))
    }
}

do {
    try generate(outRoot: outputRoot(arguments: CommandLine.arguments))
} catch {
    fputs("generate-app-icons: \(error)\n", stderr)
    exit(1)
}
