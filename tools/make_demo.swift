// Рисует анимацию для README: набрали «ghbdtn rfr ltkf?» в английской
// раскладке — Yatype исправил на «привет как дела?» и переключил раскладку.
//   swift tools/make_demo.swift docs/images/demo.gif

import AppKit
import ImageIO
import UniformTypeIdentifiers

let width: CGFloat = 900
let height: CGFloat = 400
let scale: CGFloat = 2 // двойное разрешение — чётко на Retina

// Цвета
let background = NSColor(calibratedRed: 0.94, green: 0.95, blue: 0.97, alpha: 1)
let card = NSColor.white
let ink = NSColor(calibratedRed: 0.11, green: 0.12, blue: 0.15, alpha: 1)
let muted = NSColor(calibratedRed: 0.45, green: 0.47, blue: 0.52, alpha: 1)
let accent = NSColor(calibratedRed: 0.17, green: 0.42, blue: 0.90, alpha: 1)
let fixed = NSColor(calibratedRed: 0.86, green: 0.96, blue: 0.88, alpha: 1)

struct Frame {
    var text: String
    var lang: String
    /// Сколько последних символов подсветить как исправленные.
    var highlight: Int = 0
    var caption: String
    var delay: Double
}

func text(_ s: String, size: CGFloat, weight: NSFont.Weight, color: NSColor) -> NSAttributedString {
    NSAttributedString(string: s, attributes: [.font: NSFont.systemFont(ofSize: size, weight: weight), .foregroundColor: color])
}

func draw(_ f: Frame) -> CGImage {
    let bitmap = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: Int(width * scale), pixelsHigh: Int(height * scale),
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    bitmap.size = NSSize(width: width, height: height)
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
    background.setFill()
    NSRect(x: 0, y: 0, width: width, height: height).fill()

    // Полоска «строки меню» со значком раскладки.
    let bar = NSRect(x: 60, y: height - 78, width: width - 120, height: 40)
    NSColor.white.withAlphaComponent(0.8).setFill()
    NSBezierPath(roundedRect: bar, xRadius: 10, yRadius: 10).fill()
    text("Yatype", size: 15, weight: .semibold, color: muted).draw(at: NSPoint(x: bar.minX + 18, y: bar.minY + 11))
    let badge = NSRect(x: bar.maxX - 74, y: bar.minY + 7, width: 56, height: 26)
    (f.lang == "RU" && f.highlight > 0 ? accent : ink).setFill()
    NSBezierPath(roundedRect: badge, xRadius: 6, yRadius: 6).fill()
    let langLabel = text(f.lang, size: 15, weight: .bold, color: .white)
    langLabel.draw(at: NSPoint(x: badge.midX - langLabel.size().width / 2, y: badge.minY + 4))

    // «Окно» с полем ввода.
    let window = NSRect(x: 60, y: 92, width: width - 120, height: 190)
    let shadow = NSShadow()
    shadow.shadowColor = NSColor.black.withAlphaComponent(0.12)
    shadow.shadowBlurRadius = 18
    shadow.shadowOffset = NSSize(width: 0, height: -4)
    NSGraphicsContext.saveGraphicsState()
    shadow.set()
    card.setFill()
    NSBezierPath(roundedRect: window, xRadius: 14, yRadius: 14).fill()
    NSGraphicsContext.restoreGraphicsState()
    for (i, color) in [NSColor.systemRed, NSColor.systemYellow, NSColor.systemGreen].enumerated() {
        color.withAlphaComponent(0.85).setFill()
        NSBezierPath(ovalIn: NSRect(x: window.minX + 18 + CGFloat(i) * 20, y: window.maxY - 26, width: 12, height: 12)).fill()
    }
    text("Сообщение", size: 14, weight: .medium, color: muted)
        .draw(at: NSPoint(x: window.midX - 36, y: window.maxY - 29))

    // Набранный текст, исправленная часть подсвечена.
    let typed = text(f.text, size: 40, weight: .regular, color: ink)
    let origin = NSPoint(x: window.minX + 36, y: window.minY + 58)
    if f.highlight > 0 {
        let plain = text(String(f.text.dropLast(f.highlight)), size: 40, weight: .regular, color: ink)
        let start = origin.x + plain.size().width
        let marked = NSRect(x: start - 6, y: origin.y - 4, width: typed.size().width - plain.size().width + 4, height: 56)
        fixed.setFill()
        NSBezierPath(roundedRect: marked, xRadius: 8, yRadius: 8).fill()
    }
    typed.draw(at: origin)
    accent.setFill()
    NSRect(x: origin.x + typed.size().width + 3, y: origin.y + 4, width: 3, height: 46).fill()

    // Подпись.
    let caption = text(f.caption, size: 20, weight: .medium, color: ink)
    caption.draw(at: NSPoint(x: width / 2 - caption.size().width / 2, y: 40))
    NSGraphicsContext.restoreGraphicsState()
    return bitmap.cgImage!
}

// Сценарий.
var frames: [Frame] = []
let forgot = "Забыли переключить раскладку…"
let done = "Yatype исправил и переключил раскладку сам"
frames.append(Frame(text: "", lang: "EN", caption: forgot, delay: 0.8))
var s = ""
for c in "ghbdtn" {
    s.append(c)
    frames.append(Frame(text: s, lang: "EN", caption: forgot, delay: 0.11))
}
frames.append(Frame(text: "ghbdtn ", lang: "EN", caption: forgot, delay: 0.3))
s = "привет "
frames.append(Frame(text: s, lang: "RU", highlight: 7, caption: done, delay: 0.8))
for c in "как дела?" {
    s.append(c)
    frames.append(Frame(text: s, lang: "RU", caption: done, delay: 0.11))
}
frames.append(Frame(text: s, lang: "RU", caption: "Бесплатно · macOS и Windows · без слежки", delay: 2.4))

let url = URL(fileURLWithPath: CommandLine.arguments[1])
let destination = CGImageDestinationCreateWithURL(url as CFURL, UTType.gif.identifier as CFString, frames.count, nil)!
CGImageDestinationSetProperties(destination, [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFLoopCount: 0]] as CFDictionary)
for f in frames {
    let props = [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFDelayTime: f.delay]] as CFDictionary
    CGImageDestinationAddImage(destination, draw(f), props)
}
CGImageDestinationFinalize(destination)
