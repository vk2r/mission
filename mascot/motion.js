.pragma library

var BODY = 228.541
var VIEW = BODY + 30
var CENTRE = BODY / 2
var BASE = BODY

var MOTION = {
  sleeping: { pulse: [0.028, 4600], tilt: 2 },
  waking: { enter: [0.92, 700], pulse: [0.03, 2200] },
  idle: { pulse: [0.014, 3600] },
  listening: { bob: [2, 2600], pulse: [0.012, 2600] },
  thinking: { sway: [1.6, 3000], pulse: [0.01, 3000] },
  searching: { bob: [3, 1400], sway: [2.2, 1400] },
  working: { bob: [2.5, 900], squash: 0.22 },
  excited: { bob: [9, 520], sway: [3, 1040], squash: 0.35 },
  surprised: { enter: [1.14, 340], jitter: [0.8, 120] },
  suspicious: { sway: [2.4, 2600], tilt: -3 },
  angry: { jitter: [1.3, 95], tilt: 2 },
  drowsy: { pulse: [0.026, 5000], tilt: 3 },
  happy: { bob: [5, 820], squash: 0.28 },
  curious: { sway: [3.4, 1900], tilt: -4 },
  confused: { sway: [3, 2200] },
  bored: { pulse: [0.016, 5200], tilt: 2 },
  proud: { bob: [1.6, 2400], pulse: [0.02, 2400] },
  shy: { pulse: [0.016, 3000], tilt: 4 },
  sad: { pulse: [0.02, 4600], tilt: 3 },
  laughing: { bob: [7, 430], squash: 0.4 },
  scared: { jitter: [2.2, 75] },
  playful: { bob: [6, 620], sway: [5, 1240], squash: 0.3 },
  celebrate: { bob: [10, 480], sway: [4, 960], squash: 0.35 },
  orbit: { circle: [6, 3200] },
  radar: { sway: [6, 2400], pulse: [0.012, 2400] },
  progress: { pulse: [0.022, 1600] },
  spawning: { enter: [0.02, 820], pulse: [0.014, 3600] },
  humming: { pulse: [0.016, 2800] },
  loading: { sway: [2.2, 1500], pulse: [0.012, 1500] },
  dictating: { bob: [2, 2000] },
  writing: { bob: [1.6, 1100] },
  sending: { bob: [3, 900] },
  receiving: { bob: [3, 900] },
  uploading: { bob: [3, 1000] },
  notifying: { bob: [4, 700], sway: [2.5, 700] },
  alerting: { jitter: [2.6, 85] },
  dragging: { tilt: -6, sway: [2, 900] },
  bouncing: { bob: [12, 560], squash: 0.45 },
  "powering-down": { settle: 0.05, tilt: 4 }
}

var BLINK = {
  idle: [6000, 14000], listening: [3000, 7000], thinking: [3500, 7000],
  searching: [1600, 4000], working: [2800, 5500], excited: [2000, 4000],
  surprised: [1800, 3500], suspicious: [4500, 8000], angry: [3500, 7000],
  happy: [2500, 5000], curious: [2500, 5500], confused: [2800, 5500],
  bored: [4000, 8000], proud: [3500, 7000], shy: [3000, 6000], sad: [4000, 8000],
  laughing: [2500, 5000], scared: [1200, 3000], playful: [2000, 4500],
  celebrate: [2200, 4500], humming: [4000, 8000], notifying: [2000, 4000],
  dragging: [2200, 4500]
}

var BLINK_MS = 320
var BLINK_SHUT = 0.42
var BLINK_FLOOR = 0.04
var SETTLE_MS = 1400

function motionOf(face) { return MOTION[face] || {} }

function _ease(e) { var a = e - 1; return 1 + (1.7 + 1) * a * a * a + 1.7 * a * a }
function _settle(e) { return e < 0.5 ? 2 * e * e : 1 - 2 * (1 - e) * (1 - e) }

