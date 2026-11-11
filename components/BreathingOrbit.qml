import QtQuick
import QtQuick.Shapes

// One half of the orbit drawn as a live ellipse, so it can open and close
// round the stars while its stroke keeps the same width. The resting shape
// matches the orbit in the mark's artwork: semi-axes `radiusX`/`radiusY` and
// `strokeWidth` in viewBox units, tilted by `tilt`, centred on the item.
// `open` scales the minor axis (1 = the resting mark). The far half is the
// whole ellipse; the near half is the arc from `startAngle` over `sweepAngle`.
Shape {
  id: orbit

  property color color: "white"
  property real viewSize: 48
  property real radiusX: 6.6
  property real radiusY: 2.4
  property real strokeWidth: 1
  property real tilt: 158
  property real open: 1
  property bool near: false
  property real startAngle: 0
  property real sweepAngle: 360

  readonly property real unit: width / viewSize

  preferredRendererType: Shape.CurveRenderer
  rotation: tilt

  ShapePath {
    strokeColor: orbit.color
    strokeWidth: orbit.strokeWidth * orbit.unit
    fillColor: "transparent"
    capStyle: ShapePath.RoundCap
    joinStyle: ShapePath.RoundJoin

    PathAngleArc {
      centerX: orbit.width / 2
      centerY: orbit.height / 2
      radiusX: orbit.radiusX * orbit.unit
      radiusY: Math.max(0.01, orbit.radiusY * orbit.open) * orbit.unit
      startAngle: orbit.near ? orbit.startAngle : 0
      sweepAngle: orbit.near ? orbit.sweepAngle : 360
    }
  }
}
