//! The checks of `scripts/check-motion.mjs` that read the panel's QML (everything past `mascot/motion.js`),
//! now that the script is gone. They pin the pieces of `Costs.qml`, `Home.qml` and `Panel.qml` that the
//! launch animation and the crew's state depend on. Each test names the lines of the original.

use std::fs;
use std::path::PathBuf;

fn qml(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join(name);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn near(name: &str, got: f64, want: f64) {
    assert!((got - want).abs() <= 1e-6, "{name}: got {got}, want {want}");
}

/// `/\bWORD\s*\{/g` matches: a whole word, optional spaces, an opening brace.
fn count_block(src: &str, word: &str) -> usize {
    let mut found = 0;
    for (at, _) in src.match_indices(word) {
        let before = src[..at].chars().next_back();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        if src[at + word.len()..].trim_start().starts_with('{') {
            found += 1;
        }
    }
    found
}

fn has(src: &str, needle: &str) -> bool {
    src.contains(needle)
}

fn ease(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

#[test]
fn costs_count_up_and_ring_use_the_pinned_ease_out() {
    // check-motion.mjs:58-63
    let costs = qml("panel/Costs.qml");
    near("ignite count at 0", ease(0.0), 0.0);
    near("ignite count at half", ease(0.5), 0.875);
    near("ignite count at 1", ease(1.0), 1.0);
    assert!(has(&costs, "1 - Math.pow(1 - page.ignite, 3)"), "Costs.qml count-up no longer uses the pinned ease-out");
    assert!(
        has(&costs, "Math.ceil(page.ignite * page.blockCount)"),
        "Costs.qml ring no longer lights ceil(t * blocks)"
    );
    near("ring lit at 0", (0.0_f64 * 40.0).ceil(), 0.0);
    near("ring lit at half", (0.5_f64 * 40.0).ceil(), 20.0);
    near("ring lit at 1", (1.0_f64 * 40.0).ceil(), 40.0);
}

#[test]
fn costs_loops_no_animation_keeps_one_timer_and_has_no_beacon() {
    // check-motion.mjs:64-66
    let costs = qml("panel/Costs.qml");
    assert!(!has(&costs, "Animation.Infinite"), "Costs.qml loops an animation of its own");
    assert_eq!(count_block(&costs, "Timer"), 1, "Costs.qml must keep exactly its one Timer (the focus debounce)");
    assert!(!has(&costs, "beacon") && !has(&costs, "Beacon"), "Costs.qml must not carry the beacon dot any more");
}

#[test]
fn costs_launch_stagger_is_the_pinned_formula() {
    // check-motion.mjs:68-76
    let costs = qml("panel/Costs.qml");
    let ignite = |launch: f64| (launch * 1000.0 / 800.0).min(1.0);
    let card = |launch: f64, index: f64| ((launch - 0.40 - 0.04 * index.min(7.0)) / 0.30).clamp(0.0, 1.0);
    near("ignite at launch 0.5", ignite(0.5), 0.625);
    near("ignite done at 800 ms", ignite(800.0 / 1000.0), 1.0);
    near("first card idle at 0.40", card(0.40, 0.0), 0.0);
    assert!(ignite(0.40) <= 0.55, "first card must start with the ring <= 55% lit");
    near("first card done at 0.70", card(0.70, 0.0), 1.0);
    near("eighth card ends at launch 0.98", card(0.98, 7.0), 1.0);
    near("cards past the eighth rise with the eighth", card(0.9, 20.0), card(0.9, 7.0));
    near("all cards arrived at launch 1", card(1.0, 30.0), 1.0);
    assert!(has(&costs, "launchMs: 1000") && has(&costs, "duration: 1000"), "Costs.qml launch must be 1000 ms");
    assert!(has(&costs, "Math.min(1, page.launch * page.launchMs / 800)"), "Costs.qml ignite formula changed");
    assert!(
        has(&costs, "(page.launch - 0.40 - 0.04 * Math.min(index, 7)) / 0.30"),
        "Costs.qml card stagger formula changed"
    );
}

#[test]
fn costs_cards_ignore_the_pointer_and_focus_until_they_arrived() {
    // check-motion.mjs:77-81
    let costs = qml("panel/Costs.qml");
    assert!(has(&costs, "enabled: card.arrived"), "card HoverHandler must be disabled until the card arrived");
    assert!(has(&costs, "activeFocusOnTab: card.arrived"), "card focus must be disabled until the card arrived");
    assert!(has(&costs, "arrived: card.arrive >= 1"), "card arrived must mean progress >= 1");
    assert!(has(&costs, "if (!page.host.motionOn) { page.launch = 1"), "with motion off the launch must sit at 1");
    let relight = costs.match_indices("page.focusIndex = -1\n").any(|(at, text)| {
        let rest = &costs[at + text.len()..];
        rest.lines().next().is_some_and(|line| line.contains("if (!page.host.motionOn)"))
    });
    assert!(relight, "relight must clear focusIndex");
}

#[test]
fn home_launch_stagger_is_the_pinned_formula() {
    // check-motion.mjs:83-103
    let home = qml("panel/Home.qml");
    let mark = |launch: f64| (launch / 0.33).clamp(0.0, 1.0);
    let title = |launch: f64| ((launch - 0.30) / 0.15).clamp(0.0, 1.0);
    let count = |launch: f64| ease(((launch - 0.20) / 0.75).clamp(0.0, 1.0));
    let row = |launch: f64, index: f64| ((launch - 0.30 - 0.05 * index.min(7.0)) / 0.30).clamp(0.0, 1.0);
    near("mark lit at 300 ms", mark(0.33), 1.0);
    near("mark dark at 0", mark(0.0), 0.0);
    near("titles start at 0.30", title(0.30), 0.0);
    near("titles done at 0.45", title(0.45), 1.0);
    near("count at 0.20", count(0.20), 0.0);
    near("count done at 0.95", count(0.95), 1.0);
    near("count half-way", count(0.575), 0.875);
    near("first row idle at 0.30", row(0.30, 0.0), 0.0);
    near("first row done at 0.60", row(0.60, 0.0), 1.0);
    near("eighth row ends at 0.95", row(0.95, 7.0), 1.0);
    near("rows past the eighth rise with the eighth", row(0.8, 20.0), row(0.8, 7.0));
    near("all rows arrived at launch 1", row(1.0, 30.0), 1.0);
    assert!(
        has(&home, "(page.launch - 0.30 - 0.05 * Math.min(index, 7)) / 0.30"),
        "Home.qml row stagger formula changed"
    );
    assert!(has(&home, "1 - Math.pow(1 - t, 3)"), "Home.qml count-up no longer uses the pinned ease-out");
    assert!(has(&home, "duration: 900"), "Home.qml launch must be 900 ms");
    assert!(
        has(&home, "onLiveChanged: if (page.live) page.relight()"),
        "Home.qml must relight when it comes on screen"
    );
}

#[test]
fn home_loops_only_the_hazard_marquee_and_adds_no_timer() {
    // check-motion.mjs:104-111
    let home = qml("panel/Home.qml");
    let infinite = home.matches("Animation.Infinite").count();
    let marquee = home.match_indices("NumberAnimation on shift {").any(|(at, _)| {
        let block = home[at..].split('}').next().unwrap_or("");
        let running = block.find("running: ").map(|at| block[at..].lines().next().unwrap_or(""));
        block.contains("loops: Animation.Infinite") && running.is_some_and(|line| line.contains("page.host.motionOn"))
    });
    assert!(
        !(infinite > 1 || (infinite == 1 && !marquee) || count_block(&home, "Timer") > 0),
        "Home.qml must not loop an animation (except the hazard marquee gated by motionOn) or add a Timer"
    );
    assert_eq!(count_block(&home, "NumberAnimation"), 1, "Home.qml must keep exactly one NumberAnimation");
    let rows = home.matches("enabled: chiefRow.arrive >= 1").count()
        + home.matches("enabled: specialistRow.arrive >= 1").count();
    assert_eq!(rows, 2, "Home rows must ignore the pointer until they arrived");
    assert!(
        has(&home, "if (!page.host.motionOn) { page.launch = 1; return }"),
        "with motion off Home's launch must sit at 1"
    );
}

/// `crewState()` of Panel.qml, as it reads there, written out in Rust. The test below pins the QML text,
/// so this copy cannot drift from it unnoticed.
fn crew_state(statuses: &[&str]) -> &'static str {
    let (mut known, mut waiting) = (false, false);
    for status in statuses {
        if *status == "working" {
            return "working";
        }
        if *status == "waiting" {
            waiting = true;
        }
        if !status.is_empty() && *status != "unknown" {
            known = true;
        }
    }
    if waiting {
        return "waiting";
    }
    if known { "ready" } else { "" }
}

#[test]
fn the_crew_state_reads_working_then_waiting_then_ready() {
    // check-motion.mjs:113-129. The original ran the QML function in JavaScript; here the function's text
    // is pinned and a Rust copy of it is run on the same five cases.
    let panel = qml("Panel.qml");
    let start = panel.find("  function crewState() {").expect("Panel.qml lost crewState()");
    let end = start + panel[start..].find("\n  }\n").expect("crewState() has no end");
    let body: Vec<&str> = panel[start..end].lines().map(str::trim).collect();
    assert_eq!(
        body,
        [
            "function crewState() {",
            "var known = false",
            "var waiting = false",
            "for (var i = 0; i < rows.length; ++i) {",
            "var st = rows[i].status",
            "if (st === \"working\") return \"working\"",
            "if (st === \"waiting\") waiting = true",
            "if (st && st !== \"unknown\") known = true",
            "}",
            "if (waiting) return \"waiting\"",
            "return known ? \"ready\" : \"\"",
        ],
        "crewState() changed: update the Rust copy in this test and its cases"
    );
    let cases: [(&[&str], &str); 5] = [
        (&["idle", "working", "waiting"], "working"),
        (&["waiting", "failed"], "waiting"),
        (&["idle", "failed", "cancelled"], "ready"),
        (&["unknown", "unknown"], ""),
        (&[], ""),
    ];
    for (rows, want) in cases {
        assert_eq!(crew_state(rows), want, "crewState({rows:?})");
    }
}

#[test]
fn home_status_word_is_bold_and_its_label_is_not() {
    // check-motion.mjs:130-131
    let home = qml("panel/Home.qml");
    let from = &home[home.find("statusLabel").expect("statusLabel")..];
    let bold_then_elide = from.match_indices("font.bold: true\n").any(|(at, text)| {
        from[at + text.len()..].trim_start_matches([' ', '\t']).starts_with("elide")
            && from[at + text.len()..].starts_with([' ', '\t'])
    });
    assert!(bold_then_elide, "Home status word must be bold");
    let label_at = home.find("id: statusLabel").expect("id: statusLabel");
    let window: String = home[label_at..].chars().take(330).collect();
    let label = window.split("Text {").next().unwrap_or("");
    let bold_label = label
        .match_indices("statusLabel")
        .any(|(at, text)| label[at + text.len()..].chars().take(300).collect::<String>().contains("font.bold"));
    assert!(!bold_label, "Home status label must not be bold");
}
