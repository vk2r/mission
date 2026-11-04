pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Shapes
import qs.Commons
import qs.Commons as Commons
import "../mascot/motion.js" as MascotMotion

Item {
  id: rowItem

  required property var modelData
  property var panel: null
  property bool chief: false
  property bool hanging: false
  property real trunkOffset: -Style.space(3)

  readonly property color foreground: rowItem.panel ? rowItem.panel.foreground : "white"
  readonly property color dim: rowItem.panel ? rowItem.panel.dim : "gray"
  readonly property string fontFamily: rowItem.panel ? rowItem.panel.fontFamily : ""
  readonly property string status: rowItem.modelData ? String(rowItem.modelData.status) : "unknown"
  readonly property bool working: rowItem.status === "working"
  readonly property string face: rowItem.modelData && rowItem.modelData.expression ? String(rowItem.modelData.expression) : "idle"
  readonly property real frameMs: MascotMotion.frameMs(rowItem.face, botAvatar.width)
  readonly property bool leading: rowItem.chief && rowItem.panel !== null && !!rowItem.panel.hostWidget
    && (rowItem.panel.teams.length > 1
      ? rowItem.panel.teamWorking(rowItem.modelData ? rowItem.modelData.department : "")
      : rowItem.panel.hostWidget.teamBusy === true)
  readonly property bool animating: rowItem.panel !== null && rowItem.panel.opened === true
    && (rowItem.working || rowItem.status === "waiting" || rowItem.leading || rowItem.face === "notifying")
  property real faceStart: Date.now()
  onFaceChanged: { rowItem.faceStart = Date.now(); rowItem.variant = 0 }
  property int variant: 0
  readonly property var variants: rowItem.modelData && rowItem.modelData.variants ? rowItem.modelData.variants : []
  readonly property var drawing: rowItem.variant > 0 && rowItem.variant < rowItem.variants.length
    ? rowItem.variants[rowItem.variant] : rowItem.modelData
  readonly property var eyePivot: {
    var pivot = rowItem.drawing ? rowItem.drawing.eyePivot : null
    return pivot && pivot.length === 2 ? pivot : [0.5, 0.5]
  }

  readonly property real inset: Style.space(8)

  width: parent ? parent.width : 0
  implicitHeight: Style.spacing.controlHeight + (rowItem.working || rowItem.chief ? Style.space(7) * 2 : 0)
  height: implicitHeight

  onAnimatingChanged: {
    if (rowItem.animating) settle.stop(); else settle.restart()
    if (!rowItem.animating) { morph.stop(); ghost.opacity = 0; ghostBody.source = ""; ghostEyes.source = ""; rowItem.variant = 0 }
  }

  Timer {
    id: variantTimer
    interval: MascotMotion.dwellMs(rowItem.face, Math.random())
    repeat: true
    running: rowItem.animating && rowItem.variants.length > 1
    onTriggered: {
      var others = []
      for (var i = 0; i < rowItem.variants.length; i++) if (i !== rowItem.variant) others.push(i)
      rowItem.nextVariant = others[Math.floor(Math.random() * others.length)]
      morph.restart()
      variantTimer.interval = MascotMotion.dwellMs(rowItem.face, Math.random())
    }
  }
  property int nextVariant: 0
  SequentialAnimation {
    id: morph
    ScriptAction {
      script: {
        ghostBody.source = avatarImage.picture
        ghostEyes.source = eyesImage.picture
        ghost.opacity = 1
        rowItem.variant = rowItem.nextVariant
      }
    }
    NumberAnimation { target: ghost; property: "opacity"; to: 0; duration: 260; easing.type: Easing.InOutQuad }
    ScriptAction { script: { ghostBody.source = ""; ghostEyes.source = "" } }
  }

  readonly property bool framed: rowItem.chief || rowItem.working
  readonly property color accentTint: Commons.Color.accent

  Rectangle {
    anchors.fill: parent
    color: rowItem.chief
      ? Qt.rgba(rowItem.accentTint.r, rowItem.accentTint.g, rowItem.accentTint.b, 0.07)
      : rowItem.working
        ? Qt.rgba(rowItem.foreground.r, rowItem.foreground.g, rowItem.foreground.b, 0.04)
        : "transparent"
    border.width: rowItem.framed ? 1 : 0
    border.color: rowItem.chief
      ? Qt.rgba(rowItem.accentTint.r, rowItem.accentTint.g, rowItem.accentTint.b, 0.55)
      : Qt.rgba(rowItem.foreground.r, rowItem.foreground.g, rowItem.foreground.b, 0.2)
  }

  readonly property real stubGap: Style.space(3)
  readonly property real stubEnd: Math.round(botAvatar.x
    + (rowItem.panel ? rowItem.panel.avatarInkInset(rowItem.modelData, botAvatar.width) : 0)
    - rowItem.stubGap)

  Rectangle {
    visible: rowItem.hanging
    anchors.verticalCenter: parent.verticalCenter
    x: rowItem.trunkOffset
    width: rowItem.framed ? -rowItem.trunkOffset : rowItem.stubEnd - x
    height: 1
    color: Qt.rgba(rowItem.foreground.r, rowItem.foreground.g, rowItem.foreground.b, 0.2)
  }

  Item {
    id: botAvatar
    width: Style.space(22)
    height: width
    anchors.left: parent.left
    anchors.leftMargin: rowItem.inset
    anchors.verticalCenter: parent.verticalCenter

    Item {
      id: sway
      width: botAvatar.width
      height: botAvatar.height
      property real u: 1
      readonly property int pixelGrid: 16
      readonly property bool pixelated: rowItem.panel ? rowItem.panel.pixelAvatars === true : false
      property real sx: 1
      property real sy: 1
      transformOrigin: Item.Center
      transform: [
        Scale {
          xScale: sway.sx
          yScale: sway.sy
          origin.x: sway.width / 2
          origin.y: sway.height * (MascotMotion.BASE + 15) / MascotMotion.VIEW
        },
        Scale {
          xScale: sway.u
          yScale: sway.u
          origin.x: sway.width / 2
          origin.y: sway.height / 2
        }
      ]

      Item {
        id: bodyLayer
        anchors.fill: parent
        layer.enabled: sway.pixelated
        layer.textureSize: Qt.size(sway.pixelGrid, sway.pixelGrid)
        layer.smooth: false
        Image {
          id: avatarImage
          readonly property string picture: rowItem.panel ? rowItem.panel.avatarSource(rowItem.drawing) : ""
          anchors.fill: parent
          source: avatarImage.picture
          sourceSize.width: 96
          sourceSize.height: 96
          fillMode: Image.PreserveAspectFit
          smooth: true
          visible: avatarImage.status !== Image.Error
          cache: false
        }
      }

      Image {
        id: eyesImage
        readonly property string picture: rowItem.panel ? rowItem.panel.avatarEyes(rowItem.drawing) : ""
        anchors.fill: parent
        source: eyesImage.picture
        sourceSize.width: 96
        sourceSize.height: 96
        fillMode: Image.PreserveAspectFit
        smooth: true
        cache: false
        visible: eyesImage.picture !== "" && avatarImage.visible
        property real blink: 1
        transform: Scale {
          xScale: 1
          yScale: eyesImage.blink
          origin.x: rowItem.eyePivot[0] * eyesImage.width
          origin.y: rowItem.eyePivot[1] * eyesImage.height
        }

        function arm() {
          var pause = MascotMotion.blinkPause(rowItem.face, Math.random())
          if (pause >= 0 && eyesImage.visible && rowItem.animating) blinkTimer.interval = pause
          if (pause >= 0 && eyesImage.visible && rowItem.animating) blinkTimer.restart()
          else blinkTimer.stop()
        }
        Timer {
          id: blinkTimer
          repeat: false
          onTriggered: blinkClose.start()
        }
        SequentialAnimation {
          id: blinkClose
          NumberAnimation {
            target: eyesImage; property: "blink"; to: MascotMotion.BLINK_FLOOR
            duration: MascotMotion.BLINK_MS * MascotMotion.BLINK_SHUT
          }
          NumberAnimation {
            target: eyesImage; property: "blink"; to: 1
            duration: MascotMotion.BLINK_MS * (1 - MascotMotion.BLINK_SHUT)
          }
          onStopped: eyesImage.arm()
        }
        Connections {
          target: rowItem
          function onAnimatingChanged() { eyesImage.arm() }
          function onFaceChanged() { eyesImage.arm() }
        }
        Component.onCompleted: eyesImage.arm()
        onVisibleChanged: eyesImage.arm()
      }

      Item {
        id: ghost
        anchors.fill: parent
        opacity: 0
        visible: opacity > 0
        Item {
          anchors.fill: parent
          layer.enabled: sway.pixelated
          layer.textureSize: Qt.size(sway.pixelGrid, sway.pixelGrid)
          layer.smooth: false
          Image {
            id: ghostBody
            anchors.fill: parent
            sourceSize.width: 96
            sourceSize.height: 96
            fillMode: Image.PreserveAspectFit
            smooth: true
            cache: false
          }
        }
        Image {
          id: ghostEyes
          anchors.fill: parent
          sourceSize.width: 96
          sourceSize.height: 96
          fillMode: Image.PreserveAspectFit
          smooth: true
          cache: false
        }
      }
    }

    Timer {
      interval: Math.max(rowItem.frameMs, 16)
      repeat: true
      running: rowItem.animating && rowItem.frameMs > 0
      triggeredOnStart: true
      onTriggered: {
        var p = MascotMotion.pose(rowItem.face, Date.now() - rowItem.faceStart)
        var k = botAvatar.width / MascotMotion.VIEW
        sway.x = p.dx * k
        sway.y = p.dy * k
        sway.rotation = p.rot
        sway.u = p.u
        sway.sx = p.sx
        sway.sy = p.sy
      }
    }

    ParallelAnimation {
      id: settle
      NumberAnimation { target: sway; property: "x"; to: 0; duration: 240; easing.type: Easing.OutCubic }
      NumberAnimation { target: sway; property: "y"; to: 0; duration: 240; easing.type: Easing.OutCubic }
      NumberAnimation { target: sway; property: "rotation"; to: 0; duration: 240; easing.type: Easing.OutCubic }
      NumberAnimation { target: sway; property: "u"; to: 1; duration: 240; easing.type: Easing.OutCubic }
      NumberAnimation { target: sway; property: "sx"; to: 1; duration: 240; easing.type: Easing.OutCubic }
      NumberAnimation { target: sway; property: "sy"; to: 1; duration: 240; easing.type: Easing.OutCubic }
      NumberAnimation { target: eyesImage; property: "blink"; to: 1; duration: 160; easing.type: Easing.OutCubic }
    }

    Text {
      visible: avatarImage.picture === "" || avatarImage.status === Image.Error
      anchors.centerIn: parent
      text: rowItem.panel ? rowItem.panel.botGlyph : ""
      color: rowItem.foreground
      font.family: rowItem.fontFamily
      font.pixelSize: Style.bar.iconFont
    }
  }

  Column {
    anchors.left: botAvatar.right
    anchors.leftMargin: Style.space(10)
    anchors.right: parent.right
    anchors.rightMargin: rowItem.inset
    anchors.verticalCenter: parent.verticalCenter
    spacing: Style.space(1)

    Item {
      id: nameLine
      width: parent.width
      height: Math.max(nameText.implicitHeight, statusRow.implicitHeight)

      Text {
        id: nameText
        readonly property real reserved: (crownText.visible ? crownText.width + Style.space(5) : 0)
          + statusRow.width + Style.space(8)
        width: Math.min(implicitWidth, Math.max(0, parent.width - reserved))
        text: rowItem.modelData.name
        textFormat: Text.PlainText
        color: rowItem.foreground
        font.family: rowItem.fontFamily
        font.pixelSize: Style.font.body
        font.weight: rowItem.chief || rowItem.working ? Font.DemiBold : Font.Normal
        elide: Text.ElideRight
      }

      Text {
        id: crownText
        visible: rowItem.chief
        x: nameText.width + Style.space(5)
        y: Math.round((nameText.height - height) / 2)
        text: rowItem.panel ? rowItem.panel.chiefGlyph : ""
        color: rowItem.dim
        font.family: rowItem.fontFamily
        font.pixelSize: Style.font.caption
      }

      Row {
        id: statusRow
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(6)

        Rectangle {
          anchors.verticalCenter: parent.verticalCenter
          width: Style.space(8)
          height: width
          radius: width / 2
          readonly property color tint: rowItem.panel ? rowItem.panel.statusColor(rowItem.status) : "gray"
          color: rowItem.panel && rowItem.panel.statusFilled(rowItem.status) ? tint : "transparent"
          border.width: 1
          border.color: tint
          Behavior on border.color { ColorAnimation { duration: 120 } }
        }

        Text {
          anchors.verticalCenter: parent.verticalCenter
          text: rowItem.panel ? rowItem.panel.statusWord(rowItem.status) : ""
          color: rowItem.panel ? rowItem.panel.statusColor(rowItem.status) : "gray"
          font.family: rowItem.fontFamily
          font.pixelSize: Style.font.caption
          font.weight: rowItem.chief ? Font.DemiBold : Font.Normal
          Behavior on color { ColorAnimation { duration: 120 } }
        }
      }
    }

    Text {
      width: parent.width
      readonly property string work: rowItem.panel ? rowItem.panel.taskFor(rowItem.modelData) : ""
      visible: work !== ""
      textFormat: Text.PlainText
      text: work
      color: rowItem.dim
      font.family: rowItem.fontFamily
      font.pixelSize: Style.font.caption
      elide: Text.ElideRight
    }
  }
}
