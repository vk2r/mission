pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import "lang/index.js" as I18n

Scope {
  id: hub

  property string keyboardLang: ""

  Process {
    command: ["hyprctl", "-j", "devices"]
    running: true
    stdout: StdioCollector {
      onStreamFinished: {
        try {
          const keyboards = JSON.parse(text).keyboards || []
          const kb = keyboards.filter(function(k) { return k.main })[0] || keyboards[0]
          if (!kb) return
          const layouts = String(kb.layout || "").split(",")
          const index = Math.max(0, Math.min(layouts.length - 1, Number(kb.active_layout_index) || 0))
          hub.keyboardLang = I18n.keyboardLanguage(layouts[index])
        } catch (e) {  }
      }
    }
  }

  readonly property string entryId: "io.github.vk2r.mission"
  readonly property string configPath: Quickshell.env("HOME") + "/.config/omarchy/shell.json"

  property var diskEntry: null
  property var pendingChanges: ({})
  property var entry: ({})
  readonly property bool entryReady: diskEntry !== null

  function findEntry(cfg) {
    var layout = cfg && cfg.bar && cfg.bar.layout ? cfg.bar.layout : null
    var sections = layout ? Object.keys(layout) : []
    for (var i = 0; i < sections.length; i++) {
      var list = layout[sections[i]] || []
      for (var j = 0; j < list.length; j++)
        if (list[j] && String(list[j].id) === hub.entryId) return list[j]
    }
    var plugins = cfg && Array.isArray(cfg.plugins) ? cfg.plugins : []
    for (var k = 0; k < plugins.length; k++)
      if (plugins[k] && String(plugins[k].id) === hub.entryId) return plugins[k]
    return null
  }

  function applyDisk(text, now) {
    var cfg = null
    try { cfg = JSON.parse(text) } catch (e) { return }
    var found = hub.findEntry(cfg)
    if (!found) return
    var disk = JSON.parse(JSON.stringify(found))
    var pending = {}
    for (var key in hub.pendingChanges) {
      var change = hub.pendingChanges[key]
      var shown = change.remove ? !Object.prototype.hasOwnProperty.call(disk, key) : JSON.stringify(disk[key]) === JSON.stringify(change.value)
      if (!shown && now - change.at < 3000) pending[key] = change
    }
    hub.pendingChanges = pending
    hub.diskEntry = disk
    hub.recompute()
  }

  function recompute() {
    var merged = JSON.parse(JSON.stringify(hub.diskEntry || {}))
    for (var key in hub.pendingChanges) {
      var change = hub.pendingChanges[key]
      if (change.remove) delete merged[key]
      else merged[key] = change.value
    }
    hub.entry = merged
  }

  function setting(key, fallback) {
    return hub.entry && Object.prototype.hasOwnProperty.call(hub.entry, key) ? hub.entry[key] : fallback
  }

  function flag(key, fallback) {
    var stored = String(hub.setting(key, fallback)).toLowerCase()
    if (fallback) return ["false", "0", "no", "off", "null", ""].indexOf(stored) < 0
    return ["true", "1", "yes", "on"].indexOf(stored) >= 0
  }

  function choice(key, allowed, fallback) {
    var stored = String(hub.setting(key, fallback))
    return allowed.indexOf(stored) >= 0 ? stored : fallback
  }

  function save(key, value, fallback) {
    var shell = hub.consumers.length > 0 && hub.consumers[0].bar ? hub.consumers[0].bar.shell : null
    if (!hub.entryReady || !shell || typeof shell.updateEntryInline !== "function") return
    var remove = value === fallback
    var next = JSON.parse(JSON.stringify(hub.entry))
    if (remove) delete next[key]
    else next[key] = value
    var pending = Object.assign({}, hub.pendingChanges)
    pending[key] = { remove: remove, value: value, at: Date.now() }
    hub.pendingChanges = pending
    hub.recompute()
    shell.updateEntryInline(hub.entryId, next)
  }

  readonly property string languageSetting: String(hub.setting("language", "auto"))
  readonly property bool notifyOn: hub.flag("notify", true)
  readonly property bool showCost: hub.flag("showCost", false)
  readonly property bool pixelAvatars: hub.flag("pixelAvatars", false)
  readonly property bool autoLaunch: hub.flag("autoLaunch", false)
  readonly property string costRange: hub.choice("costRange", ["day", "week", "month"], "day")
  readonly property string costGroup: hub.choice("costGroup", ["bot", "model"], "bot")

  FileView {
    path: hub.configPath
    watchChanges: true
    printErrors: false
    onLoaded: hub.applyDisk(text(), Date.now())
    onFileChanged: reload()
  }

  property var snapshot: ({})

  property bool ready: false

  property string state: "absent"
  property bool appRunning: false
  property bool launching: false
  property bool installed: true
  property bool alerting: false
  property bool teamBusy: false
  property int working: 0

  readonly property var bots: hub.snapshot && hub.snapshot.bots ? hub.snapshot.bots : []

  readonly property var costs: hub.snapshot && hub.snapshot.costs ? hub.snapshot.costs : null

  property real phase: 0

  property var consumers: []

  property bool stopped: false

  property string pending: ""

  readonly property bool notify: hub.notifyOn

  readonly property string engine: Qt.resolvedUrl("bin/engine").toString().replace(/^file:\/\//, "")

  function attach(widget) {
    if (consumers.indexOf(widget) < 0) consumers = consumers.concat([widget])
    hub.ensureEngine()
  }
  function detach(widget) {
    consumers = consumers.filter(function (known) { return known !== widget })
  }

  function apply(data) {
    hub.snapshot = data
    hub.ready = true
    var window = data.window ? data.window : {}
    hub.state = String(window.state ? window.state : "absent")
    hub.appRunning = window.running !== undefined ? window.running === true : hub.state !== "absent"
    hub.launching = window.launching === true
    hub.installed = window.installed !== false
    hub.alerting = data.alert === true
    hub.teamBusy = data.busy === true
    hub.working = Number(data.working ? data.working : 0)
  }

  function send(order) {
    if (!engineProc.running) {
      hub.pending = String(order)
      return
    }
    engineProc.write(String(order) + "\n")
  }

  function launchApp() {
    if (hub.appRunning || hub.launching || !hub.installed) return
    hub.launching = true
    hub.send("launch")
  }

  function refresh() {
    hub.send("refresh")
  }

  function run(action) {
    hub.send(action)
  }

  function ack() {
    hub.alerting = false
    hub.send("ack")
  }

  Process {
    id: engineProc
    command: hub.notify ? [hub.engine] : [hub.engine, "--no-notify"]
    stdinEnabled: true
    stdout: SplitParser {
      onRead: function(line) {
        var text = String(line).trim()
        if (text === "") return
        try {
          const data = JSON.parse(text)
          if (!data || data.error !== undefined) return
          hub.apply(data)
        } catch (e) {
        }
      }
    }
    onRunningChanged: {
      if (!engineProc.running) return
      if (hub.pending === "") return
      var order = hub.pending
      hub.pending = ""
      engineProc.write(order + "\n")
    }
    onExited: function(exitCode) {
      if (hub.consumers.length === 0) return
      if (hub.stopped) {
        hub.stopped = false
        quickRestart.start()
      } else {
        restart.start()
      }
    }
  }

  function ensureEngine() {
    if (consumers.length === 0 || engineProc.running) return
    if (quickRestart.running || restart.running) return
    quickRestart.start()
  }

  onNotifyChanged: {
    if (hub.consumers.length === 0) return
    if (engineProc.running) {
      hub.stopped = true
      engineProc.running = false
    } else {
      hub.ensureEngine()
    }
  }

  onConsumersChanged: {
    if (consumers.length === 0) {
      if (engineProc.running) idleStop.start()
    } else {
      idleStop.stop()
      hub.ensureEngine()
    }
  }

  Timer {
    id: quickRestart
    interval: 300
    onTriggered: {
      hub.stopped = false
      if (hub.consumers.length === 0) {
        hub.pending = ""
        return
      }
      engineProc.running = true
    }
  }

  Timer {
    id: restart
    interval: 5000
    onTriggered: if (hub.consumers.length > 0) engineProc.running = true
  }

  Timer {
    id: idleStop
    interval: 1000
    onTriggered: {
      if (hub.consumers.length > 0) {
        hub.ensureEngine()
        return
      }
      restart.stop()
      quickRestart.stop()
      hub.pending = ""
      if (engineProc.running) {
        hub.stopped = true
        engineProc.running = false
      }
    }
  }

  SequentialAnimation on phase {
    running: hub.teamBusy && hub.consumers.length > 0
    loops: Animation.Infinite
    NumberAnimation { to: 1; duration: 650; easing.type: Easing.InOutSine }
    NumberAnimation { to: 0; duration: 650; easing.type: Easing.InOutSine }
    onStopped: hub.phase = 0
  }
}
