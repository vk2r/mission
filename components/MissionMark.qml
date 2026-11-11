import QtQuick
import qs.Commons

// The mark in layers: the far half of the orbit, the central star, the two
// satellites and the near half. The satellites travel round the still central
// star by `angle` and both halves of the orbit turn the other way, so the orbit
// still passes in front of the stars and behind them; a full revolution brings
// every layer back to the resting mark. `artwork` is the asset prefix
// ("mission-mark" for the bar, "mission-mark-hero" for the panel header) and
// `viewSize`/offsets are the satellites' resting places in that viewBox.
// With `breathe` the orbit turns in depth round its long axis instead, like a
// ring spun round the stars: it opens into a full circle, closes to a line and
// comes back, once per revolution (components/BreathingOrbit.qml). The resting
// mark is the ring seen at `tilt3d`; whichever half is nearer the viewer is
// drawn over the stars. `orbit` holds its resting geometry in viewBox units. `ring` adds the still outer circle of the header artwork.
Item {
  id: mark

  property string artwork: "mission-mark"
  property real angle: 0
  property color tint: Color.foreground
  property color accent: Color.accent
  property real viewSize: 16
  property point starA: Qt.point(0, 0)
  property point starB: Qt.point(0, 0)
  property bool ring: false
  property bool breathe: false
  property var orbit: ({ rx: 0, ry: 0, stroke: 0, tilt: 0, start: 0, sweep: 0 })

  readonly property real tilt3d: mark.orbit.rx > 0 ? Math.acos(Math.min(1, mark.orbit.ry / mark.orbit.rx)) : 0
  readonly property real depth: mark.tilt3d - mark.angle * Math.PI / 180
  readonly property real open: mark.orbit.ry > 0 ? Math.abs(Math.cos(mark.depth)) * mark.orbit.rx / mark.orbit.ry : 1
  readonly property bool flipped: Math.sin(2 * mark.depth) < 0
  readonly property real alpha: Math.min(mark.tint.a, mark.accent.a)
  readonly property color orbitColor: Qt.alpha(mark.accent, 0.85 * mark.alpha)

  function art(layer) { return Qt.resolvedUrl("../assets/" + mark.artwork + "-" + layer + ".svg") }

  BrandIcon {
    visible: mark.ring
    anchors.fill: parent
    artworkFile: mark.ring ? mark.art("ring") : ""
    tint: mark.tint
    accent: mark.accent
  }
  BrandIcon {
    visible: !mark.breathe
    anchors.fill: parent
    artworkFile: mark.art("orbit-back")
    tint: mark.tint
    accent: mark.accent
    rotation: -mark.angle
  }
  BreathingOrbit {
    visible: mark.breathe
    anchors.fill: parent
    color: mark.orbitColor
    viewSize: mark.viewSize
    radiusX: mark.orbit.rx
    radiusY: mark.orbit.ry
    strokeWidth: mark.orbit.stroke
    tilt: mark.orbit.tilt
    open: mark.open
  }
  BrandIcon {
    anchors.fill: parent
    artworkFile: mark.art("star-core")
    tint: mark.tint
    accent: mark.accent
  }
  OrbitStar {
    anchors.fill: parent
    artworkFile: mark.art("star-a")
    tint: mark.tint
    accent: mark.accent
    angle: mark.angle
    viewSize: mark.viewSize
    offsetX: mark.starA.x
    offsetY: mark.starA.y
  }
  OrbitStar {
    anchors.fill: parent
    artworkFile: mark.art("star-b")
    tint: mark.tint
    accent: mark.accent
    angle: mark.angle
    viewSize: mark.viewSize
    offsetX: mark.starB.x
    offsetY: mark.starB.y
  }
  BrandIcon {
    visible: !mark.breathe
    anchors.fill: parent
    artworkFile: mark.art("orbit-front")
    tint: mark.tint
    accent: mark.accent
    rotation: -mark.angle
  }
  BreathingOrbit {
    visible: mark.breathe
    anchors.fill: parent
    color: mark.orbitColor
    viewSize: mark.viewSize
    radiusX: mark.orbit.rx
    radiusY: mark.orbit.ry
    strokeWidth: mark.orbit.stroke
    tilt: mark.orbit.tilt
    open: mark.open
    near: true
    startAngle: mark.flipped ? -(mark.orbit.start + mark.orbit.sweep) : mark.orbit.start
    sweepAngle: mark.orbit.sweep
  }
}
