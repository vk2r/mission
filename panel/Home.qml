pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import qs.Commons
import qs.Ui
import "../components"

Item {
  id: page
  required property var host

  width: parent ? parent.width : 0
  readonly property real gap: Style.space(12)
  readonly property string title: "Mission"
  readonly property string crewState: page.host.crewState()
  readonly property bool offline: page.host.hasData && !page.host.appRunning
  readonly property bool missing: page.offline && !page.host.installed
  readonly property string installUrl: "https://mausbot.com/"
  readonly property string crewWord: page.offline ? page.host.t("crewOffline")
    : page.crewState === "working" ? page.host.t("crewWorking")
    : page.crewState === "waiting" ? page.host.t("crewWaiting")
    : page.crewState === "ready" ? page.host.t("crewReady") : "\u2014"
  readonly property real bodyHeight: page.offline
    ? Math.max(teamColumn.implicitHeight, launchBox.implicitHeight + Style.space(48))
    : teamColumn.implicitHeight
  implicitHeight: header.implicitHeight + page.gap + page.bodyHeight + page.gap + footer.implicitHeight

  property real launch: 1
  readonly property bool live: page.host.showingHome && page.host.opened === true
  readonly property real markIn: Math.max(0, Math.min(1, page.launch / 0.33))
  readonly property real titlesIn: Math.max(0, Math.min(1, (page.launch - 0.30) / 0.15))
  readonly property real countIn: {
    var t = Math.max(0, Math.min(1, (page.launch - 0.20) / 0.75))
    return 1 - Math.pow(1 - t, 3)
  }
  readonly property real footerIn: {
    var t = Math.max(0, Math.min(1, (page.launch - 0.20) / 0.80))
    return 1 - Math.pow(1 - t, 3)
  }
  function rowProgress(index) {
    return Math.max(0, Math.min(1, (page.launch - 0.30 - 0.05 * Math.min(index, 7)) / 0.30))
  }
  function rowOrder(teamIndex, specialist, index) {
    var teams = page.host.teams
    var n = 0
    for (var t = 0; t < teamIndex && t < teams.length; ++t)
      n += teams[t].chiefs.length + teams[t].specialists.length
    if (specialist && teamIndex < teams.length) n += teams[teamIndex].chiefs.length
    return n + index
  }
  function relight() {
    launchAnim.stop()
    if (!page.host.motionOn) { page.launch = 1; return }
    page.launch = 0
    launchAnim.start()
  }
  onLiveChanged: if (page.live) page.relight()
  onOfflineChanged: if (!page.offline && page.live && page.host.appRunning) page.relight()
  NumberAnimation {
    id: launchAnim
    target: page
    property: "launch"
    from: 0
    to: 1
    duration: 900
  }

  Column {
    id: header
    width: parent.width
    spacing: page.gap

    Item {
      width: parent.width
      implicitHeight: Math.max(mark.height, gearButton.implicitHeight)
      height: implicitHeight

      Row {
        id: headerLeft
        anchors.left: parent.left
        anchors.right: gearButton.left
        anchors.rightMargin: Style.space(8)
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(12)

        BrandIcon {
          id: mark
          anchors.verticalCenter: parent.verticalCenter
          width: Style.space(32)
          height: width
          artworkFile: Qt.resolvedUrl("../assets/mission-mark-hero.svg")
          tint: Color.foreground
          opacity: page.markIn
          transform: Scale {
            xScale: 0.85 + 0.15 * page.markIn
            yScale: 0.85 + 0.15 * page.markIn
            origin.x: mark.width / 2
            origin.y: mark.height / 2
          }
        }

        Column {
          anchors.verticalCenter: parent.verticalCenter
          width: Math.max(0, headerLeft.width - mark.width - headerLeft.spacing)
          spacing: 0
          opacity: page.titlesIn

          Text {
            id: titleText
            width: parent.width
            textFormat: Text.PlainText
            text: page.title
            color: page.host.foreground
            font.family: page.host.fontFamily
            font.pixelSize: Style.font.title
            font.bold: true
            elide: Text.ElideRight
          }

          Row {
            width: parent.width
            spacing: 0

            Text {
              id: statusLabel
              textFormat: Text.PlainText
              text: page.host.t("statusLabel").toUpperCase() + ": "
              color: page.host.accent
              font.family: page.host.fontFamily
              font.pixelSize: 8
            }

            Text {
              width: Math.max(0, parent.width - statusLabel.width)
              textFormat: Text.PlainText
              text: page.crewWord.toUpperCase()
              color: page.host.accent
              font.family: page.host.fontFamily
              font.pixelSize: 8
              font.bold: true
              elide: Text.ElideRight
            }
          }
        }
      }

      Button {
        id: gearButton
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        iconText: "\udb81\udc93"
        bordered: false
        foreground: page.host.foreground
        fontFamily: page.host.fontFamily
        tooltipText: page.host.t("settings")
        onClicked: page.host.goTo("settings")
      }
    }

    PanelSeparator { foreground: page.host.foreground }
  }

  Flickable {
    id: teamScroller

    visible: !page.offline
    anchors.top: header.bottom
    anchors.topMargin: page.gap
    anchors.bottom: footer.top
    anchors.bottomMargin: page.gap
    width: parent.width
    contentWidth: width
    contentHeight: teamColumn.implicitHeight
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    interactive: contentHeight > height

    Column {
      id: teamColumn
      width: parent.width
      spacing: page.gap

      Column {
        id: teamList

        width: parent.width
        spacing: page.gap

        Repeater {
          id: teamRepeater
          model: ScriptModel {
            values: page.host.teams
            objectProp: "key"
          }
          delegate: Column {
            id: team
            required property var modelData
            required property int index
            width: teamList.width
            spacing: Style.space(6)

            Repeater {
              id: chiefRepeater
              model: ScriptModel {
                values: team.modelData.chiefs
                objectProp: "id"
              }
              delegate: BotRow {
                id: chiefRow
                required property int index
                panel: page.host
                chief: true
                readonly property real arrive: page.rowProgress(page.rowOrder(team.index, false, chiefRow.index))
                opacity: chiefRow.arrive
                transform: Translate { y: (1 - chiefRow.arrive) * Style.space(16) }
                enabled: chiefRow.arrive >= 1
              }
            }

            Item {
              id: specialistGroup

              visible: team.modelData.specialists.length > 0
              width: parent.width
              readonly property real groupGap: Style.space(4)
              readonly property real trunkX: Style.space(10)
              readonly property real teamIndent: Style.space(22)
              implicitHeight: visible ? groupGap + specialists.implicitHeight : 0
              height: implicitHeight

              Rectangle {
                visible: team.modelData.chiefs.length > 0
                readonly property Item headRow: page.host.lowestRow(chiefRepeater)
                readonly property real headBottom: headRow
                  ? headRow.y + headRow.height - specialistGroup.y : 0
                x: specialistGroup.trunkX - 0.5
                y: Math.round(headBottom)
                width: 1
                height: Math.max(0, Math.ceil(specialists.y + specialists.railBottom) - Math.round(headBottom))
                color: Qt.alpha(page.host.foreground, 0.22)
                opacity: page.rowProgress(page.rowOrder(team.index, true, 0))
              }

              Column {
                id: specialists

                y: specialistGroup.groupGap
                anchors.left: parent.left
                anchors.leftMargin: specialistGroup.teamIndent
                width: parent.width - specialistGroup.teamIndent
                spacing: Style.space(6)

                readonly property Item lastRow: page.host.lowestRow(specialistRepeater)
                readonly property real railBottom: lastRow ? lastRow.y + lastRow.height / 2 : 0

                Repeater {
                  id: specialistRepeater
                  model: ScriptModel {
                    values: team.modelData.specialists
                    objectProp: "id"
                  }
                  delegate: BotRow {
                    id: specialistRow
                    required property int index
                    panel: page.host
                    hanging: team.modelData.chiefs.length > 0
                    trunkOffset: specialistGroup.trunkX - specialistGroup.teamIndent
                    readonly property real arrive: page.rowProgress(page.rowOrder(team.index, true, specialistRow.index))
                    opacity: specialistRow.arrive
                    transform: Translate { y: (1 - specialistRow.arrive) * Style.space(16) }
                    enabled: specialistRow.arrive >= 1
                  }
                }
              }
            }
          }
        }
      }

      Text {
        visible: page.host.hasData && page.host.rows.length === 0
        width: parent.width
        text: page.host.t("noActivity")
        color: page.host.dim
        font.family: page.host.fontFamily
        font.pixelSize: Style.font.body
        horizontalAlignment: Text.AlignHCenter
        topPadding: Style.space(8)
        bottomPadding: Style.space(8)
      }

    }
  }

  Item {
    id: launchArea
    visible: page.offline
    anchors.top: header.bottom
    anchors.topMargin: page.gap
    anchors.bottom: footer.top
    anchors.bottomMargin: page.gap
    width: parent.width

    Column {
      id: launchBox
      anchors.centerIn: parent
      spacing: Style.space(30)

      BrandIcon {
        anchors.horizontalCenter: parent.horizontalCenter
        width: Style.space(68)
        height: width
        artworkFile: Qt.resolvedUrl(page.missing ? "../assets/launch-pad-empty.svg" : "../assets/launch-ship.svg")
        tint: page.host.accent
        accent: Qt.darker(page.host.accent, 1.6)
      }

      Column {
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: Style.space(6)

        Text {
          anchors.horizontalCenter: parent.horizontalCenter
          textFormat: Text.PlainText
          text: (page.missing ? page.host.t("awaitingVehicle") : page.host.t("readyForLaunch")).toUpperCase()
          color: page.host.foreground
          font.family: page.host.fontFamily
          font.pixelSize: Style.font.title
          font.bold: true
          font.letterSpacing: 3
        }

        Text {
          anchors.horizontalCenter: parent.horizontalCenter
          visible: page.missing
          textFormat: Text.PlainText
          text: page.host.t("notInstalled")
          color: Qt.alpha(page.host.foreground, 0.7)
          font.family: page.host.fontFamily
          font.pixelSize: Style.font.caption
        }
      }

      Button {
        anchors.horizontalCenter: parent.horizontalCenter
        enabled: page.missing || !page.host.launching
        iconText: page.missing ? String.fromCodePoint(0xF01DA) : page.host.launchGlyph
        text: page.missing ? page.host.t("goToInstall")
          : page.host.launching ? page.host.t("launching") : page.host.t("launch")
        bordered: true
        foreground: page.host.accent
        fontFamily: page.host.fontFamily
        horizontalPadding: Style.space(26)
        onClicked: page.missing ? Quickshell.execDetached(["xdg-open", page.installUrl]) : page.host.launchApp()
      }
    }
  }

  Column {
    id: footer
    anchors.bottom: parent.bottom
    width: parent.width
    spacing: page.gap

    PanelSeparator { foreground: page.host.foreground }

    Item {
      id: costRow
      width: parent.width
      implicitHeight: Math.max(costLabel.implicitHeight, costsButton.implicitHeight)
      height: implicitHeight

      Item {
        id: costClip
        visible: !page.offline
        clip: true
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: parent.width * page.footerIn

        Text {
          id: costLabel
          anchors.left: parent.left
          anchors.verticalCenter: parent.verticalCenter
          opacity: page.footerIn
          transform: Translate { x: -(1 - page.footerIn) * Style.space(16) }
          width: Math.max(0, costRow.width - costsButton.width - Style.space(8))
          textFormat: Text.PlainText
          text: page.host.costSentence(page.countIn)
          color: page.host.foreground
          font.family: page.host.fontFamily
          font.pixelSize: Style.font.caption
          wrapMode: Text.Wrap
          topPadding: Style.spacing.controlPaddingY
          bottomPadding: Style.spacing.controlPaddingY

          MouseArea {
            id: costHover
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.NoButton
          }

          PanelToolTip {
            visible: costHover.containsMouse
            text: page.host.showCost ? page.host.t("costTip") : page.host.t("taskTip")
            fontFamily: page.host.fontFamily
          }
        }
      }

      Item {
        id: costsClip
        visible: !page.offline
        clip: true
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: costsButton.width * page.footerIn

        Button {
          id: costsButton
          anchors.right: parent.right
          anchors.verticalCenter: parent.verticalCenter
          opacity: page.footerIn
          transform: Translate { x: (1 - page.footerIn) * Style.space(16) }
          iconText: page.host.costsGlyph
          text: page.host.t("costs")
          fontSize: Style.font.caption
          bordered: true
          foreground: page.host.foreground
          fontFamily: page.host.fontFamily
          onClicked: page.host.goTo("costs")
        }
      }

      Canvas {
        id: hazard
        anchors.fill: parent
        visible: page.offline
        clip: true
        property real shift: 0
        property real band: Style.space(12)
        property color base: page.missing ? page.host.foreground : page.host.accent
        property color stripe: Qt.rgba(base.r, base.g, base.b, page.missing ? 0.35 : 0.85)
        onStripeChanged: requestPaint()
        onShiftChanged: requestPaint()
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onPaint: {
          var ctx = getContext("2d")
          ctx.reset()
          ctx.fillStyle = hazard.stripe
          var w = hazard.band, period = w * 2, h = height
          for (var x = -h - period + (shift % period); x < width + period; x += period) {
            ctx.beginPath()
            ctx.moveTo(x, h)
            ctx.lineTo(x + w, h)
            ctx.lineTo(x + w + h, 0)
            ctx.lineTo(x + h, 0)
            ctx.closePath()
            ctx.fill()
          }
        }
        NumberAnimation on shift {
          from: 0
          to: hazard.band * 2
          duration: 1800
          loops: Animation.Infinite
          running: hazard.visible && page.host.motionOn
        }
      }
    }
  }
}
