pragma ComponentBehavior: Bound
import QtQuick
import QtQml.Models
import Quickshell
import qs.Commons
import qs.Ui
import "../components"

Item {
  id: page
  required property var host

  width: parent ? parent.width : 0
  implicitHeight: fixed.implicitHeight + Style.space(6) + listHeader.implicitHeight
    + breakdown.spacing + breakdownList.implicitHeight + Style.space(6)

  readonly property bool active: page.host.page === "costs"
  readonly property var teamBars: page.active ? page.host.costSeries() : []
  readonly property var focused: {
    var row = page.focusIndex >= 0 && page.focusIndex < page.rows.length ? page.rows[page.focusIndex] : null
    return row && Array.isArray(row.series) && row.series.length === page.teamBars.length ? row : null
  }
  readonly property color focusColor: page.focused ? page.rowColor(page.focused) : page.host.accent
  readonly property var bars: {
    if (!page.focused) return page.teamBars
    var out = []
    for (var i = 0; i < page.teamBars.length; ++i)
      out.push({ label: page.teamBars[i].label, costUsd: Number(page.focused.series[i]) || 0 })
    return out
  }
  readonly property real teamPeak: {
    var top = 0
    for (var i = 0; i < page.teamBars.length; ++i) {
      var value = Number(page.teamBars[i].costUsd)
      if (value > top) top = value
    }
    return top
  }
  readonly property real peak: {
    var top = 0
    for (var i = 0; i < page.bars.length; ++i) {
      var value = Number(page.bars[i].costUsd)
      if (value > top) top = value
    }
    return top
  }

  readonly property real inset: Style.space(10)
  readonly property color cardFill: Qt.rgba(page.host.foreground.r, page.host.foreground.g, page.host.foreground.b, 0.04)
  readonly property color cardBorder: Qt.rgba(page.host.foreground.r, page.host.foreground.g, page.host.foreground.b, 0.2)

  readonly property color otherColor: "#8c8a96"
  readonly property color unownedColor: "#3a3a3a"

  function botColor(id) {
    var rows = page.host.rows || []
    for (var i = 0; i < rows.length; ++i)
      if (rows[i].id === id && rows[i].colour) return rows[i].colour
    for (var j = 0; j < rows.length; ++j)
      if (rows[j].name === id && rows[j].colour) return rows[j].colour
    return page.otherColor
  }

  readonly property var modelPalette: [
    "#377FE6", "#E78531", "#8057C8", "#0EA5C6", "#D84F8B", "#D8A729",
    "#01A492", "#E5634E", "#D94B52", "#009957", "#A8ADB4", "#B3BAC4"
  ]

  readonly property var modelColors: {
    var out = Object.create(null)
    var rs = page.rows
    var used = {}
    function key(c) { return String(Qt.color(c)).toLowerCase() }
    for (var i = 0; i < rs.length; ++i) {
      var r = rs[i]
      if (!(r.botId !== undefined && r.botId !== "")) { out[r.name] = page.unownedColor; continue }
      var own = page.botColor(r.botId)
      if (own !== page.otherColor && !used[key(own)]) { out[r.name] = own; used[key(own)] = true }
    }
    var next = 0
    for (var j = 0; j < rs.length; ++j) {
      var row = rs[j]
      if (out[row.name] !== undefined) continue
      var pick = ""
      for (var tries = 0; tries < page.modelPalette.length * 3 && pick === ""; ++tries) {
        var base = page.modelPalette[next % page.modelPalette.length]
        var round = Math.floor(next / page.modelPalette.length)
        var colour = round === 0 ? base : Qt.lighter(base, 1 + 0.3 * round)
        next++
        if (!used[key(colour)]) pick = colour
      }
      out[row.name] = pick !== "" ? pick : page.otherColor
      if (pick !== "") used[key(pick)] = true
    }
    return out
  }

  function rowColor(row) {
    if (!row) return page.otherColor
    if (page.host.costGroup === "model") {
      var chosen = page.modelColors[row.name]
      return chosen !== undefined ? chosen : page.unownedColor
    }
    return row.deleted === true ? page.otherColor : page.botColor(row.botId)
  }

  readonly property var rows: page.active ? page.host.costBreakdown() : []
  readonly property int blockCount: 40

  property real launch: 1
  readonly property real launchMs: 1000
  readonly property real ignite: Math.min(1, page.launch * page.launchMs / 800)
  readonly property real titlesIn: Math.max(0, Math.min(1, (page.launch - 0.08) / 0.2))
  readonly property int lit: Math.ceil(page.ignite * page.blockCount)
  readonly property real totalUsd: {
    var report = page.host.rangeReport()
    return report ? Number(report.costUsd) : 0
  }
  readonly property string totalText: {
    if (page.ignite >= 1 || !page.host.rangeReport()) return page.host.costText()
    return "$" + (page.totalUsd * (1 - Math.pow(1 - page.ignite, 3))).toFixed(2)
  }
  function cardProgress(index) {
    return Math.max(0, Math.min(1, (page.launch - 0.40 - 0.04 * Math.min(index, 7)) / 0.30))
  }
  function relight() {
    launchAnim.stop()
    page.focusIndex = -1
    if (!page.host.motionOn) { page.launch = 1; return }
    page.launch = 0
    launchAnim.start()
  }
  onActiveChanged: if (page.active) page.relight()
  NumberAnimation {
    id: launchAnim
    target: page
    property: "launch"
    from: 0
    to: 1
    duration: 1000
  }

  property int focusIndex: -1
  function focusRow(index) { clearFocus.stop(); page.focusIndex = index }
  function blurRow(index) { if (page.focusIndex === index) clearFocus.restart() }
  Timer { id: clearFocus; interval: 90; onTriggered: page.focusIndex = -1 }
  Connections {
    target: page.host
    function onCostGroupChanged() { page.focusIndex = -1; if (page.active) page.relight() }
    function onCostRangeChanged() { page.focusIndex = -1; if (page.active) page.relight() }
  }
  readonly property var segments: {
    var colors = []
    var total = 0
    for (var i = 0; i < page.rows.length; ++i) {
      colors.push(page.rowColor(page.rows[i]))
      var cost = Number(page.rows[i].costUsd)
      if (cost > 0) total += cost
    }
    var blocks = []
    var owners = []
    if (total > 0) {
      var counts = []
      var given = 0
      for (var j = 0; j < page.rows.length; ++j) {
        var share = Math.max(0, Number(page.rows[j].costUsd)) / total * page.blockCount
        counts.push({ index: j, whole: Math.floor(share), rest: share - Math.floor(share) })
        given += Math.floor(share)
      }
      var order = counts.slice().sort(function(l, r) { return r.rest - l.rest })
      for (var k = 0; k < page.blockCount - given && k < order.length; ++k) order[k].whole += 1
      for (var m = 0; m < counts.length; ++m)
        for (var n = 0; n < counts[m].whole; ++n) { blocks.push(colors[m]); owners.push(m) }
    }
    return { blocks: blocks, owners: owners, total: total, colors: colors }
  }

  Column {
    id: fixed

    width: parent.width
    spacing: Style.space(16)

    Item {
      width: parent.width
      implicitHeight: Math.max(headerLeft.implicitHeight, headerControls.implicitHeight)
      height: implicitHeight

      Row {
        id: headerLeft

        anchors.left: parent.left
        anchors.right: headerControls.left
        anchors.rightMargin: Style.space(8)
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(12)

        Button {
          id: backButton
          anchors.verticalCenter: parent.verticalCenter
          iconText: "\udb80\udc4d"
          bordered: false
          foreground: page.host.foreground
          fontFamily: page.host.fontFamily
          tooltipText: page.host.t("backTip")
          onClicked: page.host.goTo("home")
        }

        Text {
          anchors.verticalCenter: parent.verticalCenter
          width: Math.max(0, headerLeft.width - backButton.width - headerLeft.spacing)
          textFormat: Text.PlainText
          text: page.host.t("costs")
          color: page.host.foreground
          font.family: page.host.fontFamily
          font.pixelSize: Style.font.title
          font.bold: true
          elide: Text.ElideRight
        }
      }

      Row {
        id: headerControls

        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: Style.space(6)

        RangePicker {
          id: rangePicker
          host: page.host
          width: Math.min(fitWidth, Style.space(104))
          foreground: page.host.foreground
          accent: page.host.accent
          fontSize: Style.font.caption
          rowHeight: Style.font.caption + Style.space(12)
          popupRowHeight: Style.font.caption + Style.space(14)
        }
      }
    }

    PanelSeparator { width: parent.width; foreground: page.host.foreground }

    Item {
      id: overview

      x: page.inset
      width: parent.width - 2 * page.inset
      visible: page.teamPeak > 0
      height: Math.max(leftColumn.implicitHeight, gauge.height)

      Column {
        id: leftColumn

        x: overview.width / 2
        width: overview.width / 2
        y: (overview.height - implicitHeight) / 2
        spacing: Style.space(4)

        Text {
          width: parent.width
          textFormat: Text.PlainText
          text: (page.focused
            ? (page.focused.deleted === true ? page.host.t("costDeleted") : String(page.focused.name))
            : page.host.t("costsTotal")).toUpperCase()
          elide: Text.ElideRight
          opacity: page.titlesIn
          horizontalAlignment: Text.AlignHCenter
          color: Qt.darker(page.host.foreground, 1.4)
          font.family: page.host.fontFamily
          font.pixelSize: Style.font.caption
          font.bold: true
        }

        Text {
          width: parent.width
          textFormat: Text.PlainText
          text: page.focused ? "$" + Number(page.focused.costUsd).toFixed(2) : page.totalText
          horizontalAlignment: Text.AlignHCenter
          color: page.host.foreground
          font.family: page.host.fontFamily
          font.pixelSize: Style.font.display
          font.bold: true
          elide: Text.ElideRight
        }

        Item {
          id: plot

          anchors.horizontalCenter: parent.horizontalCenter
          width: Math.min(parent.width - Style.space(16), Style.space(96))
          height: page.teamPeak > 0 ? Style.space(24) : 0
          visible: page.teamPeak > 0

          readonly property real slot: page.bars.length > 0 ? width / page.bars.length : width
          readonly property real gap: Math.max(1, Math.min(2, slot * 0.3))

          Repeater {
            model: ScriptModel {
              values: page.bars
              objectProp: "label"
            }

            delegate: Rectangle {
              required property var modelData
              required property int index

              readonly property real cost: Number(modelData.costUsd)

              width: Math.max(1, Math.round(plot.slot - plot.gap))
              x: Math.round(index * plot.slot + (plot.slot - width) / 2)
              height: page.peak > 0
                ? Math.max(cost > 0 ? 1 : 0, Math.round(cost / page.peak * plot.height))
                : 0
              y: plot.height - height
              color: page.peak > 0 && cost === page.peak
                ? page.focusColor
                : (page.focused ? Qt.alpha(page.focusColor, 0.45) : Qt.alpha(page.host.foreground, 0.45))
              Behavior on height { enabled: page.host.motionOn; NumberAnimation { duration: 120; easing.type: Easing.OutCubic } }
            }
          }

          Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: 1
            color: Qt.alpha(page.host.foreground, 0.22)
          }
        }

        Text {
          visible: page.teamPeak > 0
          width: parent.width
          textFormat: Text.PlainText
          text: page.host.trp("costsPeak", 1, { cost: "$" + page.peak.toFixed(2) }).toUpperCase()
          horizontalAlignment: Text.AlignHCenter
          color: page.focusColor
          font.family: page.host.fontFamily
          font.pixelSize: Style.font.caption
          elide: Text.ElideRight
        }
      }

      Item {
        id: gauge

        x: (overview.width / 2 - width) / 2
        y: (overview.height - height) / 2
        width: Style.space(128)
        height: width

        Canvas {
          id: ring

          anchors.fill: parent
          antialiasing: true

          readonly property real thickness: Style.space(16)
          property var blocks: {
            var all = page.segments.blocks
            var focus = page.focusIndex
            var owners = page.segments.owners
            var out = []
            for (var i = 0; i < all.length; ++i)
              out.push(i < page.lit && (focus < 0 || owners[i] === focus) ? all[i] : "")
            return out
          }
          property color ink: page.host.foreground
          onBlocksChanged: requestPaint()
          onInkChanged: requestPaint()

          function inkAt(alpha) { return Qt.rgba(ink.r, ink.g, ink.b, alpha) }

          onPaint: {
            var c = getContext("2d")
            c.reset()
            var cx = width / 2
            var cy = height / 2
            var outer = width / 2 - Style.space(10)
            var radius = outer - thickness / 2
            var count = page.blockCount
            var step = 2 * Math.PI / count
            var gap = step * 0.22
            var top = -Math.PI / 2

            c.lineCap = "butt"
            c.lineWidth = thickness
            for (var i = 0; i < count; i++) {
              c.strokeStyle = i < blocks.length && blocks[i] !== "" ? blocks[i] : inkAt(0.08)
              c.beginPath()
              c.arc(cx, cy, radius, top + i * step + gap / 2, top + (i + 1) * step - gap / 2, false)
              c.stroke()
            }

            c.lineWidth = Style.space(2)
            c.strokeStyle = inkAt(0.85)
            c.beginPath(); c.arc(cx, cy, outer + Style.space(2), 0, 2 * Math.PI); c.stroke()
            c.lineWidth = 1
            c.strokeStyle = inkAt(0.35)
            c.beginPath(); c.arc(cx, cy, outer - thickness - Style.space(3), 0, 2 * Math.PI); c.stroke()
          }
        }

        Column {
          anchors.centerIn: parent
          width: Style.space(62)
          visible: page.segments.total > 0

          Text {
            id: share

            width: parent.width
            textFormat: Text.PlainText
            text: {
              if (!parent.visible) return ""
              if (page.focusIndex < 0 || page.focusIndex >= page.rows.length) return "100%"
              var part = Number(page.rows[page.focusIndex].costUsd) / page.segments.total * 100
              return part > 0 && part < 1 ? "<1%" : Math.round(part) + "%"
            }
            horizontalAlignment: Text.AlignHCenter
            color: page.host.foreground
            font.family: page.host.fontFamily
            font.pixelSize: Style.font.title + Style.space(2)
            font.bold: true
          }

          Text {
            width: parent.width
            textFormat: Text.PlainText
            text: page.host.t("costsOfUsage").toUpperCase()
            opacity: page.titlesIn
            horizontalAlignment: Text.AlignHCenter
            color: page.host.dim
            font.family: page.host.fontFamily
            font.pixelSize: Style.font.caption
            fontSizeMode: Text.HorizontalFit
            minimumPixelSize: Style.space(7)
          }
        }
      }
    }

    Text {
      visible: page.teamPeak === 0
      width: parent.width
      textFormat: Text.PlainText
      text: page.host.t("costsEmpty")
      color: page.host.dim
      font.family: page.host.fontFamily
      font.pixelSize: Style.font.body
      horizontalAlignment: Text.AlignHCenter
    }
  }

  Column {
    id: breakdown

    y: fixed.implicitHeight + Style.space(6)
    width: parent.width
    height: Math.max(0, page.height - y - Style.space(6))
    spacing: Style.space(8)

    Item {
      id: listHeader

      x: page.inset
      width: parent.width - 2 * page.inset
      opacity: page.titlesIn
      implicitHeight: Math.max(listTitle.implicitHeight, groupPicker.implicitHeight)
      height: implicitHeight

      Text {
        id: listTitle

        anchors.left: parent.left
        anchors.right: groupPicker.left
        anchors.rightMargin: Style.space(8)
        anchors.verticalCenter: parent.verticalCenter
        textFormat: Text.PlainText
        text: page.host.t("costList").toUpperCase()
        color: Qt.darker(page.host.foreground, 1.4)
        font.family: page.host.fontFamily
        font.pixelSize: Style.font.caption
        font.bold: true
        elide: Text.ElideRight
      }

      CompactDropdown {
        id: groupPicker
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        width: Math.min(fitWidth, Style.space(112))
        foreground: page.host.foreground
        accent: page.host.accent
        fontSize: Style.font.caption
        rowHeight: Style.font.caption + Style.space(12)
        popupRowHeight: Style.font.caption + Style.space(14)
        showLabel: false
        fontFamily: page.host.fontFamily
        options: [
          { value: "bot", label: page.host.t("groupByBot") },
          { value: "model", label: page.host.t("groupByModel") }
        ]
        value: page.host.costGroup
        onChanged: function(picked) {
          page.host.save("costGroup", picked, "bot")
          groupPicker.value = Qt.binding(function() { return page.host.costGroup })
        }
      }
    }

    Flickable {
      id: listScroller

      width: parent.width
      height: Math.max(0, breakdown.height - listHeader.height - breakdown.spacing)
      contentWidth: width
      contentHeight: breakdownList.implicitHeight
      clip: true
      boundsBehavior: Flickable.StopAtBounds
      interactive: contentHeight > height

      Column {
        id: breakdownList

        x: page.inset
        width: listScroller.width - 2 * page.inset
        spacing: Style.space(6)

        Repeater {
          model: page.rows

          delegate: Rectangle {
            id: card

            required property var modelData
            required property int index

            width: parent.width
            height: botName.implicitHeight + 2 * Style.space(8)
            color: page.cardFill
            border.width: 1
            border.color: page.cardBorder
            readonly property real arrive: page.cardProgress(card.index)
            readonly property bool arrived: card.arrive >= 1
            opacity: card.arrive
            transform: Translate { y: (1 - card.arrive) * Style.space(16) }
            activeFocusOnTab: card.arrived
            onActiveFocusChanged: activeFocus ? page.focusRow(card.index) : page.blurRow(card.index)

            HoverHandler {
              enabled: card.arrived
              onHoveredChanged: hovered ? page.focusRow(card.index) : page.blurRow(card.index)
            }

            Rectangle {
              id: dot
              x: Style.space(8)
              anchors.verticalCenter: parent.verticalCenter
              width: Style.space(8)
              height: width
              radius: width / 2
              color: page.focusIndex >= 0 && page.focusIndex !== card.index
                ? page.unownedColor
                : (page.segments.colors[card.index] || page.otherColor)
            }

            Text {
              id: botName
              anchors.left: dot.right
              anchors.leftMargin: Style.space(8)
              anchors.right: botCost.left
              anchors.rightMargin: Style.space(8)
              anchors.verticalCenter: parent.verticalCenter
              textFormat: Text.PlainText
              text: !modelData ? "" : (modelData.deleted === true ? page.host.t("costDeleted") : String(modelData.name))
              color: page.host.foreground
              font.family: page.host.fontFamily
              font.pixelSize: Style.font.body
              elide: Text.ElideRight
            }

            Text {
              id: botCost
              anchors.right: parent.right
              anchors.rightMargin: Style.space(8)
              anchors.verticalCenter: parent.verticalCenter
              textFormat: Text.PlainText
              text: "$" + Number(modelData ? modelData.costUsd : 0).toFixed(2)
              color: page.host.dim
              font.family: page.host.fontFamily
              font.pixelSize: Style.font.caption
            }
          }
        }
      }
    }
  }
}
