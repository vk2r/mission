import QtQuick

// Lets a page header move the floating card. It sits under the header's
// buttons, so only the empty parts of the header grab; docked, it is inert.
MouseArea {
  id: drag
  required property var host
  enabled: drag.host.floating === true
  cursorShape: drag.enabled ? (drag.pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor) : Qt.ArrowCursor
  onPressed: function(mouse) {
    var p = drag.mapToItem(null, mouse.x, mouse.y)
    drag.host.floatDragStart(p.x, p.y)
  }
  onPositionChanged: function(mouse) {
    if (!drag.pressed) return
    var p = drag.mapToItem(null, mouse.x, mouse.y)
    drag.host.floatDragMove(p.x, p.y)
  }
  onReleased: drag.host.floatDragEnd()
  onCanceled: drag.host.floatDragEnd()
}
