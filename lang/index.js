.pragma library
.import "en.js" as Lang_en
.import "de.js" as Lang_de
.import "fr.js" as Lang_fr
.import "es.js" as Lang_es
.import "it.js" as Lang_it
.import "pt.js" as Lang_pt
.import "nl.js" as Lang_nl
.import "pl.js" as Lang_pl

var LANGUAGES = [
  { code: "en", name: "English" },
  { code: "de", name: "Deutsch" },
  { code: "fr", name: "Français" },
  { code: "es", name: "Español" },
  { code: "it", name: "Italiano" },
  { code: "pt", name: "Português" },
  { code: "nl", name: "Nederlands" },
  { code: "pl", name: "Polski" }
]

function systemLanguage(lcAll, lcMessages, lang) {
  var values = [lcAll, lcMessages, lang]
  for (var i = 0; i < values.length; i++) {
    var code = String(values[i] || "").toLowerCase().split(/[_.@-]/)[0]
    if (code === "c" || code === "posix" || code === "") continue
    return supported(code) ? code : "en"
  }
  return "en"
}

var KEYBOARD_LANGUAGES = {
  us: "en", gb: "en", uk: "en", ie: "en", au: "en", ca: "en",
  es: "es", latam: "es",
  de: "de", at: "de",
  fr: "fr", be: "fr",
  it: "it",
  pt: "pt", br: "pt",
  nl: "nl",
  pl: "pl"
}

function keyboardLanguage(layout) {
  var code = KEYBOARD_LANGUAGES[String(layout || "").toLowerCase().trim()]
  return code && supported(code) ? code : ""
}

function autoLanguage(keyboardCode, localeCode) {
  return keyboardCode && keyboardCode !== "en" ? keyboardCode : localeCode
}

function supported(code) {
  for (var i = 0; i < LANGUAGES.length; i++) if (LANGUAGES[i].code === code) return true
  return false
}

function languageName(code) {
  for (var i = 0; i < LANGUAGES.length; i++) if (LANGUAGES[i].code === code) return LANGUAGES[i].name
  return code
}

function resolve(setting, systemCode) {
  return setting && setting !== "auto" && supported(setting) ? setting : systemCode
}

function format(text, args) {
  return String(text).replace(/\{(\d+)\}/g, function(match, index) {
    var value = args && args[Number(index)]
    return value === undefined || value === null ? match : String(value)
  })
}

function formatNamed(text, values) {
  return String(text).replace(/\{([A-Za-z][A-Za-z0-9]*)\}/g, function(match, key) {
    var value = values ? values[key] : undefined
    return value === undefined || value === null ? match : String(value)
  })
}

function pluralIndex(lang, forms, n) {
  if (lang === "pl" && forms === 3) {
    var mod10 = n % 10, mod100 = n % 100
    return n === 1 ? 0 : (mod10 >= 2 && mod10 <= 4 && (mod100 < 12 || mod100 > 14)) ? 1 : 2
  }
  if (lang === "fr" || lang === "pt") return n <= 1 ? 0 : forms - 1
  return n === 1 ? 0 : forms - 1
}

function tr(lang, key, args) {
  var table = STRINGS[lang] || STRINGS.en
  var text = table[key]
  if (text === undefined) text = STRINGS.en[key]
  if (text === undefined) return key
  if (Array.isArray(text)) text = text[text.length - 1]
  return format(text, args)
}

function trn(lang, key, n, args) {
  var table = STRINGS[lang] || STRINGS.en
  var forms = table[key] !== undefined ? table[key] : STRINGS.en[key]
  if (forms === undefined) return key
  if (!Array.isArray(forms)) return format(forms, [n].concat(args || []))
  return format(forms[pluralIndex(lang, forms.length, n)], [n].concat(args || []))
}

function trp(lang, key, n, values) {
  var table = STRINGS[lang] || STRINGS.en
  var forms = table[key] !== undefined ? table[key] : STRINGS.en[key]
  if (forms === undefined) return key
  if (!Array.isArray(forms)) return formatNamed(forms, values)
  return formatNamed(forms[pluralIndex(lang, forms.length, n)], values)
}

var STRINGS = {
  en: Lang_en.STRINGS,
  de: Lang_de.STRINGS,
  fr: Lang_fr.STRINGS,
  es: Lang_es.STRINGS,
  it: Lang_it.STRINGS,
  pt: Lang_pt.STRINGS,
  nl: Lang_nl.STRINGS,
  pl: Lang_pl.STRINGS
}
