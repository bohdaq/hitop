// CI (ios.yml job store-screenshots): rewrites each PNG given without an alpha channel (opaque RGB),
// as App Store Connect wants screenshots. `swift ios/HITOP/scripts/flatten-png.swift a.png b.png …`
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

var failed = false
for path in CommandLine.arguments.dropFirst() {
    let url = URL(fileURLWithPath: path) as CFURL
    guard let source = CGImageSourceCreateWithURL(url, nil),
          let image = CGImageSourceCreateImageAtIndex(source, 0, nil),
          let context = CGContext(data: nil, width: image.width, height: image.height, bitsPerComponent: 8, bytesPerRow: 0,
                                  space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                  bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue) else {
        print("can't read \(path)"); failed = true; continue
    }
    context.setFillColor(CGColor(red: 0, green: 0, blue: 0, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: image.width, height: image.height))
    context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
    guard let flat = context.makeImage(),
          let dest = CGImageDestinationCreateWithURL(url, UTType.png.identifier as CFString, 1, nil) else {
        print("can't write \(path)"); failed = true; continue
    }
    CGImageDestinationAddImage(dest, flat, nil)
    if !CGImageDestinationFinalize(dest) { print("can't write \(path)"); failed = true }
}
exit(failed ? 1 : 0)
