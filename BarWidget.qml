pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "lang/index.js" as I18n
import "components"

BarWidget {
  id: root
  moduleName: "io.github.vk2r.mission"

  readonly property string state: EngineHub.state
  readonly property bool alerting: EngineHub.alerting
  readonly property bool teamBusy: EngineHub.teamBusy
  readonly property real spin: EngineHub.spin
  readonly property bool motionOn: root.bar ? root.bar.foregroundAnimationEnabled !== false : true

  readonly property bool windowVisible: root.state === "visible"

  readonly property bool opened: panel.item ? panel.item.opened === true : false
  // Whether this bar's panel is showing (docked or floating), for the hub's
  // header clock.
  readonly property bool markInView: panel.item ? panel.item.surfaceOpen === true : false

  readonly property string localeLang: I18n.systemLanguage(Quickshell.env("LC_ALL"), Quickshell.env("LC_MESSAGES"), Quickshell.env("LANG"))
  readonly property string keyboardLang: EngineHub.keyboardLang
  readonly property string systemLang: I18n.autoLanguage(keyboardLang, localeLang)
  readonly property string lang: I18n.resolve(EngineHub.languageSetting, systemLang)
  function t(key, args) { return I18n.tr(lang, key, args) }
  function tn(key, n, args) { return I18n.trn(lang, key, n, args) }

  readonly property real markSize: 16

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  onBarChanged: injectPanel()
  onSettingsChanged: injectPanel()

  function injectPanel() {
    var target = panel.item
    if (!target) return
    target.bar = root.bar
    target.settings = root.settings
    target.anchorItem = button
    target.hostWidget = root
  }

  function togglePanel() {
    if (panel.item) panel.item.toggle()
  }

  function close() {
    if (panel.item) panel.item.close()
  }

  function open() {
    if (panel.item) panel.item.open()
  }

  function refresh() {
    EngineHub.refresh()
  }

  function run(action) {
    EngineHub.run(action)
  }

  function ack() {
    EngineHub.ack()
  }

  Component.onCompleted: EngineHub.attach(root)
  Component.onDestruction: EngineHub.detach(root)

  function tooltipText() {
    var base = root.t("tooltip")
    return root.alerting ? base + " · " + root.t("newActivity") : base
  }

  IpcHandler {
    target: "io.github.vk2r.mission"

    function toggle(): void { root.togglePanel() }
    function open(): void { if (panel.item) panel.item.open() }
    function close(): void { if (panel.item) panel.item.close() }
    function page(name: string): void {
      if (!panel.item) return
      panel.item.open()
      panel.item.goTo(name)
    }
    function popout(): void { if (panel.item) panel.item.popOut() }
    function dock(): void { if (panel.item) panel.item.dock() }
    function show(): void { root.run("show") }
    function hide(): void { root.run("hide") }
    function refresh(): void {
      root.refresh()
      if (panel.item) panel.item.refresh()
    }
    function status(): string {
      return JSON.stringify({
        state: root.state,
        alert: root.alerting,
        panel: root.opened,
        floating: panel.item ? panel.item.floatActive === true : false,
        busy: root.teamBusy
      })
    }
  }

  Loader {
    id: panel
    active: true
    source: Qt.resolvedUrl("Panel.qml")
    visible: false
    onLoaded: root.injectPanel()
  }

  Item {
    id: holder
    anchors.fill: parent

    BarIconButton {
      id: button
      anchors.fill: parent
      bar: root.bar
      iconComponent: Component { Item {} }
      slotSize: Style.bar.iconSlot
      fontSize: Style.bar.iconFont
      active: root.windowVisible
      useActiveColor: true
      activeColor: Color.accent
      foreground: root.windowVisible ? Color.foreground : Qt.alpha(Color.foreground, 0.45)
      tooltipText: root.tooltipText()
      onPressed: function(code) {
        if (code === Qt.RightButton) {
          root.run("show")
        } else if (code === Qt.MiddleButton) {
          root.run("toggle")
        } else if (code === Qt.LeftButton) {
          root.togglePanel()
        }
      }
    }

    // The mark in layers (components/MissionMark.qml), centred on the button
    // and snapped to whole device pixels.
    MissionMark {
      id: mark
      width: root.markSize
      height: root.markSize
      readonly property real ratio: Screen.devicePixelRatio > 0 ? Screen.devicePixelRatio : 1
      x: Math.round((button.width - width) / 2 * ratio) / ratio
      y: Math.round((button.height - height) / 2 * ratio) / ratio
      scale: button.pressScale
      tint: root.windowVisible ? Color.foreground : Qt.alpha(Color.foreground, 0.45)
      accent: root.windowVisible ? Color.accent : Qt.alpha(Color.accent, 0.45)
      angle: root.motionOn ? root.spin : 0
      starA: Qt.point(-3.534, -4.712)
      starB: Qt.point(3.643, 4.712)
      breathe: true
      orbit: ({ rx: 7.135, ry: 2.642, stroke: 1.1625, tilt: 158.1, start: -142.3, sweep: 102.3 })
    }
  }

}
