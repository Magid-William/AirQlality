//! The two-step API-key prompt (EXP24).
//!
//! Pure: it owns the order the two keys are asked in, the copy shown for each
//! source, how a typed or pasted string is normalised, how it is masked for
//! display, and whether it is worth saving. `ui` only draws it and routes
//! keystrokes; nothing here touches gpui, the filesystem or the environment, so
//! every rule below is unit-testable.
//!
//! Two keys, two slots, free navigation: each step keeps its own field, so
//! Next/Prev switch the view without saving or losing anything, and saving one
//! key never forces the other (the EXP24 user report: "I don't always want to
//! go to OpenAQ first"). Every edit is local until Save, and a note under the
//! field carries the result of that step - saved, checking, accepted, rejected.

/// Which key a prompt step records, in the order they are listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// OpenAQ v3: ground-station observations.
    Openaq,
    /// OpenWeatherMap: a second model forecast.
    Owm,
}

/// How many prompts a full run has.
pub const TOTAL: usize = 2;

/// Shorter than this is almost certainly a partial paste: refuse to save it and
/// say so. Real keys are 32-40 characters, but the guard is deliberately loose
/// because providers change formats.
pub const MIN_KEY_LEN: usize = 8;

/// Longer than this is not a key (paste of a whole paragraph); the tail is
/// dropped rather than refusing the input mid-paste.
pub const MAX_KEY_LEN: usize = 200;

/// How many bullets the field shows before it stops growing. The dialog is
/// narrow and a real key is 32-40 characters, so the cap only matters for an
/// accidental giant paste.
const MASK_MAX: usize = 32;

impl Step {
    /// The first prompt in a run.
    pub fn first() -> Self {
        Step::Openaq
    }

    /// The next prompt in list order, or `None` at the end.
    pub fn next(self) -> Option<Self> {
        match self {
            Step::Openaq => Some(Step::Owm),
            Step::Owm => None,
        }
    }

    /// The previous prompt in list order, or `None` at the start.
    pub fn prev(self) -> Option<Self> {
        match self {
            Step::Openaq => None,
            Step::Owm => Some(Step::Openaq),
        }
    }

    /// 1-based position, for "1 of 2".
    pub fn position(self) -> usize {
        self.index() + 1
    }

    /// 0-based slot, for the per-step field and note.
    pub fn index(self) -> usize {
        match self {
            Step::Openaq => 0,
            Step::Owm => 1,
        }
    }

    /// The source name, as the rest of the app prints it.
    pub fn title(self) -> &'static str {
        match self {
            Step::Openaq => "OpenAQ",
            Step::Owm => "OpenWeatherMap",
        }
    }

    /// What this key adds, in one line.
    pub fn what(self) -> &'static str {
        match self {
            Step::Openaq => "Measured PM2.5 from ground stations near you.",
            Step::Owm => "A second model forecast to cross-check the first.",
        }
    }

    /// Where a free key comes from, as shown in the dialog.
    pub fn where_to_get(self) -> &'static str {
        match self {
            Step::Openaq => "free key: explore.openaq.org/account",
            Step::Owm => "free key: openweathermap.org/api_keys",
        }
    }

    /// The page behind [`Self::where_to_get`], opened by the default browser.
    pub fn url(self) -> &'static str {
        match self {
            Step::Openaq => "https://explore.openaq.org/account",
            Step::Owm => "https://openweathermap.org/api_keys",
        }
    }

    /// The environment variable that overrides the stored key.
    pub fn env_var(self) -> &'static str {
        match self {
            Step::Openaq => "AIRQLALITY_OPENAQ_KEY",
            Step::Owm => "AIRQLALITY_OWM_KEY",
        }
    }

    /// Accessible name for the key field.
    pub fn field_label(self) -> &'static str {
        match self {
            Step::Openaq => "OpenAQ API key",
            Step::Owm => "OpenWeatherMap API key",
        }
    }
}

/// How a note under the field reads: neutral, success, or something to fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Muted,
    Good,
    Warn,
}

/// The line under the field for one step: the result of the last save or key
/// check. `None` falls back to the stored-key hint ("not set" / "current: …").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub text: String,
    pub tone: Tone,
}

impl Note {
    /// "saved: …5e71" - the tail only, never the key.
    pub fn saved(tail: &str) -> Self {
        Self {
            text: format!("saved: \u{2026}{tail}"),
            tone: Tone::Muted,
        }
    }

    /// "checking the key…" while one request is in flight.
    pub fn checking() -> Self {
        Self {
            text: "checking the key\u{2026}".to_string(),
            tone: Tone::Muted,
        }
    }

    /// "key accepted" - the provider answered 200.
    pub fn accepted() -> Self {
        Self {
            text: "key accepted".to_string(),
            tone: Tone::Good,
        }
    }

