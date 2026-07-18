#!/usr/bin/env swift

import AppKit
import Foundation

enum ContractError: Error, CustomStringConvertible {
    case unreadable(String)
    case opaqueEdge(String)
    case reversedGradient

    var description: String {
        switch self {
        case let .unreadable(path): return "cannot decode PNG: \(path)"
        case let .opaqueEdge(path): return "outer edge must be fully transparent: \(path)"
        case .reversedGradient: return "launcher gradient must run dark upper-left to light lower-right"
        }
    }
}

func load(_ path: String) throws -> NSBitmapImageRep {
    guard let data = FileManager.default.contents(atPath: path),
          let image = NSBitmapImageRep(data: data) else {
        throw ContractError.unreadable(path)
    }
    return image
}

func assertTransparentEdge(_ path: String) throws {
    let image = try load(path)
    let lastX = image.pixelsWide - 1
    let lastY = image.pixelsHigh - 1
    for x in 0...lastX {
        if (image.colorAt(x: x, y: 0)?.alphaComponent ?? 1) > 0.001 ||
           (image.colorAt(x: x, y: lastY)?.alphaComponent ?? 1) > 0.001 {
            throw ContractError.opaqueEdge(path)
        }
    }
    for y in 0...lastY {
        if (image.colorAt(x: 0, y: y)?.alphaComponent ?? 1) > 0.001 ||
           (image.colorAt(x: lastX, y: y)?.alphaComponent ?? 1) > 0.001 {
            throw ContractError.opaqueEdge(path)
        }
    }
}

func rgb(_ image: NSBitmapImageRep, x: Int, y: Int) throws -> (Double, Double, Double) {
    guard let color = image.colorAt(x: x, y: y) else {
        throw ContractError.unreadable("pixel \(x),\(y)")
    }
    return (color.redComponent, color.greenComponent, color.blueComponent)
}

func distance(_ lhs: (Double, Double, Double), _ rhs: (Double, Double, Double)) -> Double {
    let dr = lhs.0 - rhs.0
    let dg = lhs.1 - rhs.1
    let db = lhs.2 - rhs.2
    return (dr * dr + dg * dg + db * db).squareRoot()
}

do {
    let launcherPath = "apps/android/app/src/main/res/mipmap-xxxhdpi/ic_launcher.png"
    let roundPath = "apps/android/app/src/main/res/mipmap-xxxhdpi/ic_launcher_round.png"
    let menuPath = "apps/macos/Resources/AppIcon/mh-save-sync-menubar-template.png"
    try assertTransparentEdge(launcherPath)
    try assertTransparentEdge(roundPath)
    try assertTransparentEdge(menuPath)

    let launcher = try load(launcherPath)
    // NSBitmapImageRep uses a bottom-left origin. These samples are clear of
    // the glyph and sit on the approved upper-left/lower-right gradient axis.
    let upperLeft = try rgb(launcher, x: 35, y: 155)
    let lowerRight = try rgb(launcher, x: 157, y: 35)
    let dark = (73.0 / 255, 54.0 / 255, 183.0 / 255)
    let light = (155.0 / 255, 114.0 / 255, 242.0 / 255)
    guard distance(upperLeft, dark) < distance(upperLeft, light),
          distance(lowerRight, light) < distance(lowerRight, dark) else {
        throw ContractError.reversedGradient
    }
    print("icon pixel contract: ok")
} catch {
    fputs("icon pixel contract: \(error)\n", stderr)
    exit(1)
}
