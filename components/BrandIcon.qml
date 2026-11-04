import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons

Image {
  id: root

  property color tint: Color.foreground
  property color accent: Color.accent
  property string artworkFile: ""

  property string artwork: ""

  function rgb(c) {
    return "rgb(" + Math.round(c.r * 255) + "," + Math.round(c.g * 255) + "," + Math.round(c.b * 255) + ")"
  }
  readonly property real alpha: Math.min(tint.a, accent.a)
  readonly property string tinted: artwork
    .replace(/(fill|stroke)="currentColor"/g, '$1="' + rgb(tint) + '"')
    .replace(/(fill|stroke)="accentColor"/g, '$1="' + rgb(accent) + '"')
    .replace(/(<svg[^>]*>)/, '$1<g opacity="' + alpha + '">')
    .replace("</svg>", "</g></svg>")

  source: tinted !== "" ? "data:image/svg+xml;utf8," + encodeURIComponent(tinted) : ""
  fillMode: Image.PreserveAspectFit
  smooth: true
  sourceSize.width: Math.max(1, Math.ceil(width * Screen.devicePixelRatio))
  sourceSize.height: Math.max(1, Math.ceil(height * Screen.devicePixelRatio))

  FileView {
    id: art
    path: decodeURIComponent((root.artworkFile !== ""
      ? Qt.resolvedUrl(root.artworkFile)
      : Qt.resolvedUrl("../assets/mission-mark.svg"))
      .toString().replace("file://", ""))
    onLoaded: root.artwork = art.text()
  }
}
