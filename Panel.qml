pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Commons as Commons
import qs.Ui
import "lang/index.js" as I18n
import "panel"

Panel {
  id: root

  moduleName: "io.github.vk2r.mission"
  ipcTarget: "io.github.vk2r.mission"
  manageIpc: false

  property var anchorItem: null
  property var hostWidget: null

  readonly property var rows: EngineHub.bots
  readonly property bool hasData: EngineHub.ready
  readonly property bool appRunning: EngineHub.appRunning
  readonly property bool launching: EngineHub.launching
  readonly property bool installed: EngineHub.installed
  function launchApp() { EngineHub.launchApp() }
  function save(key, value, fallback) { EngineHub.save(key, value, fallback) }
  readonly property var costs: EngineHub.costs
  readonly property string costRange: EngineHub.costRange
  readonly property string costGroup: EngineHub.costGroup
  readonly property bool showCost: EngineHub.showCost
  readonly property bool pixelAvatars: EngineHub.pixelAvatars
  readonly property bool autoLaunch: EngineHub.autoLaunch
  readonly property bool notifyOn: EngineHub.notifyOn

  readonly property bool motionOn: root.hostWidget && root.hostWidget.bar
    ? root.hostWidget.bar.foregroundAnimationEnabled !== false : true

  readonly property var teams: {
    var order = []
    var byKey = Object.create(null)
    for (var i = 0; i < rows.length; ++i) {
      var key = rows[i].department ? String(rows[i].department) : "General"
      if (!byKey[key]) {
        byKey[key] = { key: key, chiefs: [], specialists: [], working: false }
        order.push(byKey[key])
      }
      if (rows[i].status === "working") byKey[key].working = true
      if (rows[i].role === "chief") byKey[key].chiefs.push(rows[i])
      else byKey[key].specialists.push(rows[i])
    }
    return order
  }

  function teamWorking(department) {
    var key = department ? String(department) : "General"
    for (var i = 0; i < teams.length; ++i)
      if (teams[i].key === key) return teams[i].working
    return false
  }

  function crewState() {
    var known = false
    var waiting = false
    for (var i = 0; i < rows.length; ++i) {
      var st = rows[i].status
      if (st === "working") return "working"
      if (st === "waiting") waiting = true
      if (st && st !== "unknown") known = true
    }
    if (waiting) return "waiting"
    return known ? "ready" : ""
  }

  readonly property var barIdentity: hostWidget || root
  readonly property color foreground: Commons.Color.popups.text
  readonly property color dim: Qt.darker(foreground, 1.45)
  readonly property string fontFamily: root.bar ? root.bar.fontFamily : Style.font.family

  property string themeRed: ""
  readonly property color failedColor: root.themeRed !== "" ? root.themeRed : Commons.Color.urgent

  readonly property string botGlyph: String.fromCodePoint(0xF06A9)
  readonly property string chiefGlyph: String.fromCodePoint(0xF01A5)
  readonly property string launchGlyph: String.fromCodePoint(0xF14DE)
  readonly property string costsGlyph: String.fromCodePoint(0xF029A)
  readonly property color accent: Commons.Color.accent

  function avatarFile(url) {
    url = url ? String(url) : ""
    return /^file:\/\/\/.*\/[0-9a-f]{16}\.svg$/.test(url) ? url : ""
  }

  function avatarSource(row) {
    return root.avatarFile(row && row.avatar)
  }

  function avatarEyes(row) {
    return root.avatarFile(row && row.avatarEyes)
  }

  readonly property var mascotInk: ({
    blob: 1, capsule: 3, circle: 1, cursor: 3, diamond: 1,
    drop: 4, hexagon: 2, shield: 3, squircle: 1, star: 3
  })

  function avatarInkInset(row, slot) {
    var body = row ? String(row.body) : ""
    var units = mascotInk.hasOwnProperty(body) ? mascotInk[body] : 0
    return slot / 22 * units
  }

  readonly property string lang: hostWidget ? hostWidget.lang : "en"
  readonly property string languageSetting: EngineHub.languageSetting
  readonly property string systemLang: hostWidget ? hostWidget.systemLang : "en"
  function t(key, args) { return I18n.tr(lang, key, args) }
  function tn(key, n, args) { return I18n.trn(lang, key, n, args) }

  function statusWord(status) {
    if (status === "working") return root.t("statusWorking")
    if (status === "waiting") return root.t("statusWaiting")
    if (status === "idle") return root.t("statusIdle")
    if (status === "cancelled") return root.t("statusCancelled")
    if (status === "failed") return root.t("statusFailed")
    return root.t("statusUnknown")
  }

  function statusColor(status) {
    if (status === "working") return Commons.Color.accent
    if (status === "failed") return root.failedColor
    if (status === "waiting") return root.foreground
    return root.dim
  }

  function statusFilled(status) {
    return status === "working" || status === "failed"
  }

  function taskFor(row) {
    if (!row || row.status === "unknown") return root.t("waitingForStatus")
    return row.task ? String(row.task) : ""
  }

  function lowestRow(repeater) {
    var lowest = null
    for (var i = 0; i < repeater.count; ++i) {
      var item = repeater.itemAt(i)
      if (item && (lowest === null || item.y > lowest.y)) lowest = item
    }
    return lowest
  }

  function rangeReport() {
    var reports = root.costs
    var range = root.costRange
    if (reports && reports[range] && reports[range].ok === true) return reports[range]
    return null
  }

  function scale(part) { return part === undefined || part >= 1 ? 1 : Math.max(0, part) }

  function costText(part) {
    var report = root.rangeReport()
    if (!report) return "—"
    return "$" + (Number(report.costUsd) * root.scale(part)).toFixed(2)
  }

  function costSentence(part) {
    var report = root.rangeReport()
    if (!report) return "—"
    var f = root.scale(part)
    var tasks = f === 1 ? Number(report.tasks) : Math.round(Number(report.tasks) * f)
    var v = { cost: root.costText(f), tasks: tasks }
    var withCost = root.showCost
    if (root.costRange === "week")
      return withCost ? root.trp("costSentenceWeek", tasks, v) : root.trp("taskSentenceWeek", tasks, v)
    if (root.costRange === "month")
      return withCost ? root.trp("costSentenceMonth", tasks, v) : root.trp("taskSentenceMonth", tasks, v)
    return withCost ? root.trp("costSentenceDay", tasks, v) : root.trp("taskSentenceDay", tasks, v)
  }
  function trp(key, n, values) { return I18n.trp(root.lang, key, n, values) }

  function costSeries() {
    var report = root.rangeReport()
    var series = report && report.series ? report.series : []
    return Array.isArray(series) ? series : []
  }

  function costBreakdown() {
    var report = root.rangeReport()
    var rows = report ? report[root.costGroup === "model" ? "byModel" : "byBot"] : null
    return Array.isArray(rows) ? rows : []
  }

  readonly property real maxCardHeight: Style.space(519)
  property string page: "home"
  readonly property bool showingHome: root.page === "home"

  function goTo(name) { root.page = String(name) }

  function refresh() {
    EngineHub.refresh()
  }

  function open() {
    root.controller.show()
    if (EngineHub.alerting === true) EngineHub.ack()
    root.refresh()
    if (root.autoLaunch && EngineHub.ready) root.launchApp()
  }

  function close() {
    root.controller.hide()
    root.page = "home"
  }

  function toggle() {
    if (root.opened) root.close()
    else root.open()
  }

  KeyboardPanel {
    id: popup
    anchorItem: root.anchorItem
    owner: root.barIdentity
    bar: root.bar
    open: root.opened
    focusTarget: keyCatcher
    contentWidth: popup.fittedContentWidth(Style.space(400))
    contentHeight: popup.fittedContentHeight(
      root.page === "costs" ? costsPage.implicitHeight : homeView.implicitHeight,
      root.maxCardHeight)

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      onCloseRequested: {
        if (!root.showingHome) root.page = "home"
        else root.close()
      }
      onTextKey: function(text) {
        if (text === "\b" && !root.showingHome) root.page = "home"
      }
      onTabRequested: function(direction) { root.switchPanel(direction) }

      Flickable {
        id: scroller
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: versionFooter.visible ? versionFooter.top : parent.bottom
        anchors.bottomMargin: versionFooter.visible ? Style.space(10) : 0
        contentWidth: width
        contentHeight: root.page === "settings"
          ? Math.max(height, settingsPage.implicitHeight)
          : height
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        interactive: contentHeight > height

        Column {
          width: scroller.width
          spacing: Style.space(12)

          Settings {
            id: settingsPage
            host: root
            visible: root.page === "settings"
            width: parent.width
          }

          Costs {
            id: costsPage
            host: root
            visible: root.page === "costs"
            width: parent.width
            height: scroller.height
          }

          Home {
            id: homeView
            host: root
            width: parent.width
            height: root.showingHome ? scroller.height : implicitHeight
            opacity: root.showingHome ? 1 : 0
            enabled: root.showingHome
          }
        }
      }

      FileView {
        id: manifestFile
        path: Qt.resolvedUrl("manifest.json").toString().replace(/^file:\/\//, "")
        printErrors: false
        onLoaded: {
          try { versionFooter.text = JSON.parse(manifestFile.text()).version || "" } catch (e) { versionFooter.text = "" }
        }
      }

      Text {
        id: versionFooter
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        visible: root.page === "settings" && text !== ""
        horizontalAlignment: Text.AlignHCenter
        textFormat: Text.PlainText
        color: root.dim
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
      }

      FileView {
        id: themeColors
        path: Commons.Color.currentThemePath + "/colors.toml"
        printErrors: false
        onLoaded: {
          var match = String(text()).match(/^\s*red\s*=\s*["']?(#[0-9A-Fa-f]{6})/m)
          root.themeRed = match ? match[1] : ""
        }
      }

      Connections {
        target: Commons.Color
        function onAccentChanged() { themeColors.reload() }
      }
    }
  }
}
