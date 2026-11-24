pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Shapes
import qs.Commons
import qs.Commons as Commons

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
  // What the engine sends to move a cursor or costume: the numbers to blink and morph by, and the pose
  // of each frame (the formulas are the engine's, in avatar/mausbot/motion.rs).
  readonly property var motion: rowItem.modelData && rowItem.modelData.motion ? rowItem.modelData.motion : null
  readonly property var frame: rowItem.panel ? rowItem.panel.frameOf(rowItem.modelData) : null
  readonly property var pose: rowItem.frame && rowItem.frame.pose ? rowItem.frame.pose : null
  function between(range) { return range[0] + Math.random() * (range[1] - range[0]) }
  readonly property bool leading: rowItem.chief && rowItem.panel !== null && !!rowItem.panel.hostWidget
    && (rowItem.panel.teams.length > 1
      ? rowItem.panel.teamWorking(rowItem.modelData ? rowItem.modelData.department : "")
      : rowItem.panel.hostWidget.teamBusy === true)
  readonly property bool animating: rowItem.panel !== null && rowItem.panel.surfaceOpen === true
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
    interval: rowItem.motion ? rowItem.between(rowItem.motion.dwell) : 0
    repeat: true
    running: rowItem.animating && rowItem.variants.length > 1
    onTriggered: {
      var others = []
      for (var i = 0; i < rowItem.variants.length; i++) if (i !== rowItem.variant) others.push(i)
      rowItem.nextVariant = others[Math.floor(Math.random() * others.length)]
      morph.restart()
      variantTimer.interval = rowItem.between(rowItem.motion.dwell)
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
    width: Style.space(28)
    height: width
    anchors.left: parent.left
    anchors.leftMargin: rowItem.inset
    anchors.verticalCenter: parent.verticalCenter

    Item {
      id: sway
      width: botAvatar.width
      height: botAvatar.height
      property real u: 1
      readonly property int pixelGrid: 32
      readonly property bool pixelated: rowItem.panel ? rowItem.panel.pixelAvatars === true : false
      property real sx: 1
      property real sy: 1
      transformOrigin: Item.Center
      transform: [
        Scale {
          xScale: sway.sx
          yScale: sway.sy
          origin.x: sway.width / 2
          origin.y: sway.height * (rowItem.motion ? rowItem.motion.squashOrigin : 0.5)
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
          // An engine body's picture is its extent, wider than the body so the ribbons fit. It is drawn
          // `extent / 100` as big, with the body's 0..100 box over the slot, and spills past it: Qt does
          // not honour the overflow the app relies on, so the picture itself carries the room.
          readonly property var extent: rowItem.modelData && rowItem.modelData.avatarExtent ? rowItem.modelData.avatarExtent : null
          x: avatarImage.extent ? parent.width * avatarImage.extent[0] / 100 : 0
          y: avatarImage.extent ? parent.height * avatarImage.extent[1] / 100 : 0
          width: avatarImage.extent ? parent.width * avatarImage.extent[2] / 100 : parent.width
          height: width
          source: avatarImage.picture
          readonly property int sourcePx: Math.round(122 * (avatarImage.extent ? avatarImage.extent[2] / 100 : 1))
          sourceSize.width: avatarImage.sourcePx
          sourceSize.height: avatarImage.sourcePx
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
        sourceSize.width: 122
        sourceSize.height: 122
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
          var range = rowItem.motion ? rowItem.motion.blink : null
          var pause = range ? rowItem.between(range) : -1
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
            target: eyesImage; property: "blink"; to: rowItem.motion ? rowItem.motion.blinkFloor : 1
            duration: rowItem.motion ? rowItem.motion.blinkMs * rowItem.motion.blinkShut : 0
          }
          NumberAnimation {
            target: eyesImage; property: "blink"; to: 1
            duration: rowItem.motion ? rowItem.motion.blinkMs * (1 - rowItem.motion.blinkShut) : 0
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
            sourceSize.width: 122
            sourceSize.height: 122
            fillMode: Image.PreserveAspectFit
            smooth: true
            cache: false
          }
        }
        Image {
          id: ghostEyes
          anchors.fill: parent
          sourceSize.width: 122
          sourceSize.height: 122
          fillMode: Image.PreserveAspectFit
          smooth: true
          cache: false
        }
      }
    }

    Connections {
      target: rowItem
      // Each frame the engine sends carries the pose; its shifts are fractions of the slot.
      function onPoseChanged() {
        var p = rowItem.pose
        if (!p || !rowItem.animating) return
        sway.x = p.dx * botAvatar.width
        sway.y = p.dy * botAvatar.width
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

    Item {
      id: taskLine
      width: parent.width
      height: Math.max(taskText.implicitHeight, chatPill.visible ? chatPill.height : 0)
      visible: taskText.work !== "" || chatPill.visible

      Text {
        id: taskText
        readonly property string work: rowItem.panel ? rowItem.panel.taskFor(rowItem.modelData) : ""
        width: Math.max(0, parent.width - (chatPill.visible ? chatPill.width + Style.space(8) : 0))
        anchors.verticalCenter: parent.verticalCenter
        visible: work !== ""
        textFormat: Text.PlainText
        text: work
        color: rowItem.dim
        font.family: rowItem.fontFamily
        font.pixelSize: Style.font.caption
        elide: Text.ElideRight
      }

      // The group chat the task came from; a direct chat has none, so no pill.
      Rectangle {
        id: chatPill
        readonly property string chat: rowItem.modelData && rowItem.modelData.chat ? String(rowItem.modelData.chat) : ""
        readonly property color tint: rowItem.panel ? rowItem.panel.statusColor("working") : rowItem.accentTint
        visible: chatPill.chat !== ""
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        width: Math.min(chatLabel.implicitWidth + Style.space(8), Style.space(90))
        height: chatLabel.implicitHeight + Style.space(2)
        radius: height / 2
        color: Qt.rgba(chatPill.tint.r, chatPill.tint.g, chatPill.tint.b, 0.1)
        border.width: 1
        border.color: Qt.rgba(chatPill.tint.r, chatPill.tint.g, chatPill.tint.b, 0.5)

        Text {
          id: chatLabel
          anchors.centerIn: parent
          width: Math.min(implicitWidth, chatPill.width - Style.space(8))
          text: chatPill.chat
          textFormat: Text.PlainText
          color: chatPill.tint
          font.family: rowItem.fontFamily
          font.pixelSize: Style.space(8)
          elide: Text.ElideRight
        }
      }
    }
  }
}
