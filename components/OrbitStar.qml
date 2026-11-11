import QtQuick

// One satellite star of the mark. It travels round the mark's centre by
// `angle` degrees but is only moved, never rotated, so the star keeps its
// shape upright the whole way. `offsetX`/`offsetY` are where the star rests,
// from the centre, in the artwork's own viewBox units of `viewSize`.
BrandIcon {
  id: star

  property real angle: 0
  property real offsetX: 0
  property real offsetY: 0
  property real viewSize: 16

  readonly property real unit: width / viewSize
  readonly property real rad: angle * Math.PI / 180

  transform: Translate {
    x: (star.offsetX * Math.cos(star.rad) - star.offsetY * Math.sin(star.rad) - star.offsetX) * star.unit
    y: (star.offsetX * Math.sin(star.rad) + star.offsetY * Math.cos(star.rad) - star.offsetY) * star.unit
  }
}