function pose(face, t) {
  var m = motionOf(face)
  var s = 0, c = 0, d = m.tilt ? m.tilt : 0, u = 1, sx = 1, sy = 1
  function r(period, phase) { return Math.sin(t / period * Math.PI * 2 + (phase || 0)) }
  if (m.bob) {
    var w = r(m.bob[1])
    c -= m.bob[0] * w
    if (m.squash) {
      var q = m.squash * Math.max(0, -w)
      sy = 1 - q * 0.5
      sx = 1 + q * 0.5
    }
  }
  if (m.circle) {
    s += m.circle[0] * r(m.circle[1])
    c += m.circle[0] * r(m.circle[1], Math.PI / 2)
  }
  if (m.sway) d += m.sway[0] * r(m.sway[1])
  if (m.pulse) u *= 1 + m.pulse[0] * r(m.pulse[1])
  if (m.jitter) {
    s += m.jitter[0] * r(m.jitter[1])
    c += m.jitter[0] * r(m.jitter[1] * 0.63, 1.1)
  }
  if (m.enter) {
    var k = t / m.enter[1]
    u *= k >= 1 ? 1 : m.enter[0] + (1 - m.enter[0]) * _ease(Math.max(k, 0))
  }
  if (m.settle !== undefined) {
    var v = Math.min(Math.max(t / SETTLE_MS, 0), 1)
    u *= 1 + (m.settle - 1) * _settle(v)
  }
  return { dx: s, dy: c, rot: d, u: u, sx: sx, sy: sy }
}

function frameMs(face, size) {
  var m = motionOf(face)
  var a = size / VIEW
  function rp(amp, period) { return amp * a >= 0.2 ? Math.PI * 2 * amp * a / period : 0 }
  var n = 0
  if (m.bob) {
    n += rp(m.bob[0], m.bob[1])
    if (m.squash) n += rp(m.squash * 0.5 * BODY, m.bob[1])
  }
  if (m.sway) n += rp(m.sway[0] * Math.PI / 180 * CENTRE, m.sway[1])
  if (m.pulse) n += rp(m.pulse[0] * CENTRE, m.pulse[1])
  if (m.circle) n += rp(m.circle[0], m.circle[1])
  if (m.jitter) n += rp(m.jitter[0], m.jitter[1] * 0.63)
  if (!(n > 0)) return 0
  var steps = [1000 / 20, 1000 / 30, 1000 / 60]
  for (var i = 0; i < steps.length; i++) if (n * steps[i] <= 0.5) return steps[i]
  return 1000 / 60
}

function blinkOpen(t) {
  var a = t / BLINK_MS
  if (a >= 1) return 1
  return Math.max(a < BLINK_SHUT ? 1 - a / BLINK_SHUT : (a - BLINK_SHUT) / (1 - BLINK_SHUT), BLINK_FLOOR)
}

function blinkPause(face, unit) {
  var range = BLINK[face]
  return range ? range[0] + unit * (range[1] - range[0]) : -1
}

var FACE_DWELL = {
  "sleeping": [6000, 10000],
  "waking": [800, 800],
  "idle": [9000, 16000],
  "listening": [2800, 5000],
  "thinking": [2000, 3600],
  "searching": [1000, 1800],
  "working": [1800, 3200],
  "excited": [1100, 2000],
  "surprised": [2500, 4000],
  "suspicious": [2600, 4500],
  "angry": [2200, 3800],
  "drowsy": [4000, 8000],
  "happy": [2500, 4500],
  "curious": [1800, 3200],
  "confused": [2200, 3800],
  "bored": [3500, 6000],
  "proud": [3500, 6000],
  "shy": [3000, 5500],
  "sad": [4000, 7000],
  "laughing": [1200, 2400],
  "scared": [900, 1800],
  "playful": [1500, 3000],
  "celebrate": [1400, 2600],
  "orbit": [4000, 8000],
  "radar": [4000, 8000],
  "progress": [4000, 8000],
  "spawning": [1200, 1200],
  "humming": [5000, 9000],
  "loading": [6000, 10000],
  "dictating": [4000, 8000],
  "sending": [4000, 8000],
  "receiving": [4000, 8000],
  "uploading": [4000, 8000],
  "writing": [4000, 8000],
  "notifying": [1500, 2600],
  "alerting": [2000, 3600],
  "bouncing": [3000, 6000],
  "dragging": [1600, 3000],
  "powering-down": [6000, 9000],
}
function dwellMs(face, unit) {
  var range = FACE_DWELL[face] || FACE_DWELL.idle
  return range[0] + unit * (range[1] - range[0])
}
