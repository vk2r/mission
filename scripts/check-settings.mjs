import { readFileSync } from "node:fs"
const hubSrc = readFileSync(process.env.OMB_ENGINEHUB || new URL("../EngineHub.qml", import.meta.url), "utf8")
const ID = "io.github.vk2r.mission"
const grabFrom = (text, name) => {
  const at = text.indexOf(`  function ${name}(`)
  if (at < 0) throw new Error(`no function ${name}`)
  let depth = 0
  for (let j = text.indexOf("{", at); j < text.length; j++) {
    if (text[j] === "{") depth++
    if (text[j] === "}" && --depth === 0) return text.slice(at, j + 1).trim().replace(/^function (\w+)/, "$1")
  }
}
const QmlObject = new Proxy(Object, { get: (o, k) => (k === "hasOwn" ? undefined : o[k]) })
const makeShell = (entry0, withShell = true) => {
  const disk = { bar: { layout: { left: [], center: [], right: [JSON.parse(JSON.stringify(entry0))] } } }
  const shell = { updateEntryInline: (_id, entry) => { disk.bar.layout.right[0] = JSON.parse(JSON.stringify(entry)) } }
  const hub = { entryId: ID, diskEntry: null, pendingChanges: {}, entry: {}, consumers: withShell ? [{ bar: { shell } }] : [], get entryReady() { return this.diskEntry !== null } }
  const names = ["findEntry", "applyDisk", "recompute", "setting", "flag", "choice", "save"]
  Object.assign(hub, new Function("hub", "Object", "JSON", "Date", `return { ${names.map((n) => grabFrom(hubSrc, n)).join(", ")} }`)(hub, QmlObject, JSON, Date))
  const reread = () => hub.applyDisk(JSON.stringify(disk), Date.now())
  return { hub, disk, reread, written: () => disk.bar.layout.right[0] }
}
const make = (entry0 = { id: ID, language: "es", costRange: "week" }, loaded = true) => {
  const sh = makeShell(entry0)
  if (loaded) sh.reread()
  return sh
}
const fail = (m, v) => { throw new Error(m + ": " + JSON.stringify(v)) }
const eq = (a, b) => JSON.stringify(a) === JSON.stringify(b)

let t = make()
t.hub.save("notify", false, true)
if (t.written().costRange !== "week" || t.written().notify !== false || t.written().language !== "es") fail("lost a key", t.written())
t = make()
t.hub.save("language", "auto", "auto")
if ("language" in t.written() || t.written().costRange !== "week") fail("remove broke", t.written())
t = make()
t.hub.save("costGroup", "model", "bot")
if (t.written().costGroup !== "model" || t.written().costRange !== "week" || t.written().language !== "es") fail("costGroup model", t.written())
t.hub.save("costGroup", "bot", "bot")
if ("costGroup" in t.written() || t.written().costRange !== "week") fail("costGroup bot kept", t.written())
t = make({ id: ID, language: "es", costRange: "week", showCost: true, pixelAvatars: true })
t.hub.save("autoLaunch", true, false)
if (!t.written().autoLaunch || !t.written().showCost || !t.written().pixelAvatars) fail("autoLaunch on", t.written())
t.hub.save("pixelAvatars", false, false)
if (!t.written().autoLaunch || "pixelAvatars" in t.written()) fail("pixelAvatars stepped on autoLaunch", t.written())
t = make({ id: ID, language: "en", notify: false, costRange: "week" })
t.hub.save("costRange", "month", "day"); t.hub.save("language", "es", "auto"); t.hub.save("costGroup", "model", "bot"); t.hub.save("costGroup", "bot", "bot")
if (!eq(t.written(), { id: ID, language: "es", notify: false, costRange: "month" })) fail("a later change put stale values back", t.written())
if (t.hub.choice("costRange", ["day", "week", "month"], "day") !== "month" || t.hub.flag("notify", true) !== false) fail("hub reads", t.hub.entry)
{
  const sh = makeShell({ id: ID, language: "es", notify: false, costRange: "week" })
  sh.reread()
  const base = { id: ID, language: "es", notify: false }
  sh.hub.save("costGroup", "model", "bot")
  sh.hub.save("costRange", "month", "day")
  if (!eq(sh.written(), { ...base, costRange: "month", costGroup: "model" })) fail("two changes in a row", sh.written())
  const old = JSON.stringify({ bar: { layout: { right: [{ id: ID, language: "es", notify: false, costRange: "week" }] } } })
  sh.hub.save("costRange", "day", "day")
  sh.hub.applyDisk(old, Date.now())
  if (sh.hub.entry.costRange !== undefined) fail("a late read undid a change", sh.hub.entry)
  sh.hub.save("costGroup", "bot", "bot")
  if (!eq(sh.written(), base)) fail("change after a late read lost one", sh.written())
}
t = make()
t.disk.bar.layout.right[0] = { id: ID, language: "de", costRange: "day" }
t.reread()
t.hub.save("costGroup", "model", "bot")
if (t.written().language !== "de" || t.written().costRange !== "day") fail("ignored an edit made in the file", t.written())
t = make(); t.disk.plugins = [t.disk.bar.layout.right.pop()]; t.reread()
if (t.hub.entry.language !== "es") fail("plugin list entry not found", t.hub.entry)
t = make({ id: ID, language: "es" }, false)
t.hub.save("costGroup", "model", "bot")
if (t.written().costGroup) fail("wrote before the file was read", t.written())
t = makeShell({ id: ID }, false); t.reread()
t.hub.save("notify", false, true)
if (t.written().notify !== undefined) fail("wrote with no shell", t.written())
t = make()
t.disk.bar.layout.right[0] = JSON.parse(`{"id":"${ID}","costRange":"week","__proto__":{"x":1}}`)
t.reread()
t.hub.save("costGroup", "model", "bot")
if (!Object.hasOwn(t.written(), "__proto__") || Object.keys(t.written()).includes("x") || t.written().costGroup !== "model") fail("__proto__ key mishandled", t.written())
for (const off of [false, "false", 0, "0", "no", "off", null, "OFF"]) {
  t = make({ id: ID, notify: off }); if (t.hub.flag("notify", true) !== false) fail("notify stays on for", off)
}
for (const on of [true, "true", 1, "yes"]) {
  t = make({ id: ID, notify: on }); if (t.hub.flag("notify", true) !== true) fail("notify turned off for", on)
}
console.log("settings: single shared entry, keys never overwrite each other, late file reads, no write before the file is read ok")