    /// "rejected: HTTP 401" - the provider refused the credential.
    pub fn rejected(reason: &str) -> Self {
        Self {
            text: format!("rejected: {reason}"),
            tone: Tone::Warn,
        }
    }

    /// A neutral statement ("removed - this source is off").
    pub fn info(text: &str) -> Self {
        Self {
            text: text.to_string(),
            tone: Tone::Muted,
        }
    }

    /// A local refusal: too short, or the other source's key.
    pub fn warning(text: &str) -> Self {
        Self {
            text: text.to_string(),
            tone: Tone::Warn,
        }
    }
}

/// One open prompt run: which step is shown, the text typed on each step, and
/// the note each step carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    step: Step,
    inputs: [String; 2],
    notes: [Option<Note>; 2],
}

impl Prompt {
    /// A fresh run, at step 1 with both fields empty.
    pub fn new() -> Self {
        Self::at(Step::first())
    }

    /// A run starting at `step` (the test seam opens a later step directly).
    pub fn at(step: Step) -> Self {
        Self {
            step,
            inputs: [String::new(), String::new()],
            notes: [None, None],
        }
    }

    /// The step this prompt shows.
    pub fn step(&self) -> Step {
        self.step
    }

    /// "API keys · 1 of 2".
    pub fn label(&self) -> String {
        format!("API keys \u{b7} {} of {}", self.step.position(), TOTAL)
    }

    /// The current step's raw text. Tests only: the UI never renders it, and
    /// nothing outside this module saves anything but [`Self::validate`]'s
    /// normalised key.
    #[cfg(test)]
    pub fn input(&self) -> &str {
        &self.inputs[self.step.index()]
    }

    /// Whether the field would save nothing (empty after normalisation).
    pub fn is_empty(&self) -> bool {
        sanitize(&self.inputs[self.step.index()]).is_empty()
    }

    /// Append one typed character to the current step. Whitespace and control
    /// characters are dropped: no API key contains them, and a stray newline
    /// from a paste would otherwise make the key invalid in a way the user
    /// cannot see (the field is masked).
    pub fn push_char(&mut self, ch: char) {
        if ch.is_whitespace() || ch.is_control() {
            return;
        }
        let input = &mut self.inputs[self.step.index()];
        if input.chars().count() >= MAX_KEY_LEN {
            return;
        }
        input.push(ch);
        self.clear_note();
    }

    /// Append pasted text, normalised the same way as typed characters.
    pub fn push_text(&mut self, text: &str) {
        for ch in text.chars() {
            self.push_char(ch);
        }
    }

    /// Delete the last character of the current step.
    pub fn backspace(&mut self) {
        self.inputs[self.step.index()].pop();
        self.clear_note();
    }

    /// Empty the current step's field (the dialog's "clear" affordance).
    pub fn clear(&mut self) {
        self.inputs[self.step.index()].clear();
        self.clear_note();
    }

    /// What the current step should save.
    ///
    /// - `Ok(None)` - the field is empty: keep whatever is stored (or leave the
    ///   source off). This is why an empty field never destroys a working key.
    /// - `Ok(Some(key))` - a key to store, already normalised.
    /// - `Err(reason)` - looks like a partial paste; stay on this step.
    pub fn validate(&self) -> Result<Option<String>, &'static str> {
        let key = sanitize(&self.inputs[self.step.index()]);
        if key.is_empty() {
            return Ok(None);
        }
        if key.chars().count() < MIN_KEY_LEN {
            return Err("that looks too short \u{2014} paste the whole key");
        }
        Ok(Some(key))
    }

    /// Whether Prev does anything from here.
    pub fn can_prev(&self) -> bool {
        self.step.prev().is_some()
    }

    /// Whether Next does anything from here.
    pub fn can_next(&self) -> bool {
        self.step.next().is_some()
    }

    /// Show the next step if there is one. Nothing is saved or cleared: each
    /// step keeps its own field.
    pub fn next(&mut self) -> bool {
        match self.step.next() {
            Some(next) => {
                self.step = next;
                true
            }
            None => false,
        }
    }

    /// Show the previous step if there is one.
    pub fn prev(&mut self) -> bool {
        match self.step.prev() {
            Some(prev) => {
                self.step = prev;
                true
            }
            None => false,
        }
    }

    /// The note carried by the current step.
    pub fn note(&self) -> Option<&Note> {
        self.notes[self.step.index()].as_ref()
    }

    /// Set the note on the current step.
    pub fn set_note(&mut self, note: Note) {
        self.notes[self.step.index()] = Some(note);
    }

    /// Set the note on a specific step (a key check finishes after the user may
    /// have navigated away; the result belongs to its own step).
    pub fn set_note_at(&mut self, step: Step, note: Note) {
        self.notes[step.index()] = Some(note);
    }

    /// Forget the current step's note (called by every edit).
    pub fn clear_note(&mut self) {
        self.notes[self.step.index()] = None;
    }

    /// The masked field text for the current step: one bullet per character,
    /// capped so the field never grows past its box.
    pub fn masked(&self) -> String {
        let n = self.inputs[self.step.index()].chars().count().min(MASK_MAX);
        "\u{2022}".repeat(n)
    }
}

