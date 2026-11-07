// Asks macOS to draw an installed app's icon, as Finder, the Dock and
// launchers do, and checks its colors at small and large sizes
// (docs/specs/release.md §12). alpha.3's .icns had PNG in slots macOS reads
// as raw pixels inside an app, and small views drew a garbled purple tile.
//
//   xcrun swift .github/packaging/icon_check.swift "<path>/Cold Open.app"
import AppKit

let args = CommandLine.arguments
guard args.count == 2 else {
    FileHandle.standardError.write("usage: icon_check.swift <app>\n".data(using: .utf8)!)
    exit(2)
}
let icon = NSWorkspace.shared.icon(forFile: args[1])
print("macOS \(ProcessInfo.processInfo.operatingSystemVersionString)")

// The drawn icon's box: macOS may draw it inset, with a clear margin.
struct Box { var x0 = 0, y0 = 0, x1 = 0, y1 = 0 }
func opaque(_ rep: NSBitmapImageRep) -> Box? {
    var box: Box? = nil
    for y in 0..<rep.pixelsHigh {
        for x in 0..<rep.pixelsWide where (rep.colorAt(x: x, y: y)?.alphaComponent ?? 0) > 0.5 {
            if var b = box {
                b.x0 = min(b.x0, x); b.y0 = min(b.y0, y); b.x1 = max(b.x1, x + 1); b.y1 = max(b.y1, y + 1)
                box = b
            } else {
                box = Box(x0: x, y0: y, x1: x + 1, y1: y + 1)
            }
        }
    }
    return box
}

// The mean color of a part of the drawn icon, in fractions of its box.
func mean(_ rep: NSBitmapImageRep, _ box: Box, _ fx0: Double, _ fx1: Double, _ fy0: Double, _ fy1: Double) -> (Int, Int, Int) {
    let w = Double(box.x1 - box.x0), h = Double(box.y1 - box.y0)
    let x0 = box.x0 + Int(fx0 * w), x1 = max(x0 + 1, box.x0 + Int(fx1 * w))
    let y0 = box.y0 + Int(fy0 * h), y1 = max(y0 + 1, box.y0 + Int(fy1 * h))
    var r = 0.0, g = 0.0, b = 0.0, count = 0.0
    for y in y0..<y1 {
        for x in x0..<x1 {
            guard let c = rep.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else { continue }
            r += c.redComponent; g += c.greenComponent; b += c.blueComponent; count += 1
        }
    }
    return (Int(255 * r / count), Int(255 * g / count), Int(255 * b / count))
}

var failed = false
for points in [16, 32, 128] {
    for scale in [1, 2] {
        let px = points * scale
        let rep = NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: px, pixelsHigh: px, bitsPerSample: 8,
            samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
            bytesPerRow: 0, bitsPerPixel: 0)!
        rep.size = NSSize(width: points, height: points)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
        icon.draw(in: NSRect(x: 0, y: 0, width: points, height: points),
                  from: .zero, operation: .copy, fraction: 1)
        NSGraphicsContext.restoreGraphicsState()

        guard let box = opaque(rep), box.x1 - box.x0 >= px / 2, box.y1 - box.y0 >= px / 2 else {
            print("\(points)pt @\(scale)x: drawn nearly empty WRONG")
            failed = true
            continue
        }
        // The blue tile, left of the C, at mid-height: #0C447C.
        let tile = mean(rep, box, 0.04, 0.12, 0.42, 0.58)
        let blue = tile.2 > tile.0 + 60 && tile.2 > tile.1 + 25 && tile.0 < 80
        // The C's left stroke, in paper (#F7F5EF), where it's wide enough.
        let page = mean(rep, box, 0.17, 0.23, 0.45, 0.55)
        let paper = px < 32 || (page.0 > 170 && page.1 > 170 && page.2 > 170)
        let ok = blue && paper
        print("\(points)pt @\(scale)x: drawn at \(box.x0),\(box.y0) to \(box.x1),\(box.y1) of \(px); tile \(tile), stroke \(page) \(ok ? "ok" : "WRONG")")
        if !ok { failed = true }
    }
}
if failed {
    print("::error::macOS draws the app's icon in the wrong colors (release.md §12)")
    exit(1)
}
