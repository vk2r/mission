#!/usr/bin/env node

import { readFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { dirname, join } from "node:path"

const root = join(dirname(fileURLToPath(import.meta.url)), "..")
const source = readFileSync(join(root, "mascot", "motion.js"), "utf8").replace(/^\.pragma library\s*/, "")
const M = new Function(`${source}; return { MOTION, BLINK, FACE_DWELL, dwellMs, pose, frameMs, blinkOpen, blinkPause, VIEW };`)()
const catalogue = JSON.parse(readFileSync(join(root, "mascot", "mascot.json"), "utf8"))

const problems = []
const near = (name, got, want, eps = 1e-6) => {
  if (!(Math.abs(got - want) <= eps)) problems.push(`${name}: got ${got}, want ${want}`)
}

let p = M.pose("working", 225)
near("working dy up", p.dy, -2.5); near("working sx up", p.sx, 1); near("working sy up", p.sy, 1)
p = M.pose("working", 675)
near("working dy down", p.dy, 2.5); near("working sx down", p.sx, 1.11); near("working sy down", p.sy, 0.89)
p = M.pose("thinking", 750)
near("thinking rot", p.rot, 1.6); near("thinking u", p.u, 1.01)
p = M.pose("idle", 0)
near("idle rest u", p.u, 1); near("idle rest dx", p.dx, 0); near("idle rest rot", p.rot, 0)
near("sleeping lean", M.pose("sleeping", 0).rot, 2)
near("waking enter start", M.pose("waking", 0).u, 0.92)
near("waking enter done", M.pose("waking", 700).u, 1 + 0.03 * Math.sin(700 / 2200 * Math.PI * 2))
p = M.pose("orbit", 800)
near("orbit radius", Math.hypot(p.dx, p.dy), 6)

near("working rate", M.frameMs("working", 22), 1000 / 60)
near("scared rate at 22 px", M.frameMs("scared", 22), 0)
if (!(M.frameMs("scared", 160) > 0)) problems.push("scared at 160 px should move")
near("idle rate at 22 px", M.frameMs("idle", 22), 0)

near("blink start", M.blinkOpen(0), 1)
near("blink shut", M.blinkOpen(134.4), 0.04)
near("blink open", M.blinkOpen(320), 1)
near("blink half-way down", M.blinkOpen(67.2), 0.5)
near("working pause min", M.blinkPause("working", 0), 2800)
near("working pause max", M.blinkPause("working", 1), 5500)
near("sleeping never blinks", M.blinkPause("sleeping", 0.5), -1)

for (const face of Object.keys(catalogue.faces || {})) {
  if (!(face in M.MOTION)) problems.push(`face "${face}" has no motion entry`)
}
for (const face of Object.keys(M.BLINK)) {
  if (!(face in M.MOTION)) problems.push(`blink entry "${face}" has no motion entry`)
}

for (const face of Object.keys(catalogue.faces || {})) {
  if (!M.FACE_DWELL[face]) problems.push(`no dwell range for face ${face}`)
}
near("curious dwell low", M.dwellMs("curious", 0), 1800)
near("curious dwell high", M.dwellMs("curious", 1), 3200)
near("unknown face falls back to idle", M.dwellMs("nope", 0), 9000)

const costsSrc = readFileSync(join(root, "panel", "Costs.qml"), "utf8")
const ease = (t) => 1 - Math.pow(1 - t, 3)
near("ignite count at 0", ease(0), 0); near("ignite count at half", ease(0.5), 0.875); near("ignite count at 1", ease(1), 1)
if (!costsSrc.includes("1 - Math.pow(1 - page.ignite, 3)")) problems.push("Costs.qml count-up no longer uses the pinned ease-out")
if (!costsSrc.includes("Math.ceil(page.ignite * page.blockCount)")) problems.push("Costs.qml ring no longer lights ceil(t * blocks)")
near("ring lit at 0", Math.ceil(0 * 40), 0); near("ring lit at half", Math.ceil(0.5 * 40), 20); near("ring lit at 1", Math.ceil(1 * 40), 40)
if (/Animation\.Infinite/.test(costsSrc)) problems.push("Costs.qml loops an animation of its own")
if ((costsSrc.match(/\bTimer\s*\{/g) || []).length !== 1) problems.push("Costs.qml must keep exactly its one Timer (the focus debounce)")
if (/[Bb]eacon/.test(costsSrc)) problems.push("Costs.qml must not carry the beacon dot any more")

const igniteOf = (l) => Math.min(1, l * 1000 / 800)
const cardP = (l, i) => Math.max(0, Math.min(1, (l - 0.40 - 0.04 * Math.min(i, 7)) / 0.30))
near("ignite at launch 0.5", igniteOf(0.5), 0.625); near("ignite done at 800 ms", igniteOf(800 / 1000), 1)
near("first card idle at 0.40", cardP(0.40, 0), 0)
if (igniteOf(0.40) > 0.55) problems.push("first card must start with the ring <= 55% lit")
near("first card done at 0.70", cardP(0.70, 0), 1)
near("eighth card ends at launch 0.98", cardP(0.98, 7), 1)
near("cards past the eighth rise with the eighth", cardP(0.9, 20), cardP(0.9, 7))
near("all cards arrived at launch 1", cardP(1, 30), 1)
if (!/launchMs: 1000/.test(costsSrc) || !/duration: 1000/.test(costsSrc)) problems.push("Costs.qml launch must be 1000 ms")
if (!costsSrc.includes("Math.min(1, page.launch * page.launchMs / 800)")) problems.push("Costs.qml ignite formula changed")
if (!costsSrc.includes("(page.launch - 0.40 - 0.04 * Math.min(index, 7)) / 0.30")) problems.push("Costs.qml card stagger formula changed")
if (!/enabled: card\.arrived/.test(costsSrc)) problems.push("card HoverHandler must be disabled until the card arrived")
if (!/activeFocusOnTab: card\.arrived/.test(costsSrc)) problems.push("card focus must be disabled until the card arrived")
if (!/arrived: card\.arrive >= 1/.test(costsSrc)) problems.push("card arrived must mean progress >= 1")
if (!/if \(!page\.host\.motionOn\) \{ page\.launch = 1/.test(costsSrc)) problems.push("with motion off the launch must sit at 1")
if (!/page\.focusIndex = -1\n[^\n]*if \(!page\.host\.motionOn\)/.test(costsSrc)) problems.push("relight must clear focusIndex")

const homeSrc = readFileSync(join(root, "panel", "Home.qml"), "utf8")
const markP = (l) => Math.max(0, Math.min(1, l / 0.33))
const titleP = (l) => Math.max(0, Math.min(1, (l - 0.30) / 0.15))
const countP = (l) => ease(Math.max(0, Math.min(1, (l - 0.20) / 0.75)))
const rowP = (l, i) => Math.max(0, Math.min(1, (l - 0.30 - 0.05 * Math.min(i, 7)) / 0.30))
near("mark lit at 300 ms", markP(0.33), 1); near("mark dark at 0", markP(0), 0)
near("titles start at 0.30", titleP(0.30), 0); near("titles done at 0.45", titleP(0.45), 1)
near("count at 0.20", countP(0.20), 0); near("count done at 0.95", countP(0.95), 1)
near("count half-way", countP(0.575), 0.875)
near("first row idle at 0.30", rowP(0.30, 0), 0); near("first row done at 0.60", rowP(0.60, 0), 1)
near("eighth row ends at 0.95", rowP(0.95, 7), 1)
near("rows past the eighth rise with the eighth", rowP(0.8, 20), rowP(0.8, 7))
near("all rows arrived at launch 1", rowP(1, 30), 1)
if (!homeSrc.includes("(page.launch - 0.30 - 0.05 * Math.min(index, 7)) / 0.30")) problems.push("Home.qml row stagger formula changed")
if (!homeSrc.includes("1 - Math.pow(1 - t, 3)")) problems.push("Home.qml count-up no longer uses the pinned ease-out")
if (!/duration: 900/.test(homeSrc)) problems.push("Home.qml launch must be 900 ms")
if (!/onLiveChanged: if \(page\.live\) page\.relight\(\)/.test(homeSrc)) problems.push("Home.qml must relight when it comes on screen")
const infinite = homeSrc.match(/Animation\.Infinite/g) || []
const marquee = /NumberAnimation on shift \{[^}]*loops: Animation\.Infinite[^}]*running: [^\n}]*page\.host\.motionOn[^}]*\}/.test(homeSrc)
if (infinite.length > 1 || (infinite.length === 1 && !marquee) || /\bTimer\s*\{/.test(homeSrc)) problems.push("Home.qml must not loop an animation (except the hazard marquee gated by motionOn) or add a Timer")
if ((homeSrc.match(/\bNumberAnimation\s*\{/g) || []).length !== 1) problems.push("Home.qml must keep exactly one NumberAnimation")
if ((homeSrc.match(/enabled: (chiefRow|specialistRow)\.arrive >= 1/g) || []).length !== 2) problems.push("Home rows must ignore the pointer until they arrived")
if (!/if \(!page\.host\.motionOn\) \{ page\.launch = 1; return \}/.test(homeSrc)) problems.push("with motion off Home's launch must sit at 1")

const panelSrc = readFileSync(join(root, "Panel.qml"), "utf8")
const crewFn = panelSrc.match(/function crewState\(\) \{[\s\S]*?\n  \}\n/)
if (!crewFn) problems.push("Panel.qml lost crewState()")
else {
  const crewState = (rows) => new Function("rows", crewFn[0] + "\nreturn crewState();")(rows)
  const st = (...v) => v.map((status) => ({ status }))
  const cases = [
    [st("idle", "working", "waiting"), "working"], [st("waiting", "failed"), "waiting"],
    [st("idle", "failed", "cancelled"), "ready"], [st("unknown", "unknown"), ""], [[], ""]
  ]
  for (const [rows, want] of cases) {
    const got = crewState(rows)
    if (got !== want) problems.push(`crewState(${rows.map((r) => r.status)}) = "${got}", want "${want}"`)
  }
}
if (!/font\.bold: true\n\s+elide/.test(homeSrc.slice(homeSrc.indexOf("statusLabel")))) problems.push("Home status word must be bold")
if (/statusLabel[\s\S]{0,300}font\.bold/.test(homeSrc.slice(homeSrc.indexOf("id: statusLabel"), homeSrc.indexOf("id: statusLabel") + 330).split("Text {")[0])) problems.push("Home status label must not be bold")

if (problems.length) {
  console.error(problems.join("\n"))
  process.exit(1)
}
console.log(`motion: ${Object.keys(M.MOTION).length} faces, formulas ok`)