impl Default for Prompt {
    fn default() -> Self {
        Self::new()
    }
}

/// Trim a raw field value down to what a key can actually be: no whitespace, no
/// control characters, at most [`MAX_KEY_LEN`] characters.
pub fn sanitize(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .take(MAX_KEY_LEN)
        .collect()
}

/// The last four characters of a key, for a hint that never prints the whole
/// thing.
pub fn tail(key: &str) -> String {
    let key = sanitize(key);
    key.chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// Whether `key` is the same as the key stored for the *other* source. A user
/// who re-pastes the wrong clipboard entry into the second prompt gets a warning
/// instead of a source that answers 401 half an hour later.
pub fn duplicate_of(key: &str, other: Option<&str>) -> bool {
    let key = sanitize(key);
    match other.map(sanitize) {
        Some(other) if !other.is_empty() => key.eq_ignore_ascii_case(&other),
        _ => false,
    }
}

/// The muted line under the field: what is stored for this source right now,
/// showing at most the last four characters so the dialog never prints a key.
pub fn existing_hint(current: Option<&str>) -> String {
    match current.map(sanitize).filter(|k| !k.is_empty()) {
        None => "not set".to_string(),
        Some(key) => format!("current: \u{2026}{}", tail(&key)),
    }
}

/// Which prompt control Tab moves to. `None` is the text field; 0 Prev, 1 Next,
/// 2 the free-key link, 3 Remove, 4 Cancel, 5 Save. Disabled controls are
/// skipped and the cycle wraps back to the field.
pub fn next_action(
    current: Option<usize>,
    can_prev: bool,
    can_next: bool,
    has_key: bool,
) -> Option<usize> {
    let enabled = [can_prev, can_next, true, has_key, true, true];
    let start = match current {
        None => 0,
        Some(index) => index + 1,
    };
    let next = (start..enabled.len()).find(|&i| enabled[i]);
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_run_asks_openaq_then_openweathermap() {
        let mut prompt = Prompt::new();
        assert_eq!(prompt.step(), Step::Openaq);
        assert_eq!(prompt.label(), "API keys \u{b7} 1 of 2");
        assert!(prompt.next());
        assert_eq!(prompt.step(), Step::Owm);
        assert_eq!(prompt.label(), "API keys \u{b7} 2 of 2");
        assert!(!prompt.next(), "the second prompt is the end of the list");
        assert_eq!(prompt.step(), Step::Owm, "next past the end stays put");
        assert!(prompt.prev());
        assert_eq!(prompt.step(), Step::Openaq);
        assert!(!prompt.prev(), "step 1 has nothing before it");
    }

    #[test]
    fn navigation_never_saves_or_loses_what_was_typed() {
        let mut prompt = Prompt::new();
        prompt.push_text("0123456789abcdef");
        assert!(prompt.next());
        assert!(prompt.is_empty(), "step 2 starts empty");
        prompt.push_text("fedcba9876543210");
        assert!(prompt.prev());
        assert_eq!(prompt.input(), "0123456789abcdef", "step 1 kept its text");
        assert!(prompt.next());
        assert_eq!(prompt.input(), "fedcba9876543210", "step 2 kept its text");
    }

    #[test]
    fn each_step_keeps_its_own_note() {
        let mut prompt = Prompt::new();
        prompt.set_note(Note::saved("5e71"));
        assert_eq!(prompt.note().map(|n| n.text.as_str()), Some("saved: \u{2026}5e71"));
        prompt.next();
        assert!(prompt.note().is_none(), "the second step has its own line");
        prompt.set_note_at(Step::Openaq, Note::accepted());
        prompt.prev();
        assert_eq!(prompt.note().map(|n| n.tone), Some(Tone::Good));
    }

    #[test]
    fn typing_drops_whitespace_and_control_characters() {
        let mut prompt = Prompt::new();
        for ch in "ab c\nd\te".chars() {
            prompt.push_char(ch);
        }
        assert_eq!(prompt.input(), "abcde");
        prompt.push_text("  fg\nh  ");
        assert_eq!(prompt.input(), "abcdefgh");
    }

    #[test]
    fn the_field_has_a_hard_length_cap() {
        let mut prompt = Prompt::new();
        prompt.push_text(&"a".repeat(MAX_KEY_LEN + 50));
        assert_eq!(prompt.input().chars().count(), MAX_KEY_LEN);
    }

    #[test]
    fn backspace_and_clear_edit_only_the_current_field() {
        let mut prompt = Prompt::new();
        prompt.push_text("abcd");
        prompt.backspace();
        assert_eq!(prompt.input(), "abc");
        prompt.clear();
        assert!(prompt.is_empty());
        prompt.backspace(); // empty: no panic
        assert!(prompt.is_empty());
    }

    #[test]
    fn editing_clears_the_note_so_a_stale_result_is_never_shown() {
        let mut prompt = Prompt::new();
        prompt.set_note(Note::rejected("HTTP 401"));
        prompt.push_text("a");
        assert!(prompt.note().is_none());
        prompt.set_note(Note::checking());
        prompt.backspace();
        assert!(prompt.note().is_none());
    }

    #[test]
    fn an_empty_field_saves_nothing_so_a_stored_key_survives() {
        assert_eq!(Prompt::new().validate(), Ok(None));
        let mut prompt = Prompt::new();
        prompt.push_text("   \n\t ");
        assert_eq!(prompt.validate(), Ok(None));
    }

    #[test]
    fn a_short_paste_is_refused_with_a_reason() {
        let mut prompt = Prompt::new();
        prompt.push_text("abc123");
        assert!(prompt.validate().is_err());
    }

    #[test]
    fn a_real_key_is_normalised_before_it_is_saved() {
        let mut prompt = Prompt::new();
        prompt.push_text("  0123456789abcdef0123456789abcdef  \n");
        assert_eq!(
            prompt.validate(),
            Ok(Some("0123456789abcdef0123456789abcdef".to_string()))
        );
    }

    #[test]
    fn the_field_is_masked_and_the_mask_is_capped() {
        let mut prompt = Prompt::new();
        prompt.push_text("abc");
        assert_eq!(prompt.masked(), "\u{2022}\u{2022}\u{2022}");
        prompt.push_text(&"x".repeat(MAX_KEY_LEN));
        assert_eq!(prompt.masked().chars().count(), MASK_MAX);
    }

    #[test]
    fn the_stored_key_hint_shows_at_most_the_last_four_characters() {
        assert_eq!(existing_hint(None), "not set");
        assert_eq!(existing_hint(Some("   ")), "not set");
        assert_eq!(existing_hint(Some("0123456789abcdef")), "current: \u{2026}cdef");
        assert!(!existing_hint(Some("0123456789abcdef")).contains("0123"));
    }

    #[test]
    fn the_same_key_in_both_slots_is_caught() {
        let openaq = "a772269875e10e4cffd28fa2b990817da7d3bae229b9dea48534afe0e6915e71";
        assert!(duplicate_of(openaq, Some(openaq)));
        assert!(duplicate_of(" ABCD1234 ", Some("abcd1234")));
        assert!(!duplicate_of("b17ebb2f8a94df73e3633bc4e6fda481", Some(openaq)));
        assert!(!duplicate_of(openaq, None));
        assert!(!duplicate_of(openaq, Some("")));
    }

    #[test]
    fn the_free_key_hint_is_a_link_to_the_provider() {
        for step in [Step::Openaq, Step::Owm] {
            assert!(step.url().starts_with("https://"));
            let host = step.url().trim_start_matches("https://");
            assert!(
                step.where_to_get().contains(host),
                "the visible hint must show the address it opens"
            );
        }
        assert_eq!(Step::Openaq.url(), "https://explore.openaq.org/account");
        assert_eq!(Step::Owm.url(), "https://openweathermap.org/api_keys");
    }

    #[test]
    fn tab_skips_disabled_controls_and_wraps_to_the_field() {
        // Step 1, no key: field -> Next -> link -> Cancel -> Save -> field.
        assert_eq!(next_action(None, false, true, false), Some(1));
        assert_eq!(next_action(Some(1), false, true, false), Some(2));
        assert_eq!(next_action(Some(2), false, true, false), Some(4));
        assert_eq!(next_action(Some(4), false, true, false), Some(5));
        assert_eq!(next_action(Some(5), false, true, false), None);
        // Step 2 with a stored key: field -> Prev -> link -> Remove -> Cancel.
        assert_eq!(next_action(None, true, false, true), Some(0));
        assert_eq!(next_action(Some(0), true, false, true), Some(2));
        assert_eq!(next_action(Some(2), true, false, true), Some(3));
        assert_eq!(next_action(Some(3), true, false, true), Some(4));
    }

    #[test]
    fn the_test_seam_can_open_either_step() {
        let prompt = Prompt::at(Step::Owm);
        assert_eq!(prompt.step(), Step::Owm);
        assert_eq!(prompt.label(), "API keys \u{b7} 2 of 2");
    }
}
