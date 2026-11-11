pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.Commons
import qs.Commons as Commons
import qs.Ui

// The panel as a card that stays on screen: a full-output layer-shell surface
// on the Top layer (so Hyprland never tiles it) whose input mask is the card
// alone. The card drags by the page header, resizes by its corner and docks
// back to the bar from the header button. Position and size are stored raw and clamped on the way out, so a
// smaller output never rewrites where the card was left.
PanelWindow {
  id: win

  required property var host
  required property Component body
  property bool shown: false

  readonly property real margin: Style.gapsOut
  readonly property real minW: Style.space(320)
  readonly property real minH: Style.space(240)
  readonly property real sw: screen ? screen.width : 0
  readonly property real sh: screen ? screen.height : 0

  property real rawX: -1
  property real rawY: -1
  property real rawW: -1
  property real rawH: -1
  property bool geometryKnown: false
  property real fade: shown ? 1 : 0
  Behavior on fade {
    enabled: win.host.motionOn
    NumberAnimation { duration: Style.duration(160); easing.type: Easing.Bezier; easing.bezierCurve: Style.easeOut }
  }

  property bool dragging: false
  readonly property bool interacting: dragging || grip.pressed

  property real grabX: 0
  property real grabY: 0
  property real startX: 0
  property real startY: 0

  property real lastX: 0
  property real lastY: 0

  function dragStart(x, y) {
    grabX = x; grabY = y; startX = cardX; startY = cardY
    lastX = x; lastY = y
    dragging = true
  }

  // Never below zero: a negative raw value means "never placed" and centres
  // the card, so a drag past the left or top edge must stop at the edge.
  function dragMove(x, y) {
    if (!dragging) return
    lastX = x; lastY = y
    rawX = Math.max(0, startX + x - grabX)
    rawY = Math.max(0, startY + y - grabY)
  }

  // The output under the pointer, in global layout coordinates, or null.
  function outputAt(gx, gy) {
    var list = Quickshell.screens
    for (var i = 0; i < list.length; ++i) {
      var s = list[i]
      if (gx >= s.x && gx < s.x + s.width && gy >= s.y && gy < s.y + s.height) return s
    }
    return null
  }

  function dragEnd() {
    if (!dragging) return
    dragging = false
    // Released over another output: hand the card to that output's panel,
    // keeping it where the pointer left it.
    if (screen && (lastX < 0 || lastY < 0 || lastX >= sw || lastY >= sh)) {
      var target = outputAt(screen.x + lastX, screen.y + lastY)
      if (target && target.name !== screen.name) {
        var gx = screen.x + startX + lastX - grabX
        var gy = screen.y + startY + lastY - grabY
        var w = cardW, h = cardH
        host.save("floatX", Math.max(0, Math.round(gx - target.x)), -1)
        host.save("floatY", Math.max(0, Math.round(gy - target.y)), -1)
        host.save("floatW", w, -1)
        host.save("floatH", h, -1)
        host.save("floatScreen", target.name, "")
        return
      }
    }
    rawX = cardX
    rawY = cardY
    rawW = cardW
    rawH = cardH
    save()
  }

  Connections {
    target: win.host
    function onFloatXChanged() { if (!win.interacting) win.loadGeometry() }
    function onFloatYChanged() { if (!win.interacting) win.loadGeometry() }
    function onFloatWChanged() { if (!win.interacting) win.loadGeometry() }
    function onFloatHChanged() { if (!win.interacting) win.loadGeometry() }
  }

  function loadGeometry() {
    rawW = host.floatW
    rawH = host.floatH
    rawX = host.floatX
    rawY = host.floatY
    geometryKnown = true
  }

  function clampSize(value, low, high) {
    return Math.round(Math.max(Math.min(low, high), Math.min(value, high)))
  }

  readonly property real maxW: Math.max(minW, sw - margin * 2)
  readonly property real maxH: Math.max(minH, sh - margin * 2)
  readonly property real autoH: bodyLoader.item
    ? bodyLoader.item.wanted + card.contentTopInset + card.contentBottomInset
    : win.minH
  readonly property real cardW: clampSize(rawW > 0 ? rawW : Style.space(400), minW, maxW)
  readonly property real cardH: clampSize(rawH > 0 ? rawH : Math.min(autoH, host.maxCardHeight), minH, maxH)
  readonly property real cardX: clampSize(rawX >= 0 ? rawX : (sw - cardW) / 2, margin, Math.max(margin, sw - cardW - margin))
  readonly property real cardY: clampSize(rawY >= 0 ? rawY : margin * 2, margin, Math.max(margin, sh - cardH - margin))

  // Read all four first: each save echoes back through the Connections above,
  // and once the pointer is up that echo reloads whatever is not saved yet.
  function save() {
    var x = win.cardX, y = win.cardY, w = win.cardW, h = win.cardH
    host.save("floatX", x, -1)
    host.save("floatY", y, -1)
    host.save("floatW", w, -1)
    host.save("floatH", h, -1)
  }

  onShownChanged: if (shown && !geometryKnown) loadGeometry()
  Component.onCompleted: if (shown) loadGeometry()

  visible: shown || fade > 0
  color: "transparent"
  exclusionMode: ExclusionMode.Ignore
  anchors { top: true; left: true; bottom: true; right: true }
  mask: Region { item: card }

  WlrLayershell.namespace: "mission-floating"
  WlrLayershell.layer: WlrLayer.Top
  WlrLayershell.keyboardFocus: win.shown ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None

  BorderSurface {
    id: card
    x: win.cardX
    y: win.cardY
    width: win.cardW
    height: win.cardH
    color: Commons.Color.popups.background
    borderSpec: Border.surfaceSpec("popups", "border", Commons.Color.popups.border, Math.max(1, Style.space(2)))
    padding: Style.spacing.popupPadding
    radius: Style.cornerRadius
    opacity: win.fade

    MouseArea {
      anchors.fill: parent
      acceptedButtons: Qt.AllButtons
    }

    Item {
      id: holder
      anchors.top: parent.top
      anchors.left: parent.left
      anchors.topMargin: card.contentTopInset
      anchors.leftMargin: card.contentLeftInset
      width: card.width - card.contentLeftInset - card.contentRightInset
      height: card.height - card.contentTopInset - card.contentBottomInset
      LayoutMirroring.enabled: Commons.I18n.rightToLeft
      LayoutMirroring.childrenInherit: true

      Loader {
        id: bodyLoader
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        active: win.shown
        sourceComponent: win.body
      }
    }

    MouseArea {
      id: grip
      width: Style.space(18)
      height: width
      anchors.right: parent.right
      anchors.bottom: parent.bottom
      cursorShape: Qt.SizeFDiagCursor
      property real grabX: 0
      property real grabY: 0
      property real startW: 0
      property real startH: 0
      onPressed: function(mouse) {
        var p = mapToItem(win.contentItem, mouse.x, mouse.y)
        grabX = p.x; grabY = p.y; startW = win.cardW; startH = win.cardH
      }
      onPositionChanged: function(mouse) {
        if (!pressed) return
        var p = mapToItem(win.contentItem, mouse.x, mouse.y)
        win.rawW = Math.min(startW + p.x - grabX, win.sw - win.margin - win.cardX)
        win.rawH = Math.min(startH + p.y - grabY, win.sh - win.margin - win.cardY)
      }
      onReleased: {
        win.rawX = win.cardX
        win.rawY = win.cardY
        win.rawW = win.cardW
        win.rawH = win.cardH
        win.save()
      }

      Text {
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.rightMargin: Style.space(4)
        anchors.bottomMargin: Style.space(2)
        textFormat: Text.PlainText
        text: "◢"
        color: win.host.foreground
        opacity: grip.pressed ? 0.6 : 0.3
        font.pixelSize: Style.space(10)
      }
    }
  }
}
