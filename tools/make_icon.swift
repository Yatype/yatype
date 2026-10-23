// Рисует иконку программы 1024×1024 в PNG.
//   swift tools/make_icon.swift icon.png

import AppKit

let canvas = 1024.0
let image = NSImage(size: NSSize(width: canvas, height: canvas))
image.lockFocus()

// Скруглённый квадрат по сетке иконок macOS: 824×824 в центре.
let tile = NSRect(x: 100, y: 100, width: 824, height: 824)
let shape = NSBezierPath(roundedRect: tile, xRadius: 185, yRadius: 185)
NSGradient(
    starting: NSColor(calibratedRed: 0.27, green: 0.52, blue: 0.98, alpha: 1),
    ending: NSColor(calibratedRed: 0.12, green: 0.27, blue: 0.72, alpha: 1)
)!.draw(in: shape, angle: -90)

let centered = NSMutableParagraphStyle()
centered.alignment = .center

func label(_ text: String, y: CGFloat, alpha: CGFloat, size: CGFloat = 300) {
    let attributes: [NSAttributedString.Key: Any] = [
        .font: NSFont.systemFont(ofSize: size, weight: .bold),
        .foregroundColor: NSColor.white.withAlphaComponent(alpha),
        .paragraphStyle: centered,
    ]
    NSString(string: text).draw(in: NSRect(x: 100, y: y, width: 824, height: size * 1.3), withAttributes: attributes)
}

// Буква «ять» — от неё название Yatype, — под ней полупрозрачное латинское «Aa».
label("Ѣ", y: 360, alpha: 1.0, size: 460)
label("Aa", y: 130, alpha: 0.5, size: 240)

image.unlockFocus()

let bitmap = NSBitmapImageRep(data: image.tiffRepresentation!)!
let png = bitmap.representation(using: .png, properties: [:])!
try! png.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
