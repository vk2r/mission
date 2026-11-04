#!/usr/bin/env node

import { readFileSync, readdirSync, statSync } from "node:fs"
import { join, dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const langDir = join(root, "lang")
const problems = []

function loadTable(path) {
  const source = readFileSync(path, "utf8").replace(/^\s*\.pragma\s+library\s*$/m, "")
  const fn = new Function(source + "\nreturn STRINGS;")
  return fn()
}

const files = readdirSync(langDir).filter((name) => name.endsWith(".js") && name !== "index.js").sort()
const expectedCodes = ["de", "en", "es", "fr", "it", "nl", "pl", "pt"]
const codes = files.map((name) => name.replace(/\.js$/, ""))

for (const code of expectedCodes) {
  if (!codes.includes(code)) problems.push(`lang/${code}.js is missing`)
}
for (const code of codes) {
  if (!expectedCodes.includes(code)) problems.push(`lang/${code}.js is not a language lang/index.js imports`)
}

const tables = {}
for (const code of codes) {
  try {
    tables[code] = loadTable(join(langDir, `${code}.js`))
  } catch (error) {
    problems.push(`lang/${code}.js does not parse: ${error.message}`)
  }
}

const reference = tables.en ? Object.keys(tables.en).sort() : []
for (const code of codes) {
  const table = tables[code]
  if (!table) continue
  const keys = Object.keys(table).sort()
  for (const key of reference) {
    if (!keys.includes(key)) problems.push(`lang/${code}.js is missing the key "${key}"`)
  }
  for (const key of keys) {
    if (!reference.includes(key)) problems.push(`lang/${code}.js has the key "${key}", which en.js does not`)
    const value = table[key]
    if (typeof value === "string") {
      if (value.trim() === "") problems.push(`lang/${code}.js: "${key}" is empty`)
    } else if (Array.isArray(value)) {
      if (value.length < 2) problems.push(`lang/${code}.js: "${key}" is a plural with fewer than two forms`)
      for (const form of value) {
        if (typeof form !== "string" || form.trim() === "") problems.push(`lang/${code}.js: "${key}" has an empty form`)
      }
    } else {
      problems.push(`lang/${code}.js: "${key}" is neither a string nor a list of forms`)
    }
  }
}

const sentenceKeys = ["costSentenceDay", "costSentenceWeek", "costSentenceMonth"]
if (tables.pl) {
  for (const key of sentenceKeys) {
    const forms = tables.pl[key]
    if (Array.isArray(forms) && forms.length !== 3) {
      problems.push(`lang/pl.js: ${key} needs three forms (1 zadanie, 2 zadania, 5 zadań)`)
    }
  }
}

for (const code of codes) {
  const table = tables[code]
  if (!table) continue
  for (const key of sentenceKeys) {
    for (const form of [].concat(table[key] || [])) {
      if (!form.includes("{cost}") || !form.includes("{tasks}")) {
        problems.push(`lang/${code}.js: "${key}" needs both {cost} and {tasks}: ${form}`)
      }
    }
  }
}

const qmlFiles = []
function walk(dir) {
  for (const name of readdirSync(dir)) {
    if (name === "bin" || name === "engine" || name === "node_modules" || name === ".git") continue
    const path = join(dir, name)
    if (statSync(path).isDirectory()) walk(path)
    else if (name.endsWith(".qml")) qmlFiles.push(path)
  }
}
walk(root)

const used = new Set()
for (const path of qmlFiles) {
  const source = readFileSync(path, "utf8")
  for (const match of source.matchAll(/\b(?:t|tn|trp)\(\s*"([A-Za-z][A-Za-z0-9]*)"/g)) used.add(match[1])
}

const readByI18n = new Set([])

for (const key of used) {
  if (tables.en && tables.en[key] === undefined) problems.push(`a .qml file asks for "${key}", which lang/en.js does not define`)
}
for (const key of reference) {
  if (!used.has(key) && !readByI18n.has(key)) problems.push(`lang/en.js defines "${key}", which no .qml file asks for`)
}

for (const path of qmlFiles) {
  const source = readFileSync(path, "utf8")
  source.split("\n").forEach((line, index) => {
    const literal = line.match(/^\s*text:\s*"([^"]*)"/)
    const words = literal ? literal[1].replace(/\\u\{[0-9A-Fa-f]+\}|\\u[0-9A-Fa-f]{4}|\\x[0-9A-Fa-f]{2}/g, "") : ""
    if (literal && /[A-Za-z]{3}/.test(words)) {
      problems.push(`${path.replace(root + "/", "")}:${index + 1} has a literal text, which belongs in lang/*.js`)
    }
  })
}

const i18n = (() => {
  const source = readFileSync(join(root, "lang", "index.js"), "utf8")
  const body = source
    .replace(/^\s*\.pragma\s+library\s*$/m, "")
    .replace(/^\s*\.import\s+"[a-z]{2}\.js"\s+as\s+\w+\s*$/gm, "")
    .replace(/var STRINGS = \{[\s\S]*?\n\}/, "var STRINGS = __TABLES__")
  return new Function("__TABLES__", body + "\nreturn { tr: tr, trn: trn, trp: trp, resolve: resolve, autoLanguage: autoLanguage,"
    + " keyboardLanguage: keyboardLanguage, systemLanguage: systemLanguage }")(tables)
})()

const sentenceExpectations = [
  ["en", 1, "$1.00 TODAY · 1 TASK"], ["en", 96, "$1.00 TODAY · 96 TASKS"],
  ["es", 1, "$1.00 HOY · 1 TAREA"], ["es", 96, "$1.00 HOY · 96 TAREAS"],
  ["de", 3, "$1.00 HEUTE · 3 AUFGABEN"],
  ["fr", 1, "$1.00 AUJOURD'HUI · 1 TÂCHE"], ["fr", 3, "$1.00 AUJOURD'HUI · 3 TÂCHES"],
  ["pt", 1, "$1.00 HOJE · 1 TAREFA"], ["pt", 3, "$1.00 HOJE · 3 TAREFAS"],
  ["nl", 3, "$1.00 VANDAAG · 3 TAKEN"],
  ["it", 3, "$1.00 OGGI · 3 COMPITI"],
  ["pl", 1, "$1.00 DZIŚ · 1 ZADANIE"], ["pl", 3, "$1.00 DZIŚ · 3 ZADANIA"], ["pl", 5, "$1.00 DZIŚ · 5 ZADAŃ"],
  ["pl", 22, "$1.00 DZIŚ · 22 ZADANIA"], ["pl", 12, "$1.00 DZIŚ · 12 ZADAŃ"], ["pl", 25, "$1.00 DZIŚ · 25 ZADAŃ"]
]
for (const [code, count, expected] of sentenceExpectations) {
  const got = i18n.trp(code, "costSentenceDay", count, { cost: "$1.00", tasks: count })
  if (got !== expected) problems.push(`sentence ${code} ${count}: expected "${expected}", got "${got}"`)
}
for (const [code, key, expected] of [
  ["en", "costSentenceWeek", "$1.00 · 7 DAYS · 2 TASKS"],
  ["es", "costSentenceMonth", "$1.00 · 30 DÍAS · 2 TAREAS"],
  ["pl", "costSentenceWeek", "$1.00 · 7 DNI · 2 ZADANIA"],
  ["fr", "costSentenceMonth", "$1.00 · 30 JOURS · 2 TÂCHES"],
  ["de", "costSentenceMonth", "$1.00 · 30 TAGE · 2 AUFGABEN"]
]) {
  const got = i18n.trp(code, key, 2, { cost: "$1.00", tasks: 2 })
  if (got !== expected) problems.push(`sentence ${code} ${key}: expected "${expected}", got "${got}"`)
}

const autoExpectations = [
  ["es", "en_US.UTF-8", "", "", "es"],
  ["latam", "en_US.UTF-8", "", "", "es"],
  ["us", "es_CL.UTF-8", "", "", "es"],
  ["us", "", "en_US.UTF-8", "", "en"],
  ["", "", "", "en_GB.UTF-8", "en"],
  ["de", "fr_FR.UTF-8", "", "", "de"],
  ["", "pl_PL.UTF-8", "", "", "pl"]
]
for (const [layout, lcAll, lcMessages, lang, expected] of autoExpectations) {
  const locale = i18n.systemLanguage(lcAll, lcMessages, lang)
  const got = i18n.autoLanguage(i18n.keyboardLanguage(layout), locale)
  if (got !== expected) {
    problems.push(`auto language (layout ${layout || "none"}, ${lcAll || lcMessages || lang || "no locale"}): expected "${expected}", got "${got}"`)
  }
}

if (i18n.resolve("en", "es") !== "en") problems.push("a stored language does not win over the automatic one")
if (i18n.resolve("auto", "es") !== "es") problems.push("\"auto\" does not follow the automatic language")
if (i18n.resolve("xx", "es") !== "es") problems.push("an unknown stored language does not fall back to the automatic one")

const textExpectations = [
  ["en", "costWeek", "Week"],
  ["es", "costWeek", "Semana"],
  ["de", "costWeek", "Woche"],
  ["es", "costs", "Medidor"],
  ["en", "costs", "Gauge"],
  ["de", "costs", "Messgerät"],
  ["es", "groupByBot", "Tripulación"],
  ["es", "groupByModel", "Motor"],
  ["es", "sectionOther", "Opciones"],
  ["en", "costList", "List"],
  ["es", "costList", "Lista"],
  ["de", "groupByModel", "Triebwerk"],
  ["pl", "statusWorking", "Pracuje"]
]
for (const [code, key, expected] of textExpectations) {
  const got = i18n.tr(code, key, [])
  if (got !== expected) problems.push(`text ${code} ${key}: expected "${expected}", got "${got}"`)
}

const mustDifferFromEnglish = ["costs", "sectionCosts", "groupByBot", "groupByModel", "sectionOther", "taskTip", "showCostFooter", "pixelAvatars", "autoLaunch", "costTip", "statusLabel", "crewWorking", "crewWaiting", "crewReady"]
for (const code of codes) {
  if (code === "en" || !tables[code] || !tables.en) continue
  for (const key of mustDifferFromEnglish) {
    if (key === "statusLabel" && ["de", "nl", "pl"].includes(code)) continue
    if (key === "sectionOther" && code === "fr") continue
    if (tables[code][key] === tables.en[key]) problems.push(`lang/${code}.js: "${key}" is still the English text "${tables.en[key]}"`)
  }
  for (const key of ["taskSentenceDay", "taskSentenceWeek", "taskSentenceMonth"]) {
    if (JSON.stringify(tables[code][key]) === JSON.stringify(tables.en[key])) problems.push(`lang/${code}.js: "${key}" is still English`)
  }
}

if (problems.length > 0) {
  for (const problem of problems) console.error(`i18n: ${problem}`)
  console.error(`i18n: ${problems.length} problem(s)`)
  process.exit(1)
}
console.log(`i18n: ${codes.length} languages, ${reference.length} keys each, plurals and visible texts ok`)
