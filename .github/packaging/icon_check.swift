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

// The mean color of a box, in fractions of the icon's side.
func mean(_ rep: NSBitmapImageRep, _ x0: Double, _ x1: Double, _ y0: Double, _ y1: Double) -> (Int, Int, Int) {
    let n = Double(rep.pixelsWide)
    var r = 0.0, g = 0.0, b = 0.0, count = 0.0
    for y in Int(y0 * n)...max(Int(y0 * n), Int(y1 * n) - 1) {
        for x in Int(x0 * n)...max(Int(x0 * n), Int(x1 * n) - 1) {
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

        // The blue tile, left of the C, at mid-height: #0C447C.
        let tile = mean(rep, 0.03, 0.12, 0.42, 0.58)
        let blue = tile.2 > tile.0 + 60 && tile.2 > tile.1 + 25 && tile.0 < 80
        // The C's left stroke, in paper (#F7F5EF), where it's wide enough.
        let page = mean(rep, 0.17, 0.23, 0.45, 0.55)
        let paper = px < 32 || (page.0 > 170 && page.1 > 170 && page.2 > 170)
        let ok = blue && paper
        print("\(points)pt @\(scale)x: tile \(tile), stroke \(page) \(ok ? "ok" : "WRONG")")
        if !ok { failed = true }
    }
}
if failed {
    print("::error::macOS draws the app's icon in the wrong colors (release.md §12)")
    exit(1)
}
